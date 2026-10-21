# Gumby's Launcher

A small launcher for Diablo II: Resurrected. Save your Battle.net accounts once, then:

- **Play** on an account opens that game, already signed in, intro videos skipped.
- **Launch all** on a group opens its accounts one after another (each waits until the last game's window is up)
  and puts each game in its own quarter of the screen.

Two or more games always go into quarters of the screen (2 x 2), in the order they opened. One game is left as it is.
Set D2R to **Windowed** mode (Options > Video) so the windows can be moved.

## Several games at once

D2R refuses to start a second copy while one is open. With **Settings > Open more than one game** on (it's off until
you turn it on), the launcher closes that one lock, D2R's "DiabloII Check For Other Instances" handle, inside the running
game right before it starts the next. That's the same thing people do by hand in Process Explorer. Nothing else in
the game is read or changed. If D2R runs as administrator, run the launcher as administrator too.

## Your passwords

Passwords are locked with Windows encryption (DPAPI) for your Windows user and saved in
`%APPDATA%\GumbyLauncher\accounts.json`. They can only be unlocked by your Windows user on your PC. The launcher never shows them
again, never logs them and never sends them anywhere: they go only to D2R's own command line when a game starts
(`-username -password -address`), the same way other D2R launchers do it.

## Terror zones

The home screen shows the current and next terror zone and a countdown to the switch, read once a minute from Gumby's
zone server (read-only, no key).

## Updates

The launcher checks this repo's latest GitHub release every few hours. A new version is downloaded in the background and
only taken when its manifest is signed with the publisher's key and the file's size and SHA-256 match; then a green bar
offers **Restart to update**. Publishing: `tools\publish.ps1 -Version 0.2.0 -Notes "..."` (needs the signing key and a
GitHub token in `%USERPROFILE%\GumbyLauncher-publisher`, never in the repo).

## Problem reports

When the launcher crashes or a launch stops, it keeps a short report (what it did, step by step - games are "game 1",
"game 2") and asks whether to send it. The window shows exactly what gets sent; emails, account and group names,
passwords and the Windows user name are taken out first. Nothing is sent without the Send click.

## Build

Rust + Tauri 2. `tools\build.ps1 -Test` runs the core tests and builds `dist\GumbyLauncher.exe`.

- `core/`: accounts, groups, the launch queue, screen quarters, terror zones, reports and updates (Rust, tested with
  `cargo test -p launcher-core`)
- `app/`: the window (Tauri)
- `ui/`: the page
- `tools/preview/`: the page with sample data, for screenshots
