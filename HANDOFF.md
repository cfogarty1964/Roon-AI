# Unified Hi-Fi Control (v3) — Developer Handoff

**Project**: `unified-hifi-control` — source-agnostic hi-fi audio control bridge  
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

[lms]
host = "192.168.1.x"
port = 9000

[hqplayer]
host = "192.168.1.x"
port = 8088
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
│   ├── pages/           # Routable pages (zones, hqplayer, lms, knobs, settings)
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

lms-plugin/              # Perl-based LMS plugin (v3.3.2)
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

### LMS (`src/adapters/lms.rs`, ~2400 lines)
- **Protocol**: HTTP JSON-RPC (port 9000) + TCP CLI subscription (port 9090)
- **Architecture**: Two separate adapters — `LmsAdapter` (polling, 2–30s) + `LmsCliAdapter` (real-time events, optional). Factory: `create_lms_adapters()`.
- **Recent Fix**: Non-integer volume values now clamped safely to 0–100 (a8935b0).
- **Status**: Production.

### HQPlayer (`src/adapters/hqplayer.rs`, ~2400 lines)
- **Protocol**: TCP/XML port 4321 (control) + HTTP Digest port 8088 (profiles)
- **Multi-instance**: `HqpAdapter` + `HqpInstanceManager` + `HqpZoneLinkService`
- **Config files**: `hqp-config.json` + `hqp-zone-links.json`
- **Key semantics**: Use INDEX (not VALUE) for pipeline Set commands. Use State's `active_mode`, not Status.
- **Status**: Production. Recent pipeline semantics fixes.

### OpenHome (`src/adapters/openhome.rs`, ~950 lines)
- **Protocol**: SSDP discovery (5 OpenHome URNs) + SOAP/UPnP
- **Features**: Transport, volume, richer metadata than pure UPnP
- **Zone ID**: `openhome:<uuid>`
- **Status**: Production.

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
| `/library` | Library | Browse Roon library by genre, artist, composer, etc. |
| `/hqplayer` | HQPlayer | Config, instances, zone links, profiles, pipeline DSP |
| `/lms` | LMS | Server config, player discovery |
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

**Tools (10 total)**:

| Tool | R/W | Description |
|---|---|---|
| `hifi_zones` | R | List all zones across all adapters |
| `hifi_now_playing` | R | Track/artist/album/volume for a zone |
| `hifi_control` | RW | play/pause/next/prev/volume_set/up/down |
| `hifi_search` | R | Search library, TIDAL, Qobuz, LMS providers |
| `hifi_play` | RW | Search + play/queue/radio in one call |
| `hifi_status` | R | Bridge status, connected adapters, version |
| `hifi_hqplayer_status` | R | HQPlayer connection + pipeline state |
| `hifi_hqplayer_profiles` | R | List HQPlayer profiles |
| `hifi_hqplayer_load_profile` | RW | Switch HQPlayer profile |
| `hifi_hqplayer_set_pipeline` | RW | Change mode/samplerate/filter/shaper/dither |

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

[hqplayer]
host = "192.168.1.100"
port = 8088

[lms]
host = "192.168.1.101"
port = 9000
```

**JSON state files**: `app-settings.json`, `roon_state.json`, `hqp-config.json`, `hqp-zone-links.json`, `knobs.json`

**Key env vars**:
- `UHC_PORT` — override port (default: 8088)
- `RUST_LOG` — logging (default: `unified_hifi_control=debug`)
- `LMS_HOST` / `LMS_PORT` — auto-config LMS (used by LMS plugin)
- `LMS_POLL_INTERVAL` — base poll seconds (default: 2)
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

## LMS Plugin (v3.3.2)

**Location**: `lms-plugin/` (Perl)  
**Files**: `Plugin.pm`, `Helper.pm`, `Settings.pm`, `install.xml`

- Manages the UHC binary as a helper process (auto-start, crash restart, graceful stop)
- Install via LMS Settings → Plugins → Add repo: `lms-plugin/repo.xml` from GitHub
- Recent: Plugin category placement fix (e4a1a62)

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
- `POST /roon/browse { pop_all: true }` → root (Library, TIDAL, Qobuz, History…)
- `POST /roon/browse { item_key, session_key }` → drill into item
- `POST /roon/browse { item_key, zone_id, session_key }` → execute a browse **action** item (Shuffle, Start Radio, Play Now) on a zone
- `POST /roon/browse { pop_levels: 1, session_key }` → back one level
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

## Known Gaps / Next Steps (updated 2026-04-17)

- **Library page submenu**: When browsing to an album/track, action_list items (Play Now / Add to Queue / Start Radio) currently navigate into the submenu — a one-tap play shortcut could be added
- **Roon API fork**: Waiting for SO_REUSEADDR fix to merge upstream; then switch back to official crate
- **MCP auth**: No authentication on `/mcp` endpoint — assumes trusted LAN
- **E2E tests**: Playwright config exists in `e2e/` but coverage is limited

---

*Updated: 2026-04-17*
