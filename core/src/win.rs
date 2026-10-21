//! The Windows calls the launcher makes: finding D2R processes and their windows, moving windows into screen
//! quarters, locking passwords with DPAPI, finding the game folder, and closing D2R's one-copy lock so a second game
//! can start. Nothing here reads or writes game memory.
#![allow(non_snake_case, clippy::upper_case_acronyms)]
use std::ffi::c_void;
use std::path::PathBuf;

use crate::tile::Rect;

type H = *mut c_void;

#[repr(C)]
struct PROCESSENTRY32W { size: u32, usage: u32, pid: u32, heap: usize, module: u32, threads: u32, parent: u32, pri: i32, flags: u32, exe: [u16; 260] }
#[repr(C)]
#[derive(Default)]
struct SYSTEMTIME { year: u16, month: u16, wday: u16, day: u16, hour: u16, minute: u16, second: u16, ms: u16 }
#[repr(C)]
struct DataBlob { len: u32, data: *mut u8 }
#[repr(C)]
struct HandleEntry { object: usize, pid: usize, handle: usize, access: u32, backtrace: u16, type_index: u16, attrs: u32, reserved: u32 }
#[repr(C)]
struct UnicodeString { len: u16, max: u16, buf: *const u16 }
#[repr(C)]
#[derive(Default)]
struct RECT { left: i32, top: i32, right: i32, bottom: i32 }
#[repr(C)]
#[derive(Default)]
struct MONITORINFO { size: u32, monitor: RECT, work: RECT, flags: u32 }

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> H;
    fn Process32FirstW(snap: H, e: *mut PROCESSENTRY32W) -> i32;
    fn Process32NextW(snap: H, e: *mut PROCESSENTRY32W) -> i32;
    fn CloseHandle(h: H) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> H;
    fn GetLastError() -> u32;
    fn LocalFree(mem: H) -> H;
    fn DuplicateHandle(src_proc: H, src: H, dst_proc: H, dst: *mut H, access: u32, inherit: i32, options: u32) -> i32;
    fn GetCurrentProcess() -> H;
    fn QueryFullProcessImageNameW(h: H, flags: u32, name: *mut u16, size: *mut u32) -> i32;
    fn GetLogicalDrives() -> u32;
    fn GetLocalTime(st: *mut SYSTEMTIME);
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn TerminateProcess(h: H, code: u32) -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn GetWindowThreadProcessId(hwnd: H, pid: *mut u32) -> u32;
    fn EnumWindows(cb: extern "system" fn(H, isize) -> i32, l: isize) -> i32;
    fn IsWindowVisible(hwnd: H) -> i32;
    fn GetWindow(hwnd: H, cmd: u32) -> H;
    fn IsIconic(hwnd: H) -> i32;
    fn ShowWindow(hwnd: H, cmd: i32) -> i32;
    fn SetWindowPos(hwnd: H, after: H, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn MonitorFromWindow(hwnd: H, flags: u32) -> H;
    fn GetMonitorInfoW(m: H, info: *mut MONITORINFO) -> i32;
    fn GetWindowRect(hwnd: H, r: *mut RECT) -> i32;
    fn PostMessageW(hwnd: H, msg: u32, w: usize, l: isize) -> i32;
    fn GetWindowTextW(hwnd: H, buf: *mut u16, max: i32) -> i32;
    fn SetWindowTextW(hwnd: H, text: *const u16) -> i32;
}
#[link(name = "winmm")]
extern "system" { fn PlaySoundW(sound: *const u16, module: H, flags: u32) -> i32; }
#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(input: *const DataBlob, desc: *const u16, entropy: *const DataBlob, reserved: *mut c_void, prompt: *mut c_void, flags: u32, out: *mut DataBlob) -> i32;
    fn CryptUnprotectData(input: *const DataBlob, desc: H, entropy: *const DataBlob, reserved: *mut c_void, prompt: *mut c_void, flags: u32, out: *mut DataBlob) -> i32;
}
#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(hkey: isize, sub: *const u16, value: *const u16, flags: u32, ty: *mut u32, data: *mut u8, len: *mut u32) -> i32;
    fn RegCreateKeyExW(key: isize, sub: *const u16, reserved: u32, class: *const u16, options: u32, sam: u32, sa: *const c_void, out: *mut isize, disposition: *mut u32) -> i32;
    fn RegSetValueExW(key: isize, name: *const u16, reserved: u32, ty: u32, data: *const u8, len: u32) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}
