# CloudyAB Setup & Launch Script (Windows)
# Usage: .\scripts\setup.ps1
#
# This script:
# 1. Checks prerequisites (Rust, cargo)
# 2. Checks for Obscura browser engine
# 3. Creates default config if missing
# 4. Creates required data directories
# 5. Builds the project in release mode
# 6. Starts the CloudyAB MCP server

$ErrorActionPreference = "Stop"

$ProjectDir = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$ObscuraDir = Join-Path $ProjectDir "bin"
$ObscuraBin = Join-Path $ObscuraDir "obscura.exe"
$ConfigFile = Join-Path $ProjectDir "cloudyab.toml"
$ExampleConfig = Join-Path $ProjectDir "cloudyab.example.toml"
$DataDir = Join-Path $ProjectDir "data"
$ModelsDir = Join-Path $ProjectDir "models"

function Log-Info($msg) { Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Log-Ok($msg) { Write-Host "[OK] $msg" -ForegroundColor Green }
function Log-Warn($msg) { Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Log-Err($msg) { Write-Host "[ERROR] $msg" -ForegroundColor Red }

Write-Host ""
Write-Host "  +=======================================+" -ForegroundColor Cyan
Write-Host "  |       CloudyAB Setup & Launch        |" -ForegroundColor Cyan
Write-Host "  | Stealth Browser + AI Captcha Solver  |" -ForegroundColor Cyan
Write-Host "  +=======================================+" -ForegroundColor Cyan
Write-Host ""

# Step 1: Check Rust toolchain
Log-Info "Checking prerequisites..."

$cargoPath = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargoPath) {
    Log-Err "Rust/Cargo not found. Install from https://rustup.rs"
    Write-Host "  Download: https://win.rustup.rs/x86_64"
    exit 1
}
$rustVersion = (rustc --version) -replace "rustc ", "" -replace " .*", ""
Log-Ok "Rust $rustVersion found"

# Step 2: Create directories
Log-Info "Creating directories..."
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
New-Item -ItemType Directory -Force -Path $ModelsDir | Out-Null
New-Item -ItemType Directory -Force -Path $ObscuraDir | Out-Null
Log-Ok "Directories ready (data/, models/, bin/)"

# Step 3: Check for browser binary — download Obscura if missing
$BrowserAvailable = $false
$envBin = $env:CLOUDYAB_BROWSER_BIN

if ($envBin -and (Test-Path $envBin)) {
    Log-Ok "Browser binary from env: $envBin"
    $BrowserAvailable = $true
} elseif (Test-Path $ObscuraBin) {
    Log-Ok "Obscura found at: $ObscuraBin"
    $BrowserAvailable = $true
} else {
    Log-Info "Downloading Obscura headless browser..."

    $ObscuraUrl = "https://github.com/h4ckf0r0day/obscura/releases/latest/download/obscura-x86_64-windows.zip"
    $DownloadPath = Join-Path $ObscuraDir "obscura.zip"

    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $ObscuraUrl -OutFile $DownloadPath -UseBasicParsing
        Log-Info "Extracting Obscura..."

        Expand-Archive -Path $DownloadPath -DestinationPath $ObscuraDir -Force

        # Find obscura.exe in extracted files
        $foundBin = Get-ChildItem -Path $ObscuraDir -Recurse -Filter "obscura.exe" | Select-Object -First 1
        if ($foundBin) {
            if ($foundBin.FullName -ne $ObscuraBin) {
                Copy-Item $foundBin.FullName $ObscuraBin -Force
            }
            $BrowserAvailable = $true
            Log-Ok "Obscura installed at: $ObscuraBin"
        }

        # Clean up zip
        Remove-Item $DownloadPath -Force -ErrorAction SilentlyContinue
        # Clean up extracted subdirs (keep only the binary)
        Get-ChildItem -Path $ObscuraDir -Directory | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    } catch {
        Log-Warn "Obscura download failed: $_"
    }

    if (-not $BrowserAvailable) {
        Log-Warn "Could not auto-install Obscura."
        Write-Host "  Download manually from: https://github.com/h4ckf0r0day/obscura/releases"
        Write-Host "  Place binary at: $ObscuraBin"
        Log-Info "Continuing without browser (HTTP stealth layer only)..."
    }
}

# Step 4: Generate config if missing
if (-not (Test-Path $ConfigFile)) {
    Log-Info "Generating default configuration..."
    Copy-Item $ExampleConfig $ConfigFile

    if ($BrowserAvailable -and (Test-Path $ObscuraBin)) {
        $content = Get-Content $ConfigFile -Raw
        $escapedPath = $ObscuraBin -replace "\\", "/"
        $content = $content -replace '# binary_path = "/path/to/obscura"', "binary_path = `"$escapedPath`""
        Set-Content $ConfigFile $content
    }

    Log-Ok "Config created: $ConfigFile"
} else {
    Log-Ok "Config exists: $ConfigFile"
}

# Step 5: Build the project
Log-Info "Building CloudyAB (release mode)..."
Push-Location $ProjectDir

try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
    Log-Ok "Build successful"
} catch {
    Log-Err "Build failed. Check errors above."
    Pop-Location
    exit 1
}

$Binary = Join-Path $ProjectDir "target\release\cloudyab.exe"
if (-not (Test-Path $Binary)) {
    Log-Err "Binary not found at expected path: $Binary"
    Pop-Location
    exit 1
}

Pop-Location

# Step 6: Launch
Write-Host ""
Write-Host "  ===========================================" -ForegroundColor Green
Write-Host "    CloudyAB is ready!" -ForegroundColor Green
Write-Host "  ===========================================" -ForegroundColor Green
Write-Host ""
Write-Host "  MCP Server:  stdio (connect via MCP client)"
Write-Host "  HTTP API:    http://localhost:9222"
Write-Host "  Health:      http://localhost:9222/health"
Write-Host ""
Write-Host "  Submit a task (PowerShell):"
Write-Host "    Invoke-RestMethod -Method Post -Uri http://localhost:9222/tasks ``"
Write-Host "      -ContentType 'application/json' ``"
Write-Host "      -Body '{`"url`": `"https://example.com`", `"snapshot`": true}'"
Write-Host ""
Log-Info "Starting CloudyAB..."
Write-Host ""

& $Binary
