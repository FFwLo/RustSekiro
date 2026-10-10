# Rebuilds extracted/ and extracted/combat_data.json from your own Sekiro install.
# Usage: powershell -File tools\extract.ps1 [-Sekiro "C:\...\Sekiro"]
param([string]$Sekiro = "C:\Program Files (x86)\Steam\steamapps\common\Sekiro", [switch]$SkipArenas)
$ErrorActionPreference = "Stop"
# Python 3 runs the boss scripts, the boss arenas and the prosthetic tool export; Node (npm) the
# game's effect definitions. Without them the game still runs: no boss map scripts / arenas,
# no tool models, hand-made effects.
$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { Write-Warning "python not found: boss scripts, boss arenas and prosthetic tool models will be skipped (install Python 3 and rerun)" }
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
& $exe unpack $Sekiro $out '^/(action/|param/gameparam/|chr/c0000(\.|_a|_c1020\.)|chr/c10[12][09]\.|sound/(s?main|c1020|c1010)\.|script/(aicommon|m\d\d_\d\d_00_00)\.luabnd|other/default\.rumblebnd)'
& $exe params (Join-Path $out "param\gameparam\gameparam.parambnd.d") (Join-Path $out "json\params")
# Every enemy and boss: their chr / anim / behaviour / texture binders, Wolf's per-enemy
# deathblow and grab anims (chr/c0000_c<chr>.anibnd) and the map event scripts.
& $exe unpack $Sekiro $out '^/(chr/c[1-7][0-9]{3}\.(chr|ani|beh|tex)bnd|chr/c0000_c[0-9]{4}\.anibnd|event/.*\.emevd)'
# Blood stain decals (other/decaltex.tpf -> extracted/decal) and the HUD sprites
# (menu/hi/01_common -> extracted/hud): the export cuts both.
& $exe unpack $Sekiro $out '^/(other/decaltex|menu/hi/01_common)'
& $exe export $out
# extracted/enemies/roster.json (every placed chr, its MSB placements) and <chr>.json each.
& $exe npcs $out
# Enemy models: own textures, the texture chr (NpcParam normalChangeTexChrId) and the family
# packs c<first 3 digits>8 / 9 (c1500 zombies share c1509), then their dummy polys.
$roster = Get-Content (Join-Path $out "enemies\roster.json") -Raw | ConvertFrom-Json
foreach ($e in $roster) {
    $c = $e.chr
    $flver = Join-Path $out "chr\$c.chrbnd.d\$c.flver"
    if (-not (Test-Path $flver)) { continue }
    $tpf = Join-Path $out "chr\$c.texbnd.d\$c.tpf"
    if (-not (Test-Path $tpf)) { $tpf = "-" }
    $fams = @($c.Substring(1, 3))
    if ($e.texChr -gt 0) { $fams += ("{0:D4}" -f [int]$e.texChr).Substring(0, 3) }
    $extra = @()
    foreach ($f in $fams) { foreach ($k in 8, 9) { $extra += Join-Path $out "chr\c$f$k.texbnd.d" } }
    if ($e.texChr -gt 0) { $extra += Join-Path $out ("chr\c{0:D4}.texbnd.d" -f [int]$e.texChr) }
    $extra = @($extra | Where-Object { Test-Path $_ } | Select-Object -Unique)
    & $exe model $flver $tpf (Join-Path $out "model_$c.bin") (Join-Path $out "tex") @extra
    & $exe dummies $flver (Join-Path $out "model_$c.dummies.json")
}
# Wolf's body dummy polys (no meshes) live in the base chrbnd FLVER (camera look-at dmy 142,
# body-based attack capsules).
& $exe model (Join-Path $out "chr\c0000.chrbnd.d\c0000.flver") - (Join-Path $out "model_c0000.bin") (Join-Path $out "tex")
# Character models: each material's textures come from its MTD (mtd/allmaterialbnd), plus the
# shared hair / bandage / fabric pack parts/common_body.tpf.
& $exe unpack $Sekiro $out '^/(mtd/allmaterialbnd|parts/(common_body\.tpf|(am_m_9000|bd_m_9040|fc_m_0200|lg_m_9000|wp_a_0300|wp_a_0310)\.partsbnd))'
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
# Right weapon model 2 (WP_A_0300_2: no meshes, the Mortal Draw effect dummies 200-280 = TAE 12200..)
# and the Mortal Blade itself (WeaponModelType 2: WP_A_0310 + its scabbard WP_A_0310_1), placed by TAE 715.
& $exe model (Join-Path $d "WP_A_0300_2.flver") (Join-Path $d "WP_A_0300.tpf") (Join-Path $out "model_c0000_mortal_a_0300.bin") (Join-Path $out "tex")
$d = Join-Path $out "parts\wp_a_0310.partsbnd.d"
& $exe model (Join-Path $d "WP_A_0310.flver") (Join-Path $d "WP_A_0310.tpf") (Join-Path $out "model_c0000_mblade_a_0310.bin") (Join-Path $out "tex")
& $exe model (Join-Path $d "WP_A_0310_1.flver") (Join-Path $d "WP_A_0310.tpf") (Join-Path $out "model_c0000_mbsheath_a_0310.bin") (Join-Path $out "tex")
# Game effects (docs/HANDOFF.md section 9): the common FXRs and their textures, then the FXR
# definitions the exported characters use (tools/fxr_extract.py needs node + tools/fxr-dump npm i).
& $exe unpack $Sekiro $out 'sfxbnd_commoneffects'
if (Get-Command npm -ErrorAction SilentlyContinue) {
    Push-Location (Join-Path $PSScriptRoot "fxr-dump"); npm i --silent; Pop-Location
    $env:SEKIRO_EXTRACT = $exe
    python (Join-Path $PSScriptRoot "fxr_extract.py")
}
# Effect models (FXR Model appearance 605: Divine Abduction's leaves, the shuriken's star...):
# each model with its diffuse's texture pack from the same folder -> extracted/fxr_model.
$sfx = Join-Path $out "sfx\sfxbnd_commoneffects.ffxbnd.d"
New-Item -ItemType Directory -Force (Join-Path $out "fxr_model") | Out-Null
$fxModels = [ordered]@{ 4010 = "s04009_a"; 4011 = "s04009_a"; 4017 = "s04017"; 4018 = "s04018_a"; 4070 = "s04070";
    4100 = "s04114_a"; 4140 = "s04133_a"; 4141 = "s04133_a"; 8050 = "s08050_a"; 8060 = "s34030"; 8130 = "s08130_a";
    8131 = "s08130_a"; 8180 = "s34010"; 11601 = "s34010"; 11602 = "s00001" }
