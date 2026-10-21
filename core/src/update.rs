//! In-app updates. The owner publishes a release on GitHub (tools\publish.ps1): GumbyLauncher.exe, manifest.json
//! {app, version, sha256, size, notes} and manifest.sig, the manifest signed with the owner's Ed25519 key (kept outside
//! the repo; only its public half is in here). The launcher checks the latest release now and then, takes it only when
//! the signature, version, size and hash all check out, downloads it into %APPDATA%\GumbyLauncher\update and offers
//! "Restart to update": the running exe is renamed to .old, the new one moved into its place and started.
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const REPO: &str = "ninerboy81/gumby-launcher";
/// The publisher's Ed25519 public key (hex). Its secret half never leaves the owner's PC.
pub const PUBLIC_KEY: &str = "4965515f61cc0e57b421ab39cb2709a1c3162a0d4e087084ce2c563ba16b73c6";
pub const EXE_ASSET: &str = "GumbyLauncher.exe";
const MAX_EXE: usize = 40 << 20;
const UA: (&str, &str) = ("User-Agent", "GumbyLauncher");

pub fn version_of(s: &str) -> Option<(u32, u32, u32)> {
    let p: Vec<u32> = s.trim().trim_start_matches('v').split('.').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if p.len() == 3 { Some((p[0], p[1], p[2])) } else { None }
}
pub fn newer(candidate: &str, current: &str) -> bool {
    matches!((version_of(candidate), version_of(current)), (Some(a), Some(b)) if a > b)
}

#[derive(Deserialize, Debug, Clone)]
pub struct Manifest { pub app: String, pub version: String, pub sha256: String, pub size: u64, #[serde(default)] pub notes: String }

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 { return None; }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// The manifest, when `public` signed it, it is for this app and it is newer than `current`.
pub fn manifest_ok(manifest: &[u8], sig_hex: &str, public: &str, current: &str) -> Result<Manifest, String> {
    let (pk, sig) = (unhex(public).ok_or("bad public key")?, unhex(sig_hex).ok_or("The update's signature is damaged")?);
    if !crate::ed25519::verify(&pk, manifest, &sig) { return Err("The update is not signed by the publisher: ignored".into()); }
    let m: Manifest = serde_json::from_slice(manifest).map_err(|_| "The update's manifest is damaged".to_string())?;
    if m.app != "GumbyLauncher" { return Err("That update is for another app".into()); }
    if !newer(&m.version, current) { return Err("The update is not newer".into()); }
    if m.sha256.len() != 64 || m.size == 0 || m.size as usize > MAX_EXE { return Err("The update's manifest is damaged".into()); }
    Ok(m)
}

/// GET, following GitHub's download redirects.
fn get(url: &str, accept: &str, limit: usize) -> Result<crate::http::Response, String> {
    let mut url = url.to_string();
    for _ in 0..5 {
        let r = crate::http::request("GET", &url, &[UA, ("Accept", accept)], None, 30_000, limit, None).map_err(|e| e.0)?;
        match (r.status, &r.location) {
            (301 | 302 | 303 | 307 | 308, Some(loc)) if loc.starts_with("https://") => url = loc.clone(),
            _ => return Ok(r),
        }
    }
    Err("Too many redirects".into())
}

pub struct Ready { pub version: String, pub notes: String, pub path: PathBuf }

/// Looks for a newer release and downloads it. Ok(None): up to date (or nothing published yet).
pub fn check(current: &str, data_dir: &Path) -> Result<Option<Ready>, String> {
    let r = get(&format!("https://api.github.com/repos/{}/releases/latest", REPO), "application/vnd.github+json", 1 << 20)
        .map_err(|_| "Could not check for updates (no connection?)".to_string())?;
    if r.status == 404 { return Ok(None); }
    if r.status != 200 { return Err(format!("Could not check for updates ({})", r.status)); }
    let rel: serde_json::Value = serde_json::from_slice(&r.body).map_err(|_| "GitHub sent something unexpected".to_string())?;
    let tag = rel["tag_name"].as_str().unwrap_or("");
    if !newer(tag, current) { return Ok(None); }
    let asset = |name: &str| rel["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"] == name))
        .and_then(|x| x["browser_download_url"].as_str()).map(|s| s.to_string());
    let (Some(mu), Some(su), Some(eu)) = (asset("manifest.json"), asset("manifest.sig"), asset(EXE_ASSET)) else { return Ok(None) };
    let manifest = get(&mu, "application/octet-stream", 64 * 1024)?.body;
    let sig = get(&su, "application/octet-stream", 1024)?.body;
    let m = manifest_ok(&manifest, &String::from_utf8_lossy(&sig), PUBLIC_KEY, current)?;
    let folder = data_dir.join("update");
    let path = folder.join(format!("GumbyLauncher-{}.exe", m.version));
    let have = std::fs::read(&path).ok().filter(|b| b.len() as u64 == m.size && crate::sha256::digest_hex(b) == m.sha256.to_lowercase());
    if have.is_none() {
        let exe = get(&eu, "application/octet-stream", MAX_EXE)?;
        if exe.status != 200 { return Err(format!("The update download failed ({})", exe.status)); }
        if exe.body.len() as u64 != m.size || crate::sha256::digest_hex(&exe.body) != m.sha256.to_lowercase() {
            return Err("The downloaded update does not match its signed manifest: ignored".into());
        }
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("part");
        std::fs::write(&tmp, &exe.body).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    }
    Ok(Some(Ready { version: m.version, notes: m.notes, path }))
}

