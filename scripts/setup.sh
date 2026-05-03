#!/usr/bin/env bash
# CloudyAB Setup & Launch Script (Linux/macOS)
# Usage: ./scripts/setup.sh
#
# This script:
# 1. Checks prerequisites (Rust, cargo)
# 2. Checks for Obscura browser engine
# 3. Creates default config if missing
# 4. Creates required data directories
# 5. Builds the project in release mode
# 6. Starts the CloudyAB MCP server

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OBSCURA_DIR="$PROJECT_DIR/bin"
OBSCURA_BIN="$OBSCURA_DIR/obscura"
CONFIG_FILE="$PROJECT_DIR/cloudyab.toml"
DATA_DIR="$PROJECT_DIR/data"
MODELS_DIR="$PROJECT_DIR/models"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${CYAN}[INFO]${NC} $1"; }
log_ok() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_err() { echo -e "${RED}[ERROR]${NC} $1"; }

echo -e "${CYAN}"
echo "  ╔═══════════════════════════════════════╗"
echo "  ║         CloudyAB Setup & Launch       ║"
echo "  ║   Stealth Browser + AI Captcha Solver ║"
echo "  ╚═══════════════════════════════════════╝"
echo -e "${NC}"

# Step 1: Check Rust toolchain
log_info "Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    log_err "Rust/Cargo not found. Install from https://rustup.rs"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi
log_ok "Rust $(rustc --version | cut -d' ' -f2) found"

# Step 2: Create directories
log_info "Creating directories..."
mkdir -p "$DATA_DIR" "$MODELS_DIR" "$OBSCURA_DIR"
log_ok "Directories ready (data/, models/, bin/)"

# Step 3: Check for browser binary — download if missing
BROWSER_AVAILABLE=false
if [ -n "${CLOUDYAB_BROWSER_BIN:-}" ] && [ -f "$CLOUDYAB_BROWSER_BIN" ]; then
    log_ok "Browser binary from env: $CLOUDYAB_BROWSER_BIN"
    BROWSER_AVAILABLE=true
elif [ -f "$OBSCURA_BIN" ]; then
    chmod +x "$OBSCURA_BIN"
    log_ok "Browser found at: $OBSCURA_BIN"
    BROWSER_AVAILABLE=true
else
    log_info "Downloading stealth Chromium browser..."

    OS_TYPE="$(uname -s)"
    ARCH="$(uname -m)"

    if [ "$OS_TYPE" = "Linux" ]; then
        if [ "$ARCH" = "x86_64" ]; then
            CHROMIUM_URL="https://github.com/nicehash/nicehash-chromium/releases/latest/download/chromium-linux64.zip"
        else
            log_err "Unsupported architecture: $ARCH (need x86_64)"
            log_info "Continuing without browser..."
        fi
    elif [ "$OS_TYPE" = "Darwin" ]; then
        if [ "$ARCH" = "arm64" ]; then
            CHROMIUM_URL="https://github.com/nicehash/nicehash-chromium/releases/latest/download/chromium-mac-arm64.zip"
        else
            CHROMIUM_URL="https://github.com/nicehash/nicehash-chromium/releases/latest/download/chromium-mac64.zip"
        fi
    fi

    if [ -n "${CHROMIUM_URL:-}" ]; then
        DOWNLOAD_PATH="$OBSCURA_DIR/chromium.zip"

        if command -v curl &> /dev/null; then
            curl -L -o "$DOWNLOAD_PATH" "$CHROMIUM_URL" 2>/dev/null
        elif command -v wget &> /dev/null; then
            wget -q -O "$DOWNLOAD_PATH" "$CHROMIUM_URL"
        else
            log_err "Neither curl nor wget found. Cannot download browser."
            log_info "Continuing without browser..."
        fi

        if [ -f "$DOWNLOAD_PATH" ]; then
            log_info "Extracting browser..."
            unzip -q -o "$DOWNLOAD_PATH" -d "$OBSCURA_DIR" 2>/dev/null

            # Find the chromium binary in extracted files
            FOUND_BIN=$(find "$OBSCURA_DIR" -name "chromium" -o -name "chrome" -o -name "Chromium" | head -1)
            if [ -n "$FOUND_BIN" ]; then
                cp "$FOUND_BIN" "$OBSCURA_BIN"
                chmod +x "$OBSCURA_BIN"
                BROWSER_AVAILABLE=true
                log_ok "Browser installed at: $OBSCURA_BIN"
            else
                # Try common paths
                if [ -f "$OBSCURA_DIR/chrome-linux64/chrome" ]; then
                    cp "$OBSCURA_DIR/chrome-linux64/chrome" "$OBSCURA_BIN"
                    chmod +x "$OBSCURA_BIN"
                    BROWSER_AVAILABLE=true
                    log_ok "Browser installed at: $OBSCURA_BIN"
                fi
            fi

            rm -f "$DOWNLOAD_PATH"
            # Clean up extracted dirs but keep the binary
            find "$OBSCURA_DIR" -mindepth 1 -maxdepth 1 -type d -exec rm -rf {} + 2>/dev/null || true
        fi
    fi

    if [ "$BROWSER_AVAILABLE" = false ]; then
        log_warn "Could not auto-install browser."
        echo "  Place a Chromium-compatible binary at: $OBSCURA_BIN"
        log_info "Continuing without browser (HTTP stealth layer only)..."
    fi
fi

# Step 4: Generate config if missing
if [ ! -f "$CONFIG_FILE" ]; then
    log_info "Generating default configuration..."
    cp "$PROJECT_DIR/cloudyab.example.toml" "$CONFIG_FILE"

    if [ "$BROWSER_AVAILABLE" = true ] && [ -f "$OBSCURA_BIN" ]; then
        if [[ "$OSTYPE" == "darwin"* ]]; then
            sed -i '' "s|# binary_path = \"/path/to/obscura\"|binary_path = \"$OBSCURA_BIN\"|" "$CONFIG_FILE"
        else
            sed -i "s|# binary_path = \"/path/to/obscura\"|binary_path = \"$OBSCURA_BIN\"|" "$CONFIG_FILE"
        fi
    fi

    log_ok "Config created: $CONFIG_FILE"
else
    log_ok "Config exists: $CONFIG_FILE"
fi

# Step 5: Build the project
log_info "Building CloudyAB (release mode)..."
cd "$PROJECT_DIR"

if cargo build --release; then
    log_ok "Build successful"
else
    log_err "Build failed. Check errors above."
    exit 1
fi

BINARY="$PROJECT_DIR/target/release/cloudyab"
if [ ! -f "$BINARY" ]; then
    log_err "Binary not found at expected path: $BINARY"
    exit 1
fi

# Step 6: Launch
echo ""
echo -e "${GREEN}═══════════════════════════════════════════${NC}"
echo -e "${GREEN}  CloudyAB is ready!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════${NC}"
echo ""
echo "  MCP Server:  stdio (connect via MCP client)"
echo "  HTTP API:    http://localhost:9222"
echo "  Health:      http://localhost:9222/health"
echo ""
echo "  Submit a task:"
echo "    curl -X POST http://localhost:9222/tasks \\"
echo "      -H 'Content-Type: application/json' \\"
echo "      -d '{\"url\": \"https://example.com\", \"snapshot\": true}'"
echo ""
log_info "Starting CloudyAB..."
echo ""

exec "$BINARY"
