//! Opening games one after another. Play on an account is a queue of one; Launch all on a group is the group's
//! accounts in order. Each game must show its window and get a few seconds to settle before the next one starts
//! (two copies starting at the same moment trip over each other). With a game already open, D2R's one-copy lock is
//! closed in the running games first; that needs the "Open more than one game" switch.
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub const MAX_GAMES: usize = 4;
/// How long a starting game may take to show its window.
pub const START_TIMEOUT: Duration = Duration::from_secs(90);
/// After a game's window shows, before the next game starts.
pub const SETTLE: Duration = Duration::from_secs(5);
/// Error code the window turns into the "Open N games at once?" question.
pub const NEEDS_MULTI: &str = "needs_multi";
/// A token account's game takes its sign-in when it reaches character select; the next game waits for that, this long
/// at most.
pub const TOKEN_WAIT: Duration = Duration::from_secs(90);

/// How to start one game: the D2R arguments, and for an Authenticator account the sign-in token (and its region code)
/// that has to be in place first.
pub struct Start { pub args: Vec<String>, pub token: Option<(String, String)> }

/// What the launcher needs from Windows (a fake one in the tests).
pub trait Games {
    /// Running D2R processes.
    fn running(&mut self) -> Vec<u32>;
    fn has_window(&mut self, pid: u32) -> bool;
    /// Closes D2R's one-copy lock in a running game.
    fn close_lock(&mut self, pid: u32) -> Result<usize, String>;
    fn start(&mut self, start: Start) -> Result<(), String>;
    /// True once the game started last has taken the sign-in token it was given (or none was given).
    fn token_taken(&mut self) -> bool;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State { Waiting, Starting, Settling, Done, Failed, Skipped }

impl State {
    fn word(self) -> &'static str {
        match self { State::Waiting => "waiting", State::Starting => "starting", State::Settling => "settling", State::Done => "done", State::Failed => "failed", State::Skipped => "skipped" }
    }
    fn active(self) -> bool { matches!(self, State::Waiting | State::Starting | State::Settling) }
}

struct Step {
    account: String,
    name: String,
    state: State,
    note: String,
    since: Instant,
    before: Vec<u32>,     // games running when this one was started
    retried: bool,
    pid: Option<u32>,
    until: Instant,
    token: bool,          // an Authenticator account: settled only when the game took its sign-in
}

#[derive(Default)]
pub struct Launcher {
    label: Option<String>,
    group: Option<String>,
    steps: Vec<Step>,
    allow_multi: bool,
    error: Option<String>,
    /// How many launches have stopped with a failure, and why the last one did ("game 2: ...", no names) - the
    /// window offers a problem report for each new one.
    pub failures: u32,
    pub last_failure: String,
}

impl Launcher {
    pub fn busy(&self) -> bool { self.steps.iter().any(|s| s.state.active()) }

    /// Queues accounts (id, name) to open in this order. `running` = games open now.
    pub fn begin(&mut self, now: Instant, label: Option<String>, group: Option<String>, accounts: Vec<(String, String)>, allow_multi: bool, running: usize) -> Result<(), String> {
        if self.busy() { return Err("Wait until the games finish opening".into()); }
        if accounts.is_empty() { return Err("Pick an account".into()); }
        let total = running + accounts.len();
        if total > MAX_GAMES { return Err(format!("That would be {} games. The most is {}, one per quarter of the screen: close a game first.", total, MAX_GAMES)); }
        if total > 1 && !allow_multi { return Err(format!("{}:{}", NEEDS_MULTI, total)); }
        self.label = label;
        self.group = group.clone();
        self.allow_multi = allow_multi;
        self.error = None;
        crate::report::log(format!("{}: {} game{} ({} already open, several games {})", if group.is_some() { "Launch all" } else { "Play" },
            accounts.len(), if accounts.len() == 1 { "" } else { "s" }, running, if allow_multi { "on" } else { "off" }));
        self.steps = accounts.into_iter().map(|(account, name)| Step { account, name, state: State::Waiting, note: String::new(), since: now,
            before: Vec::new(), retried: false, pid: None, until: now, token: false }).collect();
        Ok(())
    }

    /// Stops what has not started yet (a game already starting keeps going).
    pub fn stop(&mut self) {
        crate::report::log("stopped by the user");
        for s in self.steps.iter_mut().filter(|s| s.state == State::Waiting) { s.state = State::Skipped; s.note = "Stopped".into(); }
    }

