#!/bin/bash

# ========================================
# Discord Selfbot - Linux/Mac Setup Script
# ========================================

echo ""
echo "========================================"
echo "Discord Selfbot - Linux/Mac Setup"
echo "========================================"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}[!] Rust is not installed!${NC}"
    echo -e "${YELLOW}[!] Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
    echo -e "${YELLOW}[!] Then run this script again.${NC}"
    exit 1
fi

echo -e "${GREEN}[1/7] Rust detected!${NC}"
cargo --version

# Check OS and install dependencies
echo -e "${GREEN}[2/7] Checking system dependencies...${NC}"

if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    echo "Detected Linux"
    
    if command -v apt &> /dev/null; then
        # Debian/Ubuntu
        echo "Installing OpenSSL development libraries..."
        sudo apt update
        sudo apt install -y pkg-config libssl-dev
    elif command -v dnf &> /dev/null; then
        # Fedora/RHEL
        echo "Installing OpenSSL development libraries..."
        sudo dnf install -y pkg-config openssl-devel
    elif command -v pacman &> /dev/null; then
        # Arch
        echo "Installing OpenSSL development libraries..."
        sudo pacman -S --noconfirm pkg-config openssl
    else
        echo -e "${YELLOW}[!] Could not detect package manager${NC}"
        echo -e "${YELLOW}[!] Please install pkg-config and openssl-dev manually${NC}"
    fi
elif [[ "$OSTYPE" == "darwin"* ]]; then
    # Mac OS
    echo "Detected macOS"
    
    if command -v brew &> /dev/null; then
        echo "Installing OpenSSL via Homebrew..."
        brew install openssl pkg-config
    else
        echo -e "${YELLOW}[!] Homebrew not found${NC}"
        echo -e "${YELLOW}[!] Install from: https://brew.sh/${NC}"
    fi
else
    echo -e "${YELLOW}[!] Unknown OS: $OSTYPE${NC}"
fi

# Check if .env exists
echo -e "${GREEN}[3/7] Setting up environment file...${NC}"
if [ -f .env ]; then
    echo ".env file already exists"
else
    echo "Creating .env file..."
    cp .env.example .env
    echo -e "${YELLOW}[!] Please edit .env and add your USER_TOKEN${NC}"
fi

# Update Rust
echo -e "${GREEN}[4/7] Updating Rust...${NC}"
rustup update stable
rustup default stable

# Build the project
echo -e "${GREEN}[5/7] Building selfbot (this may take 10-15 minutes first time)...${NC}"
cargo build --release --bin selfbot

if [ $? -ne 0 ]; then
    echo -e "${RED}[!] Build failed! Check the errors above.${NC}"
    exit 1
fi

echo -e "${GREEN}[6/7] Build successful!${NC}"

# Make executable
chmod +x target/release/selfbot

# Create desktop shortcut (Linux only)
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo -e "${GREEN}[7/7] Creating desktop shortcut...${NC}"
    
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    DESKTOP_FILE="$HOME/.local/share/applications/discord-selfbot.desktop"
    
    mkdir -p "$HOME/.local/share/applications"
    
    cat > "$DESKTOP_FILE" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Discord Selfbot
Comment=Discord Selfbot for Arabic AI Training
Exec=$SCRIPT_DIR/target/release/selfbot
Path=$SCRIPT_DIR
Icon=utilities-terminal
Terminal=true
Categories=Utility;Development;
EOF
    
    chmod +x "$DESKTOP_FILE"
    echo "Desktop shortcut created at: $DESKTOP_FILE"
else
    echo -e "${GREEN}[7/7] Skipping desktop shortcut (macOS)${NC}"
fi

echo ""
echo "========================================"
echo -e "${GREEN}Setup Complete!${NC}"
echo "========================================"
echo ""
echo "Next steps:"
echo "1. Edit .env and add your USER_TOKEN"
echo "   Command: nano .env  (or vim .env)"
echo "2. Run the selfbot:"
echo "   Command: cargo run --bin selfbot"
echo "   OR:      ./target/release/selfbot"
echo ""
echo "For help, read: SELFBOT_README.md"
echo ""
