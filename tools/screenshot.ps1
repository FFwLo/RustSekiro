# Captures the running game window (DPI-aware) to the given PNG, scaled by 1/2.
#   -Crop "x,y,w,h"  instead saves that region of the FULL-resolution window (fractions 0-1 of the
#                    window, e.g. "0.3,0.2,0.4,0.6"), for a sharp close look at a small area.
param([string]$Out = "$env:TEMP\shinobi_shot.png", [int]$Delay = 0, [string]$Crop = "")
Start-Sleep -Milliseconds $Delay
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public class ShotW { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; } }
'@
[ShotW]::SetProcessDPIAware() | Out-Null
$p = Get-Process sv1 -ErrorAction Stop | Select-Object -First 1
[ShotW]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; Start-Sleep -Milliseconds 400
$r = New-Object ShotW+RECT; [ShotW]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L; $h = $r.B - $r.T
$bmp = New-Object Drawing.Bitmap $w, $h; $g = [Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
if ($Crop) {
    $c = $Crop.Split(",") | ForEach-Object { [double]$_ }
    $rect = New-Object Drawing.Rectangle ([int]($c[0] * $w)), ([int]($c[1] * $h)), ([int]($c[2] * $w)), ([int]($c[3] * $h))
    $bmp.Clone($rect, $bmp.PixelFormat).Save($Out)
} else {
    $small = New-Object Drawing.Bitmap $bmp, ([int]($w / 2)), ([int]($h / 2)); $small.Save($Out)
}
"$Out"
