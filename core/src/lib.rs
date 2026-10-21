//! The launcher's core: saved accounts and groups, opening games one after another, screen quarters, terror zones,
//! problem reports and updates. The window (app/) calls `Engine::call` and `Engine::tick`; its network thread does the
//! slow fetches and hands the results in (`net_*`).
pub mod b64;
pub mod ed25519;
pub mod http;
pub mod launch;
pub mod report;
pub mod sha256;
pub mod store;
pub mod tile;
pub mod update;
pub mod win;
pub mod zones;

use launch::{Games, Launcher};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use store::{AccountInput, GroupInput, Store};
use tile::{Screen, Tiler};

/// The app's name, as people see it.
pub const APP_NAME: &str = "Gumby's Launcher";
/// Its folder name: in %APPDATA% (no apostrophe in any path).
pub const DIR_NAME: &str = "GumbyLauncher";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(DIR_NAME)
}

/// The real games: D2R.exe processes, started from the game folder. `token_written` is the sign-in value put in the
/// registry for the game started last (the game replaces it at character select).
pub struct WinGames { pub folder: Option<PathBuf>, pub token_written: Option<Vec<u8>> }
impl Games for WinGames {
    fn running(&mut self) -> Vec<u32> { win::pids_named("D2R.exe") }
    fn has_window(&mut self, pid: u32) -> bool { !win::main_windows(&[pid]).is_empty() }
    fn close_lock(&mut self, pid: u32) -> Result<usize, String> { win::close_named_handles(pid, win::D2R_INSTANCE_LOCK) }
    fn token_taken(&mut self) -> bool {
        match &self.token_written { None => true, Some(w) => win::osi_token_value().map_or(true, |now| &now != w) }
    }
    fn start(&mut self, st: launch::Start) -> Result<(), String> {
        let folder = self.folder.clone().ok_or("Could not find Diablo II: Resurrected. Set the game folder in Settings.")?;
        self.token_written = match &st.token { Some((token, region)) => Some(win::set_osi_token(token, region)?), None => None };
        let args = st.args;
        let r = std::process::Command::new(folder.join("D2R.exe")).args(&args).current_dir(&folder).spawn();
        drop(args);
        r.map(|_child| ()).map_err(|e| format!("D2R did not start ({})", e.kind()))
    }
}

pub struct Engine {
    pub store: Store,
    launcher: Launcher,
    tiler: Tiler,
    order: Vec<u32>,                  // running games, oldest first
    by_account: HashMap<String, u32>, // which game each account is in, for the games this launcher opened
    found: Option<PathBuf>,           // the game folder found on this PC
    looked: Option<Instant>,
    reports: report::Reports,
    seen_failures: u32,
    sending: Option<String>,          // the report the network thread is sending
    send_now: Option<String>,         // asked to send, not picked up yet
    zones: Value,
    zones_at: Option<Instant>,
    update: Value,                    // {state: idle|checking|current|ready|error, ...}
    update_path: Option<PathBuf>,
    check_now: bool,
    signin: &'static str,             // Battle.net sign-in window: idle | waiting | done
    pending_token: Option<String>,    // the token it caught, until the account is saved
    pub token_written: Option<Vec<u8>>, // kept across ticks for WinGames (see there)
}

fn s(p: &Value, k: &str) -> String { p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }

impl Engine {
    pub fn new(dir: &std::path::Path) -> Engine {
        Engine { store: Store::load(dir), launcher: Launcher::default(), tiler: Tiler::default(), order: Vec::new(), by_account: HashMap::new(),
            found: None, looked: None, reports: report::Reports::new(dir), seen_failures: 0, sending: None, send_now: None,
            zones: Value::Null, zones_at: None, update: json!({"state": "idle"}), update_path: None, check_now: true,
            signin: "idle", pending_token: None, token_written: None }
    }

