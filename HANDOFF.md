# Unified Hi-Fi Control (v3) — Developer Handoff

**Project**: `unified-hifi-control` — Roon AI hi-fi control bridge  
**Language**: Rust 1.84+ with Dioxus 0.7.3 fullstack (WASM client + SSR server)  
**License**: PolyForm Noncommercial 1.0.0  
**Repo**: `github.com/open-horizon-labs/unified-hifi-control` (branch: `v3`)  
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
   git clone https://github.com/open-horizon-labs/unified-hifi-control.git
   cd unified-hifi-control
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

To pre-configure adapters, create `./data/unified-hifi/unified-hifi-control.toml`:
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
docker compose exec unified-hifi-control bash  # Shell into running container
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
.\target\release\unified-hifi-control.exe
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
$env:UHC_CONFIG_DIR=".\local-data"; $env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
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
    "unified-hifi-control": {
      "type": "http",
      "url": "http://localhost:8089/mcp"
    }
  }
}
```

---

## Configuration

**Config directory resolution order**:
1. `UHC_CONFIG_DIR` env var
2. `CONFIG_DIR` env var (Node.js migration compat)
3. Platform default (`~/.config/unified-hifi-control`, `~/Library/Application Support/...`, `%APPDATA%/...`)
4. Current directory (fallback)

**New files** written to `unified-hifi/` subdirectory (Issue #76). Legacy root files read for backward compat.

**Main TOML** (`unified-hifi-control.toml`):
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
- `UHC_PORT` — override port (default: 8088)
- `RUST_LOG` — logging (default: `unified_hifi_control=debug`)
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
RUST_LOG=trace ./unified-hifi-control     # Verbose logging
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
- Running binary locks the `.exe` — use `taskkill //F //IM unified-hifi-control.exe` (double-slash for Git Bash)
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

`dx build` puts its server binary at `target\dx\unified-hifi-control\release\web\unified-hifi-control.exe`.  
The binary you actually *run* (`.\target\release\unified-hifi-control.exe`) is only updated by `cargo build`.  
Running only `dx build` leaves the old binary in place — UI changes will appear absent.

**Kill running process (PowerShell):**
```powershell
Stop-Process -Name 'unified-hifi-control' -Force -ErrorAction SilentlyContinue
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
- **API key**: `ANTHROPIC_API_KEY` env var (or `unified-hifi-control.toml`)
- **Model**: `claude-sonnet-4-6` (fast, capable, cost-effective for tool use)
- **Response**: synchronous JSON (no streaming in v1); UI shows a loading spinner

### Step-by-Step Implementation Plan

#### Step 1 — API key config

- Add `ANTHROPIC_API_KEY` env var lookup to `src/config/mod.rs` (or read directly in the AI module via `std::env`)
- Add optional `[ai]` section to `unified-hifi-control.toml` schema: `api_key = "sk-ant-…"`
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
.\target\release\unified-hifi-control.exe
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

The config file lives at `%APPDATA%\unified-hifi-control\config.toml` (Windows).  
Full path: `C:\Users\ChrisFogarty\AppData\Roaming\unified-hifi-control\config.toml`

The file already exists. Open it and set your key:

```toml
port = 8088

[ai]
api_key = "sk-ant-..."
```

Get your key from https://console.anthropic.com/settings/keys. Then stop and restart the binary:

```powershell
Stop-Process -Name 'unified-hifi-control' -Force -ErrorAction SilentlyContinue
$env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
```

**Option B — Environment variable (current session only)**

```powershell
$env:ANTHROPIC_API_KEY="sk-ant-..."
$env:RUST_LOG="debug"
.\target\release\unified-hifi-control.exe
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
%APPDATA%\unified-hifi-control\config.toml
C:\Users\ChrisFogarty\AppData\Roaming\unified-hifi-control\config.toml
```

Contents:
```toml
port = 8088

[ai]
api_key = "sk-ant-..."
```

**Note**: The `data/unified-hifi/` directory in the project root is Docker-only. The native Windows binary reads from `%APPDATA%\unified-hifi-control\` (or `UHC_CONFIG_DIR` if set).

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
- PowerShell: `Stop-Process -Name 'unified-hifi-control' -Force -ErrorAction SilentlyContinue`
- Git Bash: `taskkill //F //IM unified-hifi-control.exe`
- Note that the running binary locks the `.exe` on Windows — always stop before rebuilding.

---

## Recent Work (2026-04-19) — Display Rename to "Roon AI"

All user-visible strings renamed from "Unified Hi-Fi Control" to "Roon AI". This is a display-only change — binary name, crate name, config paths, and the Roon extension ID are unchanged to avoid breaking existing installs.

### Files changed

| File | What changed |
|---|---|
| `src/app/components/layout.rs` | Browser tab title suffix (`… - Roon AI`) + footer text |
| `src/app/components/nav.rs` | Logo `alt` attribute |
| `src/adapters/roon.rs:1492` | Roon extension display name (shown in Roon → Settings → Extensions) |
| `src/main.rs` | Startup log line, mDNS advertised name, Flash Knob page `<title>` |
| `src/mcp/mod.rs` | MCP server `title` field and instructions header |

### What was intentionally left unchanged

- **`extension_id`** (`com.muness.unified-hifi-control`) — changing this would de-authorise the existing Roon Extension and require re-pairing in Roon Settings
- **MCP `name`** (`unified-hifi-control`) — identifier used in `.mcp.json`; changing it would break existing Claude MCP configs
- **Config directory paths** (`unified-hifi-control`, `unified-hifi`) — changing these would silently lose existing settings on disk
- **Binary / crate name** — larger lift; tracked as a future task

### Bigger rename (future)

To also rename the binary to `roon-ai.exe`:
1. Change `name` in `Cargo.toml`
2. Update all `use unified_hifi_control::` module paths (automated with `cargo fix` or sed)
3. Update `RUST_LOG` default in `main.rs`
4. Update LMS plugin `Plugin.pm` / `install.xml` binary references
5. Migrate or alias the config directory
6. Update CI matrix binary names and Docker image tags

---

## Known Gaps / Next Steps (updated 2026-04-19)

- **Binary rename**: Display strings say "Roon AI" but the binary is still `unified-hifi-control.exe`. See "Bigger rename" notes above.
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
