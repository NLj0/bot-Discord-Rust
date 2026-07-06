@echo off
setlocal EnableExtensions

REM Runs selfbot live 24/7 with auto-restart (safe for 1-2+ days)
REM Put this file in the install folder (D:\discord-selfbot-ai)

cd /d "%~dp0"

if not exist "target\release\selfbot.exe" (
    echo [ERROR] selfbot.exe not found. Run install-d-drive.bat first.
    pause
    exit /b 1
)

if not exist ".env" (
    echo [ERROR] .env missing. Copy .env.example to .env and set USER_TOKEN.
    pause
    exit /b 1
)

if not exist "data" mkdir "data"
if not exist "data\state" mkdir "data\state"

echo ============================================================
echo Discord Selfbot - LIVE 24/7
echo Folder: %CD%
echo Data:   %CD%\data
echo Log:    %CD%\data\live_run.log
echo Press Ctrl+C to stop (or run stop-bot.bat from another window)
echo ============================================================
echo.

set BACKFILL=0
set BACKFILL_ONLY=0
set LIVE_POLL_SECS=2
set DATA_DIR=data

:restart
echo [%date% %time%] Starting selfbot...
target\release\selfbot.exe >> "data\live_run.log" 2>&1
echo [%date% %time%] Selfbot stopped. Restarting in 15 seconds...
if exist "data\state\selfbot.lock" del /F /Q "data\state\selfbot.lock"
timeout /t 15 /nobreak >nul
goto restart
