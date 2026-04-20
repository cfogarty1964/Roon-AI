use crate::app::api::{ai_chat, AiChatRequest, AiChatResponse, ZonesResponse};
use crate::app::components::Layout;
use crate::app::default_zone::use_default_zone;
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
struct ChatMessage {
    role: Role,
    text: String,
    actions: Vec<String>,
}

#[derive(Clone, PartialEq)]
enum Role {
    User,
    Assistant,
    Error,
}

// Signal<T> is Copy in Dioxus — pass by value, interior mutability handles writes.
fn do_send(
    mut input: Signal<String>,
    mut messages: Signal<Vec<ChatMessage>>,
    mut loading: Signal<bool>,
    selected_zone: Signal<String>,
) {
    let msg = input.read().trim().to_string();
    if msg.is_empty() || *loading.read() {
        return;
    }
    let zone = {
        let z = selected_zone.read().clone();
        if z.is_empty() { None } else { Some(z) }
    };
    messages.write().push(ChatMessage { role: Role::User, text: msg.clone(), actions: vec![] });
    input.set(String::new());
    loading.set(true);
    let req = AiChatRequest { message: msg, zone_id: zone };
    spawn(async move {
        match ai_chat(req).await {
            Ok(AiChatResponse { response, actions, error: None }) => {
                messages.write().push(ChatMessage { role: Role::Assistant, text: response, actions });
            }
            Ok(AiChatResponse { error: Some(e), .. }) => {
                messages.write().push(ChatMessage { role: Role::Error, text: e, actions: vec![] });
            }
            Err(e) => {
                messages.write().push(ChatMessage { role: Role::Error, text: e, actions: vec![] });
            }
        }
        loading.set(false);
    });
}

