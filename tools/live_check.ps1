# Live checks (last-step verification, user-approved 2026-10-08): starts Sekiro through Steam and waits
# for sekiro.exe, then prints its PID. Claude then attaches memscope-mcp (MCP tools: attach / read /
# scan / lua / scripts) and runs the checks in tools/memscope/; you do the in-game action once.
#   powershell -ExecutionPolicy Bypass -File tools\live_check.ps1 [-NoLaunch]
# Static data stays the source of truth: live results only confirm or settle "gap:" items.
param([switch]$NoLaunch, [int]$Timeout = 120)
$p = Get-Process sekiro -ErrorAction SilentlyContinue
if (-not $p -and -not $NoLaunch) {
    Start-Process "steam://rungameid/814380"
    $t = 0
    while (-not ($p = Get-Process sekiro -ErrorAction SilentlyContinue) -and $t -lt $Timeout) { Start-Sleep 2; $t += 2 }
}
if (-not $p) { "sekiro.exe not running"; exit 1 }
# MainModule is not readable from an unelevated shell (sekiro.exe denies it); memscope reports the base.
"sekiro.exe pid $($p.Id)"
"Load a save near an enemy, then tell Claude which check to run (see tools\memscope\README.md)."
