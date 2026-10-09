# Runs DecompileRefs.java on the analyzed Sekiro project without re-analysis.
# Usage: powershell -File tools\ghidra.ps1 <out.c> ref:ADDR fn:ADDR callers:ADDR callees:ADDR ...
param([Parameter(Mandatory)][string]$Out, [Parameter(ValueFromRemainingArguments)][string[]]$Targets)
$env:GHIDRA_MAXMEM = "10G"
$root = Split-Path $PSScriptRoot -Parent
& "C:\Users\User\Desktop\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat" "$root\extracted\ghidra" sekiro -process sekiro_steamless.exe -noanalysis -readOnly -scriptPath "$root\tools\ghidra_scripts" -postScript DecompileRefs.java $Out @Targets *> "$root\extracted\ghidra\script.log"
Get-Content "$root\extracted\ghidra\script.log" | Select-String "DecompileRefs.java>|ERROR" | Select-Object -Last 3
