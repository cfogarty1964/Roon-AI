# Credits

Roon AI stands on a lot of shoulders. This file records the people and projects whose work is materially present in the current source tree — either as vendored code, project lineage we still benefit from, or models / services the app depends on at runtime.

If you contributed and aren't listed, please open an issue or PR.

---

## Project lineage

Roon AI descends from **[unified-hifi-control](https://github.com/open-horizon-labs/unified-hifi-control)**, originally created by **[Muness Castle](https://github.com/muness)**. The current Roon AI branch is a heavily rewritten and renarrowed fork — LMS, HQPlayer, OpenHome, the knob hardware integration, the MQTT bridge, and the Picovoice wake-word path have all been removed since the rename — but substantial portions of the Rust codebase still trace directly to Muness's work. Specifically (as of v3.8.1), `git blame` shows him as the primary author of:

- `src/adapters/roon.rs` (the Roon protocol adapter)
- `src/coordinator.rs` (the adapter coordinator)
- `src/aggregator.rs` (the zone aggregator)
- `src/mcp/` (the MCP server scaffolding)
- Significant portions of `src/main.rs`, `src/lib.rs`
- The original Cargo manifest structure, Dioxus + Tailwind + WASM single-binary build pipeline, GitHub Actions workflows, and Docker setup

Thank you, Muness — without your foundation this project wouldn't exist.

**[Michael Herger](https://github.com/michaelherger)** contributed the LMS plugin to the upstream project (14 commits across `lms-plugin/`). The LMS code path has since been removed (see HANDOFF.md, 2026-04-19), so none of his code currently ships in the binary, but his work on the upstream is acknowledged here.

---

## Vendored libraries

### `vendor/rust-roon-api/`

The Roon API client is **[rust-roon-api](https://github.com/theappgineer/rust-roon-api)** by **[The Appgineer](https://github.com/theappgineer)**, MIT-licensed. We vendor a fork of it (~3,800 lines) at `vendor/rust-roon-api/` because we carry a small patch adding `hierarchy: Option<String>` to `BrowseOpts` / `LoadOpts` so we can reach Roon's `now_playing` hierarchy. The upstream license file is preserved at `vendor/rust-roon-api/LICENSE`. We aim to upstream the patch and switch back to a git dependency.

`rust-roon-api` is itself a Rust port of **[node-roon-api](https://github.com/RoonLabs/node-roon-api)** by **[Roon Labs](https://roonlabs.com/)**, the original reference implementation of the Roon extension protocol.

---

## Models and runtime services

### Wake-word detection

- **[openWakeWord](https://github.com/dscripka/openWakeWord)** by **[David Scripka](https://github.com/dscripka)** — Apache 2.0. The actual ONNX models powering wake-word detection (`melspectrogram.onnx`, `embedding_model.onnx`, `silero_vad.onnx`, and the keyword classifier) are openWakeWord releases. Custom "Hey Roon" classifiers are trained via openWakeWord's official Colab notebook.
- **[openwakeword-wasm-browser](https://github.com/dnavarrom/openwakeword_wasm)** by **[Daniel Navarro Mantilla](https://github.com/dnavarrom)** — MIT. The npm package that wraps the openWakeWord ONNX pipeline for browser execution via [onnxruntime-web](https://onnxruntime.ai/docs/get-started/with-javascript/web.html). Originally based on a [reference write-up by Miro Hristov](https://deepcorelabs.com/open-wake-word-on-the-web/).
- **[Silero VAD](https://github.com/snakers4/silero-vad)** by the Silero team — MIT. Voice-activity gate used inside the openWakeWord pipeline.

### AI agent

- **[Claude](https://www.anthropic.com/claude)** by **[Anthropic](https://www.anthropic.com/)** — Claude Sonnet 4.6 powers the conversational agent (streaming, tool use, MCP integration); Claude Haiku 4.5 powers auto-titling and ✨ Similar suggestions. Used via the Anthropic Messages API.

### Speech

- **[OpenAI TTS](https://platform.openai.com/docs/guides/text-to-speech)** by **[OpenAI](https://openai.com/)** — Cloud voices (Alloy, Echo, Fable, Onyx, Nova, Shimmer) for spoken replies. Used via the Audio API. Browser-native voices via the [Web Speech API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Speech_API) are the offline fallback.
- **[Web Speech API](https://wicg.github.io/speech-api/)** — The browser-native speech-to-text used when the user dictates a request via the 🎙 button.

---

## Open-source crates

The Rust crate ecosystem powers virtually every layer of Roon AI. The most load-bearing crates:

- **[Dioxus](https://dioxuslabs.com/)** (Jonathan Kelley + contributors) — the fullstack framework
- **[axum](https://github.com/tokio-rs/axum)** — HTTP server
- **[tokio](https://tokio.rs/)** — async runtime
- **[rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk)** — MCP server implementation
- **[souvlaki](https://github.com/Sinono3/souvlaki)** — cross-platform media key bindings (SMTC on Windows)
- **[tray-icon](https://github.com/tauri-apps/tray-icon)** + **[tao](https://github.com/tauri-apps/tao)** (Tauri team) — system tray + windowing
- **[reqwest](https://github.com/seanmonstar/reqwest)**, **[serde](https://serde.rs/)**, **[rcgen](https://github.com/rustls/rcgen)**, **[rustls](https://github.com/rustls/rustls)**, **[ssdp-client](https://github.com/jakobhellermann/ssdp-client)**, and many more

A complete dependency list with licenses is in `Cargo.lock` and `vendor/rust-roon-api/Cargo.toml`. License compatibility is reviewed against this project's `PolyForm-Noncommercial-1.0.0` license; all current dependencies are MIT, Apache-2.0, BSD-3-Clause, or otherwise compatible.

---

## License

This project is licensed under **[PolyForm Noncommercial 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0/)** as declared in `Cargo.toml`. Vendored MIT/Apache code at `vendor/rust-roon-api/` retains its original license (see `vendor/rust-roon-api/LICENSE`). When redistributing, please honour both this project's license and the upstream licenses of vendored components.
