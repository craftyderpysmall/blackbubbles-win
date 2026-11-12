<#
.SYNOPSIS
  Build BlackBubbles and install it for the current user (no admin needed).

.DESCRIPTION
  Builds the UI and the Tauri app, then runs the NSIS installer silently. The installer
  puts BlackBubbles in %LOCALAPPDATA%\BlackBubbles with a Start Menu shortcut.
  Settings (%LOCALAPPDATA%\craftyderpysmall\BlackBubbles\config\config.toml) are never
  touched.

  Run from this folder:  powershell -ExecutionPolicy Bypass -File install.ps1
  Uninstall:             Settings > Apps > Installed apps > BlackBubbles > Uninstall
#>
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

foreach ($tool in 'cargo', 'npm') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "$tool not found. See docs/WINDOWS_SETUP.md, 'Build from source'."
    }
}

# A running copy holds the exe open; the installer can't replace it.
Get-Process blackbubbles -ErrorAction SilentlyContinue | Stop-Process -Force

Push-Location ui
try {
    npm ci
    if ($LASTEXITCODE) { throw 'npm ci failed' }
    npm run tauri -- build --bundles nsis
    if ($LASTEXITCODE) { throw 'tauri build failed' }
} finally {
    Pop-Location
}

$setup = Get-ChildItem target\release\bundle\nsis\*-setup.exe | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $setup) { throw 'no installer was built' }
Write-Host "Installing $($setup.Name)..."
Start-Process $setup.FullName -ArgumentList '/S' -Wait

$exe = @('BlackBubbles\blackbubbles.exe', 'Programs\BlackBubbles\blackbubbles.exe') |
    ForEach-Object { Join-Path $env:LOCALAPPDATA $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($exe) {
    Start-Process $exe
    Write-Host "Installed and started: $exe"
} else {
    Write-Host 'Installed. Start BlackBubbles from the Start Menu.'
}
