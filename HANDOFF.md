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

A personal fork has been pushed to **https://github.com/cfogarty1964/unified-hifi-control** (branch: `v3`).  
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

Pushed to personal fork: `https://github.com/cfogarty1964/unified-hifi-control` (branch `v3`)

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
