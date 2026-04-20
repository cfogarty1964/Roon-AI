use dioxus::prelude::*;

/// Global default-zone state shared via context, persisted to localStorage.
#[derive(Clone, Copy)]
pub struct DefaultZoneContext {
    pub zone: Signal<String>,
}

impl DefaultZoneContext {
    pub fn get(&self) -> String {
        (self.zone)()
    }

    pub fn set(&self, zone_id: &str) {
        let mut zone = self.zone;
        zone.set(zone_id.to_string());

        #[cfg(target_arch = "wasm32")]
        save_to_storage(zone_id);
    }
}

/// Initialize default-zone context provider — call once at app root.
pub fn use_default_zone_provider() {
    #[allow(unused_mut)]
    let mut zone = use_signal(|| String::new());

    let ctx = DefaultZoneContext { zone };
    use_context_provider(|| ctx);

    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            let saved = load_from_storage();
            zone.set(saved);
        });
    }
}

/// Get the default-zone context — use in any component.
pub fn use_default_zone() -> DefaultZoneContext {
    use_context::<DefaultZoneContext>()
}

// ============ WASM-only helpers ============

#[cfg(target_arch = "wasm32")]
fn load_from_storage() -> String {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(value)) = storage.get_item("roon-ai-default-zone") {
                return value;
            }
        }
    }
    String::new()
}

#[cfg(target_arch = "wasm32")]
fn save_to_storage(zone_id: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item("roon-ai-default-zone", zone_id);
        }
    }
}
