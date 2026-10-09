# Decompiles the whole analyzed exe into extracted\decomp\*.c (resumable; game-derived, never commit).
# Then index it:  gamedb index -r extracted\decomp
$env:GHIDRA_MAXMEM = "12G"
$root = Split-Path $PSScriptRoot -Parent
& "C:\Users\User\Desktop\ghidra_12.1.4_PUBLIC\support\analyzeHeadless.bat" "$root\extracted\ghidra" sekiro -process sekiro_steamless.exe -noanalysis -readOnly -scriptPath "$root\tools\ghidra_scripts" -postScript DecompileAll.java "$root\extracted\decomp" *> "$root\extracted\ghidra\decompile_all.log"
Get-Content "$root\extracted\ghidra\decompile_all.log" | Select-String "DecompileAll.java>|ERROR" | Select-Object -Last 3
