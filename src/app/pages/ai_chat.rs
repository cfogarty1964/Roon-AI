use crate::app::api::{ai_chat, AiChatRequest, AiChatResponse, ZonesResponse};
use crate::app::components::Layout;
use dioxus::prelude::*;

// ============================================================================
// Page-local state
// ============================================================================

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

// ============================================================================
// Page component
// ============================================================================

#[component]
pub fn AiChat() -> Element {
    let mut messages = use_signal(|| Vec::<ChatMessage>::new());
    let mut input = use_signal(|| String::new());
    let mut selected_zone = use_signal(|| String::new());
    let mut loading = use_signal(|| false);

    let zones = use_resource(|| async {
        crate::app::api::fetch_json::<ZonesResponse>("/zones")
            .await
            .ok()
            .map(|r| r.zones)
            .unwrap_or_default()
    });

    // Pre-select first zone
    use_effect(move || {
        let zone_list = zones.read().clone().unwrap_or_default();
        if !zone_list.is_empty() && selected_zone.read().is_empty() {
            if let Some(z) = zone_list.iter().find(|z| z.zone_id.starts_with("roon:")) {
                selected_zone.set(z.zone_id.clone());
            } else if let Some(z) = zone_list.first() {
                selected_zone.set(z.zone_id.clone());
            }
        }
    });

    let zone_list = zones.read().clone().unwrap_or_default();

    let send = move |_: Event<MouseData>| {
        let msg = input.read().trim().to_string();
        if msg.is_empty() || *loading.read() {
            return;
        }

        let zone = {
            let z = selected_zone.read().clone();
            if z.is_empty() { None } else { Some(z) }
        };

        // Append user message immediately
        messages.write().push(ChatMessage {
            role: Role::User,
            text: msg.clone(),
            actions: vec![],
        });
        input.set(String::new());
        loading.set(true);

        let req = AiChatRequest {
            message: msg,
            zone_id: zone,
        };

        spawn(async move {
            match ai_chat(req).await {
                Ok(AiChatResponse { response, actions, error: None }) => {
                    messages.write().push(ChatMessage {
                        role: Role::Assistant,
                        text: response,
                        actions,
                    });
                }
                Ok(AiChatResponse { error: Some(e), .. }) => {
                    messages.write().push(ChatMessage {
                        role: Role::Error,
                        text: e,
                        actions: vec![],
                    });
                }
                Err(e) => {
                    messages.write().push(ChatMessage {
                        role: Role::Error,
                        text: e,
                        actions: vec![],
                    });
                }
            }
            loading.set(false);
        });
    };

    let on_keydown = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter && !e.modifiers().shift() {
            let msg = input.read().trim().to_string();
            if msg.is_empty() || *loading.read() {
                return;
            }
            let zone = {
                let z = selected_zone.read().clone();
                if z.is_empty() { None } else { Some(z) }
            };
            messages.write().push(ChatMessage {
                role: Role::User,
                text: msg.clone(),
                actions: vec![],
            });
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
    };

    rsx! {
        Layout {
            title: "AI Music Control",
            nav_active: "ai",

            // Header row
            div { class: "flex flex-wrap items-center justify-between gap-3 mb-4",
                h1 { class: "text-2xl font-semibold", "AI Music Control" }

                // Zone picker
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
                    }
                }
            }

            // Intro hint (only shown when no messages yet)
            if messages.read().is_empty() {
                div { class: "mb-4 rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted",
                    p { class: "mb-1 font-medium text-foreground", "Try asking:" }
                    ul { class: "list-disc list-inside space-y-1",
                        li { "\"Play the Adagietto from Mahler's 5th\"" }
                        li { "\"I love that piece — play similar music\"" }
                        li { "\"Queue some late-night jazz piano\"" }
                        li { "\"Pause\" or \"Turn up the volume\"" }
                    }
                }
            }

            // Chat history
            div { class: "flex flex-col gap-3 mb-4 min-h-[200px]",
                for msg in messages.read().iter() {
                    {
                        let (bubble_class, label) = match msg.role {
                            Role::User => (
                                "self-end max-w-[80%] rounded-2xl rounded-br-sm bg-primary text-primary-foreground px-4 py-2 text-sm",
                                "",
                            ),
                            Role::Assistant => (
                                "self-start max-w-[80%] rounded-2xl rounded-bl-sm bg-muted px-4 py-2 text-sm",
                                "",
                            ),
                            Role::Error => (
                                "self-start max-w-[80%] rounded-2xl rounded-bl-sm bg-destructive/10 border border-destructive/30 text-destructive px-4 py-2 text-sm",
                                "",
                            ),
                        };
                        let text = msg.text.clone();
                        let actions = msg.actions.clone();
                        rsx! {
                            div { class: "flex flex-col",
                                div { class: "{bubble_class}",
                                    if !label.is_empty() {
                                        span { class: "block text-xs opacity-60 mb-0.5", "{label}" }
                                    }
                                    "{text}"
                                }
                                if !actions.is_empty() {
                                    div { class: "mt-1 flex flex-wrap gap-1",
                                        for action in &actions {
                                            span {
                                                class: "rounded-full border border-border bg-background px-2 py-0.5 text-xs text-muted",
                                                "⚡ {action}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Loading indicator
                if *loading.read() {
                    div { class: "self-start flex gap-1 px-4 py-2",
                        span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce [animation-delay:-0.3s]" }
                        span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce [animation-delay:-0.15s]" }
                        span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce" }
                    }
                }
            }

            // Input bar
            div { class: "flex gap-2 sticky bottom-4",
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
    }
}
