@echo off
setlocal EnableExtensions

cd /d "%~dp0"

echo Stopping Discord Selfbot...

taskkill /F /IM selfbot.exe >nul 2>&1
if exist "data\state\selfbot.lock" del /F /Q "data\state\selfbot.lock"

echo Done. If a cmd window is still open, close it manually.
pause
