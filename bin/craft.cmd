@echo off
set "SCRIPT_DIR=%~dp0"
if exist "%SCRIPT_DIR%..\target\release\craft.exe" (
    "%SCRIPT_DIR%..\target\release\craft.exe" %*
) else if exist "%SCRIPT_DIR%..\target\debug\craft.exe" (
    "%SCRIPT_DIR%..\target\debug\craft.exe" %*
) else (
    craft %*
)