    /// The game folder, found on this PC by itself (a running game, Battle.net's and Steam's records, the usual folders
    /// on every drive). A folder set in Settings wins while it still holds D2R.exe. Looked up again if it goes away.
    pub fn game_folder(&mut self) -> Option<PathBuf> {
        if let Some(f) = self.store.game_folder_setting().map(PathBuf::from).filter(|f| f.join("D2R.exe").is_file()) { return Some(f); }
        if self.found.as_ref().map_or(false, |f| f.join("D2R.exe").is_file()) { return self.found.clone(); }
        if self.looked.map_or(true, |t| t.elapsed() > std::time::Duration::from_secs(5)) {
            self.looked = Some(Instant::now());
            self.found = win::find_game_folder();
        }
        self.found.clone()
    }

    fn running(&mut self, games: &mut dyn Games) -> Vec<u32> {
        let now = games.running();
        self.order.retain(|p| now.contains(p));
        for p in now { if !self.order.contains(&p) { self.order.push(p); } }
        let order = self.order.clone();
        self.by_account.retain(|_, p| order.contains(p));
        order
    }

    /// A few times a second: moves the launch queue along and keeps two or more games in their quarters.
    pub fn tick(&mut self, now: Instant, games: &mut dyn Games, screen: &mut dyn Screen) {
        let store = &self.store;
        self.launcher.tick(now, games, &|id| store.args_for(id));
        for (account, pid) in self.launcher.opened() { self.by_account.insert(account, pid); }
        let running = self.running(games);
        self.tiler.tick(now, &running, screen);
        if self.launcher.failures > self.seen_failures {   // a launch stopped: keep a report and offer to send it
            self.seen_failures = self.launcher.failures;
            self.reports.write("launch", &format!("Launch stopped: {}", self.launcher.last_failure), VERSION);
        }
    }

    /// What a report must not carry, with what goes in its place: emails, account and group names, passwords, the
    /// Windows user name.
    pub fn secrets(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (i, a) in self.store.accounts().iter().enumerate() {
            out.push((a.email.clone(), "[email]".to_string()));
            out.push((a.name.clone(), format!("[account {}]", i + 1)));
            if let Ok(pw) = self.store.password(&a.id) { out.push((pw, if a.method == "token" { "[token]" } else { "[password]" }.to_string())); }
        }
        for (i, g) in self.store.groups().iter().enumerate() { out.push((g.name.clone(), format!("[group {}]", i + 1))); }
        if let Ok(user) = std::env::var("USERNAME") { out.push((user, "[user]".to_string())); }
        if let Some(t) = &self.pending_token { out.push((t.clone(), "[token]".to_string())); }
        out
    }

    // ---------------------------------------------------------------- the network thread's side (slow work, no lock held)
    /// The terror zones are due: none yet, a minute old, or the half hour just switched.
    pub fn net_zones_due(&self, unix_now: i64) -> bool {
        let switched = self.zones["switch_at"].as_i64().map_or(false, |t| unix_now >= t + 5);
        self.zones_at.map_or(true, |t| t.elapsed().as_secs() >= 60 || (switched && t.elapsed().as_secs() >= 15))
    }
    pub fn net_set_zones(&mut self, z: Result<Value, String>) {
        self.zones_at = Some(Instant::now());
        match z { Ok(v) => self.zones = v, Err(e) => { if self.zones.is_null() || self.zones.get("error").is_some() { self.zones = json!({"error": e}); } } }
    }
    /// The report to send now, scrubbed: (id, text, reason).
    pub fn net_take_send(&mut self) -> Option<(String, String, String)> {
        let id = self.send_now.take()?;
        let p = self.reports.pending().filter(|p| p.id == id)?;
        let secrets = self.secrets();
        self.sending = Some(id.clone());
        Some((id, report::scrub(&p.text, &secrets), report::scrub(&p.reason, &secrets)))
    }
    pub fn net_sent(&mut self, id: &str, r: Result<(), String>) -> Result<(), String> {
        self.sending = None;
        if r.is_ok() { self.reports.done(id); }
        r
    }
    pub fn net_check_due(&mut self, every: std::time::Duration, last: Option<Instant>) -> bool {
        let due = self.check_now || last.map_or(true, |t| t.elapsed() >= every);
        if due && self.update["state"] != "ready" { self.check_now = false; self.update = json!({"state": "checking"}); true } else { false }
    }
    pub fn net_set_update(&mut self, r: Result<Option<update::Ready>, String>, unix_now: i64) {
        self.update = match r {
            Ok(Some(ready)) => { self.update_path = Some(ready.path); json!({"state": "ready", "version": ready.version, "notes": ready.notes}) }
            Ok(None) => json!({"state": "current", "checked_at": unix_now}),
            Err(e) => json!({"state": "error", "error": e, "checked_at": unix_now}),
        };
    }
    /// The Battle.net sign-in window opened / caught a token / closed without one.
    pub fn signin_opened(&mut self) { self.signin = "waiting"; self.pending_token = None; }
    pub fn signin_token(&mut self, token: String) { self.pending_token = Some(token); self.signin = "done"; report::log("Battle.net sign-in: token received"); }
    pub fn signin_closed(&mut self) { if self.signin == "waiting" { self.signin = "idle"; } }

