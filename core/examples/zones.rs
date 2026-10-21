//! Fetches the terror zones the way the window does: `cargo run -p launcher-core --example zones`.
fn main() { println!("{:?}", launcher_core::zones::fetch()); }
