//! Dioxus fullstack application entry point.
//!
//! This module provides the main App component that serves as the root
//! of the Dioxus application with client-side hydration.

use dioxus::prelude::*;

pub mod api;
pub mod components;
pub mod default_zone;
pub mod embedded_assets;
pub mod pages;
pub mod sse;
pub mod theme;
pub mod voice_context;

use default_zone::use_default_zone_provider;
use pages::{ConversationalAi, Library, Settings};
use sse::use_sse_provider;
use theme::use_theme_provider;
use voice_context::use_voice_provider;

/// Root app component with routing
#[component]
pub fn App() -> Element {
    // Initialize SSE context at app root (single EventSource for all pages)
    use_sse_provider();

    // Initialize theme context at app root (handles localStorage + DOM class)
    use_theme_provider();

    // Initialize default-zone context at app root (persisted to localStorage)
    use_default_zone_provider();

    // Initialize voice-picker context at app root (persisted to localStorage)
    use_voice_provider();

    rsx! {
        Router::<Route> {}
    }
}

/// Application routes. The Conversational AI page lives at the root —
/// historically it was at `/conversational` and `/` belonged to a separate
/// Zones overview page; the Zones page was removed since the conversational
/// surface (zone picker + now-playing banner + transport buttons) covers the
/// same functionality more naturally.
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[route("/")]
    ConversationalAi {},
    #[route("/library")]
    Library {},
    #[route("/settings")]
    Settings {},
}
