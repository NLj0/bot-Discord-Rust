@echo off
REM ========================================
REM Discord Selfbot - Windows Setup Script
REM ========================================

echo.
echo ========================================
echo Discord Selfbot - Windows Setup
echo ========================================
echo.

REM Check if Rust is installed
where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo [!] Rust is not installed!
    echo [!] Please install from: https://rustup.rs/
    echo [!] Then run this script again.
    pause
    exit /b 1
)

echo [1/6] Rust detected!
cargo --version

REM Check if .env exists
if exist .env (
    echo [2/6] .env file already exists
) else (
    echo [2/6] Creating .env file...
    copy .env.example .env >nul
    echo [!] Please edit .env and add your USER_TOKEN
)

REM Update Rust
echo [3/6] Updating Rust...
rustup update stable
rustup default stable

REM Build the project
echo [4/6] Building selfbot (this may take 10-15 minutes first time)...
cargo build --release --bin selfbot

if %errorlevel% neq 0 (
    echo [!] Build failed! Check the errors above.
    pause
    exit /b 1
)

echo [5/6] Build successful!

REM Create desktop shortcut
echo [6/6] Creating desktop shortcut...
set SCRIPT_DIR=%~dp0
set SELFBOT_PATH=%SCRIPT_DIR%target\release\selfbot.exe
set DESKTOP=%USERPROFILE%\Desktop
set SHORTCUT=%DESKTOP%\Discord Selfbot.lnk

powershell -Command "$WS = New-Object -ComObject WScript.Shell; $SC = $WS.CreateShortcut('%SHORTCUT%'); $SC.TargetPath = '%SELFBOT_PATH%'; $SC.WorkingDirectory = '%SCRIPT_DIR%'; $SC.Description = 'Discord Selfbot for Arabic AI'; $SC.Save()"

echo.
echo ========================================
echo Setup Complete!
echo ========================================
echo.
echo Next steps:
echo 1. Edit .env and add your USER_TOKEN
echo 2. Run: cargo run --bin selfbot
echo    OR double-click the shortcut on Desktop
echo.
echo For help, read: SELFBOT_README.md
echo.
pause
