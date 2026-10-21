//! Problem reports. The launcher keeps a short log of what it did (never a name, email or password: games are "game 1",
//! "game 2"); when it crashes or a launch stops, it saves a report in %APPDATA%\GumbyLauncher\reports and the window asks
//! whether to send it. Before it is shown or sent, emails, account and group names, passwords and the Windows user name
//! are replaced (`scrub`). Nothing is sent without the Send click.
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const LOG_LINES: usize = 200;
const KEEP: usize = 20;
static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Adds a line to the log (with the time).
pub fn log(line: impl AsRef<str>) {
    if let Ok(mut l) = LOG.lock() {
        l.push_back(format!("{}  {}", crate::win::local_hms(), line.as_ref()));
        while l.len() > LOG_LINES { l.pop_front(); }
    }
}

pub fn log_tail() -> Vec<String> { LOG.lock().map(|l| l.iter().cloned().collect()).unwrap_or_default() }

/// Replaces every secret (longest first) with its placeholder. Secrets shorter than 3 characters are left alone (a
/// 1-letter account name would wreck the text).
pub fn scrub(text: &str, secrets: &[(String, String)]) -> String {
    let mut list: Vec<&(String, String)> = secrets.iter().filter(|(s, _)| s.chars().count() >= 3).collect();
    list.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
    let mut out = text.to_string();
    for (secret, placeholder) in list {
        out = replace_ci(&out, secret, placeholder);
    }
    out
}

fn replace_ci(text: &str, needle: &str, with: &str) -> String {
    let (lt, ln) = (text.to_lowercase(), needle.to_lowercase());
    if lt.len() != text.len() || ln.is_empty() { return text.replace(needle, with); }   // lowercase changed byte lengths
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(p) = lt[i..].find(&ln) {
        out.push_str(&text[i..i + p]);
        out.push_str(with);
        i += p + ln.len();
    }
    out.push_str(&text[i..]);
    out
}

pub struct Reports { dir: PathBuf }

pub struct Pending { pub id: String, pub reason: String, pub text: String }

impl Reports {
    pub fn new(data_dir: &Path) -> Reports { Reports { dir: data_dir.join("reports") } }

    /// Saves a report: what happened, the launcher and Windows version, the log. Returns its id.
    pub fn write(&self, kind: &str, reason: &str, version: &str) -> Option<String> {
        let mut text = format!("Gumby's Launcher {} · Windows {}\nReason: {}\n\n", version, crate::win::windows_version(), reason);
        for line in log_tail() { text.push_str(&line); text.push('\n'); }
        write_file(&self.dir, kind, reason, &text)
    }

    /// The newest report not yet sent or dismissed.
    pub fn pending(&self) -> Option<Pending> {
        let mut ids: Vec<String> = std::fs::read_dir(&self.dir).ok()?.filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter_map(|n| n.strip_suffix(".txt").map(|s| s.to_string()))
            .filter(|id| !self.dir.join(format!("{}.done", id)).exists()).collect();
        ids.sort();
        let id = ids.pop()?;
        let raw = std::fs::read_to_string(self.dir.join(format!("{}.txt", id))).ok()?;
        let reason = raw.lines().find_map(|l| l.strip_prefix("Reason: ")).unwrap_or("").to_string();
        Some(Pending { id, reason, text: raw })
    }

    /// Marks a report sent or dismissed (it is not offered again); keeps only the newest reports.
    pub fn done(&self, id: &str) {
        if id.contains(['/', '\\', '.']) { return; }
        let _ = std::fs::write(self.dir.join(format!("{}.done", id)), b"");
        let mut txt: Vec<PathBuf> = std::fs::read_dir(&self.dir).map(|d| d.filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |x| x == "txt")).collect()).unwrap_or_default();
        txt.sort();
        while txt.len() > KEEP {
            let old = txt.remove(0);
            let _ = std::fs::remove_file(old.with_extension("done"));
            let _ = std::fs::remove_file(old);
        }
    }
}