#[link(name = "bcrypt")]
extern "system" { fn BCryptGenRandom(alg: H, buf: *mut u8, len: u32, flags: u32) -> i32; }
#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(class: u32, buf: *mut c_void, len: u32, ret: *mut u32) -> i32;
    fn NtQueryObject(h: H, class: u32, buf: *mut c_void, len: u32, ret: *mut u32) -> i32;
}

pub fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

fn last_error() -> u32 { unsafe { GetLastError() } }

// ------------------------------------------------------------------------------------------------ processes, windows
/// Process ids of every running process with this exe name (case-insensitive).
pub fn pids_named(name: &str) -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(2, 0);   // TH32CS_SNAPPROCESS
        if snap.is_null() || snap as isize == -1 { return out; }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.size = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let n = e.exe.iter().position(|c| *c == 0).unwrap_or(260);
            if String::from_utf16_lossy(&e.exe[..n]).eq_ignore_ascii_case(name) { out.push(e.pid); }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    out
}

extern "system" fn collect_window(hwnd: H, l: isize) -> i32 {
    unsafe {
        let out = &mut *(l as *mut (Vec<u32>, Vec<(u32, isize)>));
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        // visible top-level windows that no other window owns (GW_OWNER = 4): the game window, not a dialog
        if out.0.contains(&pid) && IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, 4).is_null() { out.1.push((pid, hwnd as isize)); }
    }
    1
}

/// The main window of each of `pids` that has one, as (pid, window handle).
pub fn main_windows(pids: &[u32]) -> Vec<(u32, isize)> {
    let mut found: (Vec<u32>, Vec<(u32, isize)>) = (pids.to_vec(), Vec::new());
    unsafe { EnumWindows(collect_window, &mut found as *mut _ as isize); }
    let mut out: Vec<(u32, isize)> = Vec::new();
    for w in found.1 { if !out.iter().any(|o| o.0 == w.0) { out.push(w); } }
    out
}

/// The whole monitor (taskbar included) that a window is on.
pub fn monitor_of(hwnd: isize) -> Option<Rect> {
    unsafe {
        let mut mi = MONITORINFO { size: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(MonitorFromWindow(hwnd as H, 2), &mut mi) == 0 { return None; }   // MONITOR_DEFAULTTONEAREST
        Some(Rect { x: mi.monitor.left, y: mi.monitor.top, w: mi.monitor.right - mi.monitor.left, h: mi.monitor.bottom - mi.monitor.top })
    }
}

/// Where a window is now.
pub fn window_rect(hwnd: isize) -> Option<Rect> {
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd as H, &mut r) } == 0 { return None; }
    Some(Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top })
}

/// A window's title bar text.
pub fn window_title(hwnd: isize) -> String {
    let mut buf = vec![0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd as H, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}
/// Renames a window's title bar (the game itself is not touched).
pub fn set_window_title(hwnd: isize, title: &str) { unsafe { SetWindowTextW(hwnd as H, wide(title).as_ptr()); } }

/// Windows' own notification sound, without waiting for it.
pub fn notify_sound() { unsafe { PlaySoundW(wide("SystemNotification").as_ptr(), std::ptr::null_mut(), 0x0001 | 0x0002_0000 | 0x0010); } }   // ASYNC | SND_ALIAS | NODEFAULT

/// Closes a game the way its window's X button does (the game saves and exits); one still running 15 seconds later is
/// ended. Only a process that is still D2R.exe is touched.
pub fn close_game(pid: u32) {
    for (_, w) in main_windows(&[pid]) { unsafe { PostMessageW(w as H, 0x0010, 0, 0); } }   // WM_CLOSE
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(15));
        if pids_named("D2R.exe").contains(&pid) {
            unsafe { let h = OpenProcess(0x0001, 0, pid); if !h.is_null() { TerminateProcess(h, 0); CloseHandle(h); } }   // PROCESS_TERMINATE
        }
    });
}

/// Moves and sizes a window (restored first if minimized). False when Windows refused.
pub fn move_window(hwnd: isize, r: Rect) -> bool {
    unsafe {
        if IsIconic(hwnd as H) != 0 { ShowWindow(hwnd as H, 9); }   // SW_RESTORE
        SetWindowPos(hwnd as H, std::ptr::null_mut(), r.x, r.y, r.w, r.h, 0x0004 | 0x0010) != 0   // NOZORDER | NOACTIVATE
    }
}

