# Roon AI

[![Build](https://github.com/cfogarty1964/Roon-AI/actions/workflows/build.yml/badge.svg?branch=v3)](https://github.com/cfogarty1964/Roon-AI/actions/workflows/build.yml)
[![GitHub Release](https://img.shields.io/github/v/release/cfogarty1964/Roon-AI)](https://github.com/cfogarty1964/Roon-AI/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/cfogarty1964/Roon-AI/total)](https://github.com/cfogarty1964/Roon-AI/releases)

A Roon and UPnP/DLNA hi-fi control bridge with a web UI, conversational AI agent (typed or spoken), library browser, and Claude MCP integration.

Control your music with your voice, a chat message, or a browser — all from one place.

---

## Features

- **Conversational AI (home page)** — type or speak natural-language requests ("play late-night jazz piano on the living room"), hear responses spoken back, see what's playing on the selected zone in a banner with ⏮ / ⏯ / ⏭ buttons. Streaming replies, hands-free mode, persistent multi-turn history, ▶ Play suggestion buttons.
- **Library browser** — browse your Roon library by genre, artist, composer, album; alphabet filter and grid layout for large collections; one-tap playback
- **Persistent default zone** — set a default zone (★) from any zone picker; it pre-selects on every page until changed
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
  roon-ai:
    image: cfogarty1964/roon-ai:latest
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

Pre-built binaries for Linux (x64, arm64, armv7), macOS (universal), and Windows are available on the [Releases](https://github.com/cfogarty1964/Roon-AI/releases) page.

**Windows:**
```powershell
$env:ANTHROPIC_API_KEY="sk-ant-..."   # optional — enables AI chat
$env:RUST_LOG="debug"
.\roon-ai.exe
# Open http://localhost:8088
```

### Synology NAS (DSM 7)

Download the SPK from [Releases](https://github.com/cfogarty1964/Roon-AI/releases):
- `*_apollolake.spk` — Intel x86_64 (DS918+, DS920+, etc.)
- `*_rtd1296.spk` — ARM64 (DS220+, DS420+, etc.)

### QNAP NAS

Download the QPKG from [Releases](https://github.com/cfogarty1964/Roon-AI/releases):
- `*_x86_64.qpkg` — Intel/AMD
- `*_arm_64.qpkg` — ARM64

---

## Configuration

Config directory (Windows default): `%APPDATA%\roon-ai\` (preserved from earlier release; not renamed in the binary rename to avoid breaking existing installs).

Override with `ROON_AI_CONFIG_DIR` env var.

**`config.toml`** (loaded via the `config` crate's `with_name`):
```toml
port = 8088

[roon]
# extension_id = "optional"
# display_name = "optional"

[ai]
api_key = "sk-ant-..."   # Anthropic API key — enables the conversational AI page
```

**Key environment variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `ROON_AI_PORT` | HTTP port | `8088` |
| `ROON_AI_CONFIG_DIR` | Config/state directory | platform default |
| `ANTHROPIC_API_KEY` | Enables AI chat (overrides TOML) | — |
| `RUST_LOG` | Log filter | `roon_ai=debug` |

---

## Conversational AI

The home page (`/`) is the conversational AI surface. It lets you control your music with natural language — typed or spoken. Uses the Claude API (claude-sonnet-4-6) with a multi-turn tool-use loop, streaming replies via SSE, and the browser's built-in speech APIs for voice input and TTS.

**Requirements:** an Anthropic API key ([console.anthropic.com](https://console.anthropic.com/settings/keys)). Set it in the config file or via `ANTHROPIC_API_KEY`. On startup the server logs either:
- `AI chat enabled (Anthropic API key found)`
- `AI chat disabled (set ANTHROPIC_API_KEY to enable)`

**Example queries:**
- "Play the Adagietto from Mahler's 5th"
- "I love that piece — start Roon Radio from it"
- "Queue some late-night jazz piano on the Living Room zone"
- "What is this?" / "Skip this" / "More like this" — uses the now-playing track on the selected zone as implicit context
- "Pause" / "Turn the volume up"

A "Now playing" banner above the chat shows the current track on the selected zone. The right panel shows every tool call the agent makes as it works. Conversation history persists across reloads via `localStorage`.

**Voice in/out:** click 🎤 to dictate a message. Toggle 🔊 Speak to have replies spoken aloud (voice picker on the Settings page; Edge on Windows ships free Microsoft "Online (Natural)" neural voices that sound studio-quality). Toggle 🎙 Hands-free for back-and-forth conversation without touching the keyboard. STT requires Chrome/Edge/Safari (not Firefox); TTS works in all browsers.

---

## Web UI Pages

| Route | Page | Purpose |
|-------|------|---------|
| `/` | Conversational AI | Natural-language chat with voice in/out, streaming, now-playing banner with transport buttons, persistent history |
| `/library` | Library | Browse Roon library with alphabet filter |
| `/settings` | Settings | Adapter enable/disable, voice picker, appearance |

### Default Zone

Any page with a zone picker shows a ☆ button next to the dropdown. Clicking it saves that zone as the default (turns ★ yellow) and pre-selects it on every page load until changed. The preference is stored in `localStorage`.

---

## MCP Server (Claude Integration)

The bridge exposes an MCP endpoint so Claude Code, Claude Desktop, and other MCP clients can control your hi-fi directly.

**Add to `.mcp.json`:**
```json
{
  "mcpServers": {
    "roon-ai": {
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
│              roon-ai.exe  (Axum + Dioxus)                     │
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
.\target\release\roon-ai.exe
```

### Hot Reload (UI development)

```bash
dx serve --platform web --features web --port 8088
```

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