/// Swaps the running exe for the downloaded one and starts it. The caller exits right after.
pub fn apply(new_exe: &Path) -> Result<(), String> {
    let cur = std::env::current_exe().map_err(|e| e.to_string())?;
    swap(&cur, new_exe)?;
    std::process::Command::new(&cur).spawn().map(|_| ()).map_err(|e| format!("The new version did not start ({})", e.kind()))
}

/// cur -> cur.old (Windows lets a running exe be renamed), new -> cur; on failure the old exe goes back.
pub fn swap(cur: &Path, new_exe: &Path) -> Result<(), String> {
    let old = cur.with_extension("old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(cur, &old).map_err(|e| format!("Could not update ({})", e.kind()))?;
    if let Err(e) = std::fs::copy(new_exe, cur) {
        let _ = std::fs::rename(&old, cur);
        return Err(format!("Could not update ({})", e.kind()));
    }
    Ok(())
}

/// At start: the previous exe and downloads that are not newer than this version go.
pub fn cleanup(current: &str, data_dir: &Path) {
    if let Ok(cur) = std::env::current_exe() { let _ = std::fs::remove_file(cur.with_extension("old")); }
    if let Ok(d) = std::fs::read_dir(data_dir.join("update")) {
        for e in d.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().to_string();
            let v = name.strip_prefix("GumbyLauncher-").and_then(|n| n.strip_suffix(".exe")).unwrap_or("");
            if !newer(v, current) { let _ = std::fs::remove_file(e.path()); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("v0.2.0", "0.1.9") && newer("1.0.0", "0.9.9") && newer("0.1.10", "0.1.9"));
        assert!(!newer("0.1.0", "0.1.0") && !newer("0.0.9", "0.1.0") && !newer("junk", "0.1.0"));
    }

    #[test]
    fn only_a_signed_newer_manifest_for_this_app_passes() {
        let secret = [7u8; 32];
        let public: String = crate::ed25519::public_key(&secret).iter().map(|b| format!("{:02x}", b)).collect();
        let sign = |m: &[u8]| -> String { crate::ed25519::sign(&secret, m).iter().map(|b| format!("{:02x}", b)).collect() };
        let good = br#"{"app":"GumbyLauncher","version":"0.2.0","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":3000000,"notes":"Drag groups"}"#;
        let m = manifest_ok(good, &sign(good), &public, "0.1.0").unwrap();
        assert_eq!((m.version.as_str(), m.notes.as_str()), ("0.2.0", "Drag groups"));
        assert!(manifest_ok(good, &sign(good), &public, "0.2.0").is_err());   // not newer
        let mut tampered = good.to_vec(); tampered[40] = b'9';
        assert!(manifest_ok(&tampered, &sign(good), &public, "0.1.0").is_err());   // signature no longer fits
        let other = br#"{"app":"Other","version":"0.2.0","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1}"#;
        assert!(manifest_ok(other, &sign(other), &public, "0.1.0").is_err());
        let wrong_key: String = crate::ed25519::public_key(&[8u8; 32]).iter().map(|b| format!("{:02x}", b)).collect();
        assert!(manifest_ok(good, &sign(good), &wrong_key, "0.1.0").is_err());
    }

    #[test]
    fn swap_puts_the_new_exe_in_place_and_keeps_the_old() {
        let dir = std::env::temp_dir().join(format!("d2r-launcher-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (cur, new) = (dir.join("GumbyLauncher.exe"), dir.join("new.exe"));
        std::fs::write(&cur, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        swap(&cur, &new).unwrap();
        assert_eq!((std::fs::read(&cur).unwrap(), std::fs::read(cur.with_extension("old")).unwrap()), (b"new".to_vec(), b"old".to_vec()));
        // a missing download leaves the current exe where it was
        assert!(swap(&cur, &dir.join("missing.exe")).is_err());
        assert_eq!(std::fs::read(&cur).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_shipped_public_key_is_a_real_key() {
        assert_eq!(unhex(PUBLIC_KEY).map(|k| k.len()), Some(32));
    }
}
