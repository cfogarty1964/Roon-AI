//! Settings context for shared app settings state.
//!
//! Provides reactive signals for navigation visibility settings that are
//! shared between the Settings page and Nav component.

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::app::api::AppSettings;

/// Global settings state shared via context
#[derive(Clone, Copy)]
pub struct SettingsContext {
    hide_knobs: Signal<bool>,
    /// Whether settings have been loaded from server
    loaded: Signal<bool>,
}

impl SettingsContext {
    /// Check if settings have been loaded
    pub fn is_loaded(&self) -> bool {
        (self.loaded)()
    }

    /// Get hide_knobs value
    pub fn hide_knobs(&self) -> bool {
        (self.hide_knobs)()
    }

    /// Update settings
    pub fn update(&self, hide_knobs: bool) {
        let mut hk = self.hide_knobs;
        hk.set(hide_knobs);
    }

    /// Mark settings as loaded
    pub fn mark_loaded(&self) {
        let mut loaded = self.loaded;
        loaded.set(true);
    }
}

/// Initialize settings context provider - call once at app root
pub fn use_settings_provider() {
    let hide_knobs = use_signal(|| false);
    let loaded = use_signal(|| false);

    let ctx = SettingsContext {
        hide_knobs,
        loaded,
    };

    use_context_provider(|| ctx);

    // Fetch initial settings from server
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            spawn(async move {
                if let Ok(settings) =
                    crate::app::api::fetch_json::<AppSettings>("/api/settings").await
                {
                    ctx.update(settings.hide_knobs_page);
                    ctx.mark_loaded();
                }
            });
        });
    }
}

/// Get settings context - use in any component
pub fn use_settings() -> SettingsContext {
    use_context::<SettingsContext>()
}
