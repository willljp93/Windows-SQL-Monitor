param(
    [ValidateSet("All", "Standard", "Offline")]
    [string]$Mode = "All"
)

$ErrorActionPreference = "Stop"

$confJsonBanner = Get-Content (Join-Path $PSScriptRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
Write-Host "=== Windows-SQL Monitor v$($confJsonBanner.version): Generador de Instaladores .exe (Modo: $Mode) ===" -ForegroundColor Cyan

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Rust/Cargo no detectado. Instala Rust desde https://rustup.rs/ y vuelve a intentarlo."
}

if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
    Write-Host "Instalando Tauri CLI..." -ForegroundColor Yellow
    cargo install tauri-cli --version "^2.0"
}

$iconDir = Join-Path $PSScriptRoot "src-tauri\icons"
$uiDir = Join-Path $PSScriptRoot "ui"
New-Item -ItemType Directory -Force -Path $iconDir | Out-Null

# Limpiar iconos compilados previos para garantizar que se aplique el nuevo diseño seleccionado
foreach ($oldIcon in @("icon.ico", "icon.png", "32x32.png", "128x128.png", "128x128@2x.png", "icon.icns", "app-icon-clean.png")) {
    $oldPath = Join-Path $iconDir $oldIcon
    if (Test-Path $oldPath) {
        Remove-Item $oldPath -Force -ErrorAction SilentlyContinue
    }
}

$cleanPngPath = Join-Path $iconDir "app-icon-clean.png"
$icoPath = Join-Path $iconDir "icon.ico"
$pngPath = Join-Path $iconDir "icon.png"
$png32Path = Join-Path $iconDir "32x32.png"
$png128Path = Join-Path $iconDir "128x128.png"
$png256Path = Join-Path $iconDir "128x128@2x.png"
$srcJpg = Join-Path $iconDir "app-icon-source.jpg"
$srcPng = Join-Path $iconDir "app-icon-source.png"
$srcImg = if (Test-Path $srcPng) { $srcPng } elseif (Test-Path $srcJpg) { $srcJpg } else { $null }

Add-Type -AssemblyName System.Drawing

function New-ResizedIconBitmap([System.Drawing.Image]$img, [int]$size, [bool]$removeWhiteBg) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)
    $g.DrawImage($img, 0, 0, $size, $size)
    $g.Dispose()

    if ($removeWhiteBg) {
        $visited = New-Object 'bool[,]' $size, $size
        $queue = New-Object System.Collections.Generic.Queue[int]
        $seeds = @(0, ($size - 1), (($size - 1) * $size), (($size * $size) - 1))
        foreach ($s in $seeds) { $queue.Enqueue($s) }
        while ($queue.Count -gt 0) {
            $pos = $queue.Dequeue()
            $x = $pos % $size
            $y = [int][math]::Floor($pos / $size)
            if ($visited[$x, $y]) { continue }
            $visited[$x, $y] = $true
            $c = $bmp.GetPixel($x, $y)
            if ($c.A -lt 20 -or ($c.R -ge 235 -and $c.G -ge 235 -and $c.B -ge 235)) {
                $bmp.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(0, 255, 255, 255))
                if ($x -gt 0) { $queue.Enqueue(($y * $size) + ($x - 1)) }
                if ($x -lt ($size - 1)) { $queue.Enqueue(($y * $size) + ($x + 1)) }
                if ($y -gt 0) { $queue.Enqueue((($y - 1) * $size) + $x) }
                if ($y -lt ($size - 1)) { $queue.Enqueue((($y + 1) * $size) + $x) }
            }
        }
    }
    return $bmp
}

function Write-ClassicIco([System.Drawing.Bitmap[]]$bitmaps, [string]$outFile) {
    $pngChunks = @()
    foreach ($b in $bitmaps) {
        $ms = New-Object System.IO.MemoryStream
        $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $pngChunks += ,($ms.ToArray())
        $ms.Dispose()
    }
    $fs = New-Object System.IO.FileStream($outFile, [System.IO.FileMode]::Create)
    $bw = New-Object System.IO.BinaryWriter($fs)
    $bw.Write([uint16]0)
    $bw.Write([uint16]1)
    $bw.Write([uint16]$bitmaps.Count)
    $offset = 6 + (16 * $bitmaps.Count)
    for ($i = 0; $i -lt $bitmaps.Count; $i++) {
        $w = $bitmaps[$i].Width
        $h = $bitmaps[$i].Height
        $bytes = $pngChunks[$i]
        $bw.Write([byte]($(if ($w -ge 256) { 0 } else { $w })))
        $bw.Write([byte]($(if ($h -ge 256) { 0 } else { $h })))
        $bw.Write([byte]0)
        $bw.Write([byte]0)
        $bw.Write([uint16]1)
        $bw.Write([uint16]32)
        $bw.Write([uint32]$bytes.Length)
        $bw.Write([uint32]$offset)
        $offset += $bytes.Length
    }
    foreach ($bytes in $pngChunks) {
        $bw.Write($bytes)
    }
    $bw.Flush()
    $bw.Close()
    $fs.Close()
}

