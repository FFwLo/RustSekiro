# Rebuilds extracted/ and extracted/combat_data.json from your own Sekiro install.
# Usage: powershell -File tools\extract.ps1 [-Sekiro "C:\...\Sekiro"]
param([string]$Sekiro = "C:\Program Files (x86)\Steam\steamapps\common\Sekiro")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root "extracted"
# A release ships the extractor prebuilt next to its keys; a source checkout builds it.
$exe = Join-Path $PSScriptRoot "sekiro-extract\sekiro-extract.exe"
if (-not (Test-Path $exe)) {
    Push-Location (Join-Path $PSScriptRoot "sekiro-extract")
    cargo build --release --quiet
    Pop-Location
    $exe = Join-Path $PSScriptRoot "sekiro-extract\target\release\sekiro-extract.exe"
}
if (-not (Test-Path (Join-Path $Sekiro "sekiro.exe"))) { throw "Sekiro not found in '$Sekiro': pass -Sekiro <install folder>" }
New-Item -ItemType Directory -Force $out | Out-Null
# The game's FMOD runtime is loaded from the install at play time (src/paths.rs sekiro_dir).
Set-Content -Path (Join-Path $out "sekiro_dir.txt") -Value $Sekiro -Encoding ascii
& $exe unpack $Sekiro $out '^/(action/|param/gameparam/|chr/c0000(\.|_a|_c1020\.)|chr/c10[12]0\.|sound/(s?main|c1020|c1010)\.|script/(aicommon|m1\d_\d\d_00_00)\.luabnd|other/default\.rumblebnd)'
& $exe params (Join-Path $out "param\gameparam\gameparam.parambnd.d") (Join-Path $out "json\params")
& $exe export $out
# Wolf's body dummy polys (no meshes) live in the base chrbnd FLVER (camera look-at dmy 142,
# body-based attack capsules).
& $exe model (Join-Path $out "chr\c0000.chrbnd.d\c0000.flver") - (Join-Path $out "model_c0000.bin") (Join-Path $out "tex")
# Character models: each material's textures come from its MTD (mtd/allmaterialbnd), plus the
# shared hair / bandage / fabric pack parts/common_body.tpf.
& $exe unpack $Sekiro $out '^/(mtd/allmaterialbnd|parts/(common_body\.tpf|(am_m_9000|bd_m_9040|fc_m_0200|lg_m_9000|wp_a_0300)\.partsbnd))'
foreach ($c in "c1020", "c1010") {
    & $exe model (Join-Path $out "chr\$c.chrbnd.d\$c.flver") (Join-Path $out "chr\$c.texbnd.d\$c.tpf") (Join-Path $out "model_$c.bin") (Join-Path $out "tex")
}
# Wolf's equipment: arm, body, face, legs, sword and its scabbard (WP_A_0300_1).
foreach ($p in "am_m_9000", "bd_m_9040", "fc_m_0200", "lg_m_9000", "wp_a_0300") {
    $d = Join-Path $out "parts\$p.partsbnd.d"
    & $exe model (Join-Path $d "$($p.ToUpper()).flver") (Join-Path $d "$($p.ToUpper()).tpf") (Join-Path $out "model_c0000_$p.bin") (Join-Path $out "tex")
}
$d = Join-Path $out "parts\wp_a_0300.partsbnd.d"
& $exe model (Join-Path $d "WP_A_0300_1.flver") (Join-Path $d "WP_A_0300.tpf") (Join-Path $out "model_c0000_sheath_a_0300.bin") (Join-Path $out "tex")
# Dummy poly directions (throw absorb facing, player.rs follow_throw).
foreach ($c in "c0000", "c1020", "c1010") { & $exe dummies (Join-Path $out "chr\$c.chrbnd.d\$c.flver") (Join-Path $out "model_$c.dummies.json") }
# Sounds: decoded by the game's own FMOD (fmodex64.dll). FMOD crashes on a few
# subsounds; the decoder records progress and resumes, so rerun until it finishes.
$dll = Join-Path $Sekiro "fmodex64.dll"
for ($i = 0; $i -lt 2000; $i++) {
    & $exe sounds-fmod $out $dll | Out-Null
    if ($LASTEXITCODE -eq 0) { break }
}