// ------------------------------------------------------------------------------------------------ time, randomness, version
fn now_local() -> SYSTEMTIME { let mut st = SYSTEMTIME::default(); unsafe { GetLocalTime(&mut st); } st }
/// "21:14:03"
pub fn local_hms() -> String { let t = now_local(); format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second) }
/// "20261020-211403"
pub fn local_stamp() -> String { let t = now_local(); format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second) }

/// `n` random bytes from Windows (BCryptGenRandom, the system RNG).
pub fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    unsafe { BCryptGenRandom(std::ptr::null_mut(), b.as_mut_ptr(), n as u32, 2); }   // BCRYPT_USE_SYSTEM_PREFERRED_RNG
    b
}

/// "11 24H2 (26100)" style: the version people see in Settings, and the build.
pub fn windows_version() -> String {
    let key = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let build = reg_string(HKLM, key, "CurrentBuild").unwrap_or_default();
    let display = reg_string(HKLM, key, "DisplayVersion").unwrap_or_default();
    let major = if build.parse::<u32>().map_or(false, |b| b >= 22000) { "11" } else { "10" };
    format!("{} {} ({})", major, display, build).replace("  ", " ")
}

// ------------------------------------------------------------------------------------------------ passwords
const ENTROPY: &[u8] = b"d2r launcher accounts v1";

fn dpapi(data: &[u8], lock: bool) -> Option<Vec<u8>> { dpapi_with(data, lock, ENTROPY) }

fn dpapi_with(data: &[u8], lock: bool, entropy: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = DataBlob { len: data.len() as u32, data: data.as_ptr() as *mut u8 };
        let entropy = DataBlob { len: entropy.len() as u32, data: entropy.as_ptr() as *mut u8 };
        let mut out = DataBlob { len: 0, data: std::ptr::null_mut() };
        let ok = if lock {
            CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null_mut(), std::ptr::null_mut(), 1, &mut out)   // UI_FORBIDDEN
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null_mut(), std::ptr::null_mut(), 1, &mut out)
        };
        if ok == 0 || out.data.is_null() { return None; }
        let bytes = std::slice::from_raw_parts(out.data, out.len as usize).to_vec();
        std::ptr::write_bytes(out.data, 0, out.len as usize);   // wipe the copy Windows handed us
        LocalFree(out.data as H);
        Some(bytes)
    }
}
/// Locks `data` for this Windows user (DPAPI); None if Windows refuses.
pub fn protect(data: &[u8]) -> Option<Vec<u8>> { dpapi(data, true) }
/// Unlocks a blob from `protect`; None for another user's or PC's blob, or a damaged one.
pub fn unprotect(blob: &[u8]) -> Option<Vec<u8>> { dpapi(blob, false) }

// ------------------------------------------------------------------------------------------------ Battle.net token sign-in
// Accounts with the Battle.net Authenticator sign in with a token instead of a password (the method of
// github.com/shupershuff/D2r-Multiboxing-Methods): D2R started with `-uid osi` reads it from the current user's registry,
// DPAPI-locked with Blizzard's fixed entropy, and writes a fresh value back once the game reaches character select.
const OSI_KEY: &str = "SOFTWARE\\Blizzard Entertainment\\Battle.net\\Launch Options\\OSI";
const OSI_ENTROPY: [u8; 16] = [0xc8, 0x76, 0xf4, 0xae, 0x4c, 0x95, 0x2e, 0xfe, 0xf2, 0xfa, 0x0f, 0x54, 0x19, 0xc0, 0x9c, 0x43];

/// Puts a sign-in token where D2R looks for it; returns the stored value, to see later when the game has taken it.
pub fn set_osi_token(token: &str, region: &str) -> Result<Vec<u8>, String> {
    let blob = dpapi_with(token.as_bytes(), true, &OSI_ENTROPY).ok_or("Windows could not lock the sign-in")?;
    unsafe {
        let mut key: isize = 0;
        if RegCreateKeyExW(HKCU, wide(OSI_KEY).as_ptr(), 0, std::ptr::null(), 0, 0x20006, std::ptr::null(), &mut key, std::ptr::null_mut()) != 0 {   // KEY_WRITE
            return Err("Windows could not open the Battle.net settings".into());
        }
        let region: Vec<u16> = wide(region);
        let ok = RegSetValueExW(key, wide("REGION").as_ptr(), 0, 1, region.as_ptr() as *const u8, (region.len() * 2) as u32) == 0   // REG_SZ
            && RegSetValueExW(key, wide("WEB_TOKEN").as_ptr(), 0, 3, blob.as_ptr(), blob.len() as u32) == 0;                       // REG_BINARY
        RegCloseKey(key);
        if !ok { return Err("Windows could not save the sign-in for the game".into()); }
    }
    Ok(blob)
}