if ($srcImg) {
    Write-Host "Preparando icono seleccionado desde: $srcImg" -ForegroundColor Cyan
    $rawImg = [System.Drawing.Image]::FromFile($srcImg)
    $rawBmp = New-Object System.Drawing.Bitmap($rawImg)
    $hasTransparentCorners = (
        $rawBmp.GetPixel(0, 0).A -lt 20 -and
        $rawBmp.GetPixel($rawBmp.Width - 1, 0).A -lt 20 -and
        $rawBmp.GetPixel(0, $rawBmp.Height - 1).A -lt 20 -and
        $rawBmp.GetPixel($rawBmp.Width - 1, $rawBmp.Height - 1).A -lt 20
    )
    if ($srcImg -eq $srcPng -and $hasTransparentCorners -and $rawBmp.Width -eq 512 -and $rawBmp.Height -eq 512) {
        Copy-Item $srcPng $cleanPngPath -Force
        $bmp512 = New-Object System.Drawing.Bitmap($cleanPngPath)
    } else {
        $bmp512 = New-ResizedIconBitmap $rawImg 512 (-not $hasTransparentCorners)
        $bmp512.Save($cleanPngPath, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $rawBmp.Dispose()
    $rawImg.Dispose()
} else {
    Write-Host "Generando icono moderno por defecto..." -ForegroundColor Yellow
    $bmp512 = New-Object System.Drawing.Bitmap(512, 512, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp512)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    $bgBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(15, 23, 42))
    $g.FillRectangle($bgBrush, 32, 56, 448, 320)
    $winBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(56, 189, 248))
    $g.FillRectangle($winBrush, 80, 110, 82, 82)
    $g.FillRectangle($winBrush, 174, 110, 82, 82)
    $g.FillRectangle($winBrush, 80, 204, 82, 82)
    $g.FillRectangle($winBrush, 174, 204, 82, 82)
    $okBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(34, 197, 94))
    $g.FillEllipse($okBrush, 310, 138, 118, 118)
    $g.Dispose()
    $bmp512.Save($cleanPngPath, [System.Drawing.Imaging.ImageFormat]::Png)
}

if (Test-Path $uiDir) {
    Copy-Item $cleanPngPath (Join-Path $uiDir "app-icon.png") -Force
}

Write-Host "Generando paquete de iconos Windows (.ico + PNGs) con cargo tauri icon..." -ForegroundColor Cyan
Push-Location $PSScriptRoot
cargo tauri icon $cleanPngPath
$iconExit = $LASTEXITCODE
Pop-Location

if ($iconExit -ne 0 -or -not (Test-Path $icoPath) -or -not (Test-Path $png32Path) -or -not (Test-Path $png128Path)) {
    Write-Host "Aplicando generador local de respaldo para icon.ico y PNGs..." -ForegroundColor Yellow
    $bmp256 = New-ResizedIconBitmap $bmp512 256 $false
    $bmp128 = New-ResizedIconBitmap $bmp512 128 $false
    $bmp48  = New-ResizedIconBitmap $bmp512 48 $false
    $bmp32  = New-ResizedIconBitmap $bmp512 32 $false
    $bmp512.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp256.Save($png256Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp128.Save($png128Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp32.Save($png32Path, [System.Drawing.Imaging.ImageFormat]::Png)
    Write-ClassicIco @($bmp32, $bmp48, $bmp128, $bmp256) $icoPath
    $bmp32.Dispose()
    $bmp48.Dispose()
    $bmp128.Dispose()
    $bmp256.Dispose()
}
$bmp512.Dispose()

# Forzar re-evaluación de recursos Windows (icon.ico / tray) aunque exista caché target previa
$buildRs = Join-Path $PSScriptRoot "src-tauri\build.rs"
if (Test-Path $buildRs) {
    (Get-Item $buildRs).LastWriteTime = Get-Date
}

$confJson = Get-Content (Join-Path $PSScriptRoot "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$ver = $confJson.version
$bundlePath = Join-Path $PSScriptRoot "src-tauri\target\release\bundle\nsis"
$releaseDir = Join-Path $PSScriptRoot "release-bundles"
New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
$generated = @()

if ($Mode -in @("All", "Standard")) {
    Write-Host "[1/2] Compilando instalador estándar ligero (.exe)..." -ForegroundColor Green
    Push-Location $PSScriptRoot
    cargo tauri build --bundles nsis
    $buildExit = $LASTEXITCODE
    Pop-Location
    if ($buildExit -ne 0) {
        Write-Error "La compilación estándar de Rust falló (código $buildExit). Revisa los mensajes anteriores."
        exit $buildExit
    }
    $stdExe = Get-ChildItem -Path $bundlePath -Filter "*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($stdExe) {
        $destStd = Join-Path $releaseDir "Windows-SQL-Monitor_$($ver)_x64-setup.exe"
        Copy-Item $stdExe.FullName $destStd -Force
        $generated += $destStd
    }
}

if ($Mode -in @("All", "Offline")) {
    Write-Host "[2/2] Empaquetando instalador offline con WebView2 incluido (.exe)..." -ForegroundColor Green
    Push-Location $PSScriptRoot
    cargo tauri build --bundles nsis --config src-tauri\tauri.offline.conf.json
    $buildExit = $LASTEXITCODE
    Pop-Location
    if ($buildExit -ne 0) {
        Write-Error "La compilación offline (WebView2) falló (código $buildExit). Revisa los mensajes anteriores."
        exit $buildExit
    }
    $offExe = Get-ChildItem -Path $bundlePath -Filter "*.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($offExe) {
        $destOff = Join-Path $releaseDir "Windows-SQL-Monitor_$($ver)_x64-offline-WebView2-setup.exe"
        Copy-Item $offExe.FullName $destOff -Force
        $generated += $destOff
    }
}

Write-Host "Instaladores generados en: $releaseDir" -ForegroundColor Cyan
foreach ($item in $generated) {
    $mb = [math]::Round(((Get-Item $item).Length / 1MB), 2)
    Write-Host "  - $item ($mb MB)" -ForegroundColor White
}
if (Test-Path $releaseDir) {
    Invoke-Item $releaseDir
}