    /// The downloaded update, once one is ready.
    pub fn update_ready(&self) -> Option<PathBuf> { if self.update["state"] == "ready" { self.update_path.clone() } else { None } }

    fn begin(&mut self, now: Instant, p: &Value, games: &mut dyn Games, label: Option<String>, group: Option<String>, ids: Vec<String>) -> Result<Value, String> {
        if p.get("multi").and_then(|v| v.as_bool()) == Some(true) { self.store.set_allow_multi(true)?; }   // "Turn on and launch"
        let accounts: Vec<(String, String)> = ids.iter().map(|id| self.store.account(id).map(|a| (a.id.clone(), a.name.clone())).ok_or("That account is not saved".to_string())).collect::<Result<_, _>>()?;
        let running = self.running(games).len();
        self.launcher.begin(now, label, group, accounts, self.store.allow_multi(), running)?;
        Ok(Value::Bool(true))
    }

    /// The window's requests.
    pub fn call(&mut self, now: Instant, op: &str, p: &Value, games: &mut dyn Games) -> Result<Value, String> {
        match op {
            "state" => Ok(self.view(games)),
            "account_save" => {
                let mut input: AccountInput = serde_json::from_value(p.clone()).map_err(|_| "Fill in every field".to_string())?;
                if input.method == "token" { input.token = self.pending_token.clone(); }
                let r = self.store.save_account(input).map(Value::from);
                if r.is_ok() { self.pending_token = None; self.signin = "idle"; }
                r
            }
            "signin_status" => Ok(json!({"state": self.signin})),
            "signin_reset" => { self.signin = "idle"; self.pending_token = None; Ok(Value::Bool(true)) }
            "account_remove" => self.store.remove_account(&s(p, "id")).map(|_| Value::Bool(true)),
            "group_save" => {
                let input: GroupInput = serde_json::from_value(p.clone()).map_err(|_| "Give the group a name and pick accounts".to_string())?;
                self.store.save_group(input).map(Value::from)
            }
            "group_remove" => self.store.remove_group(&s(p, "id")).map(|_| Value::Bool(true)),
            "play" => { let id = s(p, "id"); self.begin(now, p, games, None, None, vec![id]) }
            "launch_group" => {
                let g = self.store.group(&s(p, "id")).ok_or("That group is not saved")?.clone();
                self.begin(now, p, games, Some(g.name), Some(g.id), g.accounts)
            }
            "stop" => { self.launcher.stop(); Ok(Value::Bool(true)) }
            "report_send" => { self.send_now = Some(s(p, "id")); Ok(Value::Bool(true)) }
            "report_dismiss" => { self.reports.done(&s(p, "id")); Ok(Value::Bool(true)) }
            "report_new" => self.reports.write("manual", "Sent from Settings", VERSION).map(Value::from).ok_or("Could not save the report".to_string()),
            "update_check" => { self.check_now = true; Ok(Value::Bool(true)) }
            "set_multi" => self.store.set_allow_multi(p.get("on").and_then(|v| v.as_bool()).unwrap_or(false)).map(|_| Value::Bool(true)),
            "set_folder" => {
                let f = s(p, "folder");
                self.store.set_game_folder(if f.trim().is_empty() { None } else { Some(f.trim().to_string()) }).map(|_| Value::Bool(true))
            }
            _ => Err(format!("Unknown request {}", op)),
        }
    }

