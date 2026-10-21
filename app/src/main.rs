// The launcher window (Tauri). The page asks the core through one command, `call`. Two background threads: one ticks
// the core a few times a second (the launch queue and the screen quarters), one does the slow network work (terror
// zones, update checks, sending a report) and hands the results to the core. "Restart to update" is handled here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use launcher_core::{data_dir, report, tile::WinScreen, update, zones, Engine, WinGames, VERSION};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct App(Arc<Mutex<Engine>>);

const UPDATE_EVERY: Duration = Duration::from_secs(6 * 3600);

fn unix_now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) }

#[tauri::command]
fn call(app: tauri::AppHandle, state: tauri::State<'_, App>, op: String, p: Value) -> Result<Value, String> {
    let mut engine = state.0.lock().map_err(|_| "The launcher is busy, try again".to_string())?;
    if op == "update_apply" {
        let path = engine.update_ready().ok_or("No update is ready")?;
        report::log(format!("restarting into the update ({})", path.display()));
        update::apply(&path)?;
        app.exit(0);
        return Ok(Value::Bool(true));
    }
    let mut games = WinGames { folder: engine.game_folder() };
    engine.call(Instant::now(), &op, &p, &mut games)
}

fn network(engine: Arc<Mutex<Engine>>) {
    let mut last_check: Option<Instant> = None;
    std::thread::sleep(Duration::from_secs(2));
    loop {
        let (zones_due, send, check) = match engine.lock() {
            Ok(mut e) => (e.net_zones_due(unix_now()), e.net_take_send(), e.net_check_due(UPDATE_EVERY, last_check)),
            Err(_) => return,
        };
        if zones_due {
            let z = zones::fetch();
            if let Ok(mut e) = engine.lock() { e.net_set_zones(z); }
        }
        if let Some((id, text, reason)) = send {
            let install = report::install_id(&data_dir());
            let r = report::send(&text, &reason, VERSION, &install);
            report::log(format!("report send: {}", if r.is_ok() { "ok" } else { "failed" }));
            if let Ok(mut e) = engine.lock() { let _ = e.net_sent(&id, r); }
        }
        if check {
            last_check = Some(Instant::now());
            let r = update::check(VERSION, &data_dir());
            if let Ok(mut e) = engine.lock() { e.net_set_update(r, unix_now()); }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn main() {
    let dir = data_dir();
    report::install_panic_hook(dir.clone(), VERSION);
    update::cleanup(VERSION, &dir);
    report::log(format!("Gumby's Launcher {} started", VERSION));
    let engine = Arc::new(Mutex::new(Engine::new(&dir)));
    let ticker = engine.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(400));
        if let Ok(mut e) = ticker.lock() {
            let mut games = WinGames { folder: e.game_folder() };
            e.tick(Instant::now(), &mut games, &mut WinScreen);
        }
    });
    let net = engine.clone();
    std::thread::spawn(move || network(net));
    let app = tauri::Builder::default()
        .manage(App(engine))
        .invoke_handler(tauri::generate_handler![call])
        .run(tauri::generate_context!());
    if app.is_err() { std::process::exit(77); }
}
