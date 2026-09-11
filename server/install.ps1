# ==============================================================================
# Craft Windows PowerShell One-Line Installer
# ==============================================================================
$ErrorActionPreference = "Stop"

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "                    Craft Windows Installer                     " -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

$BaseUrl = "{{BASE_URL}}"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\craft"
if (!(Test-Path -Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$ExePath = Join-Path $InstallDir "craft.exe"
$DownloadUrl = "$BaseUrl/api/v1/download/craft-windows-amd64.exe"

Write-Host "==> Downloading Craft to $ExePath..." -ForegroundColor Yellow
Invoke-WebRequest -Uri $DownloadUrl -OutFile $ExePath -UseBasicParsing

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host "==> Adding $InstallDir to User PATH..." -ForegroundColor Yellow
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    $env:Path += ";$InstallDir"
}

Write-Host "`n✓ Craft installed successfully!" -ForegroundColor Green
Write-Host "Run 'craft --help' in a new terminal window to get started." -ForegroundColor Green
