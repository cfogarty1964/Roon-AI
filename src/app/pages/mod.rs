//! Dioxus fullstack page components.
//!
//! These pages use Dioxus signals and server functions instead of inline JavaScript.

mod conversational_ai;
mod library;
mod settings;
mod zones;

pub use conversational_ai::ConversationalAi;
pub use library::Library;
pub use settings::Settings;
pub use zones::Zones;
