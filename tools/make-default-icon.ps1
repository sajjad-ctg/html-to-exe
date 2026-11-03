# Generates shell/assets/default.ico — the neutral placeholder icon every
# exported exe ships with until the user picks their own.
#
# ICO layout used: Vista+ format with a single PNG entry (256x256), which
# every Windows shell surface renders. Hand-assembled because it is five
# lines of binary next to the PNG payload.

$ErrorActionPreference = "Stop"

$outDir = Join-Path $PSScriptRoot "..\shell\assets"
New-Item -ItemType Directory -Path $outDir -Force | Out-Null
$png = Join-Path $outDir "default.png"
$ico = Join-Path $outDir "default.ico"

Add-Type -AssemblyName System.Drawing

# 256x256 canvas: accent background, white "run" chevron + arrow.
$bmp = New-Object System.Drawing.Bitmap(256, 256)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias

$bg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 32, 51, 87))
$g.FillRectangle($bg, 0, 0, 256, 256)

$white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, 18)
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
# big chevron ">" plus an underline: "launch an App"
$g.DrawLine($pen, 96, 72, 156, 128)
$g.DrawLine($pen, 156, 128, 96, 184)
$g.DrawLine($pen, 76, 204, 180, 204)

$bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
$bg.Dispose(); $white.Dispose(); $pen.Dispose()

# ---------------------------------------------------------------- ico pickle

$pngBytes = [IO.File]::ReadAllBytes($png)
$icoBytes = New-Object IO.MemoryStream
$w = New-Object IO.BinaryWriter($icoBytes)
$w.Write([uint16]0)          # reserved
$w.Write([uint16]1)          # type: icon
$w.Write([uint16]1)          # image count
# directory entry: 0,0 width/height mean 256
$w.Write([byte]0); $w.Write([byte]0)
$w.Write([byte]0)            # palette count
$w.Write([byte]0)            # reserved
$w.Write([uint16]1)          # planes
$w.Write([uint16]32)         # bits per pixel
$w.Write([uint32]$pngBytes.Length)
$w.Write([uint32]22)         # image data offset (6 + 16)
$w.Write($pngBytes)
$w.Flush()
[IO.File]::WriteAllBytes($ico, $icoBytes.ToArray())

Remove-Item $png
Write-Host "written: $ico ($([Math]::Round($pngBytes.Length/1KB)) KB of art)"
