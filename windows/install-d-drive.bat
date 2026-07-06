@echo off
setlocal EnableExtensions

REM ============================================================
REM Install Discord Selfbot on D: drive (Windows)
REM Default folder: D:\discord-selfbot-ai
REM Change INSTALL_DIR below if you want another path.
REM ============================================================

set "INSTALL_DIR=D:\discord-selfbot-ai"
set "REPO=https://github.com/NLj0/bot-Discord-Rust.git"
set "BRANCH=cursor/discord-selfbot-arabic-ai-bd43"

echo.
echo ============================================================
echo Discord Selfbot - Install on D: drive
echo Target: %INSTALL_DIR%
echo ============================================================
echo.

where cargo >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Rust is not installed.
    echo Install from: https://rustup.rs/
    pause
    exit /b 1
)

where git >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Git is not installed.
    echo Install from: https://git-scm.com/download/win
    pause
    exit /b 1
)

if not exist "D:\" (
    echo [ERROR] Drive D: was not found on this PC.
    pause
    exit /b 1
)

if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

if exist "%INSTALL_DIR%\.git" (
    echo [1/5] Updating existing repo...
    pushd "%INSTALL_DIR%"
    git fetch origin
    git checkout %BRANCH%
    git pull origin %BRANCH%
    popd
) else (
    echo [1/5] Cloning repo to %INSTALL_DIR% ...
    git clone --branch %BRANCH% --single-branch %REPO% "%INSTALL_DIR%"
    if errorlevel 1 (
        echo [ERROR] git clone failed.
        pause
        exit /b 1
    )
)

pushd "%INSTALL_DIR%"

if not exist ".env" (
    echo [2/5] Creating .env from .env.example ...
    copy /Y .env.example .env >nul
    echo.
    echo IMPORTANT: Open .env and set USER_TOKEN, CHANNEL_ID, SERVER_ID
    echo File: %INSTALL_DIR%\.env
    echo.
) else (
    echo [2/5] .env already exists - keeping your settings
)

echo [3/5] Building release selfbot (first build may take 10-20 min)...
rustup default stable
cargo build --release --bin selfbot --bin export_training
if errorlevel 1 (
    echo [ERROR] Build failed.
    popd
    pause
    exit /b 1
)

if not exist "data" mkdir "data"
if not exist "data\clean" mkdir "data\clean"
if not exist "data\state" mkdir "data\state"
if not exist "data\reports" mkdir "data\reports"

echo [4/5] Creating start/stop scripts in install folder...
copy /Y "windows\run-live-forever.bat" "run-live-forever.bat" >nul
copy /Y "windows\stop-bot.bat" "stop-bot.bat" >nul
copy /Y "windows\status-bot.bat" "status-bot.bat" >nul
copy /Y "windows\start-hidden.vbs" "start-hidden.vbs" >nul

echo [5/5] Done.
echo.
echo ============================================================
echo Install complete: %INSTALL_DIR%
echo ============================================================
echo Next steps:
echo   1. Edit %INSTALL_DIR%\.env  (USER_TOKEN required)
echo   2. Double-click run-live-forever.bat  (visible window)
echo      OR double-click start-hidden.vbs   (hidden background)
echo   3. Check status-bot.bat
echo   4. Stop with stop-bot.bat
echo.
echo Data saves to: %INSTALL_DIR%\data\
echo Logs:          %INSTALL_DIR%\data\live_run.log
echo ============================================================
popd
pause
