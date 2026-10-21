# Builds the launcher into dist\GumbyLauncher.exe (one file; needs the WebView2 runtime that Windows 10/11 already has).
# The Windows resource compiler can't handle an apostrophe in the source path, so the source is mirrored to
# C:\Users\Public\gumby-build\src and compiled there.
#   -Test   runs the core tests first
param([switch]$Test)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$build = 'C:\Users\Public\gumby-build'
if ($env:LAUNCHER_RUST) { $env:PATH = "$env:LAUNCHER_RUST;" + $env:PATH }
$env:CARGO_TARGET_DIR = "$build\target"
$rc = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\rc.exe" -ErrorAction SilentlyContinue | Sort-Object FullName | Select-Object -Last 1
if ($rc) { $env:RC = $rc.FullName }
& robocopy $repo "$build\src" /MIR /XD target dist .git gen /NFL /NDL /NJH /NJS /NP | Out-Null
if ($LASTEXITCODE -ge 8) { throw "robocopy failed ($LASTEXITCODE)" }
# the owner's own icon (art\icon.ico, kept out of the repo) replaces the plain one for the exe and the taskbar
if (Test-Path "$repo\art\icon.ico") {
  Copy-Item "$repo\art\icon.ico" "$build\src\app\icons\icon.ico" -Force
  (Get-Item "$build\src\app\build.rs").LastWriteTime = Get-Date   # the icon is compiled in by build.rs: make it run again
  'using art\icon.ico'
}
Push-Location "$build\src"
try {
  $offline = if ($env:LAUNCHER_ONLINE) { '' } else { '--offline' }
  if ($Test) {
    $out = & cmd /c "cargo test -p launcher-core $offline 2>&1"; $code = $LASTEXITCODE
    $out | Select-String -Pattern '^test result|FAILED|panicked' | ForEach-Object Line
    if ($code -ne 0) { throw "tests failed ($code)" }
  }
  $out = & cmd /c "cargo build --release -p gumby-launcher $offline 2>&1"; $code = $LASTEXITCODE
  $out | Select-String -Pattern '^error|could not compile|Finished' | ForEach-Object Line
  if ($code -ne 0) { $out | Select-Object -Last 30; throw "cargo build failed ($code)" }
} finally { Pop-Location }
Copy-Item "$build\src\Cargo.lock" "$repo\Cargo.lock" -Force
New-Item -ItemType Directory -Force "$repo\dist" | Out-Null
try { Copy-Item "$build\target\release\gumby-launcher.exe" "$repo\dist\GumbyLauncher.exe" -Force -ErrorAction Stop }
catch { Write-Warning "dist\GumbyLauncher.exe is in use (the launcher is open); the new build is in $build\target\release" }
$mb = [math]::Round((Get-Item "$repo\dist\GumbyLauncher.exe").Length / 1MB, 1)
"built dist\GumbyLauncher.exe ($mb MB)"
