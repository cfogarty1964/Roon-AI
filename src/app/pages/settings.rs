//! Settings page component.
//!
//! Adapter settings and discovery status using Dioxus resources.

use dioxus::prelude::*;

use crate::app::api::{AdapterSettings, AppSettings, RoonStatus};
use crate::app::components::Layout;
use crate::app::sse::use_sse;
use crate::app::theme::{use_theme, Theme};
use crate::app::voice_context::use_voice;
use crate::app::wake_word_context::use_wake_word;

/// OpenAI cloud TTS voices, paired with a short descriptor for the picker.
/// `value` in the dropdown is `openai:<id>`; the JS in conversational_ai.rs
/// detects the `openai:` prefix and routes through `POST /api/tts`.
const OPENAI_VOICES: &[(&str, &str)] = &[
    ("alloy", "Alloy — neutral, balanced"),
    ("echo", "Echo — warm, conversational"),
    ("fable", "Fable — expressive, British"),
    ("onyx", "Onyx — deep, authoritative"),
    ("nova", "Nova — friendly, upbeat"),
    ("shimmer", "Shimmer — soft, gentle"),
];

/// UPnP status response
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq)]
struct UpnpStatus {
    renderer_count: usize,
}

/// Settings page component.
#[component]
pub fn Settings() -> Element {
    let sse = use_sse();
    let theme_ctx = use_theme();
    let voice_ctx = use_voice();
    let wake_ctx = use_wake_word();

    // Adapter toggle signals
    let mut roon_enabled = use_signal(|| true);
    let mut upnp_enabled = use_signal(|| false);

    // Load settings resource
    let settings = use_resource(|| async {
        crate::app::api::fetch_json::<AppSettings>("/api/settings")
            .await
            .ok()
    });

    // Sync settings to signals when loaded
    use_effect(move || {
        if let Some(Some(s)) = settings.read().as_ref() {
            roon_enabled.set(s.adapters.roon);
            upnp_enabled.set(s.adapters.upnp);
        }
    });

    // Discovery status resources
    let mut roon_status = use_resource(|| async {
        crate::app::api::fetch_json::<RoonStatus>("/roon/status")
            .await
            .ok()
    });
    let mut upnp_status = use_resource(|| async {
        crate::app::api::fetch_json::<UpnpStatus>("/upnp/status")
            .await
            .ok()
    });

    // Refresh discovery on SSE events
    let event_count = sse.event_count;
    use_effect(move || {
        let _ = event_count();
        if sse.should_refresh_discovery() {
            roon_status.restart();
            upnp_status.restart();
        }
    });

    // Save settings handler
    let save_settings = move || {
        let settings = AppSettings {
            adapters: AdapterSettings {
                roon: roon_enabled(),
                upnp: upnp_enabled(),
            },
        };
        spawn(async move {
            let _ = crate::app::api::post_json_no_response("/api/settings", &settings).await;
        });
    };

    let roon_st = roon_status.read().clone().flatten();
    let upnp_st = upnp_status.read().clone().flatten();

    rsx! {
        Layout {
            title: "Settings".to_string(),
            nav_active: "settings".to_string(),

            h1 { class: "text-2xl font-bold mb-6", "Settings" }

            // Features section (adapters + page visibility)
            section { class: "mb-8",
                div { class: "mb-4",
                    h2 { class: "text-xl font-semibold", "Features" }
                    p { class: "text-muted text-sm", "Zone sources and page visibility" }
                }

                div { class: "card p-6",
                    table { class: "w-full", id: "features-table",
                        thead {
                            tr { class: "border-b border-default",
                                th { class: "text-left py-2 px-3 font-semibold w-12", "" }
                                th { class: "text-left py-2 px-3 font-semibold", "Feature" }
                                th { class: "text-left py-2 px-3 font-semibold", "Status" }
                            }
                        }
                        tbody {
                            // Roon (adapter only, no dedicated page)
                            tr { class: "border-b border-default",
                                td { class: "py-2 px-3",
                                    input {
                                        r#type: "checkbox",
                                        class: "checkbox",
                                        aria_label: "Enable Roon",
                                        checked: roon_enabled(),
                                        onchange: move |_| {
                                            roon_enabled.toggle();
                                            save_settings();
                                        }
                                    }
                                }
                                td { class: "py-2 px-3", "Roon" }
                                td { class: "py-2 px-3",
                                    if roon_enabled() {
                                        if let Some(ref status) = roon_st {
                                            if status.connected {
                                                if let Some(ref name) = status.core_name {
                                                    span { class: "status-ok", "✓ {name}" }
                                                } else {
                                                    span { class: "status-ok", "✓ Core" }
                                                }
                                            } else {
                                                span { class: "status-err", "✗ Not connected" }
                                            }
                                        } else {
                                            "..."
                                        }
                                    } else {
                                        span { class: "text-muted", "-" }
                                    }
                                }
                            }
                            // UPnP/DLNA
                            tr { class: "border-b border-default",
                                td { class: "py-2 px-3",
                                    input {
                                        r#type: "checkbox",
                                        class: "checkbox",
                                        aria_label: "Enable UPnP/DLNA",
                                        checked: upnp_enabled(),
                                        onchange: move |_| {
                                            upnp_enabled.toggle();
                                            save_settings();
                                        }
                                    }
                                }
                                td { class: "py-2 px-3", "UPnP/DLNA" }
                                td { class: "py-2 px-3",
                                    if upnp_enabled() {
                                        if let Some(ref status) = upnp_st {
                                            if status.renderer_count > 0 {
                                                span { class: "status-ok", "✓ {status.renderer_count} renderers" }
                                            } else {
                                                "Searching..."
                                            }
                                        } else {
                                            "..."
                                        }
                                    } else {
                                        span { class: "text-muted", "-" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Voice section — TTS voice picker for the Conversational AI page
            section { class: "mb-8",
                div { class: "mb-4",
                    h2 { class: "text-xl font-semibold", "Voice" }
                    p { class: "text-muted text-sm",
                        "Pick the voice used for spoken replies on the Conversational AI page. Browser voices come from your browser/OS for free — on Windows, Edge with an \"Online (Natural)\" entry sounds best. OpenAI cloud voices sound the same on every browser but require an OpenAI API key in config (~$0.005 per reply)."
                    }
                }

                div { class: "card p-6",
                    {
                        let voice_list = voice_ctx.list();
                        let current = voice_ctx.get();
                        rsx! {
                            div { class: "flex flex-col gap-2",
                                label { class: "text-sm font-medium", "TTS voice" }
                                select {
                                    class: "input text-sm py-2 max-w-md",
                                    value: "{current}",
                                    onchange: move |e| voice_ctx.set(&e.value()),
                                    option { value: "", "System default" }
                                    optgroup { label: "OpenAI cloud (cross-browser)",
                                        for v in OPENAI_VOICES.iter() {
                                            option {
                                                value: "openai:{v.0}",
                                                selected: current == format!("openai:{}", v.0),
                                                "{v.1}"
                                            }
                                        }
                                    }
                                    if !voice_list.is_empty() {
                                        optgroup { label: "Browser / OS voices",
                                            for v in voice_list.iter() {
                                                option {
                                                    value: "{v.name}",
                                                    selected: v.name == current,
                                                    "{v.name} ({v.lang})"
                                                }
                                            }
                                        }
                                    }
                                }
                                p { class: "text-xs text-muted",
                                    if voice_list.is_empty() {
                                        "No browser voices detected. OpenAI voices listed above are still available."
                                    } else {
                                        "{voice_list.len()} browser voice(s) available. Choice persists across reloads."
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Wake-word section — "Hey Roon AI" hands-free trigger
            section { class: "mb-8",
                div { class: "mb-4",
                    h2 { class: "text-xl font-semibold", "Hands-free wake word" }
                    p { class: "text-muted text-sm",
                        "Say \"Hey Roon AI\" to start a voice request without clicking the mic. "
                        "Runs entirely in your browser via Picovoice Porcupine — no audio leaves the device until you ask the AI to do something. "
                        "Setup is one-time but requires three things: a free Picovoice account + access key, a trained \".ppn\" model file, and the Porcupine WebAssembly bundle. "
                        "See "
                        code { class: "text-xs", "src/app/wake_word_context.rs" }
                        " for the full setup walkthrough."
                    }
                }
                div { class: "card p-6",
                    div { class: "flex flex-col gap-4",
                        div { class: "flex items-center gap-3",
                            input {
                                r#type: "checkbox",
                                class: "checkbox",
                                aria_label: "Enable wake word",
                                checked: (wake_ctx.enabled)(),
                                onchange: move |e| wake_ctx.set_enabled(e.value() == "true" || e.value() == "on"),
                            }
                            span { class: "text-sm", "Listen for \"Hey Roon AI\"" }
                        }
                        div { class: "flex flex-col gap-2",
                            label { class: "text-sm font-medium", "Picovoice access key" }
                            input {
                                r#type: "password",
                                class: "input text-sm py-2 max-w-md",
                                placeholder: "Paste from console.picovoice.ai",
                                value: "{wake_ctx.access_key}",
                                oninput: move |e| wake_ctx.set_access_key(&e.value()),
                            }
                            p { class: "text-xs text-muted",
                                "Status: "
                                span { class: "font-medium", "{wake_ctx.status}" }
                            }
                        }
                    }
                }
            }

            // Theme Settings section
            section { class: "mb-8",
                div { class: "mb-4",
                    h2 { class: "text-xl font-semibold", "Appearance" }
                    p { class: "text-muted text-sm", "Choose your preferred color theme" }
                }

                div { class: "card p-6",
                    div { class: "grid grid-cols-2 sm:grid-cols-4 gap-4",
                        for theme in [Theme::System, Theme::Light, Theme::Dark, Theme::Oled] {
                            button {
                                class: if theme_ctx.get() == theme { "btn-primary py-3" } else { "btn-outline py-3" },
                                onclick: move |_| theme_ctx.set(theme),
                                "{theme.label()}"
                            }
                        }
                    }
                    p { class: "mt-4 text-sm text-muted",
                        match theme_ctx.get() {
                            Theme::System => "Using your system's color scheme preference.",
                            Theme::Light => "Light theme for bright environments.",
                            Theme::Dark => "Dark theme for low-light environments.",
                            Theme::Oled => "Pure black theme for AMOLED displays.",
                        }
                    }
                }
            }

        }
    }
}
