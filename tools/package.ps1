# Packages a release: dist/Sv1-v<version>/ (the game, a default config, extract.bat, the
# prebuilt extractor with its keys and defs, the tools the extraction runs, README, licences)
# and dist/Sv1-v<version>-windows.zip. Build first:
#   cargo build --release
#   cargo build --release --manifest-path tools\sekiro-extract\Cargo.toml
# Usage: powershell -File tools\package.ps1 [-Version 0.2.0]
param([string]$Version = "")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
$name = "Sv1-v$Version"
$dist = Join-Path $root "dist\$name"
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force (Join-Path $dist "tools\sekiro-extract") | Out-Null
New-Item -ItemType Directory -Force (Join-Path $dist "tools\fxr-dump") | Out-Null

Copy-Item (Join-Path $root "target\release\sv1.exe") $dist
# The default config is the committed one, not the developer's working copy.
git -C $root show HEAD:config.toml | Set-Content -Path (Join-Path $dist "config.toml") -Encoding utf8
foreach ($f in "README.md", "LICENSE.md", "THIRD_PARTY.md") { Copy-Item (Join-Path $root $f) $dist }
Set-Content -Path (Join-Path $dist "extract.bat") -Encoding ascii -Value @'
@echo off
rem Builds the "extracted" folder from your own copy of Sekiro.
rem For a non-default install: extract.bat -Sekiro "D:\path\to\Sekiro"
rem Without the boss arenas (faster, every boss on the gate map): extract.bat -SkipArenas
powershell -ExecutionPolicy Bypass -File "%~dp0tools\extract.ps1" %*
pause
'@
# The extraction script and the Python tools it runs.
foreach ($f in "extract.ps1", "boss_events.py", "boss_scripts.py", "boss_arenas.py", "tool_export.py", "fxr_extract.py") {
    Copy-Item (Join-Path $root "tools\$f") (Join-Path $dist "tools")
}
# The FXR dumper (node): its sources, not node_modules (npm i runs in extract.ps1).
Get-ChildItem (Join-Path $root "tools\fxr-dump") -File | Copy-Item -Destination (Join-Path $dist "tools\fxr-dump")
# The extractor, prebuilt, with the keys, the param defs and the export state list.
$x = Join-Path $root "tools\sekiro-extract"
Copy-Item (Join-Path $x "target\release\sekiro-extract.exe") (Join-Path $dist "tools\sekiro-extract")
Copy-Item (Join-Path $x "export_states.txt") (Join-Path $dist "tools\sekiro-extract")
Copy-Item -Recurse (Join-Path $x "defs") (Join-Path $dist "tools\sekiro-extract\defs")
Copy-Item -Recurse (Join-Path $x "keys") (Join-Path $dist "tools\sekiro-extract\keys")

$zip = Join-Path $root "dist\$name-windows.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $dist -DestinationPath $zip -CompressionLevel Optimal
"packaged: $zip ({0:N1} MB)" -f ((Get-Item $zip).Length / 1MB)