    /// Everything the window shows (never a password).
    pub fn view(&mut self, games: &mut dyn Games) -> Value {
        let running = self.running(games);
        let slots: HashMap<u32, usize> = self.tiler.placed().into_iter().map(|(slot, pid)| (pid, slot)).collect();
        let open: Vec<Value> = self.by_account.iter().map(|(a, p)| json!({"account": a, "quarter": slots.get(p).copied()})).collect();
        let mut v = self.store.view();
        v["running"] = json!(running.len());
        v["open"] = Value::Array(open);
        v["launch"] = self.launcher.view();
        v["game_folder"] = json!(self.game_folder().map(|f| f.display().to_string()));
        v["app"] = json!(APP_NAME);
        v["version"] = json!(VERSION);
        v["zones"] = self.zones.clone();
        v["update"] = self.update.clone();
        v["report"] = match self.reports.pending() {
            Some(p) => { let secrets = self.secrets();
                json!({"id": p.id, "reason": report::scrub(&p.reason, &secrets), "text": report::scrub(&p.text, &secrets), "sending": self.sending.as_deref() == Some(&p.id)}) }
            None => Value::Null,
        };
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::tests::FakeGames;
    use crate::tile::{quarter, Rect};

    const MON: Rect = Rect { x: 0, y: 0, w: 2560, h: 1440 };
    #[derive(Default)]
    struct Shots { windows: Vec<(u32, isize)>, rects: HashMap<isize, Rect> }
    impl Screen for Shots {
        fn windows(&mut self, pids: &[u32]) -> Vec<(u32, isize)> { self.windows.iter().copied().filter(|w| pids.contains(&w.0)).collect() }
        fn monitor_of(&mut self, _: isize) -> Option<Rect> { Some(MON) }
        fn rect_of(&mut self, w: isize) -> Option<Rect> { self.rects.get(&w).copied() }
        fn move_window(&mut self, w: isize, to: Rect) -> bool { self.rects.insert(w, to); true }
    }

    #[test]
    fn launch_all_opens_the_group_in_order_into_quarters() {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Engine::new(&dir);
        let mut g = FakeGames { window_after: 3, ..Default::default() };
        let mut screen = Shots::default();
        let mut t = Instant::now();
        let ids: Vec<String> = ["barb", "sorc", "pally"].iter().map(|n| e.call(t, "account_save", &json!({"name": n, "email": format!("{}@example.com", n), "password": "pw", "region": "Americas"}), &mut g).unwrap().as_str().unwrap().to_string()).collect();
        let gid = e.call(t, "group_save", &json!({"name": "Farm team", "accounts": ids}), &mut g).unwrap().as_str().unwrap().to_string();
        // first Launch all: the window asks to turn on several games
        assert_eq!(e.call(t, "launch_group", &json!({"id": gid}), &mut g), Err("needs_multi:3".into()));
        e.call(t, "launch_group", &json!({"id": gid, "multi": true}), &mut g).unwrap();
        assert!(e.store.allow_multi());
        for _ in 0..80 {
            screen.windows = g.procs.iter().filter(|p| p.1 == 0).map(|p| (p.0, p.0 as isize)).collect();
            e.tick(t, &mut g, &mut screen);
            g.advance();
            t += std::time::Duration::from_millis(500);
        }
        assert_eq!(g.started, vec!["barb@example.com", "sorc@example.com", "pally@example.com"]);
        let pids: Vec<u32> = g.procs.iter().map(|p| p.0).collect();
        for (slot, pid) in pids.iter().enumerate() { assert_eq!(screen.rects[&(*pid as isize)], quarter(MON, slot)); }
        let v = e.view(&mut g);
        assert_eq!(v["running"], 3);
        assert_eq!(v["open"].as_array().unwrap().len(), 3);
        assert!(!v.to_string().contains("\"pw\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn play_opens_one_and_a_second_play_asks_first() {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-play-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Engine::new(&dir);
        let mut g = FakeGames { window_after: 1, ..Default::default() };
        let mut screen = Shots::default();
        let mut t = Instant::now();
        let a = e.call(t, "account_save", &json!({"name": "A", "email": "a@example.com", "password": "pw", "region": "Europe"}), &mut g).unwrap();
        let b = e.call(t, "account_save", &json!({"name": "B", "email": "b@example.com", "password": "pw", "region": "Europe"}), &mut g).unwrap();
        e.call(t, "play", &json!({"id": a}), &mut g).unwrap();
        for _ in 0..30 { e.tick(t, &mut g, &mut screen); g.advance(); t += std::time::Duration::from_millis(500); }
        assert_eq!(g.started, vec!["a@example.com"]);
        assert_eq!(e.call(t, "play", &json!({"id": b}), &mut g), Err("needs_multi:2".into()));
        assert!(e.call(t, "set_folder", &json!({"folder": "C:\\nowhere"}), &mut g).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_caught_token_goes_into_the_saved_account_only() {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-signin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Engine::new(&dir);
        let mut g = FakeGames::default();
        let t = Instant::now();
        assert!(e.call(t, "account_save", &json!({"name": "Mule", "region": "Americas", "method": "token"}), &mut g).is_err());
        e.signin_opened();
        assert_eq!(e.call(t, "signin_status", &json!({}), &mut g).unwrap()["state"], "waiting");
        e.signin_token("US-0123456789abcdef-7".into());
        assert_eq!(e.call(t, "signin_status", &json!({}), &mut g).unwrap()["state"], "done");
        // a token sent by the page itself is ignored: only the caught one counts
        let id = e.call(t, "account_save", &json!({"name": "Mule", "region": "Americas", "method": "token", "token": "US-evil-1"}), &mut g).unwrap();
        assert_eq!(e.store.password(id.as_str().unwrap()).unwrap(), "US-0123456789abcdef-7");
        assert_eq!(e.call(t, "signin_status", &json!({}), &mut g).unwrap()["state"], "idle");
        assert!(!e.view(&mut g).to_string().contains("0123456789abcdef"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stopped_launch_offers_a_report_without_names() {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-report-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Engine::new(&dir);
        let mut g = FakeGames { window_after: 1_000_000, ..Default::default() };   // never opens a window
        let mut screen = Shots::default();
        let mut t = Instant::now();
        let id = e.call(t, "account_save", &json!({"name": "SecretBarb", "email": "barb@example.com", "password": "hunter22", "region": "Americas"}), &mut g).unwrap();
        e.call(t, "play", &json!({"id": id}), &mut g).unwrap();
        assert!(e.view(&mut g)["report"].is_null());
        for _ in 0..200 { e.tick(t, &mut g, &mut screen); g.advance(); t += std::time::Duration::from_millis(500); }
        let v = e.view(&mut g);
        let r = &v["report"];
        assert_eq!(r["reason"], "Launch stopped: game 1: D2R did not open its window");
        let text = r["text"].as_str().unwrap();
        assert!(text.contains("game 1 FAILED"));
        assert!(!text.contains("SecretBarb") && !text.contains("barb@example.com") && !text.contains("hunter22"));
        e.call(t, "report_dismiss", &json!({"id": r["id"]}), &mut g).unwrap();
        assert!(e.view(&mut g)["report"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
