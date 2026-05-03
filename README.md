# CloudyAB

<p align="center">
  <img src=".github/res/Cloudy.png" alt="CloudyAB" />
</p>

Stealth headless browser with MCP support and AI captcha solving.

CloudyAB bypasses Cloudflare, AWS WAF, and other anti-bot protections using a multi-layer architecture: fast HTTP-level stealth for simple pages, automatic escalation to a full browser engine for JavaScript-heavy sites, and AI-powered captcha solving when challenges are detected.

## Features

- **Stealth HTTP layer** — TLS fingerprint spoofing (JA3/JA4), Cloudflare JS challenge solving via embedded JS interpreter
- **Browser engine** — Obscura headless browser with built-in anti-detection (stealth mode)
- **Auto-escalation** — starts with fast HTTP, automatically escalates to browser when protection requires it
- **AI captcha solving** — local ONNX models for text OCR, image classification, and slider puzzles
- **Human-like interaction** — Bézier curve mouse movements, realistic typing delays for captcha submission
- **AI browsing agent** — give it a natural language goal, it navigates autonomously using an LLM
- **MCP server** — 8 tools exposed via Model Context Protocol for AI agent integration
- **HTTP task queue** — async API with webhook callbacks, retry logic, task persistence (SQLite)
- **Fully configurable** — TOML config file, enable/disable any subsystem, set API keys

## Quick Start

### One-command setup (recommended)

```bash
# Linux/macOS
chmod +x scripts/setup.sh
./scripts/setup.sh
```

```powershell
# Windows (PowerShell)
.\scripts\setup.ps1
```

The setup script will:
1. Check for Rust toolchain
2. Download Obscura browser automatically
3. Create default config
4. Build in release mode
5. Start CloudyAB

### Manual setup

```bash
# Build
cargo build --release

# Generate default config
./target/release/cloudyab --init

# Edit cloudyab.toml (set browser path, API keys, etc.)
# Then run:
./target/release/cloudyab
```

## Testing

Once CloudyAB is running, test it with the HTTP API:

### Health check

```bash
curl http://localhost:9222/health
```

```powershell
Invoke-RestMethod http://localhost:9222/health
```

### Submit a navigation task

```bash
curl -X POST http://localhost:9222/tasks \
  -H "Content-Type: application/json" \
  -d '{"url": "https://www.storyblocks.com", "snapshot": true}'
```

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:9222/tasks -ContentType 'application/json' -Body '{"url":"https://www.storyblocks.com","snapshot":true}'
```

### Force browser layer (for JS-heavy sites)

```bash
curl -X POST http://localhost:9222/tasks \
  -H "Content-Type: application/json" \
  -d '{"url": "https://www.storyblocks.com", "snapshot": true, "layer": "browser"}'
```

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:9222/tasks -ContentType 'application/json' -Body '{"url":"https://www.storyblocks.com","snapshot":true,"layer":"browser"}'
```

### Check task result

```bash
curl http://localhost:9222/tasks/<task_id>
```

```powershell
Invoke-RestMethod http://localhost:9222/tasks/<task_id>
```

### List all tasks

```bash
curl http://localhost:9222/tasks
```

```powershell
Invoke-RestMethod http://localhost:9222/tasks
```

### Queue statistics

```bash
curl http://localhost:9222/queue/stats
```

```powershell
Invoke-RestMethod http://localhost:9222/queue/stats
```

### Cancel a task

```bash
curl -X DELETE http://localhost:9222/tasks/<task_id>
```

```powershell
Invoke-RestMethod -Method Delete -Uri http://localhost:9222/tasks/<task_id>
```

### Retry a failed task

```bash
curl -X POST http://localhost:9222/tasks/<task_id>/retry
```

```powershell
Invoke-RestMethod -Method Post -Uri http://localhost:9222/tasks/<task_id>/retry
```

## Configuration

CloudyAB loads configuration from `cloudyab.toml` (or path in `CLOUDYAB_CONFIG` env var). See `cloudyab.example.toml` for all options.

Key sections:

```toml
[engine]
auto_escalate = true    # HTTP → browser on failure
timeout_secs = 30
http_port = 9222        # Task queue API port (0 = disabled)

[stealth_http]
enabled = true

[browser]
enabled = true
binary_path = "bin/obscura.exe"  # Or set CLOUDYAB_BROWSER_BIN env var

[solver]
enabled = true
models_dir = "models"

[ai]
enabled = true
provider = "openai"
api_key = "sk-..."
model = "gpt-4o-mini"

[proxy]
url = "socks5://127.0.0.1:1080"
```

## MCP Tools

| Tool | Description |
|------|-------------|
| `navigate` | Navigate to a URL with stealth protection bypass |
| `snapshot` | Get page accessibility tree with @eN element refs |
| `click` | Click an element by ref |
| `fill` | Fill a text input by ref |
| `type_text` | Type with realistic keystroke timing |
| `screenshot` | Capture page as PNG |
| `get_cookies` | Extract session cookies |
| `ai_browse` | Autonomous AI-powered browsing for a goal |

## HTTP Task Queue API

Runs on port 9222 alongside the MCP server.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/tasks` | POST | Submit a new task |
| `/tasks` | GET | List all tasks (filter with `?status=pending`) |
| `/tasks/:id` | GET | Get task status and result |
| `/tasks/:id` | DELETE | Cancel a pending/running task |
| `/tasks/:id/retry` | POST | Retry a failed task |
| `/queue/stats` | GET | Queue statistics |

Task request body:

```json
{
  "url": "https://example.com",
  "layer": "browser",
  "snapshot": true,
  "cookies": true,
  "webhook_url": "https://your-server.com/callback",
  "max_retries": 3
}
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│              MCP Server (stdio)                  │
│              HTTP Task Queue (:9222)             │
├─────────────────────────────────────────────────┤
│              Orchestrator (core)                 │
│         auto-escalation + captcha detect        │
├──────────────────┬──────────────────────────────┤
│  Stealth HTTP    │    Obscura Browser (CDP)     │
│  (Layer 1)       │    (Layer 2)                 │
│  TLS spoofing    │    Built-in stealth          │
│  JS challenges   │    Full JS rendering         │
├──────────────────┴──────────────────────────────┤
│  Captcha Solver  │  Cookie Store  │  Human Sim  │
│  ONNX models     │  SQLite        │  Bézier     │
└──────────────────┴──────────────────────────────┘
```

## Crate Structure

| Crate | Purpose |
|-------|---------|
| `cloudyab-types` | Shared types, DTOs, enums (zero deps) |
| `cloudyab-core` | Orchestration, routing, config, traits |
| `cloudyab-stealth-http` | TLS fingerprinting + JS challenge solver |
| `cloudyab-browser` | Obscura CDP integration with stealth |
| `cloudyab-solver` | ONNX captcha AI (text, image, slider) |
| `cloudyab-cookies` | SQLite cookie persistence |
| `cloudyab-human` | Bézier mouse, realistic keyboard, scroll |
| `cloudyab-snapshot` | Accessibility tree → JSON with @eN refs |
| `cloudyab-server` | MCP binary that wires everything together |

## Requirements

- Rust 1.75+
- Obscura browser (auto-downloaded by setup script, or set `CLOUDYAB_BROWSER_BIN`)
- ONNX model files in `models/` directory for captcha solving (optional)
- OpenAI API key for AI browsing (optional)

## License

MIT
