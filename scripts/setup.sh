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

# Step 3: Check for Obscura browser binary
BROWSER_AVAILABLE=false
if [ -n "${CLOUDYAB_BROWSER_BIN:-}" ] && [ -f "$CLOUDYAB_BROWSER_BIN" ]; then
    log_ok "Browser binary from env: $CLOUDYAB_BROWSER_BIN"
    BROWSER_AVAILABLE=true
elif [ -f "$OBSCURA_BIN" ]; then
    chmod +x "$OBSCURA_BIN"
    log_ok "Obscura browser found at: $OBSCURA_BIN"
    BROWSER_AVAILABLE=true
else
    log_warn "Obscura browser binary not found at: $OBSCURA_BIN"
    echo ""
    echo "  CloudyAB requires a stealth browser binary (Obscura or compatible)."
    echo "  Options:"
    echo "    1. Place binary at: $OBSCURA_BIN"
    echo "    2. Set env: export CLOUDYAB_BROWSER_BIN=/path/to/browser"
    echo "    3. Set in cloudyab.toml: browser.binary_path = \"/path/to/browser\""
    echo ""
    log_info "Continuing without browser (HTTP stealth layer only)..."
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
