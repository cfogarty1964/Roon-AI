# Roon AI

[![Build](https://github.com/open-horizon-labs/unified-hifi-control/actions/workflows/build.yml/badge.svg?branch=v3)](https://github.com/open-horizon-labs/unified-hifi-control/actions/workflows/build.yml)
[![GitHub Release](https://img.shields.io/github/v/release/open-horizon-labs/unified-hifi-control)](https://github.com/open-horizon-labs/unified-hifi-control/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/open-horizon-labs/unified-hifi-control/total)](https://github.com/open-horizon-labs/unified-hifi-control/releases)

A Roon and UPnP/DLNA hi-fi control bridge with a web UI, AI natural language chat, library browser, and Claude MCP integration.

Control your music with your voice, a chat message, a hardware knob, or a browser — all from one place.

---

## Features

- **Zones page** — all Roon and UPnP zones at a glance, with transport and volume controls
- **AI Music Control** — type natural language requests ("play late-night jazz piano on the living room zone") and the built-in Claude AI agent searches and plays
- **Library browser** — browse your Roon library by genre, artist, composer, album; alphabet filter and grid layout for large collections; one-tap playback
- **Persistent default zone** — set a default zone (★) from any zone picker; it pre-selects on every page until changed
- **ESP32 Knob** — hardware volume/transport knob with OTA firmware management
- **MCP server** — Claude AI tools for external agents and Claude Code

---

## Supported Sources

| Source | Discovery | Transport | Search |
|--------|-----------|-----------|--------|
| Roon | SOOD | ✅ full | ✅ library + TIDAL + Qobuz |
| UPnP / DLNA | SSDP | ✅ play/pause/vol | — |

---

## Installation

### Docker (Linux host recommended)

`network_mode: host` is required for Roon SOOD and UPnP SSDP multicast discovery. On Windows/macOS Docker Desktop runs inside a Linux VM and blocks multicast — use the native binary instead (see below).

```yaml
# docker-compose.yml
services:
  unified-hifi-control:
    image: muness/unified-hifi-control:latest
    network_mode: host
    volumes:
      - ./data:/data
    environment:
      - CONFIG_DIR=/data
      - ANTHROPIC_API_KEY=sk-ant-...   # optional — enables AI chat
    restart: unless-stopped
```

```bash
docker compose up -d
# Open http://localhost:8088
```

### Native Binary (Windows — recommended for Roon)

Pre-built binaries for Linux (x64, arm64, armv7), macOS (universal), and Windows are available on the [Releases](https://github.com/open-horizon-labs/unified-hifi-control/releases) page.

**Windows:**
```powershell
$env:ANTHROPIC_API_KEY="sk-ant-..."   # optional — enables AI chat
$env:RUST_LOG="debug"
.\unified-hifi-control.exe
# Open http://localhost:8088
```

### Synology NAS (DSM 7)

Download the SPK from [Releases](https://github.com/open-horizon-labs/unified-hifi-control/releases):
- `*_apollolake.spk` — Intel x86_64 (DS918+, DS920+, etc.)
- `*_rtd1296.spk` — ARM64 (DS220+, DS420+, etc.)

### QNAP NAS

Download the QPKG from [Releases](https://github.com/open-horizon-labs/unified-hifi-control/releases):
- `*_x86_64.qpkg` — Intel/AMD
- `*_arm_64.qpkg` — ARM64

---

## Configuration

Config directory (Windows default): `%APPDATA%\unified-hifi-control\unified-hifi\`

Override with `UHC_CONFIG_DIR` env var.

**`unified-hifi-control.toml`:**
```toml
port = 8088

[roon]
# extension_id = "optional"
# display_name = "optional"

[ai]
api_key = "sk-ant-..."   # Anthropic API key — enables the /ai chat page
```

**Key environment variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `UHC_PORT` | HTTP port | `8088` |
| `UHC_CONFIG_DIR` | Config/state directory | platform default |
| `ANTHROPIC_API_KEY` | Enables AI chat (overrides TOML) | — |
| `RUST_LOG` | Log filter | `unified_hifi_control=debug` |
| `FIRMWARE_AUTO_UPDATE` | Knob firmware auto-download | `true` |

---

## AI Music Control

The `/ai` page lets you control your music with natural language. It uses the Claude API (claude-sonnet-4-6) with a multi-turn tool-use loop.

**Requirements:** an Anthropic API key ([console.anthropic.com](https://console.anthropic.com/settings/keys)). Set it in the config file or via `ANTHROPIC_API_KEY`. On startup the server logs either:
- `AI chat enabled (Anthropic API key found)`
- `AI chat disabled (set ANTHROPIC_API_KEY to enable)`

**Example queries:**
- "Play the Adagietto from Mahler's 5th"
- "I love that piece — start Roon Radio from it"
- "Queue some late-night jazz piano on the Living Room zone"
- "Pause" / "Turn the volume up"

The right panel shows every tool call the agent makes as it works.

---

## Web UI Pages

| Route | Page | Purpose |
|-------|------|---------|
| `/` | Zones | All zones, now-playing, transport + volume |
| `/ai` | AI Music Control | Natural language chat |
| `/library` | Library | Browse Roon library with alphabet filter |
| `/knobs` | Knobs | ESP32 roon-knob firmware management |
| `/settings` | Settings | Adapter enable/disable, page visibility |

### Default Zone

Any page with a zone picker shows a ☆ button next to the dropdown. Clicking it saves that zone as the default (turns ★ yellow) and pre-selects it on every page load until changed. The preference is stored in `localStorage`.

---

## MCP Server (Claude Integration)

The bridge exposes an MCP endpoint so Claude Code, Claude Desktop, and other MCP clients can control your hi-fi directly.

**Add to `.mcp.json`:**
```json
{
  "mcpServers": {
    "unified-hifi-control": {
      "type": "http",
      "url": "http://<bridge-host>:8088/mcp"
    }
  }
}
```

**Available tools:**

| Tool | Description |
|------|-------------|
| `hifi_zones` | List all zones across all adapters |
| `hifi_now_playing` | Track, artist, album, volume for a zone |
| `hifi_control` | play / pause / next / prev / volume |
| `hifi_search` | Search library, TIDAL, Qobuz |
| `hifi_play` | Search + play/queue/radio in one call |
| `hifi_status` | Bridge status and connected adapters |

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                   Browser / Claude AI                         │
│           http://localhost:8088   /mcp endpoint               │
└────────────────────────┬─────────────────────────────────────┘
                         │ HTTP + SSE
┌────────────────────────▼─────────────────────────────────────┐
│           unified-hifi-control.exe  (Axum + Dioxus)           │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │               Tokio Event Bus                         │    │
│  │  ZoneDiscovered · ZoneUpdated · NowPlayingChanged     │    │
│  └──────────────┬────────────────────────┬──────────────┘    │
│                 │                        │                     │
│            ┌───▼───┐               ┌────▼───┐                │
│            │ Roon  │               │  UPnP  │                │
│            │ SOOD  │               │  SSDP  │                │
│            └───────┘               └────────┘                │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │               ZoneAggregator                          │    │
│  └──────┬──────────────┬──────────────┬──────────┬──────┘    │
│      ┌──▼──┐       ┌───▼──┐      ┌───▼──┐   ┌───▼──┐        │
│      │ API │       │ SSE  │      │ MCP  │   │  AI  │        │
│      └─────┘       └──────┘      └──────┘   └──────┘        │
└──────────────────────────────────────────────────────────────┘
```

---

## Building from Source

### Prerequisites (one-time)

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --locked --version 0.7.3
# Download tailwindcss.exe (Windows) from github.com/tailwindlabs/tailwindcss/releases
```

### Build (Windows)

```bash
# 1. Tailwind CSS  (v4 CLI has a mkdir bug on Windows — use tmp dir)
mkdir -p tmp_css
./tailwindcss.exe -i src/input.css -o tmp_css/tailwind.css --content "src/app/**/*.rs"
mv tmp_css/tailwind.css public/tailwind.css
rmdir tmp_css

# 2. WASM bundle + server binary
dx build --release --platform web --features web
cargo build --release --features server
```

> Always run both `dx build` and `cargo build`. `dx build` produces the WASM; `cargo build` embeds it into the final `.exe`. Running only one leaves the other stale.

### Run

```powershell
$env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
```

### Hot Reload (UI development)

```bash
dx serve --platform web --features web --port 8088
```

---

## roon-knob Firmware

The bridge automatically downloads new [roon-knob](https://github.com/muness/roon-knob) firmware from GitHub every 6 hours. Knobs check `/firmware/version` on startup and OTA-update directly from the bridge.

---

## Version History

| Version | Stack | Notes |
|---------|-------|-------|
| **v1** | Node.js | Proof of concept |
| **v2** | Node.js | Production release — multi-backend, in-memory event bus |
| **v3** | Rust | Complete rewrite — Roon + UPnP, AI chat, library browser, single static binary (~15 MB RAM) |

---

## License

[PolyForm Noncommercial 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0/)