/// The stored token value as it is now (D2R replaces it when it reaches character select).
pub fn osi_token_value() -> Option<Vec<u8>> {
    unsafe {
        const RRF_RT_REG_BINARY: u32 = 0x8;
        let (sub, val) = (wide(OSI_KEY), wide("WEB_TOKEN"));
        let mut len: u32 = 0;
        if RegGetValueW(HKCU, sub.as_ptr(), val.as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), std::ptr::null_mut(), &mut len) != 0 { return None; }
        let mut buf = vec![0u8; len as usize];
        if RegGetValueW(HKCU, sub.as_ptr(), val.as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), buf.as_mut_ptr(), &mut len) != 0 { return None; }
        buf.truncate(len as usize);
        Some(buf)
    }
}

/// Reads the token D2R would see (for the tests: what `set_osi_token` stored unlocks to the token).
pub fn osi_token_text() -> Option<String> {
    osi_token_value().and_then(|b| dpapi_with(&b, false, &OSI_ENTROPY)).and_then(|t| String::from_utf8(t).ok())
}

/// The token in the address Battle.net's sign-in page sends the browser to: http://localhost:0/?ST=US-...&flowTrackingId=...
pub fn token_from_redirect(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    let st = query.split('&').find_map(|kv| kv.strip_prefix("ST="))?;
    let ok = st.len() >= 10 && st.len() <= 200 && st.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if ok { Some(st.to_string()) } else { None }
}

// ------------------------------------------------------------------------------------------------ game folder
const UNINSTALL: &str = "Microsoft\\Windows\\CurrentVersion\\Uninstall\\Diablo II Resurrected";
const HKLM: isize = 0x8000_0002u32 as i32 as isize;
const HKCU: isize = 0x8000_0001u32 as i32 as isize;

fn reg_string(root: isize, sub: &str, value: &str) -> Option<String> {
    unsafe {
        const RRF_RT_REG_SZ: u32 = 0x2;
        let (sub, val) = (wide(sub), wide(value));
        let mut len: u32 = 0;
        if RegGetValueW(root, sub.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut len) != 0 || len < 2 { return None; }
        let mut buf = vec![0u16; (len as usize + 1) / 2];
        if RegGetValueW(root, sub.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut u8, &mut len) != 0 { return None; }
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]);
        if s.trim().is_empty() { None } else { Some(s.trim().trim_matches('"').to_string()) }
    }
}

/// The folder of a running D2R.exe, if one is open.
fn running_game_folder() -> Option<PathBuf> {
    for pid in pids_named("D2R.exe") {
        unsafe {
            let h = OpenProcess(0x1000, 0, pid);   // PROCESS_QUERY_LIMITED_INFORMATION
            if h.is_null() { continue; }
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok != 0 {
                if let Some(dir) = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])).parent() { return Some(dir.to_path_buf()); }
            }
        }
    }
    None
}

/// Library folders in Steam's libraryfolders.vdf (`"path"  "D:\\SteamLibrary"` lines).
pub fn steam_libraries(vdf: &str) -> Vec<PathBuf> {
    vdf.lines().filter_map(|l| {
        let parts: Vec<&str> = l.split('"').collect();
        if parts.len() >= 4 && parts[1].eq_ignore_ascii_case("path") { Some(PathBuf::from(parts[3].replace("\\\\", "\\"))) } else { None }
    }).collect()
}

/// Every fixed drive on this PC (C:\, D:\, ...).
fn fixed_drives() -> Vec<PathBuf> {
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32).filter(|i| mask & (1 << i) != 0).map(|i| PathBuf::from(format!("{}:\\", (b'A' + i as u8) as char)))
        .filter(|d| unsafe { GetDriveTypeW(wide(&d.to_string_lossy()).as_ptr()) } == 3).collect()   // DRIVE_FIXED
}