fn write_file(dir: &Path, kind: &str, _reason: &str, text: &str) -> Option<String> {
    std::fs::create_dir_all(dir).ok()?;
    let stamp = crate::win::local_stamp();
    let id = (1..100).map(|n| format!("{}-{}-{}", stamp, kind, n)).find(|id| !dir.join(format!("{}.txt", id)).exists())?;
    std::fs::write(dir.join(format!("{}.txt", id)), text).ok()?;
    Some(id)
}

/// Saves a report about a crash (called from the panic hook: no locks, no window).
pub fn install_panic_hook(data_dir: PathBuf, version: &'static str) {
    std::panic::set_hook(Box::new(move |info| {
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let what = info.payload().downcast_ref::<&str>().map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown".into());
        log(format!("CRASH at {}: {}", place, what));
        let reason = format!("The launcher crashed ({})", place);
        Reports::new(&data_dir).write("crash", &reason, version);
    }));
}

/// A random id for this install, so the owner can tell reports from different friends apart (no name, no PC data).
pub fn install_id(data_dir: &Path) -> String {
    let path = data_dir.join("install-id.txt");
    if let Ok(s) = std::fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit()) { return s; }
    }
    let id: String = crate::win::random_bytes(4).iter().map(|b| format!("{:02x}", b)).collect();
    let _ = std::fs::create_dir_all(data_dir);
    let _ = std::fs::write(&path, &id);
    id
}

/// Sends a (scrubbed) report to the owner's server (blocking, 10 s at most).
pub fn send(text: &str, reason: &str, version: &str, install: &str) -> Result<(), String> {
    let body = serde_json::json!({"version": version, "reason": reason, "install": install, "text": text}).to_string();
    if body.len() > 60 * 1024 { return Err("The report is too big to send".into()); }
    let r = crate::http::request("POST", &format!("{}/launcher/report", crate::zones::SERVER),
        &[("User-Agent", "GumbyLauncher"), ("Content-Type", "application/json")], Some(body.as_bytes()), 10_000, 64 * 1024, None)
        .map_err(|_| "Could not reach the report server. Try again later.".to_string())?;
    match r.status {
        200 => Ok(()),
        429 => Err("Too many reports this hour. Try again later.".into()),
        s => Err(format!("The report server refused it ({})", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_takes_out_every_secret() {
        let secrets = vec![("me@example.com".to_string(), "[email]".to_string()), ("MuleSorc".to_string(), "[account 2]".to_string()),
            ("S3cret!".to_string(), "[password]".to_string()), ("Bo".to_string(), "[account 3]".to_string()),
            ("Player1".to_string(), "[user]".to_string())];
        let text = "game 2 muleSORC (me@example.com) pw S3cret! at C:\\Users\\Player1\\x; Bo stays";
        let out = scrub(text, &secrets);
        assert_eq!(out, "game 2 [account 2] ([email]) pw [password] at C:\\Users\\[user]\\x; Bo stays");
        // a longer secret wins over a shorter one inside it
        let out = scrub("MuleSorc2", &[("MuleSorc".into(), "[a]".into()), ("MuleSorc2".into(), "[b]".into())]);
        assert_eq!(out, "[b]");
    }

    #[test]
    fn reports_are_saved_offered_once_and_trimmed() {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-reports-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let r = Reports::new(&dir);
        assert!(r.pending().is_none());
        log("Launch all: 3 games");
        log("game 2: D2R closed right after starting");
        let id = r.write("launch", "Launch stopped: game 2 - D2R closed right after starting", "0.1.0").unwrap();
        let p = r.pending().unwrap();
        assert_eq!(p.id, id);
        assert_eq!(p.reason, "Launch stopped: game 2 - D2R closed right after starting");
        assert!(p.text.contains("Gumby's Launcher 0.1.0") && p.text.contains("game 2: D2R closed right after starting"));
        r.done(&id);
        assert!(r.pending().is_none());
        for n in 0..(KEEP + 5) { let id = r.write("manual", &format!("n{}", n), "0.1.0").unwrap(); r.done(&id); }
        let left = std::fs::read_dir(dir.join("reports")).unwrap().filter(|e| e.as_ref().unwrap().path().extension().unwrap() == "txt").count();
        assert_eq!(left, KEEP);
        let a = install_id(&dir);
        assert_eq!((a.len(), install_id(&dir)), (8, a.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
