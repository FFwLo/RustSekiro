# Closes the game, rebuilds the release exe (errors only), starts it with logging, and prints
# the model / warning / error lines. -Shot also saves a screenshot after the wait.
#   powershell -ExecutionPolicy Bypass -File tools\relaunch.ps1 [-Shot out.png] [-Wait 14] [-NoBuild]
param([string]$Shot = "", [int]$Wait = 14, [switch]$NoBuild)
$root = Split-Path $PSScriptRoot -Parent
Stop-Process -Name sv1 -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
if (-not $NoBuild) {
    Push-Location $root
    $out = cargo build --release 2>&1 | Out-String
    Pop-Location
    $errs = $out -split "`n" | Where-Object { $_ -match "^(error|warning: unused)" -or $_ -match "^\s+-->" }
    if ($out -match "error(\[E\d+\])?:") { $errs | Select-Object -First 20; "BUILD FAILED"; exit 1 }
}
$log = Join-Path $env:TEMP "shinobi_out.txt"; $err = Join-Path $env:TEMP "shinobi_err.txt"
$p = Start-Process -FilePath (Join-Path $root "target\release\sv1.exe") -WorkingDirectory $root `
    -RedirectStandardOutput $log -RedirectStandardError $err -PassThru
Start-Sleep -Seconds $Wait
if ($p.HasExited) { "GAME EXITED (code $($p.ExitCode))" }
Get-Content $log, $err -ErrorAction SilentlyContinue | Select-String -Pattern "meshes|WARN|ERROR|panic" |
    Select-Object -First 15 | ForEach-Object { (($_.Line -replace "\[[0-9;]*m", '') -replace '^\S+Z\s+', '') }
if ($Shot) {
    & (Join-Path $PSScriptRoot "screenshot.ps1") -Out $Shot | Out-Null
    "shot: $Shot"
}
