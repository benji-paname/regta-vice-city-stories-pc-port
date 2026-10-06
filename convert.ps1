Add-Type -AssemblyName System.Drawing
$src = "C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en\outputs\VCS-Rust\apercu-banshee.bmp"
$dst = "C:\Users\BENJAMIN\Documents\Codex\2026-10-05\en\outputs\VCS-Rust\apercu-banshee.png"
$image = [System.Drawing.Image]::FromFile($src)
$image.Save($dst, [System.Drawing.Imaging.ImageFormat]::Png)
$image.Dispose()
Write-Host "PNG genere avec succes"
