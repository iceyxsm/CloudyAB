# CloudyAB

Stealth headless browser with MCP support and AI-powered captcha solving.

Built in Rust for performance and low memory footprint (200-500MB RAM).

## Features

- **Multi-layer stealth**: HTTP-level TLS fingerprinting (Layer 1) + full browser engine (Layer 2)
- **No Chrome dependency**: Uses Obscura engine — standalone, undetectable
- **AI captcha solving**: Local ONNX models for text OCR, image classification, slider puzzles
- **MCP server**: Full Model Context Protocol interface for AI agent integration
- **Agent-friendly output**: JSON accessibility tree with `@eN` refs (agent-browser compatible)
- **Cookie persistence**: SQLite store with import/export
- **Human-like interaction**: Bézier mouse curves, realistic typing, natural scrolling
- **Cloudflare/AWS WAF bypass**: TLS fingerprint spoofing + JS challenge solver

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   MCP Server (stdio)                │
├─────────────────────────────────────────────────────┤
│              Unified Cookie Store (SQLite)          │
├─────────────────────────────────────────────────────┤
│                                                     │
│  Layer 1: HTTP Stealth     (cloudscraper approach)  │
│  ├─ TLS fingerprint spoofing (JA3/JA4)              │
│  ├─ JS challenge solver (boa engine)                │
│  └─ Handles basic CF/AWS WAF without browser        │
│                                                     │
│  Layer 2: Full Browser     (nodriver approach)      │
│  ├─ Obscura engine (no Chrome dependency)           │
│  ├─ Direct protocol, no webdriver binary            │
│  ├─ Bézier mouse + realistic typing                 │
│  └─ Accessibility tree → JSON snapshot output       │
│                                                     │
│  Layer 3: Captcha Solver   (pluggable)              │
│  ├─ Local ONNX models (text, image, slider)         │
│  └─ Cloud API fallback (OpenAI, Gemini, etc.)       │
│                                                     │
└─────────────────────────────────────────────────────┘
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `navigate` | Go to URL (auto-selects layer) |
| `click` | Click element by `@eN` ref |
| `fill` | Fill input by ref |
| `type_text` | Type with realistic keystroke timing |
| `snapshot` | Get accessibility tree JSON |
| `get_cookies` | Extract cookies (by domain or all) |
| `set_cookies` | Set cookies for session |
| `screenshot` | Capture page as PNG |
| `solve_captcha` | Trigger AI captcha solver |
| `mouse_move` | Move mouse with Bézier curve |
| `scroll` | Scroll with natural pattern |

## Quick Start

```bash
# Build
cargo build --release

# Run MCP server
./target/release/cloudyab
```

## MCP Configuration

Add to your MCP client config:

```json
{
  "mcpServers": {
    "cloudyab": {
      "command": "./target/release/cloudyab",
      "args": []
    }
  }
}
```

## Project Structure

```
cloudyab/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── types/                    # Shared types (zero logic)
│   ├── core/                     # Orchestration, routing, traits
│   ├── stealth-http/             # TLS fingerprinting, CF solver
│   ├── browser/                  # Obscura engine wrapper
│   ├── solver/                   # ONNX captcha AI (pluggable)
│   ├── cookies/                  # SQLite cookie store
│   ├── human/                    # Bézier mouse, typing, scroll
│   └── snapshot/                 # Accessibility tree → JSON
├── cloudyab-server/              # MCP server binary
└── models/                       # ONNX models (downloaded on first run)
```

## RAM Budget

| Component | RAM |
|-----------|-----|
| Browser engine (Obscura) | ~30 MB |
| HTTP stealth layer | ~20 MB |
| ONNX Runtime + models | ~100-160 MB |
| SQLite + cookies | ~5 MB |
| Page DOM + tree | ~20-50 MB |
| **Total** | **~225-265 MB** |

## License

MIT
