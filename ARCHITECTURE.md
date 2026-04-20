# Unified Hi-Fi Control — Architecture & Startup Guide

**Version:** v3 (Rust rewrite)  
**Port:** 8088  
**Stack:** Rust + Axum + Dioxus (SSR + WASM) + Tokio + Tailwind CSS

---

## What It Does

A source-agnostic hi-fi audio control bridge. It discovers and controls audio sources across your LAN under a single web UI and Claude AI / MCP interface.

**Supported sources:** Roon · UPnP/DLNA

---

## Runtime Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Browser / Claude AI                        │
│                http://localhost:8088   /mcp endpoint              │
└───────────────────────────┬──────────────────────────────────────┘
                            │ HTTP + SSE
┌───────────────────────────▼──────────────────────────────────────┐
│              unified-hifi-control.exe  (Axum server)              │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                    AdapterCoordinator                     │    │
│  │   starts/stops adapters based on config, handles Ctrl+C  │    │
│  └────────────────────────┬─────────────────────────────────┘    │
│                           │                                        │
│  ┌────────────────────────▼─────────────────────────────────┐    │
│  │                   Tokio Event Bus                         │    │
│  │   ZoneDiscovered · ZoneUpdated · ZoneRemoved             │    │
│  │   NowPlayingChanged · Command · CommandResponse           │    │
│  └──┬──────────────────────────────────────┬────────────────┘    │
│     │                                      │                       │
│  ┌──▼──┐                              ┌───▼────┐                  │
│  │Roon │                              │  UPnP  │                  │
│  │     │                              │        │                  │
│  └──┬──┘                              └───┬────┘                  │
│     │SOOD+WebSocket                       │SSDP+SOAP              │
│                                                                    │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                   ZoneAggregator                            │  │
│  │   single source of truth: HashMap<zone_id, Zone>           │  │
│  └──────────────────────┬─────────────────────────────────────┘  │
│                         │                                          │
│          ┌──────────────┼──────────────┬──────────┐               │
│       ┌──▼──┐      ┌───▼───┐      ┌───▼───┐  ┌───▼───┐          │
│       │ API │      │  SSE  │      │  MCP  │  │  AI   │          │
│       │     │      │/events│      │ /mcp  │  │ chat  │          │
│       └─────┘      └───────┘      └───────┘  └───────┘          │
└──────────────────────────────────────────────────────────────────┘
```

### Zone ID Format

All zones are identified by a prefixed string: `roon:<id>` or `upnp:<uuid>`

### Real-Time Updates (SSE)

The web UI subscribes to `GET /events` (Server-Sent Events). Events include `ZoneDiscovered`, `ZoneUpdated`, `NowPlayingChanged`, `VolumeChanged`, `RoonConnected`, `RoonDisconnected`. Any HTTP client (browser, ESP32, curl) can subscribe.

---

## Local Setup (Windows — Native Binary)

> **Why native binary on Windows?** Docker Desktop on Windows uses a Linux VM, which blocks multicast UDP. Running the native binary puts it directly on your LAN so Roon SOOD discovery and SSDP/UPnP can reach your devices.

### Prerequisites (one-time)

| Tool | Status | Command |
|------|--------|---------|
| Rust 1.95.0 | ✅ installed | `rustc --version` |
| WASM target | ✅ installed | `rustup target list --installed` |
| Dioxus CLI 0.7.3 | ✅ installed | `dx --version` |
| Tailwind CSS binary | ✅ `tailwindcss.exe` in project root | — |

### Build (after code changes)

```bash
# 1. Tailwind CSS  (Windows workaround — v4 CLI has a mkdir bug on existing dirs)
mkdir -p tmp_css
./tailwindcss.exe -i src/input.css -o tmp_css/tailwind.css --content "src/app/**/*.rs"
mv tmp_css/tailwind.css public/tailwind.css
rmdir tmp_css

# 2. WASM bundle
dx build --release --platform web --features web

