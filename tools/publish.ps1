# Publishes a new version: every installed copy offers "Restart to update" within a few hours.
#   tools\publish.ps1 -Version 0.2.0 -Notes "Groups can be dragged into a new order."
#   tools\publish.ps1 -Version 0.2.0 -Notes "..." -DryRun     builds, signs and checks, uploads nothing
# Needs, in %USERPROFILE%\GumbyLauncher-publisher (never in the repo; back the folder up):
#   signing.key        the release signing key (made once with: cargo run -p launcher-core --example sign -- new-key <file>)
#   github-token.txt   a fine-grained GitHub token for ninerboy81/gumby-launcher only, permission "Contents: read and write"
# Steps: version into the three manifests -> build with tests -> manifest.json (version, sha256, size, notes) signed ->
# GitHub release v<version> with GumbyLauncher.exe, manifest.json, manifest.sig.
param([Parameter(Mandatory)][string]$Version, [Parameter(Mandatory)][string]$Notes, [switch]$DryRun)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$secrets = "$env:USERPROFILE\GumbyLauncher-publisher"
$key = "$secrets\signing.key"
$owner = 'ninerboy81/gumby-launcher'
if ($Version -notmatch '^\d{1,3}\.\d{1,3}\.\d{1,6}$') { throw 'Version must look like 0.2.0' }
if (-not (Test-Path $key)) { throw "No signing key at $key" }

# the key must be the one the installed copies trust
$env:CARGO_TARGET_DIR = 'C:\Users\Public\gumby-build\target'
if ($env:LAUNCHER_RUST) { $env:PATH = "$env:LAUNCHER_RUST;" + $env:PATH }
$public = (& cargo run -q --offline -p launcher-core --example sign -- public $key).Trim()
$shipped = (Select-String -Path "$repo\core\src\update.rs" -Pattern 'pub const PUBLIC_KEY: &str = "([0-9a-f]{64})"').Matches[0].Groups[1].Value
if ($public -ne $shipped) { throw 'signing.key is not the key in core\src\update.rs - the installed copies would refuse this update' }

# version everywhere (files keep their line endings)
function Set-Version($path, $pattern, $replacement) {
  $text = [IO.File]::ReadAllText($path)
  $new = [regex]::Replace($text, $pattern, $replacement, 'Multiline')
  if ($new -eq $text -and $text -notmatch [regex]::Escape($Version)) { throw "version not found in $path" }
  [IO.File]::WriteAllText($path, $new)
}
Set-Version "$repo\core\Cargo.toml" '^version = "[^"]*"' "version = `"$Version`""
Set-Version "$repo\app\Cargo.toml" '^version = "[^"]*"' "version = `"$Version`""
Set-Version "$repo\app\tauri.conf.json" '"version": "[^"]*"' "`"version`": `"$Version`""

& "$PSScriptRoot\build.ps1" -Test
$exe = "$repo\dist\GumbyLauncher.exe"
$out = "$repo\dist\release-$Version"
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $exe "$out\GumbyLauncher.exe" -Force
$hash = (Get-FileHash "$out\GumbyLauncher.exe" -Algorithm SHA256).Hash.ToLower()
$size = (Get-Item "$out\GumbyLauncher.exe").Length
$manifest = [ordered]@{ app = 'GumbyLauncher'; version = $Version; sha256 = $hash; size = $size; notes = $Notes } | ConvertTo-Json -Compress
[IO.File]::WriteAllText("$out\manifest.json", $manifest, (New-Object Text.UTF8Encoding $false))
& cargo run -q --offline -p launcher-core --example sign -- sign $key "$out\manifest.json" "$out\manifest.sig"
if ($LASTEXITCODE -ne 0) { throw 'signing failed' }
"signed $Version ($([math]::Round($size / 1MB, 1)) MB, sha256 $hash)"
if ($DryRun) { "dry run: nothing uploaded ($out)"; return }

$tokenFile = "$secrets\github-token.txt"
if (-not (Test-Path $tokenFile)) { throw "No GitHub token at $tokenFile" }
$headers = @{ Authorization = "Bearer $((Get-Content $tokenFile -Raw).Trim())"; Accept = 'application/vnd.github+json'; 'User-Agent' = 'GumbyLauncher-publish' }
$body = @{ tag_name = "v$Version"; name = "Gumby's Launcher $Version"; body = "$Notes`n`nDownload **GumbyLauncher.exe** and run it. manifest.json and manifest.sig are for the in-app updater." } | ConvertTo-Json
$release = Invoke-RestMethod -Method Post -Uri "https://api.github.com/repos/$owner/releases" -Headers $headers -Body $body -ContentType 'application/json'
foreach ($f in 'GumbyLauncher.exe', 'manifest.json', 'manifest.sig') {
  $type = if ($f -like '*.exe') { 'application/vnd.microsoft.portable-executable' } else { 'application/octet-stream' }
  $url = "https://uploads.github.com/repos/$owner/releases/$($release.id)/assets?name=$f"
  Invoke-RestMethod -Method Post -Uri $url -Headers $headers -InFile "$out\$f" -ContentType $type | Out-Null
  "uploaded $f"
}
"published v${Version}: $($release.html_url)"
