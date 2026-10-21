//! The publisher's signing tool (used by tools\publish.ps1):
//!   cargo run -p launcher-core --example sign -- new-key <key file>          makes a key, prints its public half
//!   cargo run -p launcher-core --example sign -- sign <key file> <manifest> <signature file>
//!   cargo run -p launcher-core --example sign -- verify <manifest> <signature file>   as an installed copy would
//! The key file holds the 32-byte secret as hex. Keep it outside the repo and back it up: without it no update can be
//! published to the installed copies.
use launcher_core::{ed25519, update, win};

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{:02x}", x)).collect() }

fn read_key(path: &str) -> [u8; 32] {
    let s = std::fs::read_to_string(path).expect("read the key file");
    let s = s.trim();
    let v: Vec<u8> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex key")).collect();
    v.try_into().expect("a 32-byte key")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(|s| s.as_str()) {
        Some("new-key") if a.len() == 2 => {
            if std::path::Path::new(&a[1]).exists() { eprintln!("{} exists already: not overwritten", a[1]); std::process::exit(2); }
            let secret: [u8; 32] = win::random_bytes(32).try_into().unwrap();
            std::fs::write(&a[1], hex(&secret)).expect("write the key file");
            println!("{}", hex(&ed25519::public_key(&secret)));
        }
        Some("public") if a.len() == 2 => println!("{}", hex(&ed25519::public_key(&read_key(&a[1])))),
        Some("sign") if a.len() == 4 => {
            let msg = std::fs::read(&a[2]).expect("read the manifest");
            std::fs::write(&a[3], hex(&ed25519::sign(&read_key(&a[1]), &msg))).expect("write the signature");
        }
        Some("verify") if a.len() == 3 => {
            let msg = std::fs::read(&a[1]).expect("read the manifest");
            let sig = std::fs::read_to_string(&a[2]).expect("read the signature");
            match update::manifest_ok(&msg, &sig, update::PUBLIC_KEY, "0.0.0") {
                Ok(m) => println!("ok: {} {} ({} bytes)", m.app, m.version, m.size),
                Err(e) => { eprintln!("REFUSED: {}", e); std::process::exit(1); }
            }
        }
        _ => { eprintln!("usage: new-key <key> | public <key> | sign <key> <manifest> <sig> | verify <manifest> <sig>"); std::process::exit(2); }
    }
}
