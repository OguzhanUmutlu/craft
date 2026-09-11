# ==============================================================================
# Craft Universal Static PowerShell Installer (Windows x64)
# https://craft.oguzhanumutlu.com
# ==============================================================================
$ErrorActionPreference = "Stop"

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "                    Craft Windows Installer                     " -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

$BaseUrl = if ($env:CRAFT_BASE_URL) { $env:CRAFT_BASE_URL } else { "https://craft.oguzhanumutlu.com" }
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\craft"
if (!(Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$ExePath = Join-Path $InstallDir "craft.exe"
$DownloadUrl = "$BaseUrl/downloads/craft-windows-amd64.exe"

Write-Host "==> Downloading Craft from $DownloadUrl..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $DownloadUrl -OutFile $ExePath -UseBasicParsing

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host "==> Adding $InstallDir to User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path += ";$InstallDir"
}

Write-Host "`n✓ Craft installed successfully!" -ForegroundColor Green
Write-Host "Run 'craft --help' in a new terminal window to get started." -ForegroundColor Green