# 3. Server binary
cargo build --release --features server
```

> First build takes ~5 minutes. Subsequent builds use Cargo's incremental cache.

### Run

```powershell
$env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
```

Open **http://localhost:8088**

Roon SOOD discovery starts automatically. Your Roon Core appears in Zones within seconds. Authorise once in **Roon Settings → Extensions**.

### Stop / Restart

**PowerShell** (recommended):
```powershell
# Stop
Stop-Process -Name 'unified-hifi-control' -Force -ErrorAction SilentlyContinue

# Start
$env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
```

**Git Bash / WSL**:
```bash
# Stop
taskkill //F //IM unified-hifi-control.exe

# Start
RUST_LOG=debug ./target/release/unified-hifi-control.exe
```

> Note: the running binary locks the `.exe` file on Windows. Always stop the process before rebuilding.

### Hot Reload (UI development)

```bash
dx serve --platform web --features web --port 8088
```

Recompiles and refreshes the browser on changes to `src/`.

### Override Config Directory

```powershell
$env:UHC_CONFIG_DIR=".\local-data"; $env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
```

---

## Configuration

Config is stored in `%APPDATA%\unified-hifi-control\unified-hifi\` (Windows default) or the directory set by `UHC_CONFIG_DIR`.

**Main config** (`unified-hifi-control.toml`):
```toml
port = 8088

[roon]
# extension_id and display_name optional

[ai]
api_key = "sk-ant-..."   # Anthropic API key for AI chat
```

**State files**: `app-settings.json`, `roon_state.json`, `knobs.json`

---

## Control Surfaces

| Surface | How |
|---------|-----|
| Web UI | http://localhost:8088 |
| AI Chat | http://localhost:8088/ai — natural language music control |
| Claude AI (MCP) | MCP endpoint at http://localhost:8088/mcp |
| ESP32 knob | Hardware volume/transport knob, managed via `/knobs` page |

### Web UI Pages

| Route | Page |
|-------|------|
| `/` | Zones — all zones with transport + volume controls |
| `/ai` | AI Music Control — natural language chat |
| `/library` | Library — browse Roon library |
| `/knobs` | Knobs — ESP32 firmware management |
| `/settings` | Settings — adapter enable/disable |

### Claude AI / MCP Tools

Add to `.mcp.json`:
```json
{
  "mcpServers": {
    "unified-hifi-control": {
      "type": "http",
      "url": "http://localhost:8088/mcp"
    }
  }
}
```

Available tools: `hifi_zones` · `hifi_now_playing` · `hifi_control` · `hifi_search` · `hifi_play` · `hifi_status`

---

## Key Source Files

| Path | Purpose |
|------|---------|
| [src/main.rs](src/main.rs) | Server entry point |
| [src/app/mod.rs](src/app/mod.rs) | Dioxus UI root + routing |
| [src/adapters/roon.rs](src/adapters/roon.rs) | Roon adapter (~2000 lines) |
| [src/adapters/upnp.rs](src/adapters/upnp.rs) | UPnP/DLNA adapter (~900 lines) |
| [src/coordinator.rs](src/coordinator.rs) | Adapter lifecycle manager |
| [src/aggregator.rs](src/aggregator.rs) | Zone state aggregation |
| [src/ai/mod.rs](src/ai/mod.rs) | AI chat — Anthropic API, tool loop, markdown rendering |
| [src/mcp/mod.rs](src/mcp/mod.rs) | MCP tools (Claude AI) |
| [src/bus/](src/bus/) | Tokio broadcast event bus |
| [src/app/pages/ai_chat.rs](src/app/pages/ai_chat.rs) | AI chat UI page |
| [src/app/pages/library.rs](src/app/pages/library.rs) | Library browser page |
| [src/app/default_zone.rs](src/app/default_zone.rs) | Default-zone context + localStorage persistence |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Internal architecture detail |

---

## Default Zone

A persistent default zone can be set from any page that has a zone picker (`/ai`, `/library`). A ☆ button sits next to the zone `<select>`; clicking it saves the current selection as the default (turns ★ yellow). The choice is stored in `localStorage` under the key `roon-ai-default-zone` and pre-selected automatically on every page load until changed.

The shared state is managed by `DefaultZoneContext` (`src/app/default_zone.rs`), initialised at the app root alongside the theme and settings contexts.

---

*Updated: 2026-04-19 — default zone persistence; Roon + UPnP only; AI chat with markdown rendering*
