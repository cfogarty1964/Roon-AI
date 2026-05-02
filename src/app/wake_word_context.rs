//! Wake-word ("Hey Roon") shared state.
//!
//! Detection runs entirely in the browser via the
//! [openWakeWord](https://github.com/dscripka/openWakeWord) ONNX models,
//! wrapped by the [`openwakeword-wasm-browser`](https://github.com/dnavarrom/openwakeword_wasm)
//! npm package (MIT). The Rust side stays adapter-agnostic — it just talks to
//! the `window.RoonWake` JS object that `WAKE_WORD_INSTALL_JS` (in
//! `conversational_ai.rs`) installs.
//!
//! No accounts, no cloud round-trips, no approval gates: replaces the prior
//! Picovoice Porcupine integration end-to-end.
//!
//! Setup the user has to do (one-time):
//!
//! 1. Run `scripts/setup-wake-word.ps1` (Windows) or `scripts/setup-wake-word.sh`
//!    (Linux/macOS). It downloads `openwakeword-wasm-browser`, bundles it to
//!    `public/wake-word/openwakeword.js` via esbuild, and copies the ONNX
//!    models + onnxruntime-web wasm runtime into `public/wake-word/`.
//! 2. Train a custom "Hey Roon" model in the official Colab — synthetic TTS
//!    data, no recordings needed, ~1 hour:
//!    <https://github.com/dscripka/openWakeWord#training-new-models>
//! 3. Save the resulting `.onnx` as `public/wake-word/models/wake_word.onnx`.
//!    (Until then, the setup script seeds it with the pretrained
//!    `hey_jarvis_v0.1.onnx` so the toggle works the moment the build picks
//!    up the assets.)
//! 4. Rebuild (`dx build` then `cargo build --features server`) so the new
//!    files get embedded into the single-binary server.
//! 5. Toggle the wake word on in Settings → "Hands-free wake word".
//!
//! The detection threshold (0.0–1.0, default 0.5) is the only runtime knob.
//! Higher = fewer false triggers but more missed wakes; lower = the inverse.

use dioxus::prelude::*;
use serde::Deserialize;

/// Events from `WAKE_WORD_LISTEN_JS`. Matches the JSON `{kind: "..."}`
/// shape that the eval task sends back.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WakeEvent {
    Ready,
    Detected,
    Failed { reason: String },
}

/// JS module that wraps the [openwakeword-wasm-browser] package for
/// browser-side wake-word detection. Idempotent: re-running this script is
/// safe if the module is already installed.
///
/// [openwakeword-wasm-browser]: https://github.com/dnavarrom/openwakeword_wasm
///
/// Until the user runs `scripts/setup-wake-word.{ps1,sh}` and rebuilds, the
/// engine never initialises and every method is a graceful no-op.
const WAKE_WORD_INSTALL_JS: &str = r#"
if (!window.RoonWake) {
    window.RoonWake = {
        ready: false,
        active: false,
        paused: false,
        engine: null,
        onDetection: null,
        threshold: 0.5,
        async _ensureSdkLoaded() {
            if (window.OpenWakeWord) return true;
            try {
                const head = await fetch('/wake-word/openwakeword.js', { method: 'HEAD' });
                if (!head.ok) return false;
            } catch (e) { return false; }
            if (document.querySelector('script[data-roon-oww]')) {
                for (let i = 0; i < 30 && !window.OpenWakeWord; i++) {
                    await new Promise(r => setTimeout(r, 100));
                }
                return !!window.OpenWakeWord;
            }
            return await new Promise((resolve) => {
                const s = document.createElement('script');
                s.src = '/wake-word/openwakeword.js';
                s.dataset.roonOww = '1';
                s.onload = () => resolve(!!window.OpenWakeWord);
                s.onerror = () => resolve(false);
                document.head.appendChild(s);
            });
        },
        async init(threshold) {
            if (this.ready) return true;
            const t = parseFloat(threshold);
            if (Number.isFinite(t) && t > 0 && t < 1) this.threshold = t;
            const sdk = await this._ensureSdkLoaded();
            if (!sdk) return false;
            try {
                const Engine = (window.OpenWakeWord && (window.OpenWakeWord.WakeWordEngine
                    || window.OpenWakeWord.default));
                if (!Engine) return false;
                this.engine = new Engine({
                    baseAssetUrl: '/wake-word/models',
                    ortWasmPath:  '/wake-word/ort/',
                    keywords: ['wake_word'],
                    modelFiles: { wake_word: 'wake_word.onnx' },
                    detectionThreshold: this.threshold,
                });
                await this.engine.load();
                this.engine.on('detect', () => {
                    if (this.paused) return;
                    try { if (this.onDetection) this.onDetection(); } catch (e) {}
                });
                this.ready = true;
                return true;
            } catch (e) {
                console.warn('openWakeWord init failed:', e);
                return false;
            }
        },
        async start() {
            if (!this.ready || this.active) return this.active;
            try {
                await this.engine.start();
                this.active = true;
                this.paused = false;
                return true;
            } catch (e) { return false; }
        },
        async pause() { this.paused = true; },
        async resume() { this.paused = false; },
        async stop() {
            if (this.active) {
                try { await this.engine.stop(); } catch (e) {}
                this.active = false;
            }
            this.engine = null;
            this.ready = false;
            this.paused = false;
            this.onDetection = null;
        }
    };
}
return true;
"#;

