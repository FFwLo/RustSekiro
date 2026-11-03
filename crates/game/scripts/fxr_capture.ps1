param([switch]$NoFxr)

$ErrorActionPreference = 'Stop'
$output = 'target/alt/fxr'
New-Item -ItemType Directory -Path $output -Force | Out-Null
$suffix = if ($NoFxr) { '-standin' } else { '' }
$extra = if ($NoFxr) { @('--no-fxr') } else { @() }
$exe = 'target/alt/release/sekiro-rs.exe'

function Capture([string]$Name, [string[]]$Options, [int[]]$Steps) {
    $shots = ($Steps | ForEach-Object { "${_}:${output}/${Name}${suffix}-${_}.png" }) -join ','
    $ErrorActionPreference = 'Continue'
    & $exe @Options @extra --simple-ai --brain-memory off --profile-systems --visual-stats --shots $shots --exit 2>&1 |
        Tee-Object -FilePath "${output}/${Name}${suffix}.txt"
    if ($LASTEXITCODE -ne 0) { throw "FXR capture failed: $Name" }
}

$owl = @('--arena', 'owl', '--passive', '--god', '--lock-on', '--spawn', '104.5,54.0,244.06,135,0', '--enemy-distance', '4', '--script', 'crates/game/scripts/owl_tools.txt')
Capture 'firecracker' ($owl + @('--force-act', '3010')) @(100,103,106)
Capture 'smoke' ($owl + @('--force-act', '3069')) @(100,103,106)
Capture 'warp' ($owl + @('--force-act', '3039')) @(100,103,106)
Capture 'shuriken' ($owl + @('--force-act', '3018')) @(50,53,56)
Capture 'deflect' @('--arena', 'owl', '--god', '--force-act', '3003', '--spawn', '106,54.04,242,-46,0', '--enemy-distance', '2', '--script', 'crates/game/scripts/owl_blending.txt') @(59,62,65)
Capture 'blood' @('--play', '--passive', '--break-posture', '--script', 'crates/game/scripts/soldier_deathblow.txt') @(180,183,186)
Capture 'lightning' @('--arena', 'genichiro', '--genichiro-phase', 'tomoe', '--god', '--passive', '--force-act', '3039', '--lock-on', '--spawn', '107.829,54.044,241.44,0,0', '--enemy-distance', '6', '--script', 'crates/game/scripts/genichiro_lightning.txt') @(100,103,106,117,120,123)
Capture 'arrow' @('--arena', 'genichiro', '--god', '--passive', '--force-act', '3009', '--lock-on', '--script', 'crates/game/scripts/owl_tools.txt') @(100,103,106)
Capture 'billboard-preview' @('--play', '--passive', '--god', '--fxr-preview', '300240', '--script', 'crates/game/scripts/owl_tools.txt') @(152,154,156)
