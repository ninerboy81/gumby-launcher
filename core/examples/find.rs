//! Prints where the game was found: `cargo run -p launcher-core --example find`.
fn main() { println!("{:?}", launcher_core::win::find_game_folder()); }