/// Long-running eval: receives `{threshold}` via `dioxus.recv()`, inits the
/// engine, then sends `{kind:"ready"}` and forwards each detection as
/// `{kind:"detected"}`. Sends `{kind:"failed", reason}` if the engine can't
/// initialise. The task is abandoned (eval dropped) when the user disables
/// the wake word — at which point a one-shot `RoonWake.stop()` halts it.
const WAKE_WORD_LISTEN_JS: &str = r#"
const cfg = await dioxus.recv();
const ok = await window.RoonWake.init(cfg && cfg.threshold);
if (!ok) {
    dioxus.send({ kind: 'failed', reason: 'init failed (run scripts/setup-wake-word and rebuild)' });
    return;
}
window.RoonWake.onDetection = () => { try { dioxus.send({ kind: 'detected' }); } catch (e) {} };
const started = await window.RoonWake.start();
if (!started) {
    dioxus.send({ kind: 'failed', reason: 'failed to acquire microphone' });
    return;
}
dioxus.send({ kind: 'ready' });
await new Promise(() => {});
"#;

#[allow(dead_code)]
const ENABLED_KEY: &str = "roon-ai-wake-word-enabled";
/// Stored as a string so localStorage stays simple. Parsed to f32 at use site.
/// Default behaviour when absent / unparseable: 0.5.
#[allow(dead_code)]
const THRESHOLD_KEY: &str = "roon-ai-wake-threshold";

#[derive(Clone, Copy)]
pub struct WakeWordContext {
    /// User opted in to listening for the wake word.
    pub enabled: Signal<bool>,
    /// Confidence threshold (0.0–1.0). Higher = stricter (fewer false
    /// triggers, more missed wakes). Stored as a string for localStorage
    /// simplicity; parsed where used. Empty string means "use default 0.5".
    pub threshold: Signal<String>,
    /// Detection counter — the conversational page watches this and triggers
    /// the same code path as a manual mic-click whenever it increments.
    /// Starts at 0; increments on each "Hey Roon" detection.
    pub detected_count: Signal<u32>,
    /// Human-readable status for the Settings UI: "Listening", "Idle",
    /// "Asset files missing", etc. Set by the conversational page's init effect.
    pub status: Signal<String>,
}

impl WakeWordContext {
    pub fn set_enabled(&self, on: bool) {
        let mut e = self.enabled;
        e.set(on);
        save_enabled(on);
    }

