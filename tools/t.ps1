# Quiet test run: prints only failures, panics, compile errors and the totals (the PowerShell twin of t.sh).
#   powershell -ExecutionPolicy Bypass -File tools\t.ps1 [test-name-filter]
param([Parameter(ValueFromRemainingArguments = $true)] [string[]] $Rest)
Set-Location (Split-Path $PSScriptRoot -Parent)
# Like t.sh's grep -A6: a matching line and the 6 after it, at most 60 lines.
$pattern = '^error|^warning: unused|test result|FAILED|panicked|^---- '
$after = 0
$shown = 0
# cmd merges cargo's stderr into stdout: Windows PowerShell would wrap each stderr line as an error record.
cmd /c "cargo test --profile quick --lib $($Rest -join ' ') 2>&1" | ForEach-Object {
    $line = "$_"
    if ($line -match $pattern) { $after = 7 }
    if ($after -gt 0) {
        if ($shown -lt 60) { $line; $shown++ }
        $after--
    }
}
exit $LASTEXITCODE
