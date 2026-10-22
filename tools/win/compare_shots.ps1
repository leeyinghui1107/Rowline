#Requires -Version 5
<#
.SYNOPSIS
    Side-by-side comparison of the original game's screenshot and Rowline.
.DESCRIPTION
    Crops the original's client game area (604x371 logical at any DPI) and
    Rowline's game area, scales both to the same logical size and writes a
    single PNG with captions.
.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File tools\win\compare_shots.ps1 `
        -Original C:\shots\original.png -Ours build\live.png
#>
param(
    [Parameter(Mandatory = $true)][string]$Original,
    [Parameter(Mandatory = $true)][string]$Ours,
    [string]$Out = "build\compare_original_vs_rowline.png",
    # The original client game area, in the source image's pixels.
    [int]$OriginalX = 15,
    [int]$OriginalY = 101,
    [int]$OriginalW = 1208,
    [int]$OriginalH = 742,
    # Rowline's game area (below the 40 px menu strip at 200%).
    [int]$OursY = 40,
    [int]$OursW = 1208,
    [int]$OursH = 744
)

Add-Type -AssemblyName System.Drawing

$imgOriginal = [System.Drawing.Bitmap]::FromFile((Resolve-Path $Original))
$imgOurs = [System.Drawing.Bitmap]::FromFile((Resolve-Path $Ours))

$W = 604
$H = 371
$HDR = 26
$PAD = 10
$canvasW = ($W * 2) + ($PAD * 3)
$canvasH = $H + $HDR + ($PAD * 2) + 34

$canvas = New-Object System.Drawing.Bitmap $canvasW, $canvasH
$g = [System.Drawing.Graphics]::FromImage($canvas)
$g.Clear([System.Drawing.Color]::FromArgb(255, 245, 245, 245))
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$fontH = New-Object System.Drawing.Font "Segoe UI", 11, ([System.Drawing.FontStyle]::Bold)
$fontS = New-Object System.Drawing.Font "Segoe UI", 9
$brush = [System.Drawing.Brushes]::Black

$srcO = New-Object System.Drawing.Rectangle $OriginalX, $OriginalY, $OriginalW, $OriginalH
$dstO = New-Object System.Drawing.Rectangle $PAD, ($HDR + $PAD), $W, $H
$g.DrawString("Original WinLinez v1.30 (client 604x371)", $fontH, $brush, $PAD, 4)
$g.DrawImage($imgOriginal, $dstO, $srcO, [System.Drawing.GraphicsUnit]::Pixel)

$srcU = New-Object System.Drawing.Rectangle 0, $OursY, $OursW, $OursH
$dstU = New-Object System.Drawing.Rectangle (($PAD * 2) + $W), ($HDR + $PAD), $W, $H
$g.DrawString("Rowline (Classic theme, 604x371 game area)", $fontH, $brush, (($PAD * 2) + $W), 4)
$g.DrawImage($imgOurs, $dstU, $srcU, [System.Drawing.GraphicsUnit]::Pixel)

$g.DrawRectangle([System.Drawing.Pens]::Gray, ($PAD - 1), ($HDR + $PAD - 1), ($W + 1), ($H + 1))
$g.DrawRectangle([System.Drawing.Pens]::Gray, (($PAD * 2) + $W - 1), ($HDR + $PAD - 1), ($W + 1), ($H + 1))

$noteY = $HDR + $PAD + $H + 6
$g.DrawString("Same layout: board origin (138,44), 9x9 cells of 36 px, HUD 44 px, figures at x=40 / x=506.", $fontS, $brush, $PAD, $noteY)
$g.DrawString("Characters are newly drawn (the original artwork is copyrighted); colours sampled from the 200% reference shot.", $fontS, $brush, $PAD, ($noteY + 16))

$dir = Split-Path -Parent $Out
if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
$canvas.Save((Join-Path (Get-Location) $Out), [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output "wrote $Out"

$g.Dispose(); $canvas.Dispose(); $imgOriginal.Dispose(); $imgOurs.Dispose()