    fn fail(&mut self, i: usize, why: String) {
        self.steps[i].state = State::Failed;
        self.steps[i].note = why.clone();
        self.error = Some(format!("{}: {}", self.steps[i].name, why));
        self.failures += 1;
        self.last_failure = format!("game {}: {}", i + 1, why);
        crate::report::log(format!("game {} FAILED: {}", i + 1, why));
        for s in self.steps.iter_mut().filter(|s| s.state == State::Waiting) { s.state = State::Skipped; s.note = "Not opened".into(); }
    }

    fn start_game(&mut self, i: usize, now: Instant, games: &mut dyn Games, args_for: &dyn Fn(&str) -> Result<Start, String>) {
        let running = games.running();
        if running.len() >= MAX_GAMES { return self.fail(i, format!("{} games are open already", running.len())); }
        if !running.is_empty() {
            if !self.allow_multi { return self.fail(i, "D2R is already open. Turn on \"Open more than one game\" in Settings.".into()); }
            let mut closed = 0;
            for &pid in &running {
                match games.close_lock(pid) { Ok(n) => closed += n, Err(e) => return self.fail(i, e) }
            }
            crate::report::log(format!("game {}: closed {} lock(s) in {} running game(s)", i + 1, closed, running.len()));
        }
        let mut token = false;
        let started = args_for(&self.steps[i].account).and_then(|st| { token = st.token.is_some(); games.start(st) });
        if let Err(e) = started { return self.fail(i, e); }
        self.steps[i].token = token;
        let s = &mut self.steps[i];
        s.state = State::Starting;
        s.note = if s.retried { "Trying again…".into() } else { "Starting…".into() };
        s.since = now;
        s.before = running;
        crate::report::log(format!("game {} started{}", i + 1, if s.retried { " (second try)" } else { "" }));
    }

    /// Moves the queue along; called a few times a second.
    pub fn tick(&mut self, now: Instant, games: &mut dyn Games, args_for: &dyn Fn(&str) -> Result<Start, String>) {
        let Some(i) = self.steps.iter().position(|s| s.state.active()) else { return };
        match self.steps[i].state {
            State::Waiting => self.start_game(i, now, games, args_for),
            State::Starting => {
                let running = games.running();
                let new: Vec<u32> = running.iter().copied().filter(|p| !self.steps[i].before.contains(p)).collect();
                if let Some(&pid) = new.iter().find(|&&p| games.has_window(p)) {
                    let s = &mut self.steps[i];
                    s.state = State::Settling;
                    s.pid = Some(pid);
                    s.until = now + SETTLE;
                    s.note = if s.token { "Window open, waiting for character select…".into() } else { "Window open, signing in…".into() };
                    crate::report::log(format!("game {}: window after {:.1} s", i + 1, now.duration_since(s.since).as_secs_f32()));
                } else if new.is_empty() {
                    // the game we started is gone already: usually the lock of a game that was still loading took it
                    // down - once more, after closing the locks again
                    if self.steps[i].retried { return self.fail(i, "D2R closed right after starting".into()); }
                    self.steps[i].retried = true;
                    crate::report::log(format!("game {}: gone right after starting, trying once more", i + 1));
                    self.start_game(i, now, games, args_for);
                } else if now.duration_since(self.steps[i].since) > START_TIMEOUT {
                    self.fail(i, "D2R did not open its window".into());
                }
            }
            State::Settling => {
                let pid = self.steps[i].pid.unwrap_or(0);
                if !games.running().contains(&pid) { return self.fail(i, "The game closed while it was loading".into()); }
                let s = &self.steps[i];
                let taken = !s.token || games.token_taken();
                let gave_up = s.token && now >= s.until + TOKEN_WAIT;
                if s.token && now >= s.until && (taken || gave_up) {
                    crate::report::log(format!("game {}: {}", i + 1, if taken { "took its sign-in (character select)" } else { "did not take its sign-in in time, going on" }));
                }
                if now >= self.steps[i].until && (taken || gave_up) {
                    self.steps[i].state = State::Done;
                    self.steps[i].note = "Open".into();
                    crate::report::log(format!("game {}: open", i + 1));
                }
            }
            _ => {}
        }
    }

    /// The pids this launcher opened, in order (for the window).
    pub fn opened(&self) -> Vec<(String, u32)> { self.steps.iter().filter_map(|s| s.pid.map(|p| (s.account.clone(), p))).collect() }

