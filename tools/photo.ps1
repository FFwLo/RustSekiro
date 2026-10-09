# Visual check in one small image: runs the game in photo mode (src/photo.rs: back / front / side
# views of Wolf with the enemy in front, then Wolf mid-slash), crops each shot to the characters
# and tiles them 2x2 into one contact sheet (~600x500 px, cheap to look at).
#   powershell -ExecutionPolicy Bypass -File tools\photo.ps1 [-Out sheet.png] [-NoBuild]
#   -Poses "GroundAttackCombo1:9,StandDeflectEasySmall_V1_F:5" -Cols 4   side view of each Wolf state at a TAE frame
param([string]$Out = "$env:TEMP\shinobi_sheet.png", [switch]$NoBuild, [string]$Poses = "", [int]$Cols = 2)
$root = Split-Path $PSScriptRoot -Parent
Stop-Process -Name sv1 -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
if (-not $NoBuild) {
    Push-Location $root
    $b = cargo build --release 2>&1 | Out-String
    Pop-Location
    if ($b -match "error(\[E\d+\])?:") { "BUILD FAILED"; exit 1 }
}
$dir = Join-Path $env:TEMP "shinobi_photo"
Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
$env:SHINOBI_PHOTO = $dir
if ($Poses) { $env:SHINOBI_PHOTO_POSES = $Poses }
$p = Start-Process -FilePath (Join-Path $root "target\release\sv1.exe") -WorkingDirectory $root -PassThru
$env:SHINOBI_PHOTO = $null; $env:SHINOBI_PHOTO_POSES = $null
if (-not $p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force; "TIMEOUT (photo mode did not finish)" }
Add-Type -AssemblyName System.Drawing
$shots = Get-ChildItem $dir -Filter *.png | Sort-Object Name
if (-not $shots) { "NO SHOTS"; exit 1 }
# Crop: the middle of the frame, where Wolf and the enemy stand.
$cw = if ($Cols -gt 2) { 200 } else { 300 }; $ch = [int]($cw * 0.83)
$sheet = New-Object System.Drawing.Bitmap ($cw * $Cols), ($ch * [math]::Ceiling($shots.Count / $Cols))
$g = [System.Drawing.Graphics]::FromImage($sheet)
$g.InterpolationMode = 'HighQualityBicubic'
$i = 0
foreach ($s in $shots) {
    $img = [System.Drawing.Image]::FromFile($s.FullName)
    $src = New-Object System.Drawing.Rectangle ([int]($img.Width * 0.28)), ([int]($img.Height * 0.12)), ([int]($img.Width * 0.44)), ([int]($img.Height * 0.80))
    $dst = New-Object System.Drawing.Rectangle (($i % $Cols) * $cw), ([math]::Floor($i / $Cols) * $ch), $cw, $ch
    $g.DrawImage($img, $dst, $src, [System.Drawing.GraphicsUnit]::Pixel)
    $g.DrawString($s.BaseName, (New-Object System.Drawing.Font "Consolas", 9), [System.Drawing.Brushes]::Yellow, $dst.X + 4, $dst.Y + 4)
    $img.Dispose(); $i++
}
$sheet.Save($Out)
"sheet: $Out ($($shots.Count) shots)"