/// Places D2R may be installed, most likely first: a running game, Battle.net's install record, Steam's libraries,
/// then the usual folder names on every fixed drive.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.extend(running_game_folder());
    for root in [HKLM, HKCU] {
        for sub in [format!("SOFTWARE\\WOW6432Node\\{}", UNINSTALL), format!("SOFTWARE\\{}", UNINSTALL)] {
            out.extend(reg_string(root, &sub, "InstallLocation").map(PathBuf::from));
        }
    }
    if let Some(steam) = reg_string(HKCU, "Software\\Valve\\Steam", "SteamPath") {
        let steam = PathBuf::from(steam.replace('/', "\\"));
        let mut libs = vec![steam.clone()];
        if let Ok(vdf) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) { libs.extend(steam_libraries(&vdf)); }
        for lib in libs { out.push(lib.join("steamapps").join("common").join("Diablo II Resurrected")); }
    }
    for drive in fixed_drives() {
        for dir in ["Program Files (x86)", "Program Files", "Games", "Program Files (x86)\\Battle.net", "Battle.net", "Blizzard", ""] {
            out.push(drive.join(dir).join("Diablo II Resurrected"));
        }
    }
    out
}

/// The first of `places` that holds D2R.exe.
pub fn first_install(places: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    places.into_iter().find(|c| c.join("D2R.exe").is_file())
}

/// Where D2R is installed on this PC, found without asking (see `candidates`).
pub fn find_game_folder() -> Option<PathBuf> { first_install(candidates()) }

// ------------------------------------------------------------------------------------------------ one-copy lock
/// The name of D2R's one-copy lock (as Process Explorer shows it, under \Sessions\N\BaseNamedObjects\).
pub const D2R_INSTANCE_LOCK: &str = "DiabloII Check For Other Instances";