    pub fn view(&self) -> Value {
        let steps: Vec<Value> = self.steps.iter().map(|s| json!({"account": s.account, "name": s.name, "state": s.state.word(), "note": s.note})).collect();
        json!({"busy": self.busy(), "label": self.label, "group": self.group, "steps": steps, "error": self.error})
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Pretend D2R: a started game gets a pid at once and its window `window_after` ticks later. A game started while
    /// another still holds its lock is gone before anyone looks, like the real one.
    #[derive(Default)]
    pub struct FakeGames {
        pub procs: Vec<(u32, u32, bool)>,   // pid, ticks until its window, lock held
        pub started: Vec<String>,           // emails, in start order
        pub closed_locks: Vec<u32>,
        pub window_after: u32,
        pub lock_late: bool,                // the first game takes its lock only after a second game was started once
        pub next: u32,
        pub token_after: u32,               // ticks until a started game takes its sign-in token
        pub tokens: Vec<String>,            // tokens handed over, in order
        pub token_left: Option<u32>,
    }
    impl FakeGames {
        pub fn advance(&mut self) {
            for p in self.procs.iter_mut() { p.1 = p.1.saturating_sub(1); }
            if let Some(t) = self.token_left.as_mut() { *t = t.saturating_sub(1); }
        }
    }
    impl Games for FakeGames {
        fn running(&mut self) -> Vec<u32> { self.procs.iter().map(|p| p.0).collect() }
        fn has_window(&mut self, pid: u32) -> bool { self.procs.iter().any(|p| p.0 == pid && p.1 == 0) }
        fn close_lock(&mut self, pid: u32) -> Result<usize, String> {
            self.closed_locks.push(pid);
            if self.lock_late { self.lock_late = false; return Ok(0); }   // not taken yet: nothing to close this time
            let p = self.procs.iter_mut().find(|p| p.0 == pid).ok_or("no such game")?;
            let had = p.2 as usize; p.2 = false; Ok(had)
        }
        fn token_taken(&mut self) -> bool { self.token_left.map_or(true, |t| t == 0) }
        fn start(&mut self, st: Start) -> Result<(), String> {
            let args = st.args;
            self.started.push(if let Some((t, _)) = st.token { self.tokens.push(t.clone()); self.token_left = Some(self.token_after); t } else { args[1].clone() });
            assert_eq!(args.last().unwrap(), "-skiplogovideo");
            if self.procs.iter().any(|p| p.2) { self.next += 1; return Ok(()); }   // refused: the process comes and goes before we look
            self.next += 1;
            self.procs.push((1000 + self.next, self.window_after, true));
            Ok(())
        }
    }

    fn args(id: &str) -> Result<Start, String> {
        if let Some(t) = id.strip_prefix("tok:") { return Ok(Start { args: vec!["-uid".into(), "osi".into(), "-skiplogovideo".into()], token: Some((t.to_string(), "NA".into())) }); }
        Ok(Start { args: crate::store::launch_args(&format!("{}@example.com", id), "pw", "Americas")?, token: None })
    }
    fn run(l: &mut Launcher, g: &mut FakeGames, t: &mut Instant, ticks: usize) {
        for _ in 0..ticks { l.tick(*t, g, &args); g.advance(); *t += Duration::from_millis(500); }
    }
    fn accts(ids: &[&str]) -> Vec<(String, String)> { ids.iter().map(|i| (i.to_string(), i.to_uppercase())).collect() }

    #[test]
    fn a_group_opens_in_order_each_after_the_last_is_up() {
        let mut t = Instant::now();
        let mut g = FakeGames { window_after: 4, ..Default::default() };
        let mut l = Launcher::default();
        l.begin(t, Some("Farm team".into()), Some("g1".into()), accts(&["barb", "sorc", "pally"]), true, 0).unwrap();
        run(&mut l, &mut g, &mut t, 3);
        assert_eq!(g.started, vec!["barb@example.com"]);            // the second waits for the first window
        run(&mut l, &mut g, &mut t, 6);
        assert_eq!(g.started.len(), 1, "and for the settle time after it");
        run(&mut l, &mut g, &mut t, 60);
        assert_eq!(g.started, vec!["barb@example.com", "sorc@example.com", "pally@example.com"]);
        assert!(!l.busy());
        assert_eq!(l.opened().iter().map(|o| o.0.as_str()).collect::<Vec<_>>(), vec!["barb", "sorc", "pally"]);
        // before each later start, the running games' locks were closed
        assert_eq!(g.closed_locks, vec![1001, 1001, 1002]);
        assert!(l.view()["steps"].as_array().unwrap().iter().all(|s| s["state"] == "done"));
    }

    #[test]
    fn several_games_need_the_switch() {
        let t = Instant::now();
        let mut l = Launcher::default();
        assert_eq!(l.begin(t, None, None, accts(&["a", "b"]), false, 0), Err("needs_multi:2".into()));
        assert_eq!(l.begin(t, None, None, accts(&["a"]), false, 1), Err("needs_multi:2".into()));   // Play while a game is open
        assert!(l.begin(t, None, None, accts(&["a"]), false, 0).is_ok());                          // one game: no switch needed
        assert!(l.begin(t, None, None, accts(&["b"]), true, 0).is_err());                          // still busy
        let mut l = Launcher::default();
        assert!(l.begin(t, None, None, accts(&["a", "b", "c"]), true, 2).unwrap_err().contains("5 games"));
    }

    #[test]
    fn a_game_refused_by_a_late_lock_is_tried_once_more() {
        let mut t = Instant::now();
        let mut g = FakeGames { window_after: 2, lock_late: true, ..Default::default() };
        let mut l = Launcher::default();
        l.begin(t, None, None, accts(&["a", "b"]), true, 0).unwrap();
        run(&mut l, &mut g, &mut t, 40);
        assert_eq!(g.started, vec!["a@example.com", "b@example.com", "b@example.com"]);
        assert_eq!(g.procs.len(), 2);
        assert!(l.view()["error"].is_null());
    }

    #[test]
    fn a_failure_stops_the_rest_and_says_why() {
        let mut t = Instant::now();
        let mut g = FakeGames { window_after: 1_000_000, ..Default::default() };   // never shows a window
        let mut l = Launcher::default();
        l.begin(t, Some("G".into()), None, accts(&["a", "b"]), true, 0).unwrap();
        run(&mut l, &mut g, &mut t, (START_TIMEOUT.as_millis() / 500) as usize + 4);
        let v = l.view();
        assert_eq!(v["steps"][0]["state"], "failed");
        assert_eq!(v["steps"][1]["state"], "skipped");
        assert_eq!(v["error"], "A: D2R did not open its window");
        assert_eq!(g.started.len(), 1);
        // a bad saved password fails before anything starts
        let mut l = Launcher::default();
        let mut g = FakeGames::default();
        l.begin(t, None, None, accts(&["a"]), false, 0).unwrap();
        l.tick(t, &mut g, &|_| Err("The saved password is damaged".into()));
        assert!(g.started.is_empty() && !l.busy());
    }

    #[test]
    fn a_token_account_holds_the_next_game_until_it_took_its_sign_in() {
        let mut t = Instant::now();
        let mut g = FakeGames { window_after: 2, token_after: 40, ..Default::default() };   // character select 20 s after start
        let mut l = Launcher::default();
        l.begin(t, Some("G".into()), Some("g".into()), accts(&["tok:US-aaaa1111-1", "b"]), true, 0).unwrap();
        run(&mut l, &mut g, &mut t, 20);   // 10 s: window up, settle time over, token not taken yet
        assert_eq!(g.started, vec!["US-aaaa1111-1"]);
        assert_eq!(l.view()["steps"][0]["note"], "Window open, waiting for character select…");
        run(&mut l, &mut g, &mut t, 40);
        assert_eq!(g.started, vec!["US-aaaa1111-1", "b@example.com"]);
        // a token never taken: the next game goes after TOKEN_WAIT anyway
        let mut t = Instant::now();
        let mut g = FakeGames { window_after: 1, token_after: 1_000_000, ..Default::default() };
        let mut l = Launcher::default();
        l.begin(t, None, None, accts(&["tok:US-bbbb2222-2", "c"]), true, 0).unwrap();
        run(&mut l, &mut g, &mut t, ((SETTLE + TOKEN_WAIT).as_millis() / 500) as usize + 10);
        assert_eq!(g.started.len(), 2);
    }

    #[test]
    fn stop_skips_what_has_not_started() {
        let mut t = Instant::now();
        let mut g = FakeGames { window_after: 3, ..Default::default() };
        let mut l = Launcher::default();
        l.begin(t, None, None, accts(&["a", "b", "c"]), true, 0).unwrap();
        run(&mut l, &mut g, &mut t, 1);
        l.stop();
        run(&mut l, &mut g, &mut t, 40);
        assert_eq!(g.started, vec!["a@example.com"]);
        assert!(!l.busy());
    }
}
