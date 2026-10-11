# Development helper: start the app, capture each tab, convert to PNG.
#
# The app is built with `--features screenshot-hook`, which makes it dump one
# frame as raw RGBA (with a "W H " text header) and exit. This script turns
# those dumps into PNGs so the layout can be eyeballed without a display.
#
#   pwsh -File tools\shoot.ps1 -Tabs main,models,account,settings,plugins

param(
    [string[]]$Tabs = @('main', 'models', 'account', 'settings', 'plugins'),
    [string]$Exe = 'D:\ProjectRem\target\release\projectrem-gui.exe',
    [string]$OutDir = "$env:TEMP\opencode",
    [int]$TimeoutMs = 45000,
    [switch]$KeepSession
)

Add-Type -AssemblyName System.Drawing

if (-not (Test-Path $Exe)) { throw "missing $Exe - run: cargo build --release --features screenshot-hook" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (-not $KeepSession) { Remove-Item -Recurse -Force "$env:APPDATA\ProjectRem" -ErrorAction SilentlyContinue }

foreach ($tab in $Tabs) {
    $raw = Join-Path $OutDir "shot_$tab.raw"
    Remove-Item $raw -ErrorAction SilentlyContinue

    $env:PROJECTREM_SHOT = $raw
    $env:PROJECTREM_TAB = $tab
    $proc = Start-Process -FilePath $Exe -PassThru `
        -RedirectStandardOutput "$env:TEMP\shot.out" -RedirectStandardError "$env:TEMP\shot.err"
    $proc.WaitForExit($TimeoutMs) | Out-Null

    if (-not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        Write-Host "$tab : TIMEOUT"
        continue
    }
    if (-not (Test-Path $raw)) {
        Write-Host "$tab : NO SHOT"
        Get-Content "$env:TEMP\shot.err" -ErrorAction SilentlyContinue | Select-Object -First 5
        continue
    }

    $bytes = [System.IO.File]::ReadAllBytes($raw)
    $head = [System.Text.Encoding]::ASCII.GetString($bytes, 0, 20)
    $sp1 = $head.IndexOf(' ')
    $sp2 = $head.IndexOf(' ', $sp1 + 1)
    $w = [int]$head.Substring(0, $sp1)
    $h = [int]$head.Substring($sp1 + 1, $sp2 - $sp1 - 1)
    $off = $sp2 + 1

    $bmp = [System.Drawing.Bitmap]::new($w, $h)
    for ($y = 0; $y -lt $h; $y++) {
        for ($x = 0; $x -lt $w; $x++) {
            $i = $off + ($y * $w + $x) * 4
            # egui's ColorImage::as_raw is straight RGBA.
            $bmp.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $bytes[$i], $bytes[$i + 1], $bytes[$i + 2]))
        }
    }
    $png = Join-Path $OutDir "ui_$tab.png"
    $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Remove-Item $raw -ErrorAction SilentlyContinue
    Write-Host "$tab : ${w}x${h} -> $png"
}

Remove-Item Env:PROJECTREM_SHOT, Env:PROJECTREM_TAB -ErrorAction SilentlyContinue
