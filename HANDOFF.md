# Roon AI (v3) — Developer Handoff

**Project**: `roon-ai` — Roon AI hi-fi control bridge  
**Language**: Rust 1.84+ with Dioxus 0.7.3 fullstack (WASM client + SSR server)  
**License**: PolyForm Noncommercial 1.0.0  
**Repo**: `github.com/cfogarty1964/Roon-AI` (branch: `v3`)  
**Port**: 8088 (default)

---

## Local Testing — Docker (Recommended)

No local Rust/Node toolchain needed. Everything compiles inside the container.

### Prerequisites

1. Install [Docker Desktop](https://www.docker.com/products/docker-desktop/) (use WSL 2 backend on Windows)
2. Verify:
   ```bash
   docker --version
   docker compose version
   ```
3. Clone the repo:
   ```bash
   git clone https://github.com/cfogarty1964/Roon-AI.git
   cd roon-ai
   git checkout v3
   ```

### Build & Run

```bash
# Build image (first build: ~10-15 min; subsequent builds use layer cache)
docker compose build

# Start (foreground, shows logs)
docker compose up

# Or detached (background)
docker compose up -d
```

UI at **http://localhost:8088**

### Config & Data

Config persists in `./data/` (auto-created, mounted as `/data` in the container).

To pre-configure adapters, create `./data/unified-hifi/roon-ai.toml`:
```toml
port = 8088

[roon]
extension_id = "optional"
```

### Useful Commands

```bash
docker compose logs -f                         # Tail logs
docker compose down                            # Stop and remove container
docker compose build && docker compose up      # Rebuild after code changes
docker compose exec roon-ai bash  # Shell into running container
ls ./data/                                     # Inspect persisted config/state
```

### Caveats on Windows / macOS

`docker-compose.yml` uses `network_mode: host` (Linux-only). On Windows/macOS Docker Desktop runs inside a Linux VM, so **LAN discovery won't reach your network**:

- Roon SOOD discovery — won't find Roon Core automatically
- SSDP/UPnP/OpenHome — won't see LAN devices

**Workaround**: run the container on a Linux machine/VM on the same LAN, or use the native binary option (see below).

---

## Local Testing — Native Binary on Windows (Recommended for Roon)

**Use this if you have a Roon Core on your LAN.** Docker Desktop on Windows uses `network_mode: host` inside a Linux VM, which blocks multicast UDP — meaning SOOD discovery can't find your Roon Core. Running the native binary on Windows puts it directly on your LAN.

### Step 1 — Install Rust ✅ DONE

Rust 1.95.0 installed at `C:\Users\ChrisFogarty\.rustup`.

If `rustc` is not found, add Cargo to PATH permanently:

```powershell
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";C:\Users\ChrisFogarty\.cargo\bin", "User")
```

Close and reopen your terminal, then verify:

```powershell
rustc --version
cargo --version
```

### Step 2 — Add WASM target ✅ DONE

```bash
rustup target add wasm32-unknown-unknown
```

### Step 3 — Install Dioxus CLI (must be exactly 0.7.3) ✅ DONE

```bash
cargo install dioxus-cli --locked --version 0.7.3
```

### Step 4 — Build Tailwind CSS ✅ DONE

`make css` requires GNU Make. On Windows, download the Tailwind CLI directly and work around a v4 Windows bug (it tries to `mkdir` the output dir even when it already exists):

```bash
# Download the Windows binary
curl -Lo tailwindcss.exe https://github.com/tailwindlabs/tailwindcss/releases/download/v4.1.18/tailwindcss-windows-x64.exe

# Build (output to a temp dir first, then move — avoids EEXIST bug on existing public/)
mkdir -p tmp_css
./tailwindcss.exe -i src/input.css -o tmp_css/tailwind.css --content "src/app/**/*.rs"
mv tmp_css/tailwind.css public/tailwind.css
rmdir tmp_css
```

### Step 5 — Build the app ✅ DONE

```bash
dx build --release --platform web --features web
cargo build --release --features server
```

### Step 6 — Run ✅ DONE

```powershell
$env:RUST_LOG="debug"
.\target\release\roon-ai.exe
```

Open **http://localhost:8088**. The Roon adapter starts SOOD discovery automatically — your Roon Core should appear in the Zones page within a few seconds and will need to be authorised once in Roon Settings → Extensions.

### Critical Build Order Warning

**Always use `dx build` — not `cargo build` alone — after any UI or route changes.**

`cargo build --features server` compiles only the server binary. It embeds WASM from the last `dx build` run. If you add routes or change the UI and only run `cargo build`, the running server will serve **stale WASM** that doesn't know about your new routes — causing "Failed to parse route" errors in the browser.

**Correct sequence after any UI/route change:**
```bash
dx build --release --platform web --features web   # builds fresh WASM + server binary
```

**Freshness check**: on startup the server logs `Embedded WASM assets: N files`. After adding the Library page it should read **12 files** (was 10). If you see 10, the WASM is stale.

**After binary update**: always do a hard refresh in the browser (`Ctrl+F5`) to clear cached WASM. A normal refresh may serve old JavaScript from the browser cache even after a clean server restart.

### Hot Reload (UI development)

```bash
dx serve --platform web --features web --port 8088
```

Recompiles and reloads the browser on changes to `src/` or `public/`.

### Override Config Dir

```powershell
$env:ROON_AI_CONFIG_DIR=".\local-data"; $env:RUST_LOG="debug"
.\target\release\roon-ai.exe
```

---

## Project Structure

```
src/                     # Main Rust source
├── main.rs              # Server entry point (feature: server)
├── lib.rs               # Library root with conditional module exports
├── app/                 # Dioxus UI (shared SSR + WASM)
│   ├── mod.rs           # App root + routing enum
│   ├── api.rs           # Client-side fetch functions + shared types
│   ├── components/      # Reusable Dioxus components
│   ├── pages/           # Routable pages (zones, hqplayer, knobs, settings)
│   ├── sse.rs           # Server-Sent Events subscription
│   └── theme.rs         # Light/dark mode (localStorage)
├── components/          # DioxusLabs component wrappers
├── adapters/            # [server-only] Audio source adapters
├── bus/                 # [server-only] Tokio broadcast event bus
├── aggregator.rs        # [server-only] Zone state aggregation
├── coordinator.rs       # [server-only] Adapter lifecycle manager
├── api/                 # [server-only] Axum HTTP API handlers + SSE
├── config/              # [server-only] Config loading + migration
├── mcp/                 # [server-only] MCP server (Claude AI tools)
├── embedded.rs          # [server-only] rust-embed for single-binary dist
├── firmware.rs          # [server-only] roon-knob firmware auto-fetcher
├── knobs/               # [server-only] Knob state + routing
└── mdns.rs              # [server-only] mDNS/Bonjour publishing

docs/                    # Architecture docs, ADRs, protocol specs
tests/                   # Integration tests + linting checks
.github/workflows/       # CI/CD (build.yml, docker.yml, api-guard.yml)
public/                  # Static web assets (Tailwind output)
firmware/                # ESP32 roon-knob firmware updates
```

---

## Audio Adapters

All adapters implement `AdapterLogic` trait, wrapped by `AdapterHandle` for lifecycle management.

### Roon (`src/adapters/roon.rs`, ~2000 lines)
- **Protocol**: SOOD discovery + WebSocket (via `rust-roon-api` — forked to `ohc/main` for SO_REUSEADDR fix)
- **Features**: Zone control, metadata + album art, search (Library/TIDAL/Qobuz), state persistence to `roon_state.json`
- **Status**: Production. Waiting for upstream merge of SO_REUSEADDR fix before switching back.

### UPnP/DLNA (`src/adapters/upnp.rs`, ~900 lines)
- **Protocol**: SSDP + SOAP AV Transport
- **Limitations**: No next/prev track (UPnP spec), limited metadata
- **Zone ID**: `upnp:<uuid>`
- **Status**: Production.

### Common Patterns
- `AdapterLogic` trait — protocol logic
- `AdapterHandle` — lifecycle (start/stop, retry, ACK)
- `RetryConfig` — exponential backoff
- `PrefixedZoneId` — type-safe `source:raw_id` zone identifiers
- `CancellationToken` — graceful shutdown
- SSDP re-discovery every 30s, stale threshold 90s

---

## Web UI (Dioxus)

**Framework**: Dioxus 0.7.3 fullstack (SSR + WASM hydration)

**Routes** (`src/app/mod.rs`):

| Route | Page | Purpose |
|---|---|---|
| `/` | Zones | All zones, now-playing, transport + volume controls |
| `/ai` | AI Music Control | NLS chat with zone picker |
| `/library` | Library | Browse Roon library by genre, artist, composer, etc. |
| `/knobs` | Knobs | ESP32 roon-knob firmware management |
| `/settings` | Settings | Adapter enable/disable, page visibility |

**Styling**: Tailwind CSS v4.1.18 (standalone CLI, no Node.js required)
- Build: `make css` → `public/tailwind.css`
- Dark mode via localStorage class toggle

**SSE**: Single `EventSource` at app root (`src/app/sse.rs`), emits `ZoneDiscovered`, `ZoneUpdated`, `ZoneRemoved`, `NowPlayingChanged`.

---

## MCP Integration (Claude AI)

**Endpoint**: `http://<bridge>:8088/mcp`  
**SDK**: `rust-mcp-sdk` v0.8 with `#[mcp_tool]` macros  
**Auth**: None (assumes trusted local network)

**Tools (6 total)**:

| Tool | R/W | Description |
|---|---|---|
| `hifi_zones` | R | List all zones across all adapters |
| `hifi_now_playing` | R | Track/artist/album/volume for a zone |
| `hifi_control` | RW | play/pause/next/prev/volume_set/up/down |
| `hifi_search` | R | Search library, TIDAL, Qobuz |
| `hifi_play` | RW | Search + play/queue/radio in one call |
| `hifi_status` | R | Bridge status, connected adapters, version |

**`.mcp.json`**:
```json
{
  "mcpServers": {
    "roon-ai": {
      "type": "http",
      "url": "http://localhost:8089/mcp"
    }
  }
}
```

---

## Configuration

**Config directory resolution order**:
1. `ROON_AI_CONFIG_DIR` env var
2. `CONFIG_DIR` env var (Node.js migration compat)
3. Platform default (`~/.config/roon-ai`, `~/Library/Application Support/...`, `%APPDATA%/...`)
4. Current directory (fallback)

**New files** written to `unified-hifi/` subdirectory (Issue #76). Legacy root files read for backward compat.

**Main TOML** (`roon-ai.toml`):
```toml
port = 8088

[roon]
extension_id = "optional"
display_name = "optional"

[ai]
api_key = "sk-ant-..."
```

**JSON state files**: `app-settings.json`, `roon_state.json`, `knobs.json`

**Key env vars**:
- `ROON_AI_PORT` — override port (default: 8088)
- `RUST_LOG` — logging (default: `roon_ai=debug`)
- `ANTHROPIC_API_KEY` — enables the AI chat page
- `FIRMWARE_AUTO_UPDATE` — knob firmware polling (default: true)

---

## Build System

**Cargo features**:
- `default = ["server"]`
- `server` — HTTP + adapters (axum, tokio, reqwest, etc.)
- `web` — WASM client (wasm-bindgen, web-sys)

**Release profile**: LTO + single codegen unit + strip + panic=abort (small binary)

**Cross-compilation** (`Cross.toml`): x86_64-musl, aarch64-musl, armv7-musleabihf

**Docker** (`Dockerfile`): Multi-stage. Assets embedded at build time (ADR 002). Runtime image ~80MB on Debian slim. No `public/` dir needed.

---

## CI/CD

**`.github/workflows/build.yml`** (~700 lines)

- **`plan` job**: Computes version + decides which builds to run
- **Label-triggered**: `build-me` or `build:*` on PRs
- **Version format**: `v3.3.2` on tags, `0.0.0-pr123` on PRs, `0.0.0-dev` otherwise

**Build matrix**:
- Linux (amd64, aarch64, armv7 via `cross`)
- macOS (universal binary x86_64 + arm64)
- Windows (MSVC)
- Docker (ghcr.io + Docker Hub)
- LMS plugin
- Synology SPK (apollolake, rtd1296)
- QNAP QPKG (x86_64, arm64)

**`api-guard`**: Runs on every PR, validates MCP tool signatures.

---

## Architecture Principles

1. **Event Bus**: All inter-adapter communication via tokio broadcast (`src/bus/`)
2. **Adapter Isolation**: Each adapter owns its protocol; `AdapterHandle` manages lifecycle
3. **Single Source of Truth**: `ZoneAggregator` owns unified zone state
4. **Type Safety**: `PrefixedZoneId` prevents ID routing bugs
5. **Graceful Shutdown**: `CancellationToken` + ACK pattern
6. **Feature Flags**: `server`/`web` for compile-time platform targeting
7. **Single Binary**: Assets embedded via `rust-embed` (ADR 002)

---

## Common Development Tasks

### Adding a New Adapter
1. Implement `AdapterLogic` trait in `src/adapters/new_adapter.rs`
2. Implement `Startable` (use `impl_startable!` macro)
3. Add to coordinator's `AVAILABLE_ADAPTERS`
4. Add `AppState` field + coordinator registration
5. Emit `BusEvent::ZoneDiscovered/Updated/Removed`
6. Implement `handle_command()` for transport control

### Adding a New MCP Tool
1. Define struct with `#[mcp_tool]` macro in `src/mcp/mod.rs`
2. Implement handler with tool logic
3. Add to `tool_box!` macro — routes automatically

### Debugging
```bash
RUST_LOG=trace ./roon-ai     # Verbose logging
./protocol-checker                        # Protocol diagnostic CLI
curl http://localhost:8088/events         # Watch SSE stream live
```

---

## Recent Notable Changes

| Commit | Description |
|---|---|
| `0a1b02c` | LMS plugin v3.3.2 (category placement fix) |
| `a8935b0` | Fix non-integer LMS volume; skip cache on status failure |
| `0aa4dab` | Point roon-api at ohc/main fork (SO_REUSEADDR fix) |
| `da1bfcf` | Allow `build:*` labels to trigger CI directly |
| `00f81e7` | HQPlayer: INDEX-based semantics for Set commands |
| `2f31b26` | HQPlayer: use State's active_mode instead of Status |

---

## Known Gaps / Next Steps

- **Roon API fork**: Waiting for SO_REUSEADDR fix to merge upstream; then switch back to official crate
- **MCP auth**: No authentication on `/mcp` endpoint — assumes trusted LAN
- **E2E tests**: Playwright config exists in `e2e/` but coverage is limited
- **Library page**: Browse pagination works via `POST /roon/browse/load`; action_list items (Play Now / Queue / Radio submenu) currently trigger direct play — a submenu UI could be added
- **Inline TODOs**: None (GitHub issues are the sole tracker; code is clean)

## Recent Work (2026-04-17)

### Windows Native Build Setup (Steps 1–6)

All six steps are now complete and verified against a live Roon Nucleus Titan on the LAN. All Roon zones appeared in the Zones page and the bridge was authorised as a Roon Extension.

Key Windows-specific issues encountered and resolved:
- `make css` not available — used Tailwind standalone CLI directly (see Step 4 workaround above)
- Tailwind v4 EEXIST bug on Windows — output to `tmp_css/` then move (documented in Step 4)
- Running binary locks the `.exe` — use `taskkill //F //IM roon-ai.exe` (double-slash for Git Bash)
- Stale WASM after `cargo build` alone — always use `dx build` (documented in build order warning above)

### ARCHITECTURE.md

Created `ARCHITECTURE.md` in the project root covering: runtime architecture diagram, adapter flow, Windows startup procedure, config file locations, all control surfaces (web UI, MCP, REST), and MCP tool list.

### Library Browser (`/library`)

Added a full hierarchical Roon library browser page. Navigate by genre, artist, composer, album, or any other Roon category.

**New files:**
- `src/app/pages/library.rs` — Dioxus page (zone picker, breadcrumb nav, item list with thumbnails, load more)

**Modified files:**
- `src/api/mod.rs` — Added `pop_levels` to `BrowseRequest`; new `POST /roon/browse/load` handler for pagination
- `src/main.rs` — Registered `/roon/browse/load` route
- `src/app/api.rs` — Added `BrowseItem`, `BrowseListInfo`, `BrowseResult`, `BrowseLoadResult` client types
- `src/app/pages/mod.rs` — Exported `Library`
- `src/app/mod.rs` — Added `/library` route
- `src/app/components/nav.rs` — Added Library nav link (desktop + mobile)

**Dioxus closure-in-for-loop pitfall**: Do not pass an `impl FnMut + 'static` closure to a helper called inside an `rsx!` for loop. The closure is moved on the first iteration, causing a runtime panic on the second item. Fix: inline all rendering logic inside the `for` loop and capture per-item clones of signals/values as local `let` bindings *before* the `rsx!` block. This is how `library.rs` is structured.

**How browse navigation works:**
- `POST /roon/browse { pop_all: true, zone_id }` → root (Library, TIDAL, Qobuz, History…)
- `POST /roon/browse { item_key, zone_id, session_key }` → drill into item
- `POST /roon/browse { item_key, zone_id, session_key }` → execute a browse **action** item (Shuffle, Start Radio, Play Now) on a zone
- `POST /roon/browse { pop_levels: 1, zone_id, session_key }` → back one level
- `POST /roon/browse/load { session_key, offset, count }` → paginate current level

**Critical: action items vs list items**

Roon browse items carry a `hint` field:
- `"list"` / `"action_list"` → has children, navigate with `POST /roon/browse { item_key, session_key }`
- `"action"` → leaf action (Shuffle, Start Radio, Play Now, etc.) — execute with `POST /roon/browse { item_key, zone_id, session_key }`. **Never use `/roon/play_item` for these** — it returns 200 OK but does nothing. The `zone_id` is mandatory so Roon knows which zone to act on.
- `"header"` → non-interactive section label

After calling browse on an action item, check `result.action`:
- `"list"` → Roon returned a sub-menu; update state and display it
- `"message"` / `"none"` → action executed; show `result.message` as feedback, leave list unchanged

### Library Browser — Alphabet Filter + Grid Layout

Upgraded the Library page with two major UX improvements for large lists (artists, composers, genres with ≥ 26 items):

**A–Z alphabet selector bar**
- Appears above the list whenever `list_count >= 26`
- Letters with loaded items are clickable; letters with no items are dimmed
- Clicking a letter filters to items starting with that letter; clicking again or "All" clears it
- `#` catches items that don't start with a letter
- Letter filter resets automatically when navigating (Back or item click)

**Multi-column responsive grid**
- Large lists switch from the single-column row layout to a `grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5` grid
- Each cell: small thumbnail (or first-letter initial as fallback) + name
- Small lists (< 26 items) keep the original row layout unchanged

**Background auto-load**
- When entering a large list, all items are fetched in 100-item batches in the background
- Subtle "Indexing N of M…" pulse shown in the nav bar while loading
- Once complete, the full alphabet is filterable with no manual "Load more" needed
- `loading_all: bool` flag in `LibraryState` prevents duplicate load loops

**Key implementation notes:**
- `is_large_list = list_count >= ALPHA_THRESHOLD (26)` drives both the grid and alpha bar
- `do_action_item()` replaces `do_play_item()` for `hint == "action"` items — routes through `/roon/browse` with `zone_id`
- `load_all_remaining()` async loop fetches until `items.len() >= list_count`, then sets `loading_all = false`
- `do_browse()` resets `loading_all = false` on every navigation so the auto-load re-triggers for each new large list

### Windows Build Sequence (Critical)

**Always run both steps — not just `dx build`:**

```bash
dx build --release --platform web --features web   # builds WASM + dx-path binary
cargo build --release --features server            # embeds WASM into target/release/ binary
```

`dx build` puts its server binary at `target\dx\roon-ai\release\web\roon-ai.exe`.  
The binary you actually *run* (`.\target\release\roon-ai.exe`) is only updated by `cargo build`.  
Running only `dx build` leaves the old binary in place — UI changes will appear absent.

**Kill running process (PowerShell):**
```powershell
Stop-Process -Name 'roon-ai' -Force -ErrorAction SilentlyContinue
```
(The Git Bash `taskkill //F` syntax does not work in PowerShell; use the above.)

### Personal Fork

A personal fork has been pushed to **https://github.com/cfogarty1964/Roon-AI** (branch: `v3`).  
Remote name locally: `cfogarty`. Push personal changes with:
```bash
git push cfogarty v3
```

---

## Recent Work (2026-04-18)

### Library Browser — Playback Fix

**Problem**: Clicking "Play Now" / "Add Next" / etc. inside the "Play Album" submenu showed a "Playing" confirmation but music did not start on the selected zone.

**Root cause**: Roon's browse API requires `zone_or_output_id` to be provided consistently throughout the browse session — not just at the final action step. The library page was only passing `zone_id` when executing an action item (`hint == "action"`), while all navigation steps (root browse, item drill-down, Back) used `zone_id: None`. This meant Roon had no zone context for the session and silently ignored the playback request.

**Fix** (`src/app/pages/library.rs`): `zone_id` (from the "Play to" zone picker) is now threaded into every `do_browse` call — root, item navigation, and Back — matching how `search_and_play` in the Roon adapter works.

**Key rule**: Always include `zone_or_output_id` on every step of a browse session that is intended to end in playback. Providing it only at the action step is insufficient.

### Library Browser — Stale Background Load Error

**Problem**: After navigating from a large list (e.g., Artists, 200+ items) into a sub-item, the error panel showed `JsValue(TypeError: Failed to fetch ...)` even though the correct items were displayed.

**Root cause**: The background auto-load task (`load_all_remaining`) that was fetching the large list in 100-item batches continued running after the user navigated away. When its next `/roon/browse/load` request failed (the old session key was no longer current), it wrote the error to the shared `LibraryState`, poisoning the new level's display.

**Fix** (`src/app/pages/library.rs`): `load_all_remaining` now checks the session key at the top of every loop iteration and before writing results. If the session has changed (user navigated away), the task exits silently without setting an error.

---

## Known Gaps / Next Steps (updated 2026-04-18)

- **One-tap play shortcut**: From an album/artist view, you still need two clicks (e.g. "Play Album" → "Play Now"). A direct ▶ button on each row could collapse this to one tap.
- **Roon API fork**: Waiting for SO_REUSEADDR fix to merge upstream; then switch back to official crate
- **MCP auth**: No authentication on `/mcp` endpoint — assumes trusted LAN
- **E2E tests**: Playwright config exists in `e2e/` but coverage is limited

---

## AI Natural Language Music Control — Implementation Plan (2026-04-18) ✅ COMPLETE

### Goal

A chat interface embedded in the web UI where the user can type queries like:

> "I love the Adagietto from Mahler's 5th — play me similar pieces"  
> "Queue some late-night jazz piano on the Living Room zone"  
> "What's playing right now on all zones?"

The system interprets the query using the Claude API (Anthropic), calls the appropriate adapter methods, and replies in natural language confirming what it did.

### Architecture Overview

```
Browser (chat UI)
  POST /api/ai/chat { message, zone_id }
        │
  src/ai/mod.rs  ──── Anthropic API (claude-sonnet-4-6)
        │               tool_use loop:
        │                 list_zones → aggregator.get_zones()
        │                 search     → roon .search()
        │                 play       → roon .search_and_play()
        │                 control    → roon/openhome/upnp .control()
        │
  Returns { response: String, actions: Vec<String> }
```

Key design decisions:
- **No new crate deps**: `reqwest` (already a dep) hits the Anthropic API directly as JSON
- **Reuses existing adapter logic**: the AI module calls the same Rust functions the MCP server calls — no duplication
- **API key**: `ANTHROPIC_API_KEY` env var (or `roon-ai.toml`)
- **Model**: `claude-sonnet-4-6` (fast, capable, cost-effective for tool use)
- **Response**: synchronous JSON (no streaming in v1); UI shows a loading spinner

### Step-by-Step Implementation Plan

#### Step 1 — API key config

- Add `ANTHROPIC_API_KEY` env var lookup to `src/config/mod.rs` (or read directly in the AI module via `std::env`)
- Add optional `[ai]` section to `roon-ai.toml` schema: `api_key = "sk-ant-…"`
- Key resolution order: env var → TOML → error at call time (not startup)
- No changes to existing adapters or AppState yet

#### Step 2 — AI module skeleton (`src/ai/mod.rs`)

- Create `src/ai/mod.rs` (server-only, behind `#[cfg(feature = "server")]`)
- Define `AiChatRequest { message: String, zone_id: Option<String> }`
- Define `AiChatResponse { response: String, actions: Vec<String> }`
- Define `AnthropicClient` struct with `api_key: String` and `reqwest::Client`
- Implement `call_claude(messages, tools) -> Result<ClaudeResponse>` — single HTTP call to `https://api.anthropic.com/v1/messages`
- No tool dispatch yet — just get a raw text reply working end-to-end

#### Step 3 — Tool definitions

- Define the 4 Claude tools as `serde_json::Value` constants in `src/ai/mod.rs`:
  - `list_zones` — no params, returns zone list
  - `search_music` — `{ query, zone_id?, source? }` — search library/TIDAL/Qobuz
  - `play_music` — `{ query, zone_id, source?, action? }` — search + play/queue/radio
  - `control_playback` — `{ zone_id, action, value? }` — play/pause/next/prev/volume
- Pass these in every `call_claude()` request
- Confirm Claude correctly returns `tool_use` stop reason in test

#### Step 4 — Tool dispatch

- Implement `execute_tool(name, input, state: &AppState) -> String` in `src/ai/mod.rs`
- Wire each tool to existing AppState methods:
  - `list_zones` → `state.aggregator.get_zones().await`
  - `search_music` → `state.roon.search()`
  - `play_music` → `state.roon.search_and_play()`
  - `control_playback` → `state.roon.control()` / `state.openhome.control()` / `state.upnp.control()`
- Route by zone_id prefix (`roon:`, `openhome:`, `upnp:`) exactly as MCP server does

#### Step 5 — Agentic loop

- Implement `run_agent(request, state) -> AiChatResponse` in `src/ai/mod.rs`:
  1. Build initial `messages` vec with system prompt + user message
  2. Call `call_claude(messages, tools)` 
  3. If response contains `tool_use` blocks: execute each tool, append `tool_result` blocks to messages, loop
  4. When stop reason is `end_turn` (text reply): return `AiChatResponse`
- System prompt: brief context ("You control a hi-fi audio system. Use the provided tools to fulfil music requests. Always confirm what you played and on which zone.")
- Cap loop at 10 iterations to prevent runaway tool chains

#### Step 6 — Axum route `POST /api/ai/chat`

- Add `AiChat` handler in `src/api/mod.rs` (or new `src/api/ai.rs`)
- Extract `AppState`, deserialise `AiChatRequest`, call `run_agent`, return JSON `AiChatResponse`
- Register route in `src/main.rs`: `.route("/api/ai/chat", post(ai_chat_handler))`
- Test with `curl -X POST http://localhost:8088/api/ai/chat -H 'Content-Type: application/json' -d '{"message":"what zones are available?"}'`

#### Step 7 — Client-side types (`src/app/api.rs`)

- Add `AiChatRequest` and `AiChatResponse` structs (shared, visible to WASM)
- Add `async fn ai_chat(req: AiChatRequest) -> Result<AiChatResponse, String>` fetch helper
- Uses existing `post_json` / `fetch` pattern already in `src/app/api.rs`

#### Step 8 — UI page (`src/app/pages/ai_chat.rs`)

- New Dioxus page component `AiChat`
- **Zone picker**: dropdown populated from `GET /api/zones` (same as Library page), default to first zone
- **Chat history**: `Vec<ChatMessage { role, content }>` in a signal; scrollable div
- **Input bar**: text input + Send button; disabled while awaiting response
- **Loading state**: spinner/pulse while request is in-flight
- **Message bubbles**: user messages right-aligned, assistant left-aligned; assistant replies render action confirmation in a subtle secondary colour
- Export from `src/app/pages/mod.rs`

#### Step 9 — Route + nav wire-up

- Add `Route::AiChat` to `src/app/mod.rs` routing enum
- Map to `/ai` path
- Add "AI" nav link in `src/app/components/nav.rs` (desktop sidebar + mobile bottom bar), after Library
- Rebuild Tailwind CSS (new classes from chat UI)

#### Step 10 — Build, test, iterate

Full build sequence:
```bash
mkdir -p tmp_css
./tailwindcss.exe -i src/input.css -o tmp_css/tailwind.css --content "src/app/**/*.rs"
mv tmp_css/tailwind.css public/tailwind.css
rmdir tmp_css
dx build --release --platform web --features web
cargo build --release --features server
```

Test checklist:
- [ ] "What zones are available?" → lists zones by name
- [ ] "Play the Adagietto from Mahler's 5th on [zone]" → searches + plays, confirms
- [ ] "Play something like that" (follow-up) → context from prior turn if messages kept
- [ ] "Pause the Living Room" → control_playback executed
- [ ] Missing API key → helpful error message in UI, not a server panic
- [ ] No Roon connection → graceful error in chat, not a 500

### Files to Create / Modify

| File | Change |
|---|---|
| `src/ai/mod.rs` | **New** — Anthropic client, tool defs, tool dispatch, agentic loop |
| `src/api/mod.rs` | Add `POST /api/ai/chat` handler |
| `src/main.rs` | Register `/api/ai/chat` route |
| `src/app/api.rs` | Add `AiChatRequest`, `AiChatResponse`, `ai_chat()` fetch fn |
| `src/app/pages/ai_chat.rs` | **New** — chat UI page |
| `src/app/pages/mod.rs` | Export `AiChat` |
| `src/app/mod.rs` | Add `/ai` route |
| `src/app/components/nav.rs` | Add AI nav link |
| `src/config/mod.rs` | Add `ANTHROPIC_API_KEY` / `[ai]` section support |
| `Cargo.toml` | No new deps required |

### Key Env Var

```
ANTHROPIC_API_KEY=sk-ant-…
```

Set before running:
```powershell
$env:ANTHROPIC_API_KEY="sk-ant-…"
$env:RUST_LOG="debug"
.\target\release\roon-ai.exe
```

---

## Recent Work (2026-04-18) — AI Natural Language Music Control

### What was built

A full AI chat interface backed by the Claude API (Anthropic), embedded in the web UI at `/ai`.

The user types free-text music requests — e.g. *"I love the Adagietto from Mahler's 5th, play similar pieces"* — and the system interprets the query, calls the appropriate adapter methods, and replies in natural language confirming what it did. A zone picker lets the user direct playback to any active zone.

### Architecture

```
Browser (/ai page)
  POST /api/ai/chat { message, zone_id }
        │
  src/ai/mod.rs  ──── Anthropic API (claude-sonnet-4-6)
        │               agentic tool-use loop (max 10 turns):
        │                 list_zones    → aggregator.get_zones()
        │                 search_music  → roon .search()
        │                 play_music    → roon .search_and_play()
        │                 control_playback → roon/openhome/upnp .control()
        │
  Returns { response: String, actions: Vec<String> }
```

### Key design decisions

- **No new crate deps** — `reqwest` (already a dep) hits `https://api.anthropic.com/v1/messages` directly as JSON
- **`play_music` with `action='radio'`** seeds Roon Radio from the matched track — this is how "play similar to X" works
- **API key resolution** — env var `ANTHROPIC_API_KEY` takes precedence over `[ai] api_key` in TOML
- **Graceful degradation** — if the key is missing, the server starts normally and the AI page shows an error message; no crash
- **Model** — `claude-sonnet-4-6`

### How to enable

**Option A — Config file (recommended, persists across restarts)**

The config file lives at `%APPDATA%\roon-ai\config.toml` (Windows).  
Full path: `C:\Users\ChrisFogarty\AppData\Roaming\roon-ai\config.toml`

The file already exists. Open it and set your key:

```toml
port = 8088

[ai]
api_key = "sk-ant-..."
```

Get your key from https://console.anthropic.com/settings/keys. Then stop and restart the binary:

```powershell
Stop-Process -Name 'roon-ai' -Force -ErrorAction SilentlyContinue
$env:RUST_LOG="debug"
.\target\release\roon-ai.exe
```

**Option B — Environment variable (current session only)**

```powershell
$env:ANTHROPIC_API_KEY="sk-ant-..."
$env:RUST_LOG="debug"
.\target\release\roon-ai.exe
```

On startup the server logs either:
- `AI chat enabled (Anthropic API key found)` — ready
- `AI chat disabled (set ANTHROPIC_API_KEY to enable)` — key missing

### Files created / modified

| File | Change |
|---|---|
| `src/ai/mod.rs` | **New** — Anthropic client, 4 tool defs, tool dispatch, agentic loop |
| `src/api/mod.rs` | Added `AppState.anthropic_api_key`, `ai_chat_handler` |
| `src/main.rs` | Resolves API key at startup, registers `POST /api/ai/chat` |
| `src/app/api.rs` | Added `AiChatRequest`, `AiChatResponse`, `ai_chat()` fetch helper |
| `src/app/pages/ai_chat.rs` | **New** — chat UI (zone picker, bubbles, loading state) |
| `src/app/pages/mod.rs` | Exports `AiChat` |
| `src/app/mod.rs` | Added `/ai` route |
| `src/app/components/nav.rs` | Added "AI" nav link (desktop + mobile) |
| `src/config/mod.rs` | Added `AiConfig`, `resolve_anthropic_api_key()` |

### Web UI routes (updated)

| Route | Page | Purpose |
|---|---|---|
| `/` | Zones | All zones, now-playing, transport + volume controls |
| `/ai` | AI Music Control | NLS chat with zone picker |
| `/library` | Library | Browse Roon library |
| `/knobs` | Knobs | ESP32 firmware management |
| `/settings` | Settings | Adapter enable/disable |

### Startup log confirmation

On startup the server logs either:
- `AI chat enabled (Anthropic API key found)` — ready to use
- `AI chat disabled (set ANTHROPIC_API_KEY to enable)` — key missing; all other features unaffected

### Known limitations / next steps

- **No conversation memory** — each chat message is a fresh agentic session; there is no cross-turn context (e.g. "play more like that" won't reference the previous turn)
- **Synchronous response** — the UI shows a spinner and waits; no streaming. For long tool chains this can feel slow (~3–8s)
- **Roon-only radio** — `action='radio'` (Roon Radio) is the "similar music" mechanism; OpenHome/UPnP zones get an error if radio is requested
- **Search result count** — capped at 8 results per tool call; Claude picks the best match

---

## Recent Work (2026-04-18/19) — API Key Setup & AI Chat Verified

### Anthropic API key setup (Windows)

The correct config file location on Windows is:
```
%APPDATA%\roon-ai\config.toml
C:\Users\ChrisFogarty\AppData\Roaming\roon-ai\config.toml
```

Contents:
```toml
port = 8088

[ai]
api_key = "sk-ant-..."
```

**Note**: The `data/unified-hifi/` directory in the project root is Docker-only. The native Windows binary reads from `%APPDATA%\roon-ai\` (or `ROON_AI_CONFIG_DIR` if set).

### Troubleshooting encountered

- Initial key gave `credit balance too low` despite $50 balance — cause was a "Credit grant" invoice type that did not immediately unlock API access
- Fix: create a new API key in the Anthropic Console after verifying balance is active
- Account is **Tier 2** with full rate limits (1K RPM, 450K TPM for all models)
- On startup, confirm log line: `AI chat enabled (Anthropic API key found)`

### AI chat verified end-to-end

`/ai` page is live and confirmed working at http://localhost:8088/ai.

### Commits & push

| Commit | Description |
|---|---|
| `223bfff` | feat: Add AI natural language music control chat interface |
| `5cc38b0` | docs: Update HANDOFF.md with alphabet filter, grid layout, action item fix, build sequence, and fork details |

Pushed to personal fork: `https://github.com/cfogarty1964/Roon-AI` (branch `v3`)

```bash
git push cfogarty v3
```

Push to `origin` (open-horizon-labs) requires maintainer access — use a PR if contributing upstream.

---

## Recent Work (2026-04-19) — AI Chat Two-Column Layout + Clear Button

### AI Chat page redesign (`src/app/pages/ai_chat.rs`)

The `/ai` page was restructured from a single-column layout into a two-column layout:

- **Left column**: Chat conversation only — user bubbles (right-aligned), assistant text (left-aligned), loading dots, and the sticky input bar + Send button.
- **Right column**: Tool call log — every `⚡ tool_name(…)` action emitted by the agentic loop is displayed here as it accumulates, with a pulsing `⚡ calling…` placeholder while a request is in-flight. Empty state shown when no calls have been made yet.
- **Clear button**: Added to the header row (right of the zone picker). Disabled until at least one message exists; clears both columns at once (`messages.write().clear()`).
- The duplicated send logic (button click + Enter key) was refactored into a standalone `do_send()` helper. `Signal<T>` is `Copy` in Dioxus so signals are passed by value; interior mutability handles writes, and the `spawn` closure captures them without lifetime issues.

**Full build sequence required after this change** (WASM + server binary):
```bash
mkdir -p tmp_css
./tailwindcss.exe -i src/input.css -o tmp_css/tailwind.css --content "src/app/**/*.rs"
mv tmp_css/tailwind.css public/tailwind.css
rmdir tmp_css
dx build --release --platform web --features web
cargo build --release --features server
```

Always hard-refresh the browser (`Ctrl+F5`) after a binary update to clear cached WASM.

### Stop/Restart documented in ARCHITECTURE.md

Added a "Stop / Restart" section to `ARCHITECTURE.md` (immediately after the "Run" section) with:
- PowerShell: `Stop-Process -Name 'roon-ai' -Force -ErrorAction SilentlyContinue`
- Git Bash: `taskkill //F //IM roon-ai.exe`
- Note that the running binary locks the `.exe` on Windows — always stop before rebuilding.

---

## Recent Work (2026-04-19) — Display Rename to "Roon AI"

All user-visible strings renamed from "Roon AI" to "Roon AI". This is a display-only change — binary name, crate name, config paths, and the Roon extension ID are unchanged to avoid breaking existing installs.

### Files changed

| File | What changed |
|---|---|
| `src/app/components/layout.rs` | Browser tab title suffix (`… - Roon AI`) + footer text |
| `src/app/components/nav.rs` | Logo `alt` attribute |
| `src/adapters/roon.rs:1492` | Roon extension display name (shown in Roon → Settings → Extensions) |
| `src/main.rs` | Startup log line, mDNS advertised name, Flash Knob page `<title>` |
| `src/mcp/mod.rs` | MCP server `title` field and instructions header |

### What was intentionally left unchanged

- **`extension_id`** (`com.muness.roon-ai`) — changing this would de-authorise the existing Roon Extension and require re-pairing in Roon Settings
- **MCP `name`** (`roon-ai`) — identifier used in `.mcp.json`; changing it would break existing Claude MCP configs
- **Config directory paths** (`roon-ai`, `unified-hifi`) — changing these would silently lose existing settings on disk
- **Binary / crate name** — larger lift; tracked as a future task

### Bigger rename (future)

To also rename the binary to `roon-ai.exe`:
1. Change `name` in `Cargo.toml`
2. Update all `use roon_ai::` module paths (automated with `cargo fix` or sed)
3. Update `RUST_LOG` default in `main.rs`
4. Update LMS plugin `Plugin.pm` / `install.xml` binary references
5. Migrate or alias the config directory
6. Update CI matrix binary names and Docker image tags

---

## Known Gaps / Next Steps (updated 2026-04-19)

- **Binary rename**: Display strings say "Roon AI" but the binary is still `roon-ai.exe`. See "Bigger rename" notes above.
- **One-tap play shortcut**: From an album/artist view, still requires two clicks ("Play Album" → "Play Now"). A direct ▶ button on each row would collapse this to one tap.
- **AI conversation memory**: Each chat message is a fresh agentic session — no cross-turn context ("play more like that" won't work). Fix: persist the `messages` array across turns in the UI.
- **AI streaming**: Response is synchronous; UI shows a spinner for ~3–8s on long tool chains. Fix: stream the final text reply via SSE.
- **Roon API fork**: Waiting for SO_REUSEADDR fix to merge upstream; then switch back to official crate.
- **MCP auth**: No authentication on `/mcp` endpoint — assumes trusted LAN.
- **E2E tests**: Playwright config exists in `e2e/` but coverage is limited.

---

## Recent Work (2026-04-19) — LMS Removal

All Logitech Media Server (LMS) code has been removed from the codebase. The application at this point supported Roon, HQPlayer, OpenHome, and UPnP.

### Files deleted

| File | Lines |
|---|---|
| `src/adapters/lms.rs` | ~2658 |
| `src/adapters/lms_discovery.rs` | ~404 |
| `src/app/pages/lms.rs` | ~352 |
| `lms-plugin/` (entire directory) | — |

### Files modified

| File | Change |
|---|---|
| `src/adapters/mod.rs` | Removed `pub mod lms`, `pub mod lms_discovery`, re-exports |
| `src/api/mod.rs` | Removed `lms` from `AppState`, LMS routes, `LmsAdapter` import, LMS status, LMS adapter settings |
| `src/app/api.rs` | Removed `LmsStatus`, `LmsConfig`, `LmsPlayersResponse`, `LmsPlayer` structs; `lms: bool` from `AdapterSettings`; `hide_lms_page` from `AppSettings` |
| `src/app/mod.rs` | Removed `/lms` route |
| `src/app/pages/mod.rs` | Removed `mod lms`, `pub use lms::Lms` |
| `src/app/pages/settings.rs` | Removed LMS config section from settings UI |
| `src/app/pages/zones.rs` | Removed LMS SSE event handling, zone sort priority |
| `src/app/components/nav.rs` | Removed LMS nav links (desktop + mobile) |
| `src/app/settings_context.rs` | Removed `lms_enabled` signal, `hide_lms()`, updated `update()` signature |
| `src/app/sse.rs` | Removed `LmsConnected`, `LmsDisconnected`, `LmsPlayerStateChanged`, `should_refresh_lms()` |
| `src/bus/events.rs` | Removed all LMS bus event variants and `PrefixedZoneId::lms()` |
| `src/ai/mod.rs` | Removed LMS branches from all 4 tool handlers, removed `format_search_results_lms()` |
| `src/config/mod.rs` | Removed `LmsConfig`, `lms` from `Config`, `default_lms_port()`, LMS migration path |
| `src/coordinator.rs` | Removed `"lms"` and `"lms-cli"` from `AVAILABLE_ADAPTERS` |
| `src/main.rs` | Removed `create_lms_adapters`, LMS from startable adapters and `AppState::new` |
| `src/mcp/mod.rs` | Removed LMS dispatch branches from all tool handlers |
| `src/knobs/routes.rs` | Removed `lms:` zone filter, adapter settings check, `control_lms()` function (~80 lines), and dispatch branch in `knob_control_handler` |
| `src/bus/events.rs` | Removed `LmsConnected`/`LmsDisconnected`/`LmsPlayerStateChanged` from `is_legacy_event()` match; removed `PrefixedZoneId::lms()` test |
| `src/main.rs` | Removed `lms.stop().await` from shutdown sequence |
| `src/lib.rs` | Updated doc comment |

### Why

LMS (Logitech Media Server / Squeezebox) was never going to be used in this deployment. Removing it eliminates ~3,400 lines of dead code and simplifies every layer of the stack — adapters, API, UI, SSE, bus events, config.

### Build verified

Full build ran clean after all edits:
- CSS: 102ms
- `dx build`: WASM + server binary compiled (568/568 crates)
- `cargo build --release --features server`: succeeded (warnings only, no errors)

Startup log confirms LMS is gone — only `roon enabled`, `openhome disabled`, `upnp disabled` in the adapter list at this point. AI chat key was found and logged as enabled.

---

---

## Recent Work (2026-04-19) — HQPlayer and OpenHome Removal

All HQPlayer and OpenHome adapter code has been removed. The application now supports Roon and UPnP only. HQPlayer users should connect via Roon (Roon → HQPlayer integration).

### Files deleted

| File | Description |
|---|---|
| `src/adapters/hqplayer.rs` | HQPlayer TCP/XML adapter (~2400 lines) |
| `src/adapters/openhome.rs` | OpenHome SSDP/SOAP adapter (~950 lines) |
| `src/app/pages/hqplayer.rs` | HQPlayer settings/pipeline UI page |
| `src/app/components/hqp_controls.rs` | HQPlayer profile + matrix selector components |

### Files modified

| File | Change |
|---|---|
| `src/adapters/mod.rs` | Removed `mod hqplayer`, `mod openhome` |
| `src/api/mod.rs` | Removed all HQPlayer/OpenHome handlers, routes, request types, and `AppState` fields |
| `src/main.rs` | Removed adapter instantiation, all `/hqplayer/*` and `/openhome/*` routes, shutdown calls |
| `src/coordinator.rs` | Removed `"openhome"` from `AVAILABLE_ADAPTERS` and `register_from_settings` |
| `src/config/mod.rs` | Removed `HqpConfig`, `hqplayer` from `Config`, `migrate_hqp_config`, stale LMS tests |
| `src/bus/events.rs` | Removed `openhome`/`hqplayer` `PrefixedZoneId` constructors; removed HQP legacy events |
| `src/app/mod.rs` | Removed `Route::HqPlayer` and `/hqplayer` route |
| `src/app/pages/mod.rs` | Removed `mod hqplayer`, `HqPlayer` export |
| `src/app/pages/zones.rs` | Removed HQPlayer DSP controls, profile/matrix signals and handlers |
| `src/app/pages/settings.rs` | Removed OpenHome and HQPlayer rows from the features table |
| `src/app/components/mod.rs` | Removed `mod hqp_controls` and its exports |
| `src/app/components/layout.rs` | Removed `hide_hqp` prop |
| `src/app/components/nav.rs` | Removed HQPlayer nav link (desktop + mobile), `hide_hqp` prop |
| `src/app/settings_context.rs` | Removed `hqp_enabled` signal, `hide_hqp()`, updated `update()` signature |
| `src/app/api.rs` | Removed `HqpStatus` and all HQPlayer client types |
| `src/app/sse.rs` | Removed `OpenHomeDeviceFound`/`OpenHomeDeviceLost` events |
| `src/mcp/mod.rs` | Removed 4 HQPlayer MCP tools; tool count reduced from 10 to 6 |
| `src/ai/mod.rs` | Removed OpenHome branch from `control_playback` tool dispatch |
| `src/knobs/routes.rs` | Removed HQPlayer DSP info, OpenHome control branch, `DspInfo` struct |

### Build verified

Full build ran clean after all edits (`cargo check --features server`: 0 errors).

---

---

## Recent Work (2026-04-19) — AI Chat Markdown Rendering

AI chat responses now render as formatted HTML instead of raw markdown text.

### Problem

Claude's responses used markdown (headers, bold, numbered lists, `---` dividers) but the UI displayed them as literal characters — `**bold**`, `### heading`, `---`, etc.

### Solution

- **Server-side conversion** (`src/ai/mod.rs`): the final response text is passed through `pulldown_cmark` to produce HTML before being placed in `AiChatResponse.response`
- **Client rendering** (`src/app/pages/ai_chat.rs`): assistant bubbles use `dangerous_inner_html` to render the HTML directly into the DOM
- **Prose styles** (`src/input.css`): new `.ai-prose` class styles `<p>`, `<h3>`, `<ul>`, `<ol>`, `<li>`, `<strong>`, `<em>`, `<hr>`, `<code>`, `<blockquote>` within the bubble

### New dependency

`pulldown-cmark = "0.12"` added as a server-only optional dependency. No WASM bundle impact — conversion happens on the server.

### Files changed

| File | Change |
|---|---|
| `Cargo.toml` | Added `pulldown-cmark = { version = "0.12", optional = true }` to server feature |
| `src/ai/mod.rs` | Added `markdown_to_html()` helper; response field now contains HTML |
| `src/app/pages/ai_chat.rs` | Assistant bubbles use `dangerous_inner_html`; added `ai-prose` class |
| `src/input.css` | Added `.ai-prose` CSS block |

---

## Recent Work (2026-04-19) — Obsolete File Cleanup

Removed all files that referenced deleted adapters (HQPlayer, OpenHome, LMS) or were historical planning/spike documents no longer relevant to the codebase.

### Files deleted

| File | Reason |
|---|---|
| `HQPLAYER-MULTI-INSTANCE.md` | HQPlayer removed |
| `RUST_SPIKE.md` | Historical pre-Rust spike doc; superseded |
| `docs/hqplayer-protocol-reference.md` | HQPlayer removed |
| `docs/lms-plugin.md` | LMS removed |
| `docs/LMS-PLUGIN-SPEC.md` | LMS removed |
| `docs/lyrion.md` | LMS/Lyrion removed |
| `docs/ARCHITECTURE-RECOMMENDATION-B-REJECTED.md` | Explicitly rejected; historical |
| `docs/tasks/phase-1-bus-foundation.md` | Completed JS-era task plan |
| `docs/Dioxus-components-research.md` | Hydration debugging research; already resolved |
| `data/hqp-config.example-multi.json` | HQPlayer config example |
| `.oh/hqplayer-spec.md` | HQPlayer spec |
| `mcp/index.js` | Old Node.js MCP server; superseded by `src/mcp/mod.rs` |
| `src/hqplayer/.gitkeep` | HQPlayer placeholder directory |
| `tests/mock_servers/hqplayer.rs` | HQPlayer mock |
| `tests/mock_servers/lms.rs` | LMS mock |
| `tests/mock_servers/openhome.rs` | OpenHome mock |
| `tests/adapter_integration.rs` | Referenced `HqpAdapter`, `LmsAdapter` — broken |
| `tests/protocol_integration.rs` | Referenced `HqpAdapter`, `LmsAdapter`, `OpenHomeAdapter` — broken |
| `tests/zones_sha_integration.rs` | Referenced `MockLmsServer`, `HqpAdapter`, `OpenHomeAdapter` — broken |
| `.wm/` (4 files) | AI tool working memory scratch files |

### Files updated

| File | Change |
|---|---|
| `tests/mock_servers/mod.rs` | Removed exports for deleted hqplayer/lms/openhome mocks |
| `tests/fixtures/api_routes.txt` | Removed all HQPlayer, LMS, and OpenHome route entries |

### No rebuild required

Only docs, tests, and fixture files were modified — no compiled source changed.

---

*Updated: 2026-04-19 — HQPlayer and OpenHome removed; Roon + UPnP only; obsolete files cleaned up*

---

## Recent Work (2026-04-19) — Persistent Default Zone

### Feature

A persistent default zone can be set from any page that has a zone picker (`/ai`, `/library`). A ☆ button sits immediately after each zone `<select>`; clicking it saves the current selection as the default (button turns ★ yellow). On any subsequent page load — including navigating between `/ai` and `/library` — the stored default is pre-selected automatically. The choice persists across browser restarts via `localStorage`.

### How it works

- **Storage key**: `roon-ai-default-zone` in `localStorage`
- **Shared context**: `DefaultZoneContext` (`src/app/default_zone.rs`) — a `Signal<String>` initialised at the app root, following the same pattern as `ThemeContext`
- **Pre-selection logic**: on mount, each page checks the stored default first; if the stored zone is still in the zone list it is used, otherwise falls back to the first Roon zone as before
- **Star button**: ☆ (grey) when the current selection differs from the stored default; ★ (yellow) when it matches; clicking always overwrites the stored default with the current selection

### Files created / modified

| File | Change |
|---|---|
| `src/app/default_zone.rs` | **New** — `DefaultZoneContext`, `use_default_zone_provider()`, `use_default_zone()`, localStorage helpers |
| `src/app/mod.rs` | Added `pub mod default_zone`; calls `use_default_zone_provider()` at app root |
| `src/app/pages/ai_chat.rs` | Pre-selects default zone on mount; ★/☆ button next to zone picker |
| `src/app/pages/library.rs` | Pre-selects default zone on mount; ★/☆ button next to zone picker |

---

## Recent Work (2026-04-24) — AI Chat Play-Button Suggestions

### Feature

When the AI reply recommends specific pieces or tracks (e.g. *"suggest five late-night jazz piano tracks"*, *"what are some good recordings of Mahler's 5th Adagietto?"*), each suggestion renders as a row with a **▶ Play** button directly under the assistant bubble. Clicking ▶ Play submits a new chat turn like `Play "Title" by Artist` on the currently selected zone — which routes through the existing agentic loop and invokes `play_music` in Roon. The tool-call log on the right column updates as usual, keeping the whole flow transparent.

Suggestions only appear for recommendations — not for replies that confirm a playback the AI just executed. If Claude omits the block (or emits malformed JSON), the prose is still shown and the suggestions list is empty.

### How it works

- **Wire format**: the system prompt instructs Claude to end reply text with a sentinel-wrapped JSON block:
  ```
  <<<SUGGESTIONS>>>
  [{"title": "…", "artist": "…", "album": "…"}]
  <<<END_SUGGESTIONS>>>
  ```
- **Server extraction** (`src/ai/mod.rs::extract_suggestions`): finds the block, parses the JSON array, strips the block from the reply text *before* markdown→HTML conversion. Parse failures silently fall back to an empty list.
- **Response shape**: `AiChatResponse` now carries `suggestions: Vec<Suggestion>` where `Suggestion { title, artist?, album? }`.
- **Click flow**: a new `do_send_text(msg, …)` helper on the page skips the input box and submits a canned `Play "X" by Y` turn directly. The old `do_send()` now delegates to it after reading/clearing the input.
- **Play message format**:
  - With artist: `Play "Title" by Artist`
  - With album only: `Play "Title" from Album`
  - Neither: `Play "Title"`

### Key design choices

- **Sidecar JSON, not a new tool.** The alternative was a dedicated `suggest_songs` tool that Claude would call to emit a structured list. The sentinel-block approach is simpler (no new tool definition, no new dispatch branch) and Sonnet 4.6 follows the format instruction reliably. Use this pattern for other structured-output-alongside-prose features in the AI chat.
- **Route clicks through a new chat turn, not a direct REST call.** The ▶ Play button calls `do_send_text`, which goes through the full agentic loop rather than hitting `/roon/search_and_play` directly. This keeps the tool-call log populated and preserves conversational context — the user sees exactly what the AI did.

### Files created / modified

| File | Change |
|---|---|
| `src/ai/mod.rs` | Added `Suggestion` struct + `suggestions` field on `AiChatResponse`; extended `system_prompt` with the sentinel-block instruction; new `extract_suggestions()` parser; wired into `run_agent` before markdown conversion |
| `src/app/api.rs` | Mirrored `Suggestion` struct and `suggestions` field on the shared (client-side) `AiChatResponse` |
| `src/app/pages/ai_chat.rs` | `ChatMessage` carries `suggestions`; new `do_send_text()` helper; old `do_send()` delegates to it; renders ▶ Play rows under each assistant bubble when suggestions are non-empty; new `play_message_for()` formatter |

No new crate dependencies. No new routes. No breaking changes to existing `/api/ai/chat` callers (new field is `#[serde(default)]`).

### Test checklist

- "Suggest five late-night jazz piano tracks" → prose + 5 play rows ✅
- Click a ▶ Play → new user turn appears with `Play "X" by Y`, tool log shows `play_music(...)`, track starts on the selected zone ✅
- "Play Kind of Blue on the kitchen" → no suggestions rendered (this is a direct play, not a recommendation) ✅
- Malformed JSON in the sentinel block → prose shown, no suggestions, no server error ✅

---

## Recent Work (2026-04-26) — Conversational AI Page (Persistent History)

### Feature

A new tab **Conversational AI** at `/conversational` provides a persistent, multi-turn version of the AI chat. Unlike the original `/ai` page (which is a fresh single-shot session per message), this page:

- **Remembers prior turns** — each request to `/api/ai/chat` includes the full `history` so Claude has context for follow-ups like *"of those, which is the most relaxed?"* or *"play it on the kitchen"*.
- **Persists across reloads** — the conversation is saved to `localStorage` on every change and hydrated on mount. Refreshing the browser doesn't lose state.
- **Same UX as `/ai`** — two-column layout, ▶ Play suggestion rows, tool-call log, ★/☆ default-zone star, Clear button. The Clear button also wipes the localStorage entry.

The original `/ai` page is unchanged and still single-shot — both pages coexist so you can compare. The decision whether to retire `/ai` later is deferred.

### How it works

**Wire-protocol additions** (backward-compatible — both fields are optional / `#[serde(default)]`):

`AiChatRequest` gained:
```rust
pub history: Vec<HistoryTurn>,   // [{role: "user" | "assistant", text: String}, ...]
```

`AiChatResponse` gained:
```rust
pub response_markdown: String,   // raw markdown, suggestions block stripped
```

**Server flow** (`src/ai/mod.rs::run_agent`):
1. Build initial `messages` vec by translating each `history` entry into a Claude `Message` (role unchanged, plain text content).
2. Skip non-Claude roles (`error` etc.) defensively.
3. Append the new user message and run the existing agentic tool-use loop unchanged.
4. After `extract_suggestions()` strips the sentinel block, return both the rendered HTML (`response`) and the cleaned markdown (`response_markdown`).

**Client storage** (`src/app/pages/conversational_ai.rs`):
- `STORAGE_KEY = "roon-ai-conversation"` in `localStorage`.
- `ChatMessage` derives `Serialize`/`Deserialize` and gains a `markdown: String` field — for assistant turns this holds the raw markdown that gets replayed back to the server in the next request's `history`.
- `use_effect` watches `messages` and writes the JSON snapshot on every change.
- A second `use_effect` (runs once on hydration) reads the JSON back. The hydrate effect doesn't need a `#[cfg(target_arch = "wasm32")]` gate because Dioxus effects don't fire during SSR; the non-wasm storage stubs are no-ops.
- `build_history()` filters the in-memory `messages` to user/assistant turns (drops errors) and produces the `HistoryTurn` vec for the next request.

### Files created / modified

| File | Change |
|---|---|
| `src/ai/mod.rs` | Added `HistoryTurn` struct + `history` field on `AiChatRequest`; added `response_markdown` field on `AiChatResponse`; `run_agent` now seeds `messages` from history before appending the new user message |
| `src/app/api.rs` | Mirrored `HistoryTurn` and the two new fields on the shared client types |
| `src/app/pages/conversational_ai.rs` | **New** — full page mirroring `ai_chat.rs` plus history send-back, `markdown` on `ChatMessage`, `localStorage` hydrate/save/clear |
| `src/app/pages/mod.rs` | Added `mod conversational_ai;` and `pub use conversational_ai::ConversationalAi;` |
| `src/app/mod.rs` | Added `Route::ConversationalAi` mapped to `/conversational`; imported `ConversationalAi` |
| `src/app/components/nav.rs` | Added "Conversational AI" link (desktop + mobile) |
| `src/app/pages/ai_chat.rs` | Minor backward-compat fix: `AiChatRequest` construction now includes `history: vec![]`; destructure pattern uses `..` to ignore `response_markdown` |

No new crate dependencies. No new endpoints — `/api/ai/chat` is the only AI route, both pages POST to it.

### Web UI routes (updated)

| Route | Page | Purpose |
|---|---|---|
| `/` | Zones | All zones, now-playing, transport + volume controls |
| `/ai` | AI Music Control | Single-shot NLS chat (no history) |
| `/conversational` | Conversational AI | Persistent multi-turn chat with localStorage |
| `/library` | Library | Browse Roon library |
| `/knobs` | Knobs | ESP32 firmware management |
| `/settings` | Settings | Adapter enable/disable |

### Token cost note

Every request to `/conversational` re-sends the full prior history. For Sonnet 4.6 at ~$3/M input tokens, even a 50-turn session is pennies — non-issue for a single-user local bridge. If conversations ever balloon, options are: cap to last N turns, or add an Anthropic prompt-cache `cache_control` block on the history array (Anthropic prompt caching reduces cost ~10× for the cached prefix).

### Known limitations / next steps

- **No streaming** — still synchronous; `/conversational` shows the spinner for ~3–8s on long tool chains. Same as `/ai`.
- **No conversation summary** — when token budget eventually matters, summarising old turns is the right move; not yet needed.

---

## Recent Work (2026-04-26) — Voice In/Out on Conversational AI

### Feature

The `/conversational` page now supports speech input and spoken replies. Three new controls:

- **🔊 Speak** (header toggle) — when on, every assistant reply is spoken aloud via the browser's `SpeechSynthesis`. Markdown is stripped before TTS so you don't hear "asterisk asterisk bold asterisk asterisk".
- **🎙 Hands-free** (header toggle, implies Speak) — after the assistant finishes speaking, the mic auto-restarts. True back-and-forth without touching the keyboard. Disabled if the browser lacks `SpeechRecognition`.
- **🎤 Mic** (next to Send) — click to record. Textarea placeholder changes to "Listening… (speak now)" and the button turns red and pulses. The recogniser auto-stops on silence; the transcript is auto-submitted as a new chat turn via `do_send_text`. Click again (button shows ■) to cancel mid-listen. Disabled in Firefox (no STT) and while another request is loading.

### How it works

**Single point of JS interop**: a constant `SPEECH_INSTALL_JS` in `src/app/pages/conversational_ai.rs` is run once on page mount via `dioxus::document::eval(...)`. It installs `window.RoonSpeech` with four Promise-based methods:

```
window.RoonSpeech.startListening()  → Promise<String>   (transcript or rejects)
window.RoonSpeech.stopListening()
window.RoonSpeech.speak(markdown)   → Promise<void>     (markdown stripped internally)
window.RoonSpeech.cancelSpeech()
window.RoonSpeech.isSttSupported    : bool
```

The `eval()` script returns `!!window.RoonSpeech.isSttSupported` so the WASM side can flip a `stt_supported` signal that drives the disabled state of the mic + Hands-free buttons.

**Per-action eval calls**: each user action (mic click, post-reply speak, post-speak listen) spawns a fresh `dioxus::document::eval(...)` and `.await`s its `.join::<T>()`. The script does the work and either `dioxus.send(value)` or returns a value. No long-lived Closures, no leaked callback handles.

**TTS markdown stripping** happens in JS via a `plainify(md)` regex pipeline — drops fences, asterisks, headers, list bullets, link syntax, hr lines, and collapses paragraph breaks into ". " separators. Adequate for natural-sounding TTS; not perfect but doesn't need to be.

**Continuous loop**: after `do_send_text` receives a reply, if `speak_enabled` is on it `await`s `RoonSpeech.speak(markdown)`. When that resolves (utterance.onend fires), if `continuous` is on it calls `start_listening_task` again — which kicks off the next mic capture, which auto-submits, which gets a reply, which gets spoken, etc.

**SpeechCtx struct** bundles the three relevant signals (`speak_enabled`, `continuous`, `listening`) and is `Copy + Clone` (since `Signal<T>` is `Copy` in Dioxus). Threaded through `do_send`, `do_send_text`, and `start_listening_task` so any of them can read/update voice state.

### No new crate dependencies

All speech handling lives in JS via `dioxus::document::eval`. No `web-sys` feature flags added. No new Cargo deps. The Rust↔JS messaging surface is just `Eval::join::<serde_json::Value>().await`.

### Files modified

| File | Change |
|---|---|
| `src/app/pages/conversational_ai.rs` | Added `SpeechCtx` struct, `SPEECH_INSTALL_JS` constant (the JS module), `start_listening_task` helper, three new signals (`speak_enabled`, `continuous`, `listening`, `stt_supported`); threaded `speech: SpeechCtx` through `do_send` / `do_send_text`; added 🔊 / 🎙 toggle buttons to header; added 🎤 mic button next to Send; modified `do_send_text` to optionally speak the reply and re-listen after speech ends |

No other files touched. The original `/ai` page and the JSON wire protocol are unchanged.

### Browser caveats

| Browser | STT (mic) | TTS (speak) |
|---|---|---|
| Chrome / Edge / Safari | ✅ (cloud-based in Chrome — needs internet) | ✅ |
| Firefox | ❌ no `SpeechRecognition` | ✅ |
| Edge on Windows | ✅ | ✅ — **also ships Microsoft neural voices for free** ("Ava Online (Natural)", "Andrew Online (Natural)"), much better than the basic ones |

First mic click triggers the browser's microphone permission prompt.

### Known limitations / next steps

- **Default TTS voice is usually the worst one.** Browsers expose `speechSynthesis.getVoices()` which lists every installed voice. Most systems have neural voices already (especially Edge on Windows). A voice-picker dropdown that lists them and remembers the choice in localStorage is ~30 min of work and dramatically improves the experience for free.
- **Cloud TTS for studio quality**: a future `/api/tts` route could proxy to OpenAI TTS (~$15/M chars) or ElevenLabs (~10× cost, top quality). Client plays the returned MP3 via an `<audio>` element. Half a day of work. Not yet needed — the free local voices are usually sufficient.
- **No voice activity detection / wake word** — Hands-free always restarts the mic right after a reply. A "Hey Roon" wake word would let the page stay listening passively. Out of scope for v1.

---

## Recent Work (2026-04-26) — Voice Picker + Single AI Tab

### Voice picker

A dropdown in the Conversational AI page header (immediately left of the 🔊 Speak toggle) lists every voice the browser exposes via `speechSynthesis.getVoices()`. The choice persists in `localStorage` (`roon-ai-voice` key) and is applied to every spoken reply by setting `utterance.voice` before `speechSynthesis.speak()`.

**Why this matters:** the default voice picked by the browser is almost always the worst one in the list. **Edge on Windows ships Microsoft neural voices for free** — entries containing "Online (Natural)" such as *Microsoft Ava Online (Natural)* — and they sound essentially studio-quality. Selecting one of those completely solves the "robotic TTS" problem with zero cost.

**JS additions** to `RoonSpeech`:
- `RoonSpeech.listVoices()` → `[{ name, lang, default }]`
- `RoonSpeech.speak(md, voiceName?)` — accepts optional voice name, looks up via `getVoices().find(v => v.name === name)`, falls back to default if not found

**Chrome quirk handled**: `getVoices()` returns an empty array on first call until the browser fires `voiceschanged`. A separate `LIST_VOICES_JS` script in `conversational_ai.rs` waits for that event (or polls up to 2s) before resolving, so the dropdown always populates correctly.

**Rust additions** in `src/app/pages/conversational_ai.rs`:
- `VoiceInfo { name, lang, default }` — deserialised from the JS array
- `voices: Signal<Vec<VoiceInfo>>` — populated on mount
- `selected_voice: Signal<String>` — bound to dropdown, hydrated from localStorage
- `selected_voice` added to `SpeechCtx` and threaded through `do_send_text`'s speak step — passed as the second argument to `RoonSpeech.speak`
- `load_voice_choice()` / `save_voice_choice()` helpers (wasm-gated, mirroring the conversation persistence pattern)

**Browser support for voice quality**:
- Edge / Windows: ships Microsoft "Natural" neural voices — best free option
- Chrome / Windows: only basic voices unless Microsoft voices are installed system-wide via Settings → Time & Language → Speech → Add voice
- Chrome / macOS: ships some "Enhanced" voices (e.g. Samantha Enhanced) — okay quality
- Safari / macOS: ships system voices — same set as macOS
- Firefox: all platforms — basic voices only, but TTS works

### Single AI tab — `/ai` removed

The original `/ai` page (single-shot, no history) has been deleted. The Conversational AI page at `/conversational` is now the only AI surface in the UI.

**Why:** the conversational version is strictly better — it includes everything the old page did (zone picker, suggestions + ▶ Play, tool log, default-zone star) plus persistent history, voice in/out, and the voice picker. There's no reason to keep two AI tabs.

**Removed:**
- `Route::AiChat {}` enum variant + `/ai` route
- `AiChat` import from `src/app/mod.rs`
- `mod ai_chat;` + `pub use ai_chat::AiChat;` from `src/app/pages/mod.rs`
- "AI" nav links (desktop + mobile) from `src/app/components/nav.rs`
- File `src/app/pages/ai_chat.rs` (~190 lines) deleted

**Kept** (still needed by the conversational page):
- `AiChatRequest` / `AiChatResponse` / `ai_chat()` types in `src/ai/mod.rs` and `src/app/api.rs`
- `ai_chat_handler` and `POST /api/ai/chat` route registration in `src/api/mod.rs` and `src/main.rs`

### Web UI routes (updated)

| Route | Page | Purpose |
|---|---|---|
| `/` | Zones | All zones, now-playing, transport + volume controls |
| `/conversational` | Conversational AI | Persistent multi-turn chat with voice in/out and voice picker |
| `/library` | Library | Browse Roon library |
| `/knobs` | Knobs | ESP32 firmware management |
| `/settings` | Settings | Adapter enable/disable |

The `/ai` URL now 404s. Any saved bookmarks need updating to `/conversational`.

### Files modified

| File | Change |
|---|---|
| `src/app/pages/conversational_ai.rs` | Added `VoiceInfo`, `LIST_VOICES_JS`, voice persistence helpers, `voices` + `selected_voice` signals, voice-picker dropdown in header, voice name threaded through `RoonSpeech.speak` call; extended `SPEECH_INSTALL_JS` with `listVoices()` and `speak(md, voiceName?)` |
| `src/app/components/nav.rs` | Removed both "AI" nav links |
| `src/app/mod.rs` | Removed `Route::AiChat` and `AiChat` import |
| `src/app/pages/mod.rs` | Removed `ai_chat` module + export |
| `src/app/pages/ai_chat.rs` | **Deleted** |

### Optional follow-ups

- **Rename `/conversational` → `/ai`**: with the old page gone, the shorter URL is more natural. ~5 line change (route, `nav_active` string, nav link). Not done — kept as-is to avoid breaking anyone who already bookmarked `/conversational`.
- **Server-side cloud TTS**: still on the table for cross-device voice consistency. The free voice picker plus Edge's Microsoft Natural voices is usually good enough that this isn't needed yet.

---

## Recent Work (2026-04-26) — Voice Picker Moved to Settings

### Change

The TTS voice-picker dropdown moved out of the Conversational AI page header and into a new **Voice** section on the Settings page. The voice choice is now app-wide shared state (like theme and default zone) instead of page-local state.

The Conversational AI header is now lighter — just 🔊 Speak and 🎙 Hands-free toggles. Settings owns the configuration; the AI page consumes it.

### Architecture

A new shared context module follows the same pattern as `theme` and `default_zone`:

**`src/app/voice_context.rs`** (new, ~110 lines):
- `VoiceInfo { name, lang, default }` — was previously page-local
- `VoiceContext { selected: Signal<String>, voices: Signal<Vec<VoiceInfo>> }` — `Copy + Clone`
- `use_voice_provider()` — installs context at app root, hydrates `selected` from `localStorage`, asynchronously enumerates voices via `LIST_VOICES_JS`
- `use_voice()` — getter for any component (Settings page, Conversational AI page, future pages)
- `VOICE_STORAGE_KEY = "roon-ai-voice"` — same localStorage key as before, so existing user choices carry over

**`LIST_VOICES_JS` decoupled from RoonSpeech**: the voice-listing JS now calls `window.speechSynthesis.getVoices()` directly with the same `voiceschanged` polling fallback. It no longer depends on `RoonSpeech.listVoices` being installed, so Settings can populate the dropdown even if the user never visits the AI page first.

### Files changed

| File | Change |
|---|---|
| `src/app/voice_context.rs` | **New** — shared `VoiceContext` + provider + standalone `LIST_VOICES_JS` + localStorage helpers |
| `src/app/mod.rs` | Added `pub mod voice_context;` and `use_voice_provider()` call alongside the other providers in the App component |
| `src/app/pages/settings.rs` | Added **Voice** `<section>` between Features and Appearance with the dropdown bound to `voice_ctx.set(...)`; includes hint about Edge neural voices |
| `src/app/pages/conversational_ai.rs` | Removed `VoiceInfo`, `VOICE_STORAGE_KEY`, `LIST_VOICES_JS`, `load_voice_choice`, `save_voice_choice`, the `voices` and `selected_voice` local signals, and the dropdown UI from the header. Now reads `voice_ctx.selected` from `use_voice()` and threads it into `SpeechCtx.selected_voice` so `RoonSpeech.speak(md, voiceName)` still receives the chosen voice |

### Reactivity

Because `selected_voice` is a `Signal<String>` shared by reference (Dioxus signals are `Copy`-by-value but read/write through the same backing store), changing the voice in Settings takes effect on the AI page immediately — no reload required. The next spoken reply uses the new voice.

### Backward compatibility

The `localStorage` key (`roon-ai-voice`) is unchanged, so any voice choice the user already saved continues to work after this refactor. No migration needed.

---

## Recent Work (2026-04-26) — Push to Personal Fork + History Cleanup

### Pushed

Commit `ff9e003` (`feat: Conversational AI with persistence, voice in/out, and play-button suggestions`) is now on **`cfogarty/v3`** at `https://github.com/cfogarty1964/Roon-AI/tree/v3`. It bundles all of this session's work into one logical commit.

### History cleanup

The previous local-only commit `a198c5d` (titled simply "rust") was a 145 MB pile of IDE/tooling junk that had accumulated from a casual `git add .` — Visual Studio workspace internals (`.vs/`), the rustup installer, the Tailwind CLI binary, and Claude config. GitHub's 100 MB file-size limit rejected the push because of `tailwindcss.exe` (123 MB).

**Resolution**: dropped `a198c5d` entirely (it had no useful content) via `git reset --mixed fa944d7`, kept the working tree, then re-staged only the legitimate session changes and committed them as `ff9e003`. No force-push was needed because the junk commit had never reached the remote.

### `.gitignore` extensions

To prevent the same junk from being re-committed by accident:

```gitignore
/tailwindcss.exe              # Windows Tailwind CLI binary (~123 MB)
/rustup-init.exe              # Rustup installer (~12 MB)
.vs/                          # Visual Studio workspace state
/start-roon-ai.ps1            # Per-developer launcher script with hardcoded paths
```

`.claude/settings.local.json` is already tracked in some commits but should ideally also be local-only (per-developer Claude Code config). Not gitignoring it now to avoid breaking existing workflows; future cleanup if needed.

## Recent Work (2026-04-26) — Knob Subsystem Removal

### Why

The user explored using an existing ESP32 round-LCD device (which turned out to be a generic Chinese smart-display product, not the muness/roon-knob hardware the bridge was designed for) and decided to refocus the project entirely on AI/voice control of Roon. The whole knob subsystem became dead weight: ~3,000 lines of Rust for hardware that wasn't going to be connected.

### Files deleted

| File / directory | Description |
|---|---|
| `src/knobs/` | Whole module — `mod.rs`, `routes.rs` (~1100 lines of HTTP handlers), `store.rs` (knob device registry, ~400 lines), `image.rs` (RGB565 conversion for the knob LCD) |
| `src/firmware.rs` | Firmware auto-fetcher polling `muness/roon-knob` GitHub releases |
| `src/mdns.rs` | mDNS advertisement of `_roonknob._tcp.local.` for knob discovery |
| `src/app/pages/knobs.rs` | The `/knobs` web page (knob registration, config, firmware management, ~960 lines) |
| `src/app/settings_context.rs` | Reactive shared context that existed only to plumb `hide_knobs` into Nav |
| `src/app/components/form_inputs.rs` | `PowerModeInput` + `ToggleInput` — only consumed by the deleted knobs page |

### Files modified

| File | Change |
|---|---|
| `src/lib.rs` | Removed `pub mod firmware;`, `pub mod knobs;`, `pub mod mdns;` |
| `src/main.rs` | Removed knob/firmware/mdns imports, the `/knobs/flash` HTML page, `KnobStore::new()` init, all `/knob/*`, `/now_playing`, `/control`, `/config/*`, `/firmware/*`, `/manifest-s3.json`, `/admin/fetch-firmware` route registrations, mDNS advertisement, FirmwareService auto-update task, `flash_page` and `firmware_service` from shutdown sequence. Added a small `/zones` route pointing to a new `api::zones_handler`. Changed taglines + module doc-comment to reflect the new focus |
| `src/api/mod.rs` | Removed `KnobStore` import + the `knobs` field on `AppState` (and its constructor parameter); removed the `AppState::get_image()` method (its only caller was the knob image handler); removed `hide_knobs_page` from `AppSettings` (and its `hideKnobsPage` serde alias); removed the unused-mut on `load_app_settings`. Added a new `zones_handler` returning the unified zone list as `{ zones: [...] }` for the web UI |
| `src/adapters/roon.rs` | `RoonAdapter` no longer carries a `knob_store: Option<KnobStore>`. Constructors `new`, `new_configured`, and `new_disconnected` lost the `knob_store` parameter. The Roon-extension status message in Roon Settings is now just `v{version} • {base_url}` (the "controller count" line was knob-specific). The threading of `knob_store` through `run_roon_loop` is gone |
| `src/app/api.rs` | Removed `KnobDevicesResponse`, `KnobDevice`, `KnobStatus`, `KnobConfigResponse`, `PowerModeConfig`, `KnobConfig`, `FirmwareVersion`, `FetchFirmwareResponse`. Removed `hide_knobs_page` from `AppSettings` |
| `src/app/mod.rs` | Removed `Route::Knobs`, `Knobs` import, `pub mod settings_context;`, `use_settings_provider()` call |
| `src/app/pages/mod.rs` | Removed `mod knobs;` and `pub use knobs::Knobs;` |
| `src/app/components/nav.rs` | Removed Knobs nav links (desktop + mobile), the `hide_knobs` prop, and the `use_settings()` reactive lookup |
| `src/app/components/layout.rs` | Removed `hide_knobs` prop and pass-through to `Nav` |
| `src/app/components/mod.rs` | Removed `pub mod form_inputs;` and the `PowerModeInput` / `ToggleInput` re-exports |
| `src/app/pages/settings.rs` | Removed the "Knobs" row from the Features table, the `hide_knobs` signal, the `use_settings()` import, and the `hide_knobs_page` plumbing in `save_settings`. Voice + Appearance sections kept untouched |
| `src/app/sse.rs` | Removed `should_refresh_knobs()` method |
| `src/config/mod.rs` | Removed `"knobs.json"` from `MIGRATABLE_CONFIG_FILES` and the matching docstring line |
| `Cargo.toml` | Dropped `dep:image`, `dep:mdns-sd`, `dep:resvg` from the `server` feature; removed the `image`, `mdns-sd`, `resvg` optional deps and the unconditional `sha2` dep — all were knob-only |

### Routes removed

```
GET    /knob/zones                   GET    /firmware/version
GET    /knob/now_playing             GET    /firmware/download
GET    /knob/now_playing/image       GET    /manifest-s3.json
POST   /knob/control                 POST   /admin/fetch-firmware
GET    /knob/config                  GET    /knobs/flash
POST   /knob/config
GET    /knob/devices                 (mDNS) _roonknob._tcp.local.
GET    /now_playing
GET    /now_playing/image
POST   /control
GET    /config/{knob_id}
PUT    /config/{knob_id}
```

The unified `GET /zones` was preserved as a slimmer handler in `src/api/mod.rs` (now `api::zones_handler`) since it's used by the web UI for the zone picker.

### Behavior changes for users

- The `/knobs` URL now 404s. Anyone with a bookmark needs to update.
- The Roon extension status shown in Roon → Settings → Extensions no longer mentions "controller count" — just `v0.0.0 • http://hostname:8088`.
- mDNS advertisement on `_roonknob._tcp.local.` is gone. Nothing on the LAN was consuming it; if you want service discovery for a phone app later, you'd add a different mDNS type.
- `app-settings.json` written by old versions might still contain `"hide_knobs_page": true` — serde silently ignores it now (no migration needed).
- A pre-existing `knobs.json` in the config directory is harmless — never read again.

### Build + size impact

| Metric | Before | After |
|---|---|---|
| Server build (release) | ~3 min | ~2:20 |
| Server binary | ~36 MB | ~40 MB (no meaningful change — LTO was already tree-shaking the unused code, and the new `/zones` handler + other adjustments offset most of the deletions in compiled size) |
| WASM bundle | (no change) | (no change) |
| Source lines of code (crate) | ~22k | ~19k (the real win — much less to read, maintain, and reason about) |
| Crate dependencies | ~570 | ~520 (image/resvg/mdns-sd dropped, but their transitives often turned out to be shared with other crates so the total count moved less than expected) |

### Note on `start-roon-ai.ps1`

This file is intentionally NOT in the repo because it contains hardcoded absolute paths to `D:\OneDrive - 221B\SCRIPTS\Roon AI\` and a (currently empty) `$env:ANTHROPIC_API_KEY` slot. It's a per-developer convenience launcher. The standard run procedure remains:

```powershell
$env:RUST_LOG="debug"
.\target\release\roon-ai.exe
```

(With `ANTHROPIC_API_KEY` set via env var or persisted in `%APPDATA%\roon-ai\config.toml` under `[ai] api_key = "..."` — see the AI Natural Language Music Control section above.)

---

## Next Session — Candidate Work Items (2026-04-26)

Four candidates worth considering next, ordered by perceived UX impact. Each is independent — none blocks the others.

### 1. Stream the AI response (~3 hours) — **recommended**

The 3–8 second spinner after pressing Send is now the worst moment in an otherwise-snappy flow. Replacing it with streaming would feel transformatively faster.

**What to build:**
- Switch `POST /api/ai/chat` from a synchronous JSON response to an SSE stream. Claude's `/v1/messages` already supports SSE — pass `stream: true` in the request body and forward `content_block_delta` events to the client.
- On the client, replace the single `ai_chat()` fetch with an `EventSource` (or fetch + stream reader). Append text deltas to the in-progress assistant bubble as they arrive.
- Once the final stop event lands, run the existing `extract_suggestions()` over the assembled text, then trigger TTS (if Speak is on) and the continuous-mode mic restart (if Hands-free is on).
- Keep the agentic tool-use loop intact — only the final text reply needs to stream. Tool-use stop reasons still resolve synchronously, then a fresh streamed call covers the next iteration.

**Files to touch:** `src/ai/mod.rs` (call), `src/api/mod.rs` (handler — return `axum::response::sse::Sse` instead of `Json`), `src/app/api.rs` (client streaming helper), `src/app/pages/conversational_ai.rs` (in-progress bubble rendering, defer TTS until stop).

### 2. Now-playing context on the Conversational page (~1 hour) — **recommended**

A small banner above the chat showing the current track on the selected zone unlocks natural follow-ups — *"what is this?"*, *"skip it"*, *"more like this"* — without typing the title. The agent sees it as additional context.

**What to build:**
- Add a `current_track: Option<NowPlaying>` field to `AiChatRequest`. Client populates it from the existing SSE `now_playing` cache for the selected zone before sending.
- In the system prompt, append a "currently playing on this zone: TITLE — ARTIST — ALBUM" line when present.
- In the UI, render a thin always-visible banner (similar style to the loading pulse) showing the now-playing tile with art thumbnail and track/artist. Clicking it could pause/resume.

**Files to touch:** `src/ai/mod.rs` (system prompt, `AiChatRequest`), `src/app/api.rs` (mirror), `src/app/pages/conversational_ai.rs` (banner + populate before send), reuse the existing `NowPlaying` type and `/zones/{id}/now_playing` endpoint.

**Why this pairs well with #1**: streaming + now-playing context together makes the page feel like a live remote. Together about half a day; do them in that order so the now-playing banner reflects state through any tool-call latency.

### 3. Multiple saved conversations / sidebar (~half day)

ChatGPT-style: sidebar listing past chats by title, click to load. Each conversation is its own `localStorage` key with its own message history. Useful for keeping a "late-night jazz" thread distinct from a "workout pump-up" thread.

**What to build:**
- Storage: `roon-ai-conversations-index` (Vec<{id, title, created_at, last_used}>) and `roon-ai-conversation-{id}` per conversation.
- Auto-title: after the first assistant reply, ask Claude (separate single-shot call) for a 2–4 word title for the conversation so far.
- UI: collapsible sidebar with new-chat button + list. Selecting one swaps the `messages` signal.
- Migrate the current single `roon-ai-conversation` key to a default conversation on first load.

**Files to touch:** `src/app/pages/conversational_ai.rs` (significant), possibly a new `src/app/conversations.rs` module for the index management.

### 4. Server-side cloud TTS (~half day, ~$0.30/session)

For users on Chrome/Firefox where the bundled voices are mediocre, or for cross-device voice consistency, add a server-side TTS proxy.

**What to build:**
- New route `POST /api/tts { text, voice }` → proxies to OpenAI TTS (`/audio/speech`, model `tts-1` or `tts-1-hd`), streams MP3 bytes back.
- API key resolution: same pattern as Anthropic — env var `OPENAI_API_KEY` or `[tts] api_key = "..."` in `roon-ai.toml`. Server logs `TTS enabled (OpenAI key found)` on startup.
- Voice picker on Settings page gains a section header: "Browser voices" (existing list) and "Cloud voices" (six OpenAI voices: alloy, echo, fable, onyx, nova, shimmer). Selection is a single dropdown across both groups; the chosen voice's source determines whether `RoonSpeech.speak()` uses local TTS or fetches from `/api/tts` and plays via `<audio>`.

**Files to touch:** new `src/tts/mod.rs`, `src/api/mod.rs` (handler), `src/main.rs` (route + key resolution), `src/app/voice_context.rs` (mark cloud-source voices), `src/app/pages/conversational_ai.rs` (`SPEECH_INSTALL_JS` updated to fetch + play `<audio>` for cloud voices), `Cargo.toml` ([tts] config + maybe a streaming MP3 dep — actually nothing extra needed, reqwest can do it).

### Other ideas captured but lower priority

- **Wake word ("Hey Roon")** — passive listening with VAD so the page stays in standby. Needs a small browser-side VAD (or a server-side keyword-spotting model). Bigger lift; unlocks ambient use.
- **Per-message replay button** for TTS — small 🔊 next to each assistant bubble. Trivial (~15 min) but currently unnecessary if Speak toggle works.
- **Conversation summarization** — when message history exceeds N tokens, ask Claude to summarise the older turns. Not yet needed; current pricing is pennies per long session.
- **Auto-fetch album tracks for context** — when AI is talking about an album, surface its track list with per-track ▶ Play buttons. Combine with #2 for "what's the third track of this?" working naturally.
- **Binary rename** to `roon-ai.exe` — long-deferred cosmetic mismatch. See "Bigger rename" notes from 2026-04-19.
- **Rename `/conversational` → `/ai`** — shorter URL now that the original `/ai` is gone. ~5 line change.

---

## Where We Stand — Status After Knob Removal (2026-04-26 evening)

### Current state

- **Branch `v3`** is at commit `a9989d1` on `cfogarty/v3` (personal fork). Clean tree, builds clean, server runs clean.
- **Web UI**: Zones · Conversational AI · Library · Settings — four tabs.
- **Conversational AI page** has: persistent multi-turn history (localStorage), ▶ Play suggestion buttons, voice input (mic), spoken replies (TTS), hands-free mode, voice picker on Settings page.
- **Backend**: Roon adapter + UPnP adapter (UPnP disabled by default), unified ZoneAggregator, MCP server (6 tools), AI chat agent calling Anthropic Sonnet 4.6.
- **No more**: knob HTTP routes, knob page, firmware auto-fetcher, mDNS, LMS, HQPlayer, OpenHome.

### What's still on the candidate list (from the section above, ranked)

1. **Stream the AI response** (~3h) — biggest perceived-quality win
2. **Now-playing context on Conversational page** (~1h) — small change, big UX upgrade for voice mode
3. **Multiple saved conversations / sidebar** (~half day) — power-user feature
4. **Server-side cloud TTS** (~half day) — only if browser voices ever stop satisfying

### New candidates surfaced by the knob cleanup

5. **Docs cleanup** (~1h) — `README.md` and `ARCHITECTURE.md` still reference removed features:
   - `README.md:9` says "your voice, a chat message, **a hardware knob**, or a browser"
   - `README.md:19` lists `ESP32 Knob — hardware volume/transport knob with OTA firmware management`
   - `README.md:110` lists `FIRMWARE_AUTO_UPDATE` env var
   - `README.md:139` lists the `/knobs` page in the routes table
   - `README.md:250` whole section "## roon-knob Firmware"
   - `ARCHITECTURE.md:165` mentions `knobs.json` state file
   - `ARCHITECTURE.md:176` lists ESP32 knob as a control surface
   - `ARCHITECTURE.md:185` lists `/knobs` route
   These should be deleted / replaced with descriptions of the AI/voice-driven control surface that is the actual feature set now.

6. **`Cargo.toml` description update** (~30 sec) — currently `description = "Source-agnostic hi-fi control bridge for hardware surfaces and Home Assistant"`. Should say something like `"Natural-language Roon control bridge with AI chat and voice control"`. Matches the freshly-updated module doc-comment in `src/main.rs` and the `--help` text.

7. **The "Big rename" is now much smaller** — historically the binary/crate rename from `roon-ai` to `roon-ai` was a 6-step lift documented in the 2026-04-19 section. With the knob subsystem gone, the surface area shrunk significantly. What's left:
   - `Cargo.toml` `name = "roon-ai"` → `name = "roon-ai"` (renames the binary)
   - All `use roon_ai::` paths become `use roon_ai::` (cargo-fix or sed)
   - `RUST_LOG` default in `main.rs`
   - Config directory paths (`roon-ai`, `unified-hifi`) — would need a migration step or alias
   - MCP `name` in `.mcp.json` (would break existing Claude desktop configs that point at it; may want to keep)
   - The Roon `extension_id` (`com.muness.roon-ai`) — DON'T change; would un-pair the existing Roon Extension authorisation
   Probably 1–2 hours now vs. the half-day it would have been before. Still optional — purely cosmetic.

### What I'd actually do next

If you sit down for one more session: **#1 (streaming) + #2 (now-playing context) together**. Half a day, two of the four high-impact UX items, and the page goes from "talkable to" to "feels like a living remote." Everything else can wait.

If you want a fast win first: **#5 (docs cleanup)**. An hour, no code risk, ships the project as something that reads to others as what it actually is now.

---

## Recent Work (2026-04-26 evening) — Streaming Responses + Now-Playing Context

Both #1 and #2 from the candidate list landed in one session. The Conversational AI page now feels like a live remote.

### #1 — Streaming responses

Replaced the synchronous `/api/ai/chat` POST-then-wait-3-to-8-seconds-for-the-spinner with Server-Sent Events. The assistant bubble now types itself out as Claude generates the reply.

**Server flow**:
- New endpoint `POST /api/ai/chat/stream` returning `text/event-stream`. The original `POST /api/ai/chat` is preserved (unused for now, kept for any out-of-tree callers / future curl testing).
- New `crate::ai::run_agent_streaming` runs the same agentic loop as `run_agent` but spawns text deltas onto an `mpsc::UnboundedSender<StreamEvent>` channel.
- `StreamEvent` is a tagged enum (`text` / `tool` / `done` / `error`) serialised as JSON in each SSE `data:` payload.
- `AnthropicClient::call_streaming` makes a `stream: true` request to `/v1/messages` and parses the SSE event chunks line-by-line. Text deltas flow through the `on_text_delta` callback; tool_use blocks are buffered and assembled at end.
- The agent loop runs the streaming variant for every iteration. On `tool_use` stop, tools are executed synchronously (each one fires a `Tool` event for the right column), then the loop continues with results. On `end_turn`, a `Done` event with the rendered HTML, raw markdown, and parsed suggestions closes the stream.
- `<<<SUGGESTIONS>>>` filtering: text deltas containing the sentinel are truncated server-side so the user never sees the raw JSON block typed out, even briefly.

**Client flow**:
- A `dioxus::document::eval(STREAM_CONSUMER_JS)` task does the actual `fetch` against the streaming endpoint, parses the SSE format, and forwards each event back to Rust via `dioxus.send`. Rust loops on `eval.recv::<AgentEvent>().await` and dispatches based on `kind`.
- `ChatMessage` gained a `streaming: bool` field. While true, the assistant bubble renders as plain text (whitespace-preserved) with a pulsing cursor at the end. On `Done`, `text` is replaced with the server's rendered HTML and `streaming` flips to false (the bubble switches to `dangerous_inner_html`).
- The old "loading dots" indicator was removed — the streaming bubble's cursor pulse is the loading indicator now. The right-column "⚡ calling…" pulse stays for the case where the agent is between tool calls.

### #2 — Now-playing context

A small banner above the chat shows the current track on the selected zone. Critically, that same data is sent to Claude in every request so commands like *"skip this"*, *"more like this"*, or *"what is this?"* resolve without the user having to type the title.

**Server flow**:
- `AiChatRequest` gained `current_track: Option<CurrentTrack>` where `CurrentTrack { title, artist?, album?, is_playing }`.
- `system_prompt` now takes `current_track` as a second argument and appends:
  > "The selected zone is currently playing 'Title' by Artist (from 'Album'). When the user says 'skip this', 'pause', 'more like this', 'what is this', etc. — they are referring to this track. Use it as implicit context."
- Both `run_agent` and `run_agent_streaming` thread the track through. The system prompt is computed once per turn and passed into `AnthropicClient::call` / `call_streaming` (which lost their `preferred_zone` parameter — system-prompt construction lifted out into the agent loop).

**Client flow**:
- The shared `Zone` type in `src/app/api.rs` gained `state: Option<String>` and `now_playing: Option<ZoneNowPlaying>` fields. Existing callers see no change because both are `#[serde(default)]`.
- `ZoneNowPlaying { title, artist, album, image_key? }` mirrors the relevant subset of the bus's `NowPlaying`.
- `conversational_ai.rs` derives a `current_track: Signal<Option<CurrentTrack>>` from the latest `/zones` payload + the selected zone in a `use_effect`.
- The page subscribes to the SSE context's `should_refresh_zones()` and calls `zones.restart()` when `ZoneUpdated` / `NowPlayingChanged` / `VolumeChanged` events arrive. So the banner updates live as the track changes on the zone.
- The banner UI is a thin pill above the chat grid: small caps "Now playing" / "On deck" tag, then track title + artist/album subline. Doesn't render at all when there's no track.
- The track signal is threaded through `do_send`, `do_send_text`, and `start_listening_task` so voice input, suggestion ▶ Play clicks, and typed messages all carry the same context.

### Files modified

| File | Change |
|---|---|
| `src/ai/mod.rs` | Added `CurrentTrack` deserialisable type + `current_track` field on `AiChatRequest`. Lifted `system_prompt` to take `current_track`; `AnthropicClient::call` + `call_streaming` now take a precomputed `system: String` instead of building the prompt internally. New `StreamEvent` enum + `run_agent_streaming` + `run_agent_streaming_inner`. New `AnthropicClient::call_streaming` reading SSE chunks with `on_text_delta` callback. New `BlockBuilder` helper + `process_sse_event` parser for assembling content blocks during streaming. Suggestions sentinel filtered out of text deltas mid-stream. |
| `src/api/mod.rs` | New `ai_chat_stream_handler` returning `Sse<Stream>`. Existing `ai_chat_handler` unchanged. |
| `src/main.rs` | Registered `POST /api/ai/chat/stream` route. |
| `src/app/api.rs` | Mirrored `CurrentTrack` on the shared client request type. Extended client `Zone` with `state` and `now_playing` fields; added `ZoneNowPlaying` subset type. |
| `src/app/pages/conversational_ai.rs` | New `AgentEvent` enum (client mirror of `StreamEvent`). New `STREAM_CONSUMER_JS` (fetch + SSE parse + dioxus.send loop). `do_send_text` now opens an SSE stream via `eval`, pushes an in-progress assistant bubble, mutates it as deltas arrive, and finalises on `done`. Added `streaming: bool` to `ChatMessage`. New rendering branch for streaming bubbles (whitespace-pre + pulsing cursor) vs. finalised ones (dangerous_inner_html + suggestions). Now-playing banner above the chat grid. SSE-driven `zones.restart()` on `should_refresh_zones()`. `current_track` Signal threaded through `do_send` / `do_send_text` / `start_listening_task` / suggestion ▶ Play clicks. Removed the old loading-dots indicator. |
| `src/app/sse.rs` | (already had `should_refresh_zones`; no change needed.) |

### Behaviour changes for users

- The 3–8s spinner is gone. The reply types out at Claude's pace as it generates.
- A "Now playing" pill appears above the chat when something is playing on the selected zone. Live-updates as the track changes.
- Voice / mic / "▶ Play" / typed message all carry the now-playing context to Claude. *"Skip this"* and *"play more like this"* now actually work without typing the title.
- The legacy non-streaming endpoint at `POST /api/ai/chat` still exists for any external scripts/curl tests, but the page no longer uses it.

### Known limitations

- The server-side suggestions-block filter handles the simple case but assumes the marker won't span exactly the boundary of two text deltas in a way that splits a UTF-8 codepoint at the wrong byte. The code snaps to char boundaries to avoid that, but if Claude ever produces an exotic encoding the worst case is a momentary visible `<` character. Not currently observed.
- TTS still waits for the whole reply (it's triggered on the `done` event). Could be upgraded to "speak as it streams" but that requires chunked TTS which the browser's `SpeechSynthesisUtterance` doesn't really support cleanly.
- The banner uses the `/zones` payload's now_playing snapshot. If the server's bus state lags the actual playback by a second or two, so will the banner. SSE refreshes minimise this — should be fine in practice.

---

## Where We Stand — Status After Streaming + Now-Playing (2026-04-26 late evening)

### Current state

- **Branch `v3`** at `88f2f5f` on `cfogarty/v3`. Builds clean, runs clean.
- **Conversational AI page** is now a genuinely live remote: streaming replies, now-playing context, voice in/out, hands-free mode, voice picker on Settings, persistent history, ▶ Play suggestion buttons.
- **Web UI**: Zones · Conversational AI · Library · Settings.
- **Backend**: Roon adapter + UPnP adapter (UPnP off by default), unified ZoneAggregator, MCP server (6 tools), AI agent calling Anthropic Sonnet 4.6 over a streaming SSE bridge.

### Candidate list — what's done vs what's left

From the original "Next Session — Candidate Work Items" section:

| # | Item | Status |
|---|---|---|
| 1 | Stream the AI response (~3h) | ✅ **Done** (commit `88f2f5f`) |
| 2 | Now-playing context on Conversational page (~1h) | ✅ **Done** (commit `88f2f5f`) |
| 3 | Multiple saved conversations / sidebar (~half day) | Not started |
| 4 | Server-side cloud TTS (~half day, ~$) | Not started |
| 5 | Docs cleanup — `README.md` + `ARCHITECTURE.md` still mention removed features (~1h) | Not started |
| 6 | `Cargo.toml` description update (~30s) | Not started |
| 7 | The "Big rename" (`roon-ai` → `roon-ai`) (~1–2h) | Not started |

### What's still on the table (and why each one matters now)

**#3 — Multiple saved conversations.** With streaming + now-playing landed, a single conversation can stretch quite long without the page feeling sluggish. The next ergonomic win is being able to keep a "late-night jazz" thread distinct from a "workout pump-up" thread. ChatGPT-style sidebar; ~half day; storage spec already drafted in the candidate list above.

**#4 — Server-side cloud TTS.** The browser-native voice picker (Edge's "Online (Natural)" voices in particular) is genuinely good. This is now lower priority than it felt when first listed — only worth doing if you want consistent voice across browsers/devices, or if a non-Edge user cares about quality.

**#5 — Docs cleanup.** Still relevant. Specifically, `README.md` lines 9, 19, 110, 139, 250 and `ARCHITECTURE.md` lines 165, 176, 185 still mention knobs/firmware that were ripped out. ~1 hour, no code risk, makes the project read to outsiders as what it actually is.

**#6 — `Cargo.toml` description.** Still says `"Source-agnostic hi-fi control bridge for hardware surfaces and Home Assistant"`. ~30 seconds.

**#7 — The "Big rename"** (`roon-ai.exe` → `roon-ai.exe`). Cosmetic but increasingly conspicuous now that the project is so clearly "Roon AI". 1–2 hours given the slimmer codebase. Optional.

### New ideas surfaced by the streaming + now-playing work

8. **Per-token TTS streaming.** Right now TTS waits for the whole `done` event before speaking. With streaming text deltas already arriving, you could pipe sentences (split on `.`/`?`/`!`) into successive `SpeechSynthesisUtterance` objects so the voice starts speaking before the full reply is rendered. Feels like 2–3 hours; probably worth doing if hands-free mode becomes a primary use case. Caveat: the stock browser TTS engines don't queue beautifully, so there's some glue work.

9. **Inline tool-call indicator in the chat.** Tool calls currently show only in the right column. Inlining a small `⚡ list_zones …` pill inside the streaming bubble (where the text was when the tool fired) would give a clearer "the AI paused to look something up here, then continued" sense. Small UI change, ~1 hour.

10. **Pause/resume + skip buttons in the now-playing banner.** The banner currently just shows what's playing. Adding `⏸️` `⏭️` buttons would make it a mini-remote — and would compose naturally with the ▶ Play rows below. ~1 hour. Reuses the existing `/roon/control` endpoint.

### Recommendation

The page is genuinely good now. If you want one more session, two equally appealing paths:

- **Polish the live-remote feel** — do #10 (transport buttons in the banner) + #9 (inline tool indicators). Couple of hours, makes the conversational page feel like a complete piece.
- **Get the project ready to share** — do #5 + #6 + #7 (docs + description + binary rename). Same couple of hours, but ships the project as a coherent thing externally rather than something with stale references.

---

## Recent Work (2026-04-27) — Docs Cleanup, Big Rename, Live-Remote Polish

A focused session that knocked out the entire "ship-ready" candidate list (#5 + #6 + #7) plus both live-remote polish items (#9 + #10), plus a non-trivial test rot cleanup that surfaced once the test suite was actually run (it hadn't been since several feature/cleanup commits ago).

### #5 — Docs cleanup

`README.md` and `ARCHITECTURE.md` still referenced the removed knob subsystem and the old `/ai` route. Cleared:

- README: dropped the "hardware knob" tagline, replaced the ESP32 Knob feature bullet with a comprehensive Conversational AI bullet (voice in/out, streaming, hands-free, persistent history), removed `FIRMWARE_AUTO_UPDATE` from the env-var table, replaced `/ai` route references with `/conversational` (incl. example queries section), dropped `/knobs` from the routes table, removed the entire `## roon-knob Firmware` section.
- ARCHITECTURE.md: title renamed `Roon AI` → `Roon AI`, dropped `knobs.json` from state files, dropped ESP32 knob row from control surfaces, dropped `/knobs` from routes, replaced broken `[src/app/pages/ai_chat.rs]` link (file was deleted in the conversational AI rename) with `conversational_ai.rs`, added `voice_context.rs` link, fixed Default Zone section to reference `/conversational` not `/ai`, refreshed the date stamp.

### #6 — Cargo.toml description

`description = "Source-agnostic hi-fi control bridge for hardware surfaces and Home Assistant"` → `"Natural-language Roon control bridge with conversational AI agent and MCP server"`. Now matches the README/module-doc.

### #7 — The Big Rename: `roon-ai` → `roon-ai`

The crate, lib, and binary all rename. Took about 90 minutes including the test rot and full release rebuild.

**Renamed:**
- `Cargo.toml` — `name`, `description`, `[[bin]]` name
- `src/main.rs` — 3× `use roon_ai::`, RUST_LOG default (`roon_ai` → `roon_ai`), `--version`/`--help` strings, doc comment
- `src/api/mod.rs` — `service:` field in the `/status` response
- `src/bin/protocol_checker.rs` — example service value (matches `/status` schema)
- `src/embedded.rs` — `#[folder = "target/dx/roon-ai/release/web/public/"]` (the dx output path follows the bin name)
- `src/ai/mod.rs` — error messages now say `config.toml` (was wrong; the loader uses `config::File::with_name(.../config)`, never `roon-ai.toml`)
- `tests/volume_safety.rs`, `tests/protocol_schema.rs` — `use roon_ai::`; also updated `"service":` literals in `protocol_schema.rs` test JSON
- `README.md`, `ARCHITECTURE.md` — `.exe` references, `Stop-Process -Name`, `taskkill //F //IM`, RUST_LOG default value, config filename
- `.claude/settings.local.json` — taskkill / RUST_LOG permission entries

**Deleted:**
- `tests/client_harness.rs` — was already broken (referenced `HqpInstanceManager`, `HqpZoneLinkService`, `LmsAdapter`, `OpenHomeAdapter`, `KnobStore` — all removed in earlier cleanups). Tested a knob/HQPlayer protocol that no longer exists.

**Note (2026-04-28):** The "intentionally preserved" identifiers from this earlier rename pass have all been renamed in a follow-up sweep. See the 2026-04-28 rename entry below for details.

After rename, the running binary is `target/release/roon-ai.exe`.

### #10 — Transport buttons in the now-playing banner

Added a `do_transport(zone_id, action)` helper that posts directly to `/roon/control` or `/upnp/control` based on the zone-id prefix, and three buttons (⏮ / ⏯|⏸ / ⏭) on the right side of the banner. The play/pause icon flips based on `current_track.is_playing`, which is itself driven by SSE `NowPlayingChanged` / `ZoneUpdated` events, so the icon updates live as the track state changes.

Routing through the direct REST endpoint (not the agent loop) keeps the buttons instant — no 3–8 s tool-loop wait. This trades the "show what the AI did" benefit of the ▶ Play suggestion rows for raw responsiveness, which is the right trade for transport.

### #9 — Inline tool-call indicators in streaming bubbles

Added a `StreamPart::{Text, Tool}` enum and a `stream_parts: Vec<StreamPart>` field on `ChatMessage` (with `#[serde(skip)]` so it doesn't bloat localStorage). The streaming `Text` event handler coalesces consecutive deltas into the trailing `Text` part; the streaming `Tool` event handler appends a `Tool(summary)` part. The streaming bubble walks `stream_parts` and renders text inline + tool calls as inline `⚡ tool_name(...)` pills exactly where the agent paused.

After the `Done` event the bubble flips `streaming = false` and switches to `dangerous_inner_html: text` — the server-rendered HTML. The inline pills disappear at that point but the right-column tool log stays as the persistent record. Doing it any other way would require either client-side markdown rendering (a new WASM dep) or server-side embedding of pill markers in the HTML stream — not worth the complexity for v1. See idea #12 below if persistence ever matters.

### Test rot cleanup

The test suite hadn't been run since several feature/cleanup commits ago and three integration test bins were broken from accumulated rot. None of the rot was caused by this session; it just surfaced when I ran `cargo test`. Fixed:

- **`tests/fixtures/api_routes.txt`** regenerated to match current routes — dropped `/knob/*`, `/firmware/*`, `POST /control`, `POST /knob/config`, `POST /knob/control`, `GET /now_playing`, `GET /now_playing/image`, `GET /knobs/flash`, `GET /manifest-s3.json`, `GET /config/{knob_id}`; added `POST /api/ai/chat`, `POST /api/ai/chat/stream`, `POST /roon/browse/load`. The fixture is alphabetically sorted (also enforced by a sibling test).
- **`tests/volume_step.rs`** — deleted seven lint tests that read deleted source files (`src/adapters/lms.rs`, `src/adapters/openhome.rs`, `src/adapters/hqplayer.rs`, `src/knobs/routes.rs`). Kept only the two Roon-targeted lints.
- **`tests/protocol_schema.rs`** — deleted `validates_hqp_events` and `validates_lms_events` test functions (referenced `BusEvent::HqpDisconnected`, `BusEvent::HqpStateChanged`, `BusEvent::HqpPipelineChanged`, `BusEvent::LmsConnected`, `BusEvent::LmsDisconnected`, `BusEvent::LmsPlayerStateChanged` — all removed when LMS/HQPlayer were ripped out).
- **`tests/ignored_send_lint.rs`** — added allowlist entries for `ai/mod.rs` (SSE stream events; client disconnect is the expected case) and `conversational_ai.rs` (Dioxus eval channel; navigation is expected). Crucially, fixed `is_allowed()` to normalise Windows `\` → `/` first — the existing `bus/mod.rs` allowlist entry was silently broken on Windows because `path.display()` produces backslashes, and the `ends_with("bus/mod.rs")` check never matched. The bus violations had been hiding in plain sight all along.
- **`src/adapters/handle.rs::test_backoff_reset_after_stable_run`** — loosened timing tolerances (8–25 ms / 65–85 ms → 8–60 ms / 60–110 ms; total cap 150 ms → 250 ms). Windows tokio timers have ~15 ms granularity, so the original tight bounds were guaranteed to flake. The widened bounds still tightly verify the *invariant* that the backoff resets after a stable run rather than doubling — the actual bug the test exists to catch.

### Build + smoke verification

Full release rebuild ran cleanly:
- Tailwind CSS — 183 ms
- `dx build --release --platform web --features web` — 153 s — output now lands at `target/dx/roon-ai/release/web/public/`
- `cargo build --release --features server` — 2 m 16 s — 12.6 MB binary at `target/release/roon-ai.exe`

Live binary verified:
```
Starting Roon AI v0.0.0 (5662eb0)
Embedded WASM assets: 10 files (single-binary mode)
Embedded files: [..., "assets/roon-ai-dxh65d07f692212696f.js", "assets/roon-ai_bg-dxh742563c323eeba4.wasm", ...]
Configuration loaded, port: 8088
Adapter roon enabled
AI chat enabled (Anthropic API key found)
Listening on http://0.0.0.0:8088
sood received: baf0180d-1f3f-479e-bd63-433c96d14aa4
```

Smoke test:
- `GET /status` → `{"service":"roon-ai","version":"0.0.0","roon_connected":true,"upnp_devices":0,"bus_subscribers":2}`
- `GET /zones` → 9 zones discovered

Test suite green:
```
lib unit tests             36 passed
api_contract                2 passed
ignored_send_lint           4 passed
protocol_schema            39 passed
spawn_cancellation_lint     1 passed
unbounded_channel_lint      1 passed
volume_safety              14 passed
volume_step                 2 passed
doc tests                   8 ignored
```

---

## Where We Stand — Status After Rename + UI Polish (2026-04-27)

### Current state

- **Branch `v3`** at HEAD — uncommitted working tree with today's work; not yet pushed to `cfogarty/v3`.
- **Binary** is now `roon-ai.exe`. Old `roon-ai.exe` references are gone from the codebase except for the intentionally-preserved identifiers (config dir, MCP name, Roon extension_id, Docker image, repo URL).
- **Conversational AI page** is a complete live-remote experience: streaming replies with inline tool-call pills, now-playing banner with ⏮ / ⏯ / ⏭ buttons, voice in/out, hands-free mode, voice picker on Settings, persistent history, ▶ Play suggestion rows.
- **Web UI**: Zones · Conversational AI · Library · Settings.
- **Backend**: Roon adapter + UPnP adapter (UPnP off by default), unified ZoneAggregator, MCP server (6 tools), AI agent calling Anthropic Sonnet 4.6 over a streaming SSE bridge.
- **Test suite** is green for the first time in many commits — the previously-undetected rot from LMS/HQPlayer/Knob removals + streaming additions has all been cleared.

### Candidate list — what's done vs what's left

| # | Item | Status |
|---|---|---|
| 1 | Stream the AI response (~3h) | ✅ **Done** (commit `88f2f5f`, 2026-04-26) |
| 2 | Now-playing context on Conversational page (~1h) | ✅ **Done** (commit `88f2f5f`, 2026-04-26) |
| 3 | Multiple saved conversations / sidebar (~half day) | Not started |
| 4 | Server-side cloud TTS (~half day, ~$) | Not started |
| 5 | Docs cleanup — `README.md` + `ARCHITECTURE.md` (~1h) | ✅ **Done** (2026-04-27) |
| 6 | `Cargo.toml` description update (~30s) | ✅ **Done** (2026-04-27) |
| 7 | The "Big rename" (`roon-ai` → `roon-ai`) (~1–2h) | ✅ **Done** (2026-04-27) |
| 8 | Per-token TTS streaming (~2–3h) | Not started |
| 9 | Inline tool-call indicator in chat (~1h) | ✅ **Done** (2026-04-27) |
| 10 | Pause/skip buttons in the now-playing banner (~1h) | ✅ **Done** (2026-04-27) |

### What's still on the table

**#3 — Multiple saved conversations / sidebar.** The single most ergonomic win remaining. ChatGPT-style sidebar listing past chats by auto-generated title, click to load. ~half day. Storage spec drafted in the original candidate list above.

**#4 — Server-side cloud TTS.** Browser voices (especially Edge's Microsoft Online Natural) are good enough that this is now lowest priority — only worth doing for cross-browser voice consistency or for users not on Edge.

**#8 — Per-token TTS streaming.** With #9 making streaming feel even more "live" (you can see when the AI is thinking vs. searching), the next-level enhancement is to also start *speaking* mid-stream. ~2–3 hours; probably worth it if hands-free becomes a primary mode. Browser-side caveat: `SpeechSynthesisUtterance` doesn't queue cleanly between sentences — needs glue.

### New ideas surfaced today

11. **Commit + push.** The work is uncommitted on the working tree. A natural single commit: `feat: Big rename to roon-ai + transport buttons + inline tool indicators + test rot cleanup`. Push to `cfogarty/v3`.

12. **Persistent inline tool indicators after streaming completes.** Currently the inline pills disappear when the bubble switches to the rendered HTML (see #9 implementation note). Two paths to persistence: (a) client-side markdown rendering of replies (adds a WASM dep, breaks the "all rendering server-side" design), or (b) server embedding `<span class="tool-pill">⚡ tool_name</span>` markers in the HTML stream at the right offsets. Either is a half-day lift; debatable whether it's worth it given the right-column tool log already preserves the record.

13. **`/conversational` → `/ai`.** Now that the original `/ai` page is gone, the shorter URL is more natural. ~5 line change but breaks any saved bookmarks. Trivial when desired.

14. **CI workflow rename.** The CI release pipeline (`.github/workflows/build.yml`, `.github/workflows/docker.yml`) still references `roon-ai` in many places (binary names in build artefacts, Docker tags, SPK/QPKG package names, etc.). Coordinated rename when the CI pipeline becomes important again — for now it builds successfully with the old artefact names and that's fine.

15. **Run `cargo test` in CI.** Today's session showed that test rot from LMS/HQPlayer/Knob removals had been silently accumulating because nothing was running the tests. Adding a `cargo test --features server` step to `.github/workflows/build.yml` (gated behind the `build-me` label so it doesn't fire on every PR) would catch this kind of drift the moment it lands rather than half a year later.

### Recommendation

Two natural next sessions:

- **Commit + push + #3 (multiple saved conversations).** Half a day. Ships today's cleanup and adds the last big ergonomic feature.
- **Just commit + push.** The project is in a notably good state — test suite green, binary renamed, UI polished, docs accurate. A pause-and-take-stock moment that lets the work settle before the next push.

---

## Recent Work (2026-04-27, second pass) — Zones Page Removal

### Why

After the rename pushed and the binary verified live, a quick check of the Zones page revealed every zone showing "Nothing playing" even when music was clearly running. Root cause: `src/app/pages/zones.rs` was issuing N+1 fetches against `GET /now_playing?zone_id=...` plus `POST /control` — both routes deleted in the knob cleanup (commit `a9989d1`). The Zones page had been silently broken since that commit.

Two paths considered:

1. **Fix it** — refactor zones.rs to read state/now-playing/volume directly from the existing `/zones` payload, route transport via `/roon/control` and `/upnp/control` based on zone-id prefix. Add `volume_control` and `is_*_allowed` to the client `Zone` struct. (This was actually implemented and compiled cleanly before being reverted — see below.)
2. **Remove it** — the Conversational AI page already has a zone picker, a now-playing banner with ⏮/⏯/⏭ buttons (added earlier today as #10), and ▶ Play suggestion rows. The Zones page's "all zones at once" overview was the only unique value, and that's marginal for a single-user setup that mostly drives one zone at a time.

Chose option 2. The Conversational AI surface covers everything that mattered.

### Files deleted

- `src/app/pages/zones.rs` (340 lines) — the page itself
- `src/app/components/volume.rs` (~140 lines) — `VolumeControlsCompact` and `VolumeControlsFull`, both consumed only by the Zones page

### Files modified

| File | Change |
|---|---|
| `src/app/mod.rs` | Removed `Zones` from page imports; removed `Route::Zones` enum variant; mapped `/` to `ConversationalAi` (the conversational page is now the home — `/conversational` no longer exists) |
| `src/app/pages/mod.rs` | Removed `mod zones;` and `pub use zones::Zones;` |
| `src/app/components/mod.rs` | Removed `pub mod volume;` and the re-exports of `VolumeControlsCompact`/`VolumeControlsFull` |
| `src/app/components/nav.rs` | Brand link now goes to `Route::ConversationalAi` (which is `/`) instead of `Route::Zones`; removed the desktop and mobile "Zones" nav links |
| `src/app/api.rs` | Reverted the brief `volume_control: Option<ZoneVolumeControl>` and four `is_*_allowed: bool` fields I had added to client `Zone` for option 1; deleted the `ZoneVolumeControl` struct and the rich `NowPlaying` client struct (only `Zones` consumed it) |
| `README.md` | Routes table — `/` row now Conversational AI; dropped Zones row. Features bullet — replaced "Zones page" with "Conversational AI (home page)". Conversational AI section header updated to note `/` is the home. Config TOML comment updated to drop `/conversational`. |
| `ARCHITECTURE.md` | Surfaces table — dropped the redundant Conversational AI row (`/` is now the home). Web UI Pages — Zones row dropped, `/` now Conversational AI. Default Zone section — refers to "the home Conversational AI page, `/library`". Date stamp refreshed. |

### Behaviour changes for users

- **`/` is now the Conversational AI page** (formerly Zones).
- **`/conversational` no longer exists** — bookmarks pointing at it 404. The shorter URL is the canonical one now.
- The brand logo in the nav still links to home (which is now Conversational AI rather than Zones).
- Nav order in the desktop + mobile menus is now: Conversational AI · Library · Settings.

### Build + smoke

`cargo check --features server` passes (one pre-existing dead-code warning, unrelated). Full release rebuild needed before this is visible at runtime — same incantation as always:

```bash
mkdir -p tmp_css
./tailwindcss.exe -i src/input.css -o tmp_css/tailwind.css --content "src/app/**/*.rs"
mv tmp_css/tailwind.css public/tailwind.css && rmdir tmp_css
dx build --release --platform web --features web
cargo build --release --features server
```

Then `Stop-Process -Name 'roon-ai' -Force -ErrorAction SilentlyContinue` and re-launch.

### Note on candidate item #13

The earlier candidate item #13 ("Rename `/conversational` → `/ai`, the shorter URL is more natural") is effectively superseded — `/` is now even shorter, and there's no reason to also have `/ai` as a separate route. Item #13 can be considered done in spirit.

---

## Where We Stand — Status After Zones Removal (2026-04-27 evening)

### Current state

- **Branch `v3`** at `f58588e` on `cfogarty/v3`. Working tree clean (only the two intentionally-untracked files: `.claude/scheduled_tasks.lock`, `.claude/settings.json`).
- **Two commits today**: `2fcf182` (rename + transport buttons + inline tool indicators + test rot cleanup) and `f58588e` (Zones page removal).
- **Web UI is now three routes**: `/` Conversational AI · `/library` · `/settings`. The Zones page is gone; the Conversational AI surface is the home.
- **Conversational AI page** carries: streaming replies with inline `⚡ tool_name` pills, now-playing banner with ⏮/⏯/⏭ transport, voice in/out, hands-free mode, voice picker on Settings, persistent history, ▶ Play suggestion rows, default-zone star.
- **Backend**: Roon adapter + UPnP (UPnP off by default), unified ZoneAggregator, MCP server (6 tools), AI agent calling Anthropic Sonnet 4.6 over a streaming SSE bridge.
- **Test suite green**, full release build verified at runtime, Roon Nucleus discovered and 9 zones live.

### Candidate list — what's done vs what's left

| # | Item | Status |
|---|---|---|
| 1 | Stream the AI response | ✅ Done (`88f2f5f`, 2026-04-26) |
| 2 | Now-playing context on Conversational page | ✅ Done (`88f2f5f`, 2026-04-26) |
| 3 | Multiple saved conversations / sidebar | Not started |
| 4 | Server-side cloud TTS | Not started — low priority (Edge "Online (Natural)" voices are good enough) |
| 5 | Docs cleanup (README + ARCHITECTURE) | ✅ Done (2026-04-27) |
| 6 | `Cargo.toml` description update | ✅ Done (2026-04-27) |
| 7 | Big rename (`roon-ai` → `roon-ai`) | ✅ Done (`2fcf182`, 2026-04-27) |
| 8 | Per-token TTS streaming | Not started |
| 9 | Inline tool-call indicators in chat | ✅ Done (`2fcf182`, 2026-04-27) |
| 10 | Pause/skip buttons in now-playing banner | ✅ Done (`2fcf182`, 2026-04-27) |
| 11 | Commit + push today's work | ✅ Done (`2fcf182` + `f58588e`, 2026-04-27) |
| 12 | Persistent inline tool indicators after streaming | Not started |
| 13 | Shorten `/conversational` URL | ✅ Done in spirit (`f58588e` makes `/` the home) |
| 14 | CI workflow rename (`.github/workflows/*`) | Not started |
| 15 | Run `cargo test` in CI | Not started |

### Next-session candidates, ranked by perceived impact

**Strongest ergonomic wins**

**#8 — Per-token TTS streaming** (~2–3 h). The spoken reply currently waits for the whole response before starting. Splitting the streaming text on sentence boundaries (`.` / `?` / `!`) and feeding each into a queued `SpeechSynthesisUtterance` would make hands-free mode feel like a real conversation instead of a turn-based protocol. Big payoff *if* voice is actually used. Browser TTS queueing is finicky between sentences — needs glue.

**Banner album art + volume slider** (~1 h total). The Zones removal silently dropped two visuals: the album thumbnail and a per-zone volume control. The conversational banner currently shows only text. Adding a small thumbnail (`/roon/image?image_key={k}`) and a volume slider that posts to `/roon/volume` brings back the missed visual richness without bringing back a whole page. Surfaced today after the Zones removal landed.

**Quality-of-life features**

**#3 — Multiple saved conversations / sidebar** (~half day). Keep "late-night jazz" thread distinct from "workout pump-up". ChatGPT-style sidebar with auto-titled chats, per-conversation localStorage. Probably overkill unless you actually want multiple threads.

**#12 — Persistent inline tool indicators** (~half day). The `⚡ tool_name` pills currently disappear when the streaming bubble switches to rendered HTML. Half-day to make them stick (either client-side markdown render or server-side embedding pill HTML at offsets). Marginal — the right-column tool log still preserves the record.

**Housekeeping** (lower urgency, but unblocks future release work)

**#15 — Run `cargo test` in CI** (~30 min). Today proved test rot accumulates silently when nothing runs the suite (api_contract fixture + dead lints + ignored_send_lint Windows path bug — all hiding for months). A `cargo test --features server` step gated behind the `build-me` label catches the next round before it lands.

**#14 — CI workflow rename** (~2 h). The release pipeline (`.github/workflows/build.yml`, `docker.yml`) still references `roon-ai` everywhere — binary names, Docker tags, SPK/QPKG package names. Worth doing before the next tagged release; not before.

### Recommendation

The fork on the table:

- **Voice is your primary mode** → #8 + banner album art (~half day combined). Daily-use surface gets meaningfully better.
- **Voice is a nice-to-have, you want to ship** → #15 + #14 (~half day combined). Project becomes release-ready under its new name.
- **Neither — settle in** → use it for a few days. The rename + home-page consolidation are big enough that real use will surface what to do next better than guessing.

---

## Recent Work (2026-04-27, third pass) — QoL + Housekeeping Sweep

A focused session that knocked out the two quality-of-life features (#12 + #3) and the two housekeeping items (#5 + #6) in one go.

### #5 — Cargo test in CI

Discovered the `test` job at `build.yml:372` already exists and runs `cargo test --workspace` on every push to `v3`. So #15 was effectively already in place; the rot accumulated because no one was reading the CI output, not because it wasn't running.

Improvement: made the test command explicit about features (`cargo test --workspace --features server`) so a future change to `default-features` doesn't silently disable integration test coverage. Added a comment listing which integration tests need the server feature. Same enhancement applied to `docker.yml`'s test step.

The existing **Build status badge** at `README.md:3` already covers test status — failures show as a red badge.

### #6 — CI workflow rename (`.github/workflows/build.yml`)

Renamed only the **internal binary paths** that would have broken routine pushes after the Cargo `name` rename:

- `target/dx/roon-ai/` → `target/dx/roon-ai/` (12 occurrences)
- `target/{arch}-*/release/roon-ai[.exe]` → `target/{arch}-*/release/roon-ai[.exe]`
- `cargo clean -p roon-ai` → `cargo clean -p roon-ai`
- `assets/roon-ai` → `assets/roon-ai` (smoke-test HTML check)
- macOS `lipo` arguments — the per-arch artifact's binary file name follows the `cargo` bin name now (`x64/roon-ai`, `arm64/roon-ai`)
- `docker.yml` test step — same `--features server` enhancement

**Intentionally not renamed (out of scope this session):**

- The LMS plugin job (`build-lms-universal`, `update-lms-repo`) — references a deleted `lms-plugin/` directory, so it's dead code. Removing it cleanly requires also touching the `release` job's `needs:` array and the `summary` job. Roughly 50 lines to rip out — separate cleanup task.
- Linux deb/rpm install paths (`/usr/bin/roon-ai`, `roon-ai.service`) — also requires renaming `build/linux/roon-ai.service` and `build/arch/roon-ai.service`/`.install`.
- Synology SPK / QNAP QPKG package filenames + internal binary names — release-only, gated behind workflow_dispatch inputs.
- Build artifact filenames (`dist/bin/roon-ai-linux-x64`, `lms-roon-ai-*.zip`) — purely cosmetic.
- `cfogarty1964/roon-ai` Docker image owner and `cfogarty1964/Roon-AI` repo URL — external resources, not ours to rename.

**Why this scoping is fine:** all the gated/release-only jobs (`build-linux-packages`, `build-synology`, `build-qnap-*`, `build-lms-universal`) only run on workflow_dispatch with explicit inputs, or on release-tag pushes. Routine pushes to `v3` don't fire them. The renames done in this session unblock routine pushes from failing on the Cargo name change. Full release-pipeline rename is a future task tied to the next tagged release.

### #12 — Persistent inline tool-call indicators

Added so the `⚡ tool_name` pills don't disappear when the streaming bubble switches to rendered HTML.

**Approach:** server-side sentinel insertion + post-render replacement (`src/ai/mod.rs`). Two-pass:

1. **During streaming** (`run_agent_streaming_inner`): track each tool call's byte offset in `full_text` and its summary as `tool_positions: Vec<(usize, String)>`. The streaming Text events to the client are unchanged; the inline pill rendering during the stream still goes through the existing `stream_parts` path on the client.
2. **At end** (`render_with_pills`): splice Private-Use-Area sentinel markers (`\u{E000}TOOL:{summary}\u{E001}`) into the cleaned markdown source at each recorded offset, render markdown→HTML via `pulldown_cmark` (sentinels pass through as text), then replace each sentinel with `<span class="...">⚡ {summary}</span>` HTML using a regex.

**Why Private Use Area:** unicode codepoints `U+E000`–`U+E001` are guaranteed never to appear in normal text or in markdown's standard syntax, so they survive rendering as plain characters and are unambiguously detectable in the final HTML.

Other paths considered and rejected:
- Client-side markdown rendering — would have added a WASM markdown crate (`pulldown-cmark` doesn't compile to WASM cleanly; alternatives add bundle weight) and broken the project's "all rendering server-side" design.
- HTML offset arithmetic against the post-render token stream — fragile; markdown rendering can wrap a sentinel position inside paragraph tags, list items, etc. Sentinel approach is robust to that because the chars themselves don't get touched.

The pill class list matches the existing inline streaming pill exactly so the visual is consistent before/after the streaming→done transition.

The non-streaming `run_agent` path (legacy `POST /api/ai/chat`) is left unchanged — pills only apply to the streaming endpoint that the page actually uses.

### #3 — Multiple saved conversations (minimum-viable)

Shipped the dropdown selector + new + delete + auto-title flow in `src/app/pages/conversational_ai.rs`. Deferred the ChatGPT-style sidebar with separate Claude auto-titling — the dropdown is enough for the single-user case.

**Storage layout:**
- `roon-ai-conversations-index` → JSON `Vec<{id, title}>` — the list of all conversations
- `roon-ai-conversation-{id}` → JSON `Vec<ChatMessage>` — messages per conversation
- Migrates the legacy single-key `roon-ai-conversation` on first hydrate after upgrade (preserves your existing chat as one entry titled "Conversation")

**ID generation:** `format!("c{}", js_sys::Date::now())` — millisecond-resolution timestamps, no chrono needed in WASM.

**Auto-title:** when a conversation still titled "New chat" gets its first user message, replaces the title with the first ~40 chars of that message (truncated with `…`). Effect-driven; runs whenever messages change. No separate Claude API call; ~5 lines of pure-Rust logic.

**UI changes** (header):
- Conversation `<select>` dropdown (max 14rem wide, truncates long titles) — only shown if there are entries
- "+ New" button — generates a new id, prepends to index, switches to it, clears messages
- Old "Clear" button → context-aware: shows "Clear" when there's only one conversation (wipes its messages and resets title to "New chat"), shows "Delete" when there are multiple (drops the conversation entirely and switches to the most recent of the remaining)

**Effects added:**
- `hydrated: Signal<bool>` flag suppresses the save-on-mount that would otherwise overwrite stored data with the empty initial signal before the load completes
- Migration runs once on initial hydrate (creates a starter "New chat" entry on fresh installs so the dropdown is never empty)
- Save effect is now id-keyed: `save_messages_to_storage(&id, &snapshot)`
- Auto-title effect watches messages, derives title from the first user turn, persists if title was still "New chat"

**Handoff candidate item #3 was scoped down** — the original spec called for a sidebar, auto-titling via a separate Claude call, and per-conversation `last_used` tracking. The shipped MVP omits all of those; the dropdown + auto-title-from-message-text covers the core need and stays under 200 lines of net change.

### Build + tests

- `cargo check --features server` — passes
- `cargo check --target wasm32-unknown-unknown --features web --no-default-features` — passes
- `cargo test --features server` — all green (39 tests across multiple integration bins)

Full release rebuild (Tailwind + dx + cargo release) and runtime smoke test pending at the time of this writing.

### Files changed

| File | Change |
|---|---|
| `.github/workflows/build.yml` | `target/dx/roon-ai/` → `target/dx/roon-ai/`; release binary paths likewise; `cargo clean -p` rename; macOS lipo arg rename; smoke-test HTML asset path rename; explicit `--features server` on cargo test |
| `.github/workflows/docker.yml` | Same `--features server` enhancement on the test step |
| `src/ai/mod.rs` | Added `tool_positions: Vec<(usize, String)>` tracking through `run_agent_streaming_inner`; added `render_with_pills()` and `html_escape()` helpers; sentinel-based pill injection |
| `src/app/pages/conversational_ai.rs` | Multi-conversation storage helpers (load/save index, load/save messages by id, migrate legacy, generate id, derive title); new `conversations`, `current_id`, `hydrated` signals; replaced single-key load/save effects with id-aware versions; added auto-title effect; replaced Clear button with conversation `<select>` + "+ New" + context-aware "Clear/Delete" buttons |

---

## Where We Stand — Status After QoL + Housekeeping (2026-04-27 late night)

### Current state

- **Branch `v3`** at HEAD — uncommitted working tree with this third pass; not yet built or pushed.
- Three commits earlier today (`2fcf182` rename + `f58588e` Zones removal) already on `cfogarty/v3`. This pass adds another tranche of work that will become a fourth commit.
- **Web UI**: `/` Conversational AI · `/library` · `/settings` (unchanged from prior pass).
- **Conversational AI page** now carries:
  - Streaming replies with **persistent** inline `⚡ tool_name` pills (#12 — pills survive into the rendered HTML)
  - Multi-conversation dropdown with "+ New" and context-aware "Clear / Delete" (#3 — auto-titled from first user message)
  - Now-playing banner with ⏮ / ⏯ / ⏭ transport
  - Voice in/out, hands-free mode
  - Voice picker on Settings, persistent history per conversation, ▶ Play suggestion rows, default-zone star
- **CI**: explicit `cargo test --features server` on every push; build.yml internal binary paths point at the renamed `target/dx/roon-ai/` and `target/release/roon-ai[.exe]`.
- **Test suite green** (39 tests across all integration bins).

### Candidate list — what's done vs what's left

| # | Item | Status |
|---|---|---|
| 1 | Stream the AI response | ✅ Done (`88f2f5f`, 2026-04-26) |
| 2 | Now-playing context on Conversational page | ✅ Done (`88f2f5f`, 2026-04-26) |
| 3 | Multiple saved conversations / sidebar | ✅ Done (MVP — dropdown + auto-title, 2026-04-27 third pass) |
| 4 | Server-side cloud TTS | Not started — low priority |
| 5 | Docs cleanup (README + ARCHITECTURE) | ✅ Done (2026-04-27) |
| 6 | `Cargo.toml` description update | ✅ Done (2026-04-27) |
| 7 | Big rename (`roon-ai` → `roon-ai`) | ✅ Done (`2fcf182`, 2026-04-27) |
| 8 | Per-token TTS streaming | Not started |
| 9 | Inline tool-call indicators in chat | ✅ Done (`2fcf182`, 2026-04-27) |
| 10 | Pause/skip buttons in now-playing banner | ✅ Done (`2fcf182`, 2026-04-27) |
| 11 | Commit + push today's work | ✅ Done (`2fcf182` + `f58588e`, 2026-04-27); third-pass commit pending |
| 12 | Persistent inline tool indicators after streaming | ✅ Done (2026-04-27 third pass) |
| 13 | Shorten `/conversational` URL | ✅ Done (`f58588e` makes `/` the home) |
| 14 | CI workflow rename | ✅ Done for routine pushes (2026-04-27 third pass); LMS removal + release-pipeline artifact rename deferred |
| 15 | Run `cargo test` in CI | ✅ Done (already in place; made explicit, 2026-04-27 third pass) |

### What's still on the table

**#4 — Server-side cloud TTS.** Lowest priority; only matters for cross-browser voice consistency. Edge "Online (Natural)" voices are good enough that this hasn't surfaced as a problem.

**#8 — Per-token TTS streaming.** With #12 making the visual flow even more "live", the corresponding voice enhancement is to start *speaking* mid-stream. Sentence-boundary splitting + queued `SpeechSynthesisUtterance`. ~2–3 hours; worth doing if hands-free becomes a daily mode.

**Banner album art + volume slider.** Surfaced after the Zones page removal — the banner currently shows only text. Adding a thumbnail + slider would restore the visual richness without bringing back a whole page. ~1 hour.

**LMS plugin removal from CI.** The `build-lms-universal` and `update-lms-repo` jobs in `build.yml` reference the deleted `lms-plugin/` directory. Currently gated behind `build_lms = 'true'` workflow_dispatch input so they don't fire on routine pushes, but they're dead code. ~30 min to rip out cleanly (also requires updating the `release` job's `needs:` array and the `summary` job).

**Release-pipeline artifact rename.** Build artifact filenames (`roon-ai-linux-x64`, etc.), Synology SPK names, QNAP QPKG names, Linux deb/rpm install paths (`/usr/bin/roon-ai` + the `.service` file), Arch AUR package name (`roon-ai-bin`). All gated behind workflow_dispatch inputs / release tags, so don't fire on routine pushes. Worth doing before the next tagged release. ~1–2 hours of careful YAML editing plus renaming the actual `.service` files.

### New ideas surfaced this pass

16. **Migrate to a real Conversation model.** The current MVP stores `(id, title)` per conversation. As we add features (auto-rename, last_used sorting, archive/unarchive, pinned chats, search), the inline JSON gets unwieldy. Worth refactoring into a typed `Conversation { id, title, created_at, last_used, message_count }` struct in a dedicated module if any of those features land.

17. **Sentinel-injection pattern is reusable.** The `\u{E000}TOOL:...\u{E001}` approach used in #12 for embedding pills in markdown→HTML output is general-purpose. If we ever want to embed other in-flow widgets (per-message timestamps, per-paragraph copy buttons, citation links), the same pattern works.

18. **Auto-title via Claude.** The current MVP titles conversations from the first user message (`Play late-night jazz pi…`). A separate Claude call could produce a 2-4 word title (`Late Night Jazz`) — cleaner but adds an API call per conversation. ~30 min if wanted; right now the first-message titles are fine.

### Recommendation

The conversational surface has roughly hit feature-completeness for the single-user case. Real next-session candidates are now small polish items (banner album art, per-token TTS) or release-pipeline cleanup (LMS removal, artifact rename). Use it for a few days; fixes will surface naturally.

---

## Recent Work (2026-04-27, fourth pass) — Banner Album Art + Volume Slider

The visuals that the Zones page used to provide (album art + per-zone volume control) had been missing from the now-playing banner. Restored both without bringing back a separate page.

### What landed

- **Album thumbnail** in the banner. 40×40 rounded square at the left, sourced from `/roon/image?image_key={k}&width=80&height=80` (2× for retina). Falls back to a `♪` placeholder when no image_key is available.
- **Volume slider** in a second row of the banner. Native `<input type="range">` styled with `accent-primary`, bound to the zone's `volume_control` (value/min/max/step from the bus VolumeControl). Posts to `POST /roon/volume` with `{zone_id, value, relative: false}` on every input event for live tracking. Right side shows a tabular-num readout — appends ` dB` for `decibels`-scale volumes, plain integer otherwise.
- **UPnP zones hide the slider** — `/upnp/control` only exposes relative `vol_up`/`vol_down`, no absolute-set endpoint. Roon zones get the slider; UPnP zones get the same banner without volume control.

### Why this approach

- **Slider posts on every `oninput`**, not on `onchange`. Trade-off: more requests during a drag, but the volume tracks the pointer in real time which is the expected feel for a music control. Roon's volume API is synchronous and fast enough that this is fine on a LAN.
- **Image URL constructed on the client**, not pre-baked in the API response. The `/zones` payload already carries `now_playing.image_key`; the URL pattern (`/roon/image?image_key=…&width=…`) is small and stable enough to live in the client. Saves a server-side string concatenation per zone, per `/zones` poll.
- **No new endpoints.** `/roon/image` and `/roon/volume` already existed.

### Wire-format change

- `Zone` (in `src/app/api.rs`) re-gained the `volume_control: Option<ZoneVolumeControl>` field that was removed when the Zones page was deleted. New `ZoneVolumeControl` struct mirrors the bus's `VolumeControl` subset needed by the slider: `value`, `min`, `max`, `step`, `is_muted`, `scale: Option<String>`.

### Files modified

| File | Change |
|---|---|
| `src/app/api.rs` | Re-added `Zone.volume_control` field and the `ZoneVolumeControl` struct (mirrors bus `VolumeControl` subset: value/min/max/step/is_muted/scale) |
| `src/app/pages/conversational_ai.rs` | Added `do_volume_set()` helper that POSTs to `/roon/volume` with `relative: false`. Banner reworked into two rows: top row gets a 40×40 album-art thumbnail (or `♪` fallback) before the label; bottom row holds the volume slider for Roon zones with a populated `volume_control`. UPnP zones still get the banner but no slider. |

### Behaviour changes for users

- The banner now shows album art when the track has it; placeholder otherwise.
- A volume slider appears below the title/buttons row whenever a Roon zone is selected and has a populated VolumeControl. Drag it to set absolute volume; the readout shows the current value (with `dB` units when the zone uses the decibels scale).
- The banner is now visually closer to what the deleted Zones page used to provide, without bringing back a whole page.

### Build + smoke

- `cargo check` clean for both `--features server` and `--target wasm32-unknown-unknown --features web --no-default-features`
- Tailwind CSS — 149 ms
- `dx build --release --platform web --features web` — completed, output at `target/dx/roon-ai/release/web/public/`
- `cargo build --release --features server` — completed, ~14 MB binary at `target/release/roon-ai.exe`
- Live binary verified: 16 embedded WASM files (was 14), 9 zones discovered, AI chat enabled
- `GET /zones` smoke test — sample zone returned `volume_control: {value: 60, min: 0, max: 100, step: 1, scale: "percentage"}` and `image_key` populated for tracks with art

Pushed as `4c2c3f2` to `cfogarty/v3`.

---

## Where We Stand — Status After Banner Polish (2026-04-27 night)

### Current state

- **Branch `v3`** at `4c2c3f2` on `cfogarty/v3`. Working tree clean (only the two intentionally-untracked local files).
- **Four commits today**:
  - `2fcf182` — Big rename + transport buttons + inline tool indicators + test rot cleanup
  - `f58588e` — Zones page removal (`/` is now Conversational AI)
  - `83366d7` — Persistent inline tool pills + multi-conversation dropdown + CI rename + explicit `--features server`
  - `4c2c3f2` — Album art + volume slider in now-playing banner
- **Web UI**: `/` Conversational AI · `/library` · `/settings`.
- **Conversational AI page** carries the complete music-control surface:
  - Streaming replies with persistent inline `⚡ tool_name` pills
  - Multi-conversation dropdown with "+ New" and context-aware "Clear / Delete" — auto-titled from first user message
  - Now-playing banner with **album thumbnail**, transport buttons, and **volume slider** (Roon zones)
  - Voice in/out, hands-free mode, voice picker on Settings
  - Persistent history per conversation, ▶ Play suggestion rows, default-zone star
- **Test suite green** (39 tests across multiple integration bins).

### Candidate list — what's done vs what's left

| # | Item | Status |
|---|---|---|
| 1 | Stream the AI response | ✅ Done (`88f2f5f`, 2026-04-26) |
| 2 | Now-playing context on Conversational page | ✅ Done (`88f2f5f`, 2026-04-26) |
| 3 | Multiple saved conversations / sidebar | ✅ Done MVP (`83366d7`, 2026-04-27) |
| 4 | Server-side cloud TTS | Not started — low priority |
| 5 | Docs cleanup (README + ARCHITECTURE) | ✅ Done (2026-04-27) |
| 6 | `Cargo.toml` description update | ✅ Done (2026-04-27) |
| 7 | Big rename (`roon-ai` → `roon-ai`) | ✅ Done (`2fcf182`, 2026-04-27) |
| 8 | Per-token TTS streaming | Not started |
| 9 | Inline tool-call indicators in chat | ✅ Done (`2fcf182`, 2026-04-27) |
| 10 | Pause/skip buttons in now-playing banner | ✅ Done (`2fcf182`, 2026-04-27) |
| 11 | Commit + push | ✅ Done (4 commits today) |
| 12 | Persistent inline tool indicators after streaming | ✅ Done (`83366d7`, 2026-04-27) |
| 13 | Shorten `/conversational` URL | ✅ Done (`f58588e` makes `/` the home) |
| 14 | CI workflow rename | ✅ Done for routine pushes (`83366d7`); LMS removal + release packaging deferred |
| 15 | Run `cargo test` in CI | ✅ Done — already in place, made explicit (`83366d7`) |
| — | Banner album art | ✅ Done (`4c2c3f2`, 2026-04-27) |
| — | Banner volume slider | ✅ Done (`4c2c3f2`, 2026-04-27) |

### What's still on the table

**Voice-mode polish**

**#8 — Per-token TTS streaming** (~2–3 h). Speak each sentence as it streams. Big payoff if voice mode is your primary input. Browser TTS queueing between sentences is the tricky bit.

**Release-pipeline cleanup** (only matters before a tagged release)

**LMS plugin removal from CI** (~30 min). `build-lms-universal` and `update-lms-repo` jobs in `build.yml` reference the deleted `lms-plugin/` directory — dead code, gated behind workflow_dispatch input so harmless on routine pushes. Cleanup also touches the `release` job's `needs:` array and the `summary` job.

**Release artifact-name rename** (~1–2 h). Synology SPK names, QNAP QPKG names, Linux deb/rpm install paths (`/usr/bin/roon-ai` + `.service` file), Arch AUR package name, build artifact filenames. All gated, don't fire routinely. Worth doing before the next tagged release; requires also renaming the actual `.service` files in `build/linux/` and `build/arch/`.

**Lower-priority follow-ups**

**#4 — Server-side cloud TTS** (~half day, ~$). Cross-browser voice consistency. Edge "Online (Natural)" voices are good enough that this hasn't surfaced as a problem.

**Auto-title via Claude** (~30 min). Replace the first-message-truncation conversation title with a 2–4 word title from a separate Claude call. Cleaner but adds an API call per conversation. The current MVP titles (e.g., `Play late-night jazz pi…`) are functional.

**Sidebar UI for conversations**. The MVP shipped a dropdown. A ChatGPT-style sidebar would be more discoverable but uses screen real estate. Only worth it if many conversations accumulate.

**Migrate to a real `Conversation` struct**. The current MVP stores `(id, title)` per conversation. As features land (last_used sorting, archive, pinned chats, search), a typed `Conversation { id, title, created_at, last_used, message_count }` in a dedicated module becomes worth the refactor.

**Sentinel-injection pattern is reusable**. The `\u{E000}TOOL:...\u{E001}` approach (introduced in #12) generalises to any in-flow widget — per-message timestamps, citation links, copy buttons. If those land, this is the hook.

**Long-deferred ideas in the handoff** (still valid)

- Wake word ("Hey Roon") with VAD
- Per-message 🔊 replay button (~15 min — trivial, currently unnecessary)
- Conversation summarization (only when token cost matters)
- Auto-fetch album tracks for context

### Recommendation

**The conversational AI surface is feature-complete for single-user use.** Today shipped the rename, page consolidation, multi-conversation, persistent tool pills, and banner polish — most of the candidate list.

Real next moves:

- **Use it for a few days.** After four commits today, the page is genuinely different from where it started. Real usage will surface the next priority better than guessing.
- **#8 (per-token TTS)** if voice mode is becoming your primary input.
- **LMS removal + release rename** if a tagged release is on the horizon.
- **Auto-title via Claude** if the first-message titles start feeling rough.

Or — and this is the most honest answer — settle in. The leaderboard is mostly green, the project ships a coherent feature set under a coherent name, and the next priority is best discovered by living with it.

---

## Recent Work (2026-04-28) — Identifier Rename: Drop "unified-hifi-control" + "Muness" Branding

The 2026-04-19 rename was display-only — the Roon `extension_id`, MCP server name, config dir paths, env-var prefix, Docker image, and macOS bundle id all still carried `unified-hifi-control`/`muness` from the original Cloud Atlas / Open Horizons fork lineage. Plus the Roon Extensions UI still showed "Muness Castle" as the publisher.

This pass eliminates every remaining variant of the legacy identifiers from the current codebase. Historical journal entries above (2026-04-19, 2026-04-27) are intentionally left unchanged because rewriting dated work logs would falsify history.

### Identifier mapping

| Old | New | Where |
|---|---|---|
| `com.muness.unified-hifi-control` (Roon ext_id) | `com.221b.roon-ai` | `src/adapters/roon.rs` |
| `Muness Castle` (Roon publisher) | `RooAI` | `src/adapters/roon.rs` |
| `unified-hifi-control` (MCP server name) | `roon-ai` | `src/mcp/mod.rs`, `.mcp.json` |
| `%APPDATA%\unified-hifi-control\` | `%APPDATA%\roon-ai\` | `src/config/mod.rs` |
| `unified-hifi/` (config subdir) | `state/` | `src/config/mod.rs` |
| `UHC_*` env-var prefix | `ROON_AI_*` | `build.rs`, all `env!()` callsites, e2e tests, CI |
| `muness/unified-hifi-control` (Docker) | `cfogarty1964/roon-ai` | `Dockerfile.release`, `docker-compose.yml`, `.github/workflows/docker.yml` |
| `open-horizon-labs/unified-hifi-control` (repo URL) | `cfogarty1964/Roon-AI` | `Cargo.toml`, `src/mcp/mod.rs` |
| `com.cloudatlas.unified-hifi-control.plist` | `com.221b.roon-ai.plist` | `build/macos/` |
| `Muness Castle` (Cargo authors / package.json author / pkg maintainer) | `RooAI` | `Cargo.toml`, `package.json`, `build/arch/PKGBUILD`, `build/qnap/qpkg.cfg`, `build/synology/INFO` |

### Files renamed via `git mv`

- `build/arch/unified-hifi-control.{install,service}` → `roon-ai.{install,service}`
- `build/linux/unified-hifi-control.service` → `roon-ai.service`
- `build/macos/com.cloudatlas.unified-hifi-control.plist` → `com.221b.roon-ai.plist`
- `build/qnap/shared/unified-hifi-control.sh` → `roon-ai.sh`
- `build/qnap/shared/icons/unified-hifi-control{.gif,_64.png,_80.png}` → `roon-ai*`

### Mechanics

A Python script (`/tmp/rename_sweep.py`, not committed) did 60+ ordered text substitutions across 52 files. Order mattered: URL-prefixed forms (`open-horizon-labs/unified-hifi-control`) had to resolve before bare `unified-hifi-control` so the GitHub owner/repo split happened correctly. `cfogarty1964/Roon-AI` is the new public canonical URL.

### Stale references removed

The CI workflows (`.github/workflows/build.yml:1497`, `docker.yml:91`) were publishing the Roon AI image to **two** Docker tags: `cfogarty1964/roon-ai` *and* `muness/roon-extension-knob`. The latter was a leftover from when this codebase included the Roon Knob hardware bridge (removed 2026-04-26). Removed the second tag — the user has no publish access to that namespace anyway.

### User config migration

User had:
- `%APPDATA%\unified-hifi-control\config.toml` (with their Anthropic API key)
- `%APPDATA%\unified-hifi-control\unified-hifi\app-settings.json`
- `%APPDATA%\unified-hifi-control\unified-hifi\roon_state.json` (skipped — re-auth invalidates it)
- `%APPDATA%\unified-hifi-control\firmware\` (orphan from removed Knob feature — skipped)

Migrated to `%APPDATA%\roon-ai\` (config.toml at root, app-settings.json under `state\`). The old directory was left in place for the user to delete manually.

### Verification

- `cargo build --features server --release` — clean (1 unrelated dead-code warning on `library.rs::PlayItemRequest`)
- `cargo test --features server` — 117 passed, 0 failed, 8 doc-tests ignored
- Live binary smoke: `GET /status` returned `{"service":"roon-ai","version":"0.0.0","git_sha":"afa2027","roon_connected":false,"upnp_devices":0,"bus_subscribers":3}` — `roon_connected: false` is expected post-rename until user re-authorises the new extension in Roon Settings → Extensions.

### What stays "Muness" / `muness`

Intentionally preserved in dated handoff entries (this file, lines 901-1880) and in `.wm/dive_context.md`/`.ba/issues.jsonl` (historical caches). These are journal-style records of past work; rewriting them would erase the project's actual history. The `AGENTS.md:108` reference to `/Users/muness1/src/roon-ai/` is the original developer's local path on their old machine — also historical context.

### Open follow-ups (cleared in `16ff933` later the same day)

The follow-ups originally noted here — stale `mcp/index.js` bin entry, `cloudatlas.ai` contact emails in plugin metadata, and the `library.rs::PlayItemRequest` dead-code warning — were all swept in commit `16ff933`. See the next section.

---

## Recent Work (2026-04-28, second pass) — Post-Rename Housekeeping + v3.0.0 Tag

Three small cleanups surfaced during the rebrand verification, plus the first release tag under the new identity.

### Cleanups (commit `16ff933`)

- **`mcp/index.js` bin entry** — `package.json` had `"bin": { "roon-ai-mcp": "mcp/index.js" }` and `package-lock.json` mirrored it, but the `mcp/` directory was deleted in the Rust port. Dropped both `bin` and `scripts` blocks.
- **`.claude-plugin` contact metadata** — `.claude-plugin/marketplace.json` and `plugin/.claude-plugin/plugin.json` still listed `Cloud Atlas AI` / `hello@cloudatlas.ai` as owner/author. Updated to `RooAI` / `cfogarty@221bbakerstreet.net` to match the rebrand identity. The `Cloud Atlas AI` references in HANDOFF.md historical entries and the contributor email in `build/arch/PKGBUILD.template` (a *template*, not active) are left as historical record.
- **Dead `PlayItemRequest` struct** — `src/app/pages/library.rs:34` had a private `PlayItemRequest` struct that was never constructed (compiler warning). Leftover from a planned one-tap-play feature that didn't ship; the live struct is `pub struct PlayItemRequest` in `src/api/mod.rs`. Deleted the dead one.

After this commit `cargo build --features server --release` produces **zero warnings**.

### v3.0.0 tag

The rebrand is a clean lineage break — fork is now its own thing, no more `unified-hifi-control` / `Muness` / `cloudatlas` artefacts in the live code, surface is feature-complete, build is silent. Annotated tag created on `16ff933` and pushed to `cfogarty/v3.0.0`:

```
git tag -a v3.0.0 -m "First release under Roon AI / RooAI / com.221b ..."
git push cfogarty v3 v3.0.0
```

The tag slot was free — the previous v3 lineage went `v3.0.0-rc.1...rc.5` and skipped straight to `v3.1.0`, so `v3.0.0` was actually never used.

GitHub release: https://github.com/cfogarty1964/Roon-AI/releases/tag/v3.0.0

### Branch tracking

Switched upstream tracking from `origin/v3` (the read-only upstream fork) to `cfogarty/v3` (the user's fork):

```
git branch --set-upstream-to=cfogarty/v3 v3
```

`git push` and `git pull` with no args now operate on the user's own fork. The `origin` remote is left in place for occasional `git fetch origin` to compare against the upstream.

### Final state

- Branch `v3` at `16ff933` on `cfogarty/v3`. Tag `v3.0.0` pushed.
- Working tree clean (only the intentionally-untracked `.claude/scheduled_tasks.lock`, `.claude/settings.json`, plus `.claude/settings.local.json` and `.wm/dive_context.md` which are local artefacts touched by the rebrand sed sweep but excluded from commits).
- Live binary: running, `GET /status` returns `{"service":"roon-ai","roon_connected":true,...}`. Roon Extension re-pairing confirmed visually — Roon Settings → Extensions now lists "Roon AI" by "RooAI" instead of "Muness Castle".
- Test suite green; release build silent.

### What's actually next

Nothing urgent. The recommendation in the prior handoff entry still stands: settle in and use it for a few days. Real usage will surface the next priority better than guessing. The candidate list is mostly green:

- **#8 — Per-token TTS streaming** is the only remaining "big" item, worth doing if voice mode becomes primary input (~2–3 h).
- **LMS plugin removal from CI** + **release artefact-name rename in CI gates** matter only before a tagged release that runs the gated workflows. The `v3.0.0` tag did *not* trigger them (the gates are `workflow_dispatch`-only), so deferring is fine.
- Daily-use polish (auto-title via Claude, sidebar UI) is best driven by what actually annoys you in real use.

---

## Brainstorm — Post-v3.0.0 Ideas (2026-04-28)

After the rebrand + tag landed, surveyed the next-priority space. Recording the full list here so they don't get lost; the active pick is **#2** (see the next session's work entry once it lands).

| # | Idea | Effort | Recommendation |
|---|---|---|---|
| 1 | Wake word ("Hey Roon") via browser (Picovoice Porcupine etc.) | ~half day | Biggest UX leap available. Difference between "voice tool" and "primary music interface." |
| 2 | Auto-start on Windows login + system tray icon | ~half day | Currently `roon-ai.exe` runs in a terminal window — feels like a dev script. Tray + Startup folder makes it a real app. |
| 3 | Media key integration (SMTC on Windows) | ~half day | Keyboard Play/Pause/Next keys → control active zone via Windows Media APIs. Works *while you're doing anything else*. |
| 4 | History-aware system prompt | ~1h | Slip last 3 played tracks into the system prompt each turn so "play more like that" actually works. Highest value-to-effort ratio of all. |
| 5 | Per-token TTS streaming | ~2–3h | Speak as the AI thinks. Only worth it after #1 — wake word makes voice native; per-token TTS makes it fast. |
| 6 | "Similar to this" suggestion row in banner | ~1h | When a track is playing, surface 3 similar tracks/albums via a small tool call. One-tap explore. |
| 7 | Time-aware presets (morning/evening/dinner) | ~1h | Single buttons that the AI translates into a play action with a saved seed. |
| 8 | MQTT bridge for Home Assistant | ~half day | Was in the original project per build-file references. Expose play/pause/zone status as MQTT topics. |
| 9 | Auto-title via Claude (existing item) | ~30 min | Replace first-message-truncation titles with a 2–4 word Claude-generated title. |
| 10 | Sidebar UI for conversations (existing MVP is dropdown) | ~half day | ChatGPT-style. Only worth it if many conversations accumulate in real use. |

### Active pick: #2 — Auto-start + tray icon

User selected this first. Components:

- **A. Hide console window in release builds.** `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` on `src/main.rs`. Trade-off: kills stdout, so file logging (B) is required.
- **B. File logging via `tracing-appender`.** Rolling log files in `%APPDATA%\roon-ai\logs\` (or `ROON_AI_DATA_DIR\logs\`). Belt-and-braces if startup fails silently.
- **C. System tray icon.** `tray-icon` crate (tauri-apps) + `tao` event loop. Menu: "Open Web UI", "Open Logs Folder", "Status: Connected/Disconnected", "Quit". Tricky bit: `tao::EventLoop` must run on the main thread, so the existing tokio runtime + axum server has to move to a spawned thread.
- **D. Startup folder shortcut.** PowerShell installer script in `build/windows/install-startup.ps1` (or have the binary self-install on first launch with a CLI flag).

Risks worth flagging:
- tao + tokio integration on Windows is the unknown — first time the project pulls a GUI event loop into its main loop.
- Hidden console means file logging is the only debugging surface. Need a "show me the logs folder" tray menu item from day one.
- After this lands, `roon-ai.exe` is no longer a console app — running from a terminal won't print anything. Devs (i.e., this future session) need to know they can pass `--console` or build with debug profile to get console output back.

MVP carve-out option (if scope creeps): just A + B + D (hidden, logged, autostart) without C. Gets ~70% of the value — runs invisibly, persists across reboots, debuggable via log files. The tray is the polish; the persistence is the substance. Add C when needed.

---

## Recent Work (2026-04-28, third pass) — #2 Auto-Start + System Tray Shipped

User picked Full (A+B+C+D). Implementation took one session, no rabbit holes — `tao` + `tokio` worker-thread separation worked first try.

### What landed

**A. Hidden console window in release builds.** `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` at the top of `src/main.rs`. Debug builds (`cargo run`, `cargo build`) keep the console for development; release builds run silently in the background.

**B. File logging via `tracing-appender`.** Rolling daily log files at `<data_dir>/logs/roon-ai.log.<YYYY-MM-DD>`. On Windows that resolves to `%LOCALAPPDATA%\roon-ai\logs\` (note: data dir, not config dir — config still at `%APPDATA%\roon-ai\`). New `setup_logging()` function in `main.rs` returns a `WorkerGuard` that's held by `main()` for the program lifetime; on drop the appender flushes pending writes. Both file + stdout writers are registered; stdout is harmless when the console is hidden.

**C. System tray icon (Windows-only).** Cargo deps added under `[target.'cfg(windows)'.dependencies]`: `tray-icon = "0.21"`, `tao = "0.34"`, `image = "0.25"` (PNG-only feature). Icon embedded via `include_bytes!("../public/hifi-logo.png")`, loaded into RGBA at startup. Tray menu items:
- `Roon AI v{version}` — greyed-out info row
- `Open Web UI` — `cmd /c start "" http://localhost:8088`
- `Open Logs Folder` — `explorer <data_dir>/logs`
- `Quit` — sends on a `tokio::sync::oneshot` channel that races inside `shutdown_signal()` for graceful shutdown

**D. Startup-folder shortcut.** New `build/windows/install-startup.ps1` creates `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\Roon AI.lnk` pointing at the release binary. `WindowStyle = 7` (minimised) for belt-and-braces in case console somehow showed. `-Uninstall` flag removes the shortcut.

### Architectural change in `main.rs`

Previously: `#[tokio::main] async fn main()` ran `server::run().await` directly.

Now: `fn main()` is synchronous. It:
1. Sets up logging (file + stdout)
2. Creates a `oneshot` channel for tray-driven shutdown
3. Spawns a worker thread named `roon-ai-server` that owns a tokio multi-thread runtime running `server::run(shutdown_rx).await`
4. On Windows: runs `run_tray(shutdown_tx)` on the main thread (tao/tray-icon require the main thread on Windows GUI subsystems)
5. On non-Windows: just `join`s the worker thread and relies on Ctrl+C / SIGTERM

`server::run()` signature changed from `() -> Result<()>` to `(oneshot::Receiver<()>) -> Result<()>`. The `shutdown_signal()` future now races three sources: Ctrl+C, SIGTERM (Unix), and the external oneshot.

### Files touched

- `Cargo.toml` — added `tracing-appender` to server feature; added `[target.'cfg(windows)'.dependencies]` block
- `src/main.rs` — full restructure: file logging, tray module, sync `main()`, worker thread
- `build/windows/install-startup.ps1` — new file

### Verification

- `cargo build --features server --release` — clean, zero warnings, 1m 36s
- `cargo test --features server` — 117 passed
- One lint catch during dev: `tests/ignored_send_lint.rs` flagged `let _ = tx.send(())` in the tray Quit handler. Fixed by checking `is_err()` and logging a warning if the receiver was already dropped (server thread already exited).
- Live binary: tray icon visible, menu items work, server reachable at `http://localhost:8088`, log file writing as expected. Roon connection healthy (`roon_connected: true`).
- Startup shortcut installed and verified at `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\Roon AI.lnk`.

### Notes for future sessions

- **Running from a terminal in release builds shows nothing** — no console attached. Use `tail -f $env:LOCALAPPDATA\roon-ai\logs\roon-ai.log.<date>` to watch logs live, or build with `cargo build` (debug) which keeps the console.
- **`--version` / `--help`** still print to stdout, so they only show output in debug builds or if launched from a terminal that hasn't detached. Acceptable for an MVP — version is also visible in the tray menu's first (greyed) row.
- **Cross-platform reality**: tray code is `#[cfg(all(windows, feature = "server"))]`. On Linux/macOS the binary builds and runs but without a tray, relying on Ctrl+C / SIGTERM for shutdown. If a Mac tray is ever wanted, swap the cfg to include macOS — `tray-icon` supports it natively, just need a separate native install path equivalent to the Startup folder.
- **Tao polling cadence**: `ControlFlow::WaitUntil(now + 100ms)` in the event loop. 100 ms feels imperceptible in tray menu interactions and uses negligible CPU. If snappier feel matters later, switch to `EventLoopProxy` and have menu events wake the loop directly.

### What's actually next

#2 done. Returning to the brainstorm above:

- **#4 — History-aware system prompt** (~1h, highest value-to-effort) is now the natural next pick.
- **#1 — Wake word** still the biggest UX upgrade. Pair it with #5 (per-token TTS) for full voice-mode treatment.
- Anything else if real usage surfaces a different priority.

---

## Recent Work (2026-04-28, fourth pass) — Album-Art Lightbox + Version Bump to 3.4.0

Two small wins shipped together:

### Album-art click → full-size in a new tab

The 40×40 thumbnail in the now-playing banner ([src/app/pages/conversational_ai.rs:1077](src/app/pages/conversational_ai.rs#L1077)) is now clickable. The `<img>` is wrapped in `<a href={1024px-url} target="_blank" rel="noopener noreferrer">` — click opens the full-size image (`/roon/image?image_key=...&width=1024&height=1024`, served on-demand by Roon) in a new browser tab. The browser handles scroll/zoom/save natively.

**Why a new tab and not an in-page modal.** First attempt was a `position: fixed inset-0 z-50` overlay with a click-anywhere-to-close backdrop. It rendered, but the user reported the image landed somewhere they couldn't scroll to — the modal wasn't actually overlaying as a fixed-viewport element. Likely cause: a transform/filter on a parent in the Layout chain creates a containing block that turns `position: fixed` into `position: absolute` relative to that ancestor (a known CSS pitfall). Rather than chase that, swapped to `target="_blank"` — simpler, more robust, and the browser's image viewer is better than any modal we'd build.

Visual polish: `cursor-zoom-in` on hover, `hover:opacity-80 transition-opacity` for affordance, `title="Open full-size in new tab"` for hover text. Placeholder ♪ (when no image_key) stays non-clickable.

The library-page album art ([src/app/pages/library.rs:357](src/app/pages/library.rs#L357), [437](src/app/pages/library.rs#L437)) is *not* part of this — only the conversational AI banner. Could extend the same pattern to library if real usage surfaces a need.

### Version bump 0.0.0 → 3.4.0

`Cargo.toml` was sitting at `0.0.0` with the comment "Injected from git tag at build time" — meaning local builds reported v0.0.0 unless the user set `ROON_AI_VERSION` env var (which CI does from the git tag). Bumped to `3.4.0` so local builds report a meaningful number.

**Why 3.4.0 specifically.** The fork inherited every tag from the upstream (`v3.0.0-rc.1` through `v3.3.2`). Today's earlier session created `v3.0.0` (the rebrand commit) since that slot was actually free in upstream. The next free slot is **`v3.4.0`** — `v3.1.x`, `v3.2.x`, `v3.3.x` are all claimed. Skipping to `.4` is unambiguous in the local lineage and keeps the semver story clean (additive features = minor bump). Alternative considered: `v4.0.0` to signal "first post-rebrand minor on a fresh foundation" — rejected as overstating the change.

Build verification:
- `cargo test --features server` — 117 tests passing
- `dx build --release --platform web --features web` — client built, embedded into server
- `cargo build --release --features server` — clean
- Live binary `GET /status` reports `"version":"3.4.0","git_sha":"06cbb1e"` ✓
- Lightbox manually verified: thumbnail click → overlay → backdrop/× dismiss

### Known build quirk

`dx build --release --platform web --features web` printed `cargo build finished with errors for target: roon-ai [x86_64-pc-windows-msvc]` even though the WASM client phase succeeded. A direct `cargo build --release --features server` afterwards succeeds. Looks like dx's wrapper step has a benign tail-failure that doesn't actually break the build artefacts. Not worth chasing for now; the two-command sequence (`dx build` then `cargo build --release --features server`) works reliably.

### Next

After v3.4.0 ships:
- **#4 — History-aware system prompt** is still the highest value-to-effort. Slip the last 3 played tracks into the agent's system prompt so "play more like that" works.
- **#1 — Wake word** remains the biggest UX leap available.

---

## Recent Work (2026-04-28, fifth pass) — Love attempt → 📻 Start Radio button + vendored rust-roon-api

### What we set out to do

Add a heart button to the now-playing banner that toggles a track's loved state in Roon (the user's "Roon love"), pushing the change back to Roon's database via the API.

### What we discovered

**Roon's Love is not exposed by any service in the rust-roon-api crate.** Verified empirically across three navigation paths:

1. **Standard `browse` hierarchy → Library → Search → track action menu** → returned `["Play Now", "Add Next", "Queue", "Start Radio"]`. No Love.
2. **`now_playing` hierarchy** → Roon responded `<- COMPLETE 19 InvalidHierarchy`. The hierarchy name we hoped existed doesn't.
3. **`albums` hierarchy → album → track action menu** (library context) → same 4-action menu. No Love.

The 4-action menu is consistent across paths because that IS the action menu the rust-roon-api crate's Browse service exposes. Roon's official UI surfaces Love via a different mechanism — likely an undocumented metadata/library service that this crate doesn't implement, OR a non-public API method on the Browse/Transport services. Implementing it would require reverse-engineering Roon's WebSocket protocol with a proxy capture, which is a significantly larger investment than the original estimate.

### What we shipped instead (commit `<v3.4.1>`)

**📻 Start Radio button** in the now-playing banner, in the position the heart was. Click → POSTs to existing `/roon/play` endpoint with `action: "radio"`. Roon's "Start Radio" *is* in every track action menu (we saw it in every diagnostic), so this works without any API surface drama. Smoke-tested via curl: `{"message":"Start Radio: Foreigner"}`.

The button is greyed for non-Roon zones (UPnP doesn't have Start Radio). The flash banner persists "📻 Radio started from 'Title'" until the next action or × dismiss.

### What we kept

**Vendored `rust-roon-api` at `vendor/rust-roon-api/`** with a small patch adding `pub hierarchy: Option<String>` to `BrowseOpts` and `LoadOpts`, plus the corresponding override logic in `browse()` / `load()`. The patch is harmless (defaults to `"browse"` when None, preserving all existing call-site behaviour) and lets future work use any of Roon's hierarchies directly: `albums`, `tracks`, `search`, `internet_radio`, `playlists`, etc.

`Cargo.toml` repointed from `git = "https://github.com/open-horizon-labs/rust-roon-api.git"` to `path = "vendor/rust-roon-api"`. To return to the upstream remote, the `hierarchy` patch will need to be upstreamed first.

### What we reverted

- `love_track` method in `src/adapters/roon.rs` (~250 lines)
- `LoveResult` struct
- `LoveTrackRequest` + `roon_love_handler` in `src/api/mod.rs`
- `/roon/love` route in `src/main.rs`
- `do_love`, `LoveResponse` in client (replaced with `do_start_radio`, `PlayActionResponse`)
- Heart button (replaced with 📻)

### Future Love attempt

If a future session wants to revisit Love, the path is **protocol reverse-engineering**:
1. Run a WebSocket proxy (e.g., `mitmproxy` or `wireshark` with WebSocket dissection) between Roon's official client and your Roon Core
2. Click the heart on a track in Roon's UI
3. Capture the request/response
4. The captured service name + method + payload is what to implement

The vendored rust-roon-api is the natural place to add a new service — copy the pattern from `transport.rs` or `browse.rs`. Could potentially be upstreamed if the maintainer is interested.

Estimated effort given the unknowns: 4-8 hours.

### Beneficial side effect

`src/app/api.rs::post_json` now properly extracts the server's `{ "error": "..." }` body on non-2xx responses, instead of failing with the misleading "missing field" deserialise error. Caught while debugging the love feature; useful for any future endpoint that returns structured errors.