/// Closes every handle in process `pid` to a named Event/Mutant whose name ends with `\<name>`; returns how many.
/// Uses DuplicateHandle with DUPLICATE_CLOSE_SOURCE (what people do by hand in Process Explorer). Needs
/// PROCESS_DUP_HANDLE on the game: a game run as administrator needs the launcher run as administrator too.
pub fn close_named_handles(pid: u32, name: &str) -> Result<usize, String> {
    unsafe {
        let proc_ = OpenProcess(0x40 | 0x1000, 0, pid);   // PROCESS_DUP_HANDLE | PROCESS_QUERY_LIMITED_INFORMATION
        if proc_.is_null() {
            return Err(if last_error() == 5 { "Windows refused: the game runs as administrator, so run this launcher as administrator too".into() }
                       else { format!("Windows could not open the game (error {})", last_error()) });
        }
        // every handle in the system (SystemExtendedHandleInformation = 64), grown until it fits
        let mut buf: Vec<u8> = vec![0; 1 << 20];
        let mut status;
        loop {
            let mut ret = 0u32;
            status = NtQuerySystemInformation(64, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut ret);
            if status as u32 != 0xC000_0004 || buf.len() >= (256 << 20) { break; }   // STATUS_INFO_LENGTH_MISMATCH
            buf = vec![0; (ret as usize).max(buf.len() * 2) + (1 << 16)];
        }
        if status != 0 { CloseHandle(proc_); return Err(format!("Windows could not list handles (0x{:08X})", status as u32)); }
        let count = *(buf.as_ptr() as *const usize);
        let first = buf.as_ptr().add(2 * std::mem::size_of::<usize>()) as *const HandleEntry;
        let want = format!("\\{}", name);
        let me = GetCurrentProcess();
        let mut closed = 0;
        let mut info = vec![0u8; 4096];
        let text = |info: &Vec<u8>| { let u = &*(info.as_ptr() as *const UnicodeString);
            if u.buf.is_null() { String::new() } else { String::from_utf16_lossy(std::slice::from_raw_parts(u.buf, u.len as usize / 2)) } };
        for i in 0..count.min((buf.len() / std::mem::size_of::<HandleEntry>()).saturating_sub(1)) {
            let e = &*first.add(i);
            if e.pid != pid as usize { continue; }
            let mut dup: H = std::ptr::null_mut();
            if DuplicateHandle(proc_, e.handle as H, me, &mut dup, 0, 0, 2) == 0 { continue; }   // DUPLICATE_SAME_ACCESS
            // type first (ObjectTypeInformation = 2): only events and mutexes are asked for their name (a name query
            // on some pipe handles can block)
            let mut ret = 0u32;
            let ty = if NtQueryObject(dup, 2, info.as_mut_ptr() as *mut c_void, info.len() as u32, &mut ret) == 0 { text(&info) } else { String::new() };
            let hit = (ty == "Event" || ty == "Mutant") && NtQueryObject(dup, 1, info.as_mut_ptr() as *mut c_void, info.len() as u32, &mut ret) == 0 && text(&info).ends_with(&want);
            CloseHandle(dup);
            if hit && DuplicateHandle(proc_, e.handle as H, std::ptr::null_mut(), std::ptr::null_mut(), 0, 0, 1) != 0 { closed += 1; }   // DUPLICATE_CLOSE_SOURCE
        }
        CloseHandle(proc_);
        Ok(closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "kernel32")]
    extern "system" {
        fn CreateEventW(sa: *const c_void, manual: i32, initial: i32, name: *const u16) -> H;
        fn OpenEventW(access: u32, inherit: i32, name: *const u16) -> H;
    }

    #[test]
    fn closes_only_the_named_lock_in_a_process() {
        // our own process stands in for the game: a lock with a unique name, and one with another name
        let name = format!("Launcher Test Instance Lock {}", std::process::id());
        let other = format!("Launcher Test Other {}", std::process::id());
        unsafe {
            let lock = CreateEventW(std::ptr::null(), 1, 0, wide(&format!("Local\\{}", name)).as_ptr());
            let keep = CreateEventW(std::ptr::null(), 1, 0, wide(&format!("Local\\{}", other)).as_ptr());
            assert!(!lock.is_null() && !keep.is_null());
            assert_eq!(close_named_handles(std::process::id(), &name), Ok(1));
            assert!(OpenEventW(0x00100000, 0, wide(&format!("Local\\{}", name)).as_ptr()).is_null());   // gone: no handle left
            let still = OpenEventW(0x00100000, 0, wide(&format!("Local\\{}", other)).as_ptr());
            assert!(!still.is_null());
            CloseHandle(still);
            CloseHandle(keep);
            assert_eq!(close_named_handles(std::process::id(), &name), Ok(0));
        }
        assert_eq!(std::mem::size_of::<HandleEntry>(), 40);
        assert_eq!(std::mem::size_of::<PROCESSENTRY32W>(), 568);
    }

    #[test]
    fn finds_the_game_in_steam_libraries_and_folders() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t\t\"label\"\t\t\"\"\n\t}\n}\n";
        assert_eq!(steam_libraries(vdf), vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("D:\\SteamLibrary")]);
        let root = std::env::temp_dir().join(format!("d2r-launcher-find-{}", std::process::id()));
        let game = root.join("Games").join("Diablo II Resurrected");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("D2R.exe"), b"").unwrap();
        assert_eq!(first_install(vec![root.join("nope"), game.clone()]), Some(game.clone()));
        assert_eq!(first_install(vec![root.join("nope")]), None);
        assert!(!fixed_drives().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_the_token_from_the_sign_in_redirect() {
        assert_eq!(token_from_redirect("http://localhost:0/?ST=US-d12ab21123abcdefabcdefabcdef1231-123123123&flowTrackingId=123a1234-1234"),
            Some("US-d12ab21123abcdefabcdefabcdef1231-123123123".to_string()));
        assert_eq!(token_from_redirect("http://localhost:0/?flowTrackingId=1&ST=EU-abcdef1234-99"), Some("EU-abcdef1234-99".to_string()));
        assert_eq!(token_from_redirect("http://localhost:0/?flowTrackingId=1"), None);
        assert_eq!(token_from_redirect("http://localhost:0/?ST=<script>"), None);
        assert_eq!(token_from_redirect("https://us.battle.net/login/en/"), None);
    }

    #[test]
    fn dpapi_locks_and_unlocks_for_this_user() {
        let blob = protect(b"correct horse").expect("DPAPI available");
        assert!(!blob.windows(13).any(|w| w == b"correct horse"));
        assert_eq!(unprotect(&blob).unwrap(), b"correct horse");
        let mut bad = blob.clone();
        let n = bad.len();
        bad[n - 1] ^= 0xFF;
        assert!(unprotect(&bad).is_none());
    }
}
