//! Shared voice-picker state for the AI page's text-to-speech.
//!
//! Browsers expose a list of voices via `speechSynthesis.getVoices()`. The
//! user's choice is stored in `localStorage` and consumed by the Conversational
//! AI page's `RoonSpeech.speak(md, voiceName)` JS helper. Settings page renders
//! the picker UI; this module owns the state.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
const VOICE_STORAGE_KEY: &str = "roon-ai-voice";

#[derive(Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VoiceInfo {
    pub name: String,
    pub lang: String,
    #[serde(default)]
    pub default: bool,
}

#[derive(Clone, Copy)]
pub struct VoiceContext {
    pub selected: Signal<String>,
    pub voices: Signal<Vec<VoiceInfo>>,
}

impl VoiceContext {
    pub fn get(&self) -> String {
        (self.selected)()
    }

    pub fn set(&self, name: &str) {
        let mut s = self.selected;
        s.set(name.to_string());
        save_voice_choice(name);
    }

    pub fn list(&self) -> Vec<VoiceInfo> {
        (self.voices)()
    }
}

/// Install the voice context at the app root. Hydrates the saved choice from
/// localStorage and asynchronously enumerates available voices.
pub fn use_voice_provider() {
    let mut selected = use_signal(String::new);
    let mut voices = use_signal(Vec::<VoiceInfo>::new);

    let ctx = VoiceContext { selected, voices };
    use_context_provider(|| ctx);

    use_effect(move || {
        let saved = load_voice_choice();
        if !saved.is_empty() {
            selected.set(saved);
        }
        spawn(async move {
            let e = dioxus::document::eval(LIST_VOICES_JS);
            if let Ok(list) = e.join::<Vec<VoiceInfo>>().await {
                voices.set(list);
            }
        });
    });
}

pub fn use_voice() -> VoiceContext {
    use_context::<VoiceContext>()
}

/// JS: enumerate voices, waiting briefly for `voiceschanged` if needed.
const LIST_VOICES_JS: &str = r#"
async function _waitForVoices() {
    function snap() {
        try {
            return window.speechSynthesis.getVoices().map(v => ({
                name: v.name, lang: v.lang, default: !!v.default
            }));
        } catch (e) { return []; }
    }
    let v = snap();
    if (v && v.length > 0) return v;
    return new Promise((resolve) => {
        let done = false;
        const finish = () => {
            if (done) return; done = true;
            resolve(snap());
        };
        try { window.speechSynthesis.onvoiceschanged = finish; } catch (e) {}
        let tries = 0;
        const tick = () => {
            const list = snap();
            if (list && list.length > 0) finish();
            else if (++tries < 20) setTimeout(tick, 100);
            else finish();
        };
        setTimeout(tick, 100);
    });
}
return await _waitForVoices();
"#;

// ============ localStorage helpers ============

#[cfg(target_arch = "wasm32")]
fn load_voice_choice() -> String {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(v)) = storage.get_item(VOICE_STORAGE_KEY) {
                return v;
            }
        }
    }
    String::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_voice_choice() -> String {
    String::new()
}

#[cfg(target_arch = "wasm32")]
fn save_voice_choice(name: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item(VOICE_STORAGE_KEY, name);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_voice_choice(_name: &str) {}
