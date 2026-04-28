//! Wake-word ("Hey Roon AI") shared state.
//!
//! This is a scaffolding module: the actual detection runs in the browser via
//! Picovoice's Porcupine WASM SDK, which we expect the user to vendor into
//! `public/wake-word/` along with a trained `.ppn` keyword model. Until those
//! assets exist, every code path here gracefully no-ops — the toggle in
//! Settings stays available but `init()` will report "not ready" and the
//! conversational page never opens the mic via wake word.
//!
//! Setup the user has to do (one-time):
//!
//! 1. Sign up at <https://console.picovoice.ai/> (free for personal use)
//! 2. Train a "Hey Roon AI" wake-word model and download the `.ppn` for
//!    platform "WebAssembly"
//! 3. Drop these files into the project's `public/wake-word/` directory:
//!    - `porcupine_web.iife.js` (the Picovoice browser SDK, IIFE bundle)
//!    - `pv_porcupine.wasm` (the Porcupine engine WASM binary)
//!    - `Hey-Roon-AI_en.ppn` (your trained keyword)
//! 4. Paste the Picovoice access key into Settings → "Hands-free wake word"
//!
//! Once all four are in place, the wake-word toggle activates the engine.
//! Saying the phrase opens the mic exactly like clicking the 🎤 button.

use dioxus::prelude::*;

#[allow(dead_code)]
const ENABLED_KEY: &str = "roon-ai-wake-word-enabled";
#[allow(dead_code)]
const ACCESS_KEY_KEY: &str = "roon-ai-picovoice-key";

#[derive(Clone, Copy)]
pub struct WakeWordContext {
    /// User opted in to listening for the wake word.
    pub enabled: Signal<bool>,
    /// Picovoice access key from Settings (browser-side only, persisted to
    /// localStorage). Required for the engine to initialise.
    pub access_key: Signal<String>,
    /// Detection counter — the conversational page watches this and triggers
    /// the same code path as a manual mic-click whenever it increments.
    /// Starts at 0; increments on each "Hey Roon AI" detection.
    pub detected_count: Signal<u32>,
    /// Human-readable status for the Settings UI: "Listening", "Idle",
    /// "Asset files missing", "Access key required", etc. Set by the
    /// conversational page's init effect.
    pub status: Signal<String>,
}

impl WakeWordContext {
    pub fn set_enabled(&self, on: bool) {
        let mut e = self.enabled;
        e.set(on);
        save_enabled(on);
    }

    pub fn set_access_key(&self, key: &str) {
        let mut k = self.access_key;
        k.set(key.to_string());
        save_access_key(key);
    }
}

/// Install the wake-word context at the app root. Hydrates persisted choice
/// and access key from localStorage.
pub fn use_wake_word_provider() {
    let mut enabled = use_signal(|| false);
    let mut access_key = use_signal(String::new);
    let detected_count = use_signal(|| 0u32);
    let status = use_signal(|| "Off".to_string());

    let ctx = WakeWordContext {
        enabled,
        access_key,
        detected_count,
        status,
    };
    use_context_provider(|| ctx);

    use_effect(move || {
        let saved_key = load_access_key();
        if !saved_key.is_empty() {
            access_key.set(saved_key);
        }
        if load_enabled() {
            enabled.set(true);
        }
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
fn load_access_key() -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(ACCESS_KEY_KEY).ok().flatten())
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_access_key() -> String {
    String::new()
}

#[cfg(target_arch = "wasm32")]
fn save_access_key(key: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(ACCESS_KEY_KEY, key);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_access_key(_: &str) {}
