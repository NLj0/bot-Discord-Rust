@echo off
setlocal EnableExtensions

cd /d "%~dp0"

echo ============================================================
echo Discord Selfbot - Status
echo Folder: %CD%
echo ============================================================

tasklist /FI "IMAGENAME eq selfbot.exe" 2>nul | find /I "selfbot.exe" >nul
if errorlevel 1 (
    echo Process: NOT RUNNING
) else (
    echo Process: RUNNING
    tasklist /FI "IMAGENAME eq selfbot.exe"
)

if exist "data\reports\latest.json" (
    echo.
    echo --- latest.json ---
    type "data\reports\latest.json"
) else (
    echo.
    echo No report yet (data\reports\latest.json missing)
)

if exist "data\live_run.log" (
    echo.
    echo --- last 15 log lines ---
    powershell -NoProfile -Command "Get-Content 'data\live_run.log' -Tail 15"
)

echo.
echo ============================================================
pause