    pub fn set_threshold(&self, value: &str) {
        let mut t = self.threshold;
        t.set(value.to_string());
        save_threshold(value);
    }
}

/// Install the wake-word context at the app root. Hydrates persisted choice
/// and threshold from localStorage, installs the `window.RoonWake` JS shim,
/// and runs a long-lived listener that spawns / shuts down the openWakeWord
/// engine as the user toggles `enabled` from any page.
///
/// The runtime lives at app root (not on the Conversational AI page) so the
/// listener keeps running while the user is on Settings tweaking the toggle —
/// otherwise the page-local `use_effect` unmounts when navigating away and
/// the toggle change has nothing listening to react to it.
pub fn use_wake_word_provider() {
    let mut enabled = use_signal(|| false);
    let mut threshold = use_signal(String::new);
    let mut detected_count = use_signal(|| 0u32);
    let mut status = use_signal(|| "Off".to_string());

    let ctx = WakeWordContext {
        enabled,
        threshold,
        detected_count,
        status,
    };
    use_context_provider(|| ctx);

    // Hydrate localStorage on mount.
    use_effect(move || {
        let saved = load_threshold();
        if !saved.is_empty() {
            threshold.set(saved);
        }
        if load_enabled() {
            enabled.set(true);
        }
    });

    // Install the JS engine wrapper once on app mount. Idempotent — the
    // wrapper guards `window.RoonWake` so re-running is safe.
    use_effect(move || {
        spawn(async {
            let _ = dioxus::document::eval(WAKE_WORD_INSTALL_JS)
                .join::<bool>()
                .await;
        });
    });

    // Spawn / shut down the listener whenever `enabled` or `threshold`
    // changes. Lives at app root so it reacts to toggles on Settings.
    use_effect(move || {
        let enabled_now = *enabled.read();
        let threshold_str = threshold.read().clone();

        if !enabled_now {
            status.set("Off".into());
            spawn(async {
                let _ = dioxus::document::eval(
                    "if (window.RoonWake) await window.RoonWake.stop(); return true;",
                )
                .join::<bool>()
                .await;
            });
            return;
        }

        // Empty / unparseable threshold falls back to JS default (0.5).
        let threshold_val = threshold_str
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|t| (0.0..=1.0).contains(t));

        status.set("Initialising…".into());
        spawn(async move {
            let mut eval = dioxus::document::eval(WAKE_WORD_LISTEN_JS);
            let payload = match threshold_val {
                Some(t) => serde_json::json!({ "threshold": t }),
                None => serde_json::json!({}),
            };
            let _ = eval.send(payload);
            loop {
                match eval.recv::<WakeEvent>().await {
                    Ok(WakeEvent::Ready) => {
                        status.set("Listening for 'Hey Roon'".into());
                    }
                    Ok(WakeEvent::Detected) => {
                        let next = *detected_count.peek() + 1;
                        detected_count.set(next);
                    }
                    Ok(WakeEvent::Failed { reason }) => {
                        status.set(format!("Failed: {}", reason));
                        break;
                    }
                    Err(_) => break,
                }
            }
        });
    });
}

pub fn use_wake_word() -> WakeWordContext {
    use_context::<WakeWordContext>()
}

// ============ localStorage helpers ============

#[cfg(target_arch = "wasm32")]
fn load_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(ENABLED_KEY).ok().flatten())
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn load_enabled() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn save_enabled(on: bool) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(ENABLED_KEY, if on { "true" } else { "false" });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_enabled(_: bool) {}

#[cfg(target_arch = "wasm32")]
fn load_threshold() -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(THRESHOLD_KEY).ok().flatten())
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_threshold() -> String {
    String::new()
}

#[cfg(target_arch = "wasm32")]
fn save_threshold(value: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(THRESHOLD_KEY, value);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_threshold(_: &str) {}