#[component]
pub fn AiChat() -> Element {
    let mut messages = use_signal(|| Vec::<ChatMessage>::new());
    let mut input = use_signal(|| String::new());
    let mut selected_zone = use_signal(|| String::new());
    let mut loading = use_signal(|| false);
    let default_zone_ctx = use_default_zone();

    let zones = use_resource(|| async {
        crate::app::api::fetch_json::<ZonesResponse>("/zones")
            .await
            .ok()
            .map(|r| r.zones)
            .unwrap_or_default()
    });

    use_effect(move || {
        let zone_list = zones.read().clone().unwrap_or_default();
        if !zone_list.is_empty() && selected_zone.read().is_empty() {
            let default = default_zone_ctx.get();
            if !default.is_empty() && zone_list.iter().any(|z| z.zone_id == default) {
                selected_zone.set(default);
            } else if let Some(z) = zone_list.iter().find(|z| z.zone_id.starts_with("roon:")) {
                selected_zone.set(z.zone_id.clone());
            } else if let Some(z) = zone_list.first() {
                selected_zone.set(z.zone_id.clone());
            }
        }
    });

    let zone_list = zones.read().clone().unwrap_or_default();

    let send = move |_: Event<MouseData>| do_send(input, messages, loading, selected_zone);

    let on_keydown = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter && !e.modifiers().shift() {
            do_send(input, messages, loading, selected_zone);
        }
    };

    let has_actions = messages.read().iter().any(|m| !m.actions.is_empty());

    rsx! {
        Layout {
            title: "AI Music Control",
            nav_active: "ai",

            // Full-width header
            div { class: "flex flex-wrap items-center justify-between gap-3 mb-5",
                h1 { class: "text-2xl font-semibold", "AI Music Control" }
                div { class: "flex items-center gap-3 flex-wrap",
                    if !zone_list.is_empty() {
                        div { class: "flex items-center gap-2",
                            label { class: "text-sm text-muted", "Zone:" }
                            select {
                                class: "input text-sm py-1",
                                value: "{selected_zone}",
                                oninput: move |e| selected_zone.set(e.value()),
                                for zone in &zone_list {
                                    option {
                                        value: "{zone.zone_id}",
                                        selected: zone.zone_id == *selected_zone.read(),
                                        "{zone.zone_name}"
                                    }
                                }
                            }
                            {
                                let is_default = *selected_zone.read() == default_zone_ctx.get();
                                let star_zone = selected_zone.read().clone();
                                rsx! {
                                    button {
                                        class: if is_default {
                                            "text-yellow-400 hover:text-yellow-500 text-lg leading-none transition-colors"
                                        } else {
                                            "text-muted hover:text-yellow-400 text-lg leading-none transition-colors"
                                        },
                                        title: if is_default { "Default zone (click to reconfirm)" } else { "Set as default zone" },
                                        onclick: move |_| {
                                            if !star_zone.is_empty() {
                                                default_zone_ctx.set(&star_zone);
                                            }
                                        },
                                        if is_default { "★" } else { "☆" }
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn-outline btn-sm disabled:opacity-40",
                        disabled: messages.read().is_empty(),
                        onclick: move |_| messages.write().clear(),
                        "Clear"
                    }
                }
            }

            // Two-column body
            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6 items-start",

                // ── LEFT: Conversation ──────────────────────────────────────
                div { class: "flex flex-col gap-3",

                    // Intro hint (only when empty)
                    if messages.read().is_empty() {
                        div { class: "rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted",
                            p { class: "mb-1 font-medium text-foreground", "Try asking:" }
                            ul { class: "list-disc list-inside space-y-1",
                                li { "\"Play the Adagietto from Mahler's 5th\"" }
                                li { "\"I love that piece — play similar music\"" }
                                li { "\"Queue some late-night jazz piano\"" }
                                li { "\"Pause\" or \"Turn up the volume\"" }
                            }
                        }
                    }

                    // Message bubbles — text only, no tool pills
                    div { class: "flex flex-col gap-3 min-h-[200px]",
                        for msg in messages.read().iter() {
                            {
                                let text = msg.text.clone();
                                match msg.role {
                                    Role::User => rsx! {
                                        div {
                                            class: "self-end max-w-[90%] rounded-2xl rounded-br-sm bg-primary text-primary-foreground px-4 py-2 text-sm",
                                            "{text}"
                                        }
                                    },
                                    Role::Assistant => rsx! {
                                        div {
                                            class: "self-start max-w-[90%] rounded-2xl rounded-bl-sm bg-muted px-4 py-3 ai-prose",
                                            dangerous_inner_html: "{text}",
                                        }
                                    },
                                    Role::Error => rsx! {
                                        div {
                                            class: "self-start max-w-[90%] rounded-2xl rounded-bl-sm bg-destructive/10 border border-destructive/30 text-destructive px-4 py-2 text-sm",
                                            "{text}"
                                        }
                                    },
                                }
                            }
                        }

                        // Loading dots
                        if *loading.read() {
                            div { class: "self-start flex gap-1 px-4 py-2",
                                span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce [animation-delay:-0.3s]" }
                                span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce [animation-delay:-0.15s]" }
                                span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce" }
                            }
                        }
                    }

                    // Input bar
                    div { class: "flex gap-2 sticky bottom-4 mt-2",
                        textarea {
                            class: "input flex-1 resize-none text-sm",
                            rows: "2",
                            placeholder: "Ask me to play something…",
                            value: "{input}",
                            disabled: *loading.read(),
                            oninput: move |e| input.set(e.value()),
                            onkeydown: on_keydown,
                        }
                        button {
                            class: "btn-primary self-end px-4 py-2 text-sm disabled:opacity-50",
                            disabled: *loading.read() || input.read().trim().is_empty(),
                            onclick: send,
                            if *loading.read() { "…" } else { "Send" }
                        }
                    }
                }

                // ── RIGHT: Tool call log ────────────────────────────────────
                div { class: "flex flex-col gap-2",
                    h2 { class: "text-xs font-semibold uppercase tracking-widest text-muted mb-1", "Tool Calls" }

                    if !has_actions && !*loading.read() {
                        div { class: "rounded-lg border border-dashed border-border p-6 text-sm text-muted text-center",
                            if messages.read().is_empty() {
                                "Tool calls will appear here as the AI works"
                            } else {
                                "No tool calls yet"
                            }
                        }
                    } else {
                        div { class: "flex flex-col gap-1",
                            for msg in messages.read().iter() {
                                for action in &msg.actions {
                                    {
                                        let action = action.clone();
                                        rsx! {
                                            div {
                                                class: "rounded-md border border-border bg-muted/40 px-3 py-1.5 font-mono text-xs text-muted break-all",
                                                "⚡ {action}"
                                            }
                                        }
                                    }
                                }
                            }
                            if *loading.read() {
                                div {
                                    class: "rounded-md border border-dashed border-border px-3 py-1.5 font-mono text-xs text-muted animate-pulse",
                                    "⚡ calling…"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