foreach ($m in $fxModels.GetEnumerator()) {
    $id = $m.Key
    & $exe model (Join-Path $sfx ("s{0:D5}.flver" -f $id)) (Join-Path $sfx "$($m.Value).tpf") (Join-Path $out "fxr_model\model_$id.bin") (Join-Path $out "tex")
}
# Dummy poly directions (throw absorb facing, player.rs follow_throw).
foreach ($c in "c0000", "c1020", "c1010") { & $exe dummies (Join-Path $out "chr\$c.chrbnd.d\$c.flver") (Join-Path $out "model_$c.dummies.json") }
# Sounds: decoded by the game's own FMOD (fmodex64.dll). FMOD crashes on a few
# subsounds; the decoder records progress and resumes, so rerun until it finishes.
$dll = Join-Path $Sekiro "fmodex64.dll"
for ($i = 0; $i -lt 2000; $i++) {
    & $exe sounds-fmod $out $dll | Out-Null
    if ($LASTEXITCODE -eq 0) { break }
}
# Map: the arena around the General (docs/kb/map.md). MSBs, the m11_01_00_00 pieces and hit
# collision, the m11 texture binders, the area's draw params and the shared map textures; then
# the export around c1020_0004 (60 m, full detail to 20 m, LOD 1 to 40 m: 83 fps against 57 at
# 80 m / 30 / 55, kb/map.md) into map_m11_01_00_00.{bin,hit,json} + extracted/tex.
& $exe unpack $Sekiro $out 'map/mapstudio/|map/m11_01_00_00/|map/m11_01_00_00_envmap|map/m11/|param/drawparam/m11_01|other/maptex'
& $exe bxf (Join-Path $out "map\m11_01_00_00\h11_01_00_00.hkxbhd") (Join-Path $out "map\m11_01_00_00\hit")
$env:SEKIRO_DIR = $Sekiro
$env:MAP_LOD = "20,40"
& $exe map $out m11_01_00_00 c1020_0004 60 | Tee-Object -Variable mapLog
# The map's objects (obj/*.objbnd, listed by the first pass) are unpacked, then exported too.
$need = $mapLog | Where-Object { $_ -like "objects to unpack: *" } | Select-Object -First 1
if ($need) {
    & $exe unpack $Sekiro $out ($need -replace "^objects to unpack: ", "")
    & $exe map $out m11_01_00_00 c1020_0004 60
}

# Boss phases: the map events on a boss's health bars (event/*.emevd + map/mapstudio/*.msb)
# -> extracted/enemies/boss_events.json (src/enemy.rs phase_events).
if ($python) {
    $env:PYTHONIOENCODING = "utf-8"
    python (Join-Path $PSScriptRoot "boss_events.py")
    # Boss map scripts (bullets, summons, warps) -> extracted/enemies/boss_scripts.json (src/enemy/script.rs).
    python (Join-Path $PSScriptRoot "boss_scripts.py")
    # Prosthetic tool models and their anims (parts/wp_a_07xx) -> extracted/tool (src/model/tool.rs).
    python (Join-Path $PSScriptRoot "tool_export.py") --sekiro $Sekiro --exe $exe
    # Every boss in its own arena (kb/map.md): one map at a time, about 2 GB of raw files each,
    # deleted after the export (30-40 min, 3.6 GB kept). -SkipArenas leaves every boss on the gate map.
    if (-not $SkipArenas) {
        $env:SEKIRO_EXTRACT = $exe
        python (Join-Path $PSScriptRoot "boss_arenas.py") -Sekiro $Sekiro
    }
}
