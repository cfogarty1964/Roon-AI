use crate::app::api::{ai_chat, AiChatRequest, AiChatResponse, HistoryTurn, Suggestion, ZonesResponse};
use crate::app::components::Layout;
use crate::app::default_zone::use_default_zone;
use crate::app::voice_context::use_voice;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
const STORAGE_KEY: &str = "roon-ai-conversation";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct ChatMessage {
    role: Role,
    /// Display text. For Assistant this is rendered HTML; for User/Error it's plain text.
    text: String,
    /// Raw markdown of an assistant reply (suggestions block stripped). Used to
    /// replay this turn back to the server in subsequent requests. Empty for
    /// User/Error turns.
    #[serde(default)]
    markdown: String,
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    suggestions: Vec<Suggestion>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum Role {
    User,
    Assistant,
    Error,
}

fn build_history(messages: &[ChatMessage]) -> Vec<HistoryTurn> {
    messages
        .iter()
        .filter_map(|m| match m.role {
            Role::User => Some(HistoryTurn { role: "user".into(), text: m.text.clone() }),
            Role::Assistant => Some(HistoryTurn { role: "assistant".into(), text: m.markdown.clone() }),
            Role::Error => None,
        })
        .collect()
}

#[derive(Copy, Clone)]
struct SpeechCtx {
    speak_enabled: Signal<bool>,
    continuous: Signal<bool>,
    listening: Signal<bool>,
    selected_voice: Signal<String>,
}

fn do_send_text(
    msg: String,
    mut messages: Signal<Vec<ChatMessage>>,
    mut loading: Signal<bool>,
    selected_zone: Signal<String>,
    speech: SpeechCtx,
) {
    if msg.is_empty() || *loading.read() {
        return;
    }
    let zone = {
        let z = selected_zone.read().clone();
        if z.is_empty() { None } else { Some(z) }
    };
    let history = build_history(&messages.read());
    messages.write().push(ChatMessage {
        role: Role::User,
        text: msg.clone(),
        markdown: String::new(),
        actions: vec![],
        suggestions: vec![],
    });
    loading.set(true);
    let req = AiChatRequest { message: msg, zone_id: zone, history };
    spawn(async move {
        let mut spoken_markdown: Option<String> = None;
        match ai_chat(req).await {
            Ok(AiChatResponse { response, response_markdown, actions, suggestions, error: None }) => {
                if *speech.speak_enabled.read() {
                    spoken_markdown = Some(response_markdown.clone());
                }
                messages.write().push(ChatMessage {
                    role: Role::Assistant,
                    text: response,
                    markdown: response_markdown,
                    actions,
                    suggestions,
                });
            }
            Ok(AiChatResponse { error: Some(e), .. }) => {
                messages.write().push(ChatMessage {
                    role: Role::Error,
                    text: e,
                    markdown: String::new(),
                    actions: vec![],
                    suggestions: vec![],
                });
            }
            Err(e) => {
                messages.write().push(ChatMessage {
                    role: Role::Error,
                    text: e,
                    markdown: String::new(),
                    actions: vec![],
                    suggestions: vec![],
                });
            }
        }
        loading.set(false);

        if let Some(md) = spoken_markdown {
            // Speak the assistant reply, then optionally restart the mic.
            let voice = speech.selected_voice.read().clone();
            let payload = serde_json::to_string(&md).unwrap_or_else(|_| "\"\"".into());
            let voice_payload = if voice.is_empty() {
                "null".to_string()
            } else {
                serde_json::to_string(&voice).unwrap_or_else(|_| "null".into())
            };
            let script = format!(
                "await window.RoonSpeech.speak({}, {}); return \"done\";",
                payload, voice_payload
            );
            let _ = dioxus::document::eval(&script).join::<serde_json::Value>().await;

            if *speech.continuous.read() {
                start_listening_task(messages, loading, selected_zone, speech);
            }
        }
    });
}

/// Start mic capture in a spawned task. On result, auto-submits via do_send_text.
fn start_listening_task(
    messages: Signal<Vec<ChatMessage>>,
    loading: Signal<bool>,
    selected_zone: Signal<String>,
    speech: SpeechCtx,
) {
    let mut listening = speech.listening;
    if *listening.read() || *loading.read() {
        return;
    }
    listening.set(true);
    spawn(async move {
        let script = r#"
            try {
                const text = await window.RoonSpeech.startListening();
                return { ok: true, text };
            } catch (err) {
                return { ok: false, error: String(err) };
            }
        "#;
        let result: Result<serde_json::Value, _> =
            dioxus::document::eval(script).join().await;
        listening.set(false);
        if let Ok(v) = result {
            if v.get("ok").and_then(|b| b.as_bool()).unwrap_or(false) {
                if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        do_send_text(trimmed, messages, loading, selected_zone, speech);
                    }
                }
            }
        }
    });
}

/// JS module installed once on mount. Provides:
///   window.RoonSpeech.startListening() -> Promise<String>
///   window.RoonSpeech.stopListening()
///   window.RoonSpeech.speak(md, voiceName?) -> Promise<void>  (strips markdown internally)
///   window.RoonSpeech.cancelSpeech()
///   window.RoonSpeech.listVoices() -> [{ name, lang, default }]
///   window.RoonSpeech.isSttSupported (bool)
const SPEECH_INSTALL_JS: &str = r#"
if (!window.RoonSpeech) {
    const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
    let _recog = null;

    function plainify(md) {
        return String(md || "")
            .replace(/```[\s\S]*?```/g, "")
            .replace(/`([^`]+)`/g, "$1")
            .replace(/\*\*([^*]+)\*\*/g, "$1")
            .replace(/\*([^*]+)\*/g, "$1")
            .replace(/__([^_]+)__/g, "$1")
            .replace(/_([^_]+)_/g, "$1")
            .replace(/^#{1,6}\s+/gm, "")
            .replace(/^-+\s*$/gm, "")
            .replace(/^\s*[-*+]\s+/gm, "")
            .replace(/^\s*\d+\.\s+/gm, "")
            .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
            .replace(/\n{2,}/g, ". ")
            .replace(/\n/g, " ")
            .replace(/\s+/g, " ")
            .trim();
    }

    window.RoonSpeech = {
        isSttSupported: !!SR,
        startListening() {
            return new Promise((resolve, reject) => {
                if (_recog) { try { _recog.stop(); } catch (e) {} _recog = null; }
                if (!SR) { reject("not supported"); return; }
                const r = new SR();
                r.lang = navigator.language || "en-US";
                r.interimResults = false;
                r.continuous = false;
                r.maxAlternatives = 1;
                let result = "";
                r.onresult = (e) => { result = e.results[0][0].transcript; };
                r.onerror = (e) => { _recog = null; reject(e.error || "error"); };
                r.onend = () => { _recog = null; resolve(result); };
                _recog = r;
                try { r.start(); } catch (e) { _recog = null; reject(String(e)); }
            });
        },
        stopListening() {
            if (_recog) { try { _recog.stop(); } catch (e) {} _recog = null; }
        },
        speak(md, voiceName) {
            const text = plainify(md);
            window.speechSynthesis.cancel();
            if (!text) return Promise.resolve();
            return new Promise((resolve) => {
                const u = new SpeechSynthesisUtterance(text);
                u.lang = navigator.language || "en-US";
                if (voiceName) {
                    const voice = window.speechSynthesis.getVoices().find(v => v.name === voiceName);
                    if (voice) { u.voice = voice; u.lang = voice.lang; }
                }
                u.onend = () => resolve();
                u.onerror = () => resolve();
                window.speechSynthesis.speak(u);
            });
        },
        cancelSpeech() {
            try { window.speechSynthesis.cancel(); } catch (e) {}
        },
        listVoices() {
            try {
                return window.speechSynthesis.getVoices().map(v => ({
                    name: v.name,
                    lang: v.lang,
                    default: !!v.default
                }));
            } catch (e) { return []; }
        }
    };
}
return !!window.RoonSpeech.isSttSupported;
"#;


fn do_send(
    mut input: Signal<String>,
    messages: Signal<Vec<ChatMessage>>,
    loading: Signal<bool>,
    selected_zone: Signal<String>,
    speech: SpeechCtx,
) {
    let msg = input.read().trim().to_string();
    if msg.is_empty() || *loading.read() {
        return;
    }
    input.set(String::new());
    do_send_text(msg, messages, loading, selected_zone, speech);
}

fn play_message_for(s: &Suggestion) -> String {
    match (&s.artist, &s.album) {
        (Some(a), _) if !a.is_empty() => format!("Play \"{}\" by {}", s.title, a),
        (_, Some(al)) if !al.is_empty() => format!("Play \"{}\" from {}", s.title, al),
        _ => format!("Play \"{}\"", s.title),
    }
}

#[cfg(target_arch = "wasm32")]
fn load_messages_from_storage() -> Vec<ChatMessage> {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(json)) = storage.get_item(STORAGE_KEY) {
                if let Ok(parsed) = serde_json::from_str::<Vec<ChatMessage>>(&json) {
                    return parsed;
                }
            }
        }
    }
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_messages_from_storage() -> Vec<ChatMessage> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
fn save_messages_to_storage(messages: &[ChatMessage]) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(json) = serde_json::to_string(messages) {
                let _ = storage.set_item(STORAGE_KEY, &json);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_messages_to_storage(_messages: &[ChatMessage]) {}

#[cfg(target_arch = "wasm32")]
fn clear_storage() {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.remove_item(STORAGE_KEY);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn clear_storage() {}

#[component]
pub fn ConversationalAi() -> Element {
    let mut messages = use_signal(|| Vec::<ChatMessage>::new());
    let mut input = use_signal(|| String::new());
    let mut selected_zone = use_signal(|| String::new());
    let loading = use_signal(|| false);
    let default_zone_ctx = use_default_zone();

    // Speech state
    let mut speak_enabled = use_signal(|| false);
    let mut continuous = use_signal(|| false);
    let listening = use_signal(|| false);
    let mut stt_supported = use_signal(|| true);
    // Voice choice lives in shared context (set on Settings page)
    let voice_ctx = use_voice();
    let speech = SpeechCtx { speak_enabled, continuous, listening, selected_voice: voice_ctx.selected };

    // Install JS speech module on mount; report STT support.
    use_effect(move || {
        spawn(async move {
            let e = dioxus::document::eval(SPEECH_INSTALL_JS);
            if let Ok(supported) = e.join::<bool>().await {
                stt_supported.set(supported);
            }
        });
    });

    // Hydrate from localStorage on mount (no-op on server)
    use_effect(move || {
        let saved = load_messages_from_storage();
        if !saved.is_empty() && messages.read().is_empty() {
            messages.set(saved);
        }
    });

    // Persist on every messages change
    use_effect(move || {
        let snapshot = messages.read().clone();
        save_messages_to_storage(&snapshot);
    });

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

    let send = move |_: Event<MouseData>| do_send(input, messages, loading, selected_zone, speech);

    let on_keydown = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter && !e.modifiers().shift() {
            do_send(input, messages, loading, selected_zone, speech);
        }
    };

    let on_mic = move |_: Event<MouseData>| {
        if *listening.read() {
            // Stop listening
            spawn(async move {
                let _ = dioxus::document::eval("window.RoonSpeech.stopListening();")
                    .join::<serde_json::Value>().await;
            });
        } else {
            start_listening_task(messages, loading, selected_zone, speech);
        }
    };

    let toggle_speak = move |_: Event<MouseData>| {
        let new_val = !*speak_enabled.read();
        speak_enabled.set(new_val);
        if !new_val {
            // Stop any ongoing speech and disable continuous mode
            continuous.set(false);
            spawn(async move {
                let _ = dioxus::document::eval("window.RoonSpeech && window.RoonSpeech.cancelSpeech();")
                    .join::<serde_json::Value>().await;
            });
        }
    };

    let toggle_continuous = move |_: Event<MouseData>| {
        let new_val = !*continuous.read();
        continuous.set(new_val);
        if new_val {
            // Continuous requires speak; auto-enable
            speak_enabled.set(true);
        }
    };

    let has_actions = messages.read().iter().any(|m| !m.actions.is_empty());

    rsx! {
        Layout {
            title: "Conversational AI",
            nav_active: "conversational",

            div { class: "flex flex-wrap items-center justify-between gap-3 mb-5",
                div {
                    h1 { class: "text-2xl font-semibold", "Conversational AI" }
                    p { class: "text-xs text-muted mt-0.5", "Persists across reloads. Remembers prior turns." }
                }
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
                    // Speak / hands-free toggles (voice picker lives on Settings page)
                    {
                        let speak_on = *speak_enabled.read();
                        let cont_on = *continuous.read();
                        rsx! {
                            div { class: "flex items-center gap-1",
                                button {
                                    class: if speak_on {
                                        "px-2 py-1 rounded-md border border-primary bg-primary/10 text-primary text-sm"
                                    } else {
                                        "px-2 py-1 rounded-md border border-border text-muted hover:text-foreground text-sm"
                                    },
                                    title: if speak_on { "Speaking replies — click to disable" } else { "Click to speak AI replies aloud" },
                                    onclick: toggle_speak,
                                    if speak_on { "🔊 Speak" } else { "🔇 Speak" }
                                }
                                button {
                                    class: if cont_on {
                                        "px-2 py-1 rounded-md border border-primary bg-primary/10 text-primary text-sm"
                                    } else {
                                        "px-2 py-1 rounded-md border border-border text-muted hover:text-foreground text-sm disabled:opacity-40"
                                    },
                                    disabled: !*stt_supported.read(),
                                    title: if cont_on {
                                        "Hands-free on — auto-listens after each reply"
                                    } else if !*stt_supported.read() {
                                        "Hands-free not supported in this browser"
                                    } else {
                                        "Hands-free: auto-listen after each spoken reply"
                                    },
                                    onclick: toggle_continuous,
                                    if cont_on { "🎙 Hands-free" } else { "🎙" }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn-outline btn-sm disabled:opacity-40",
                        disabled: messages.read().is_empty(),
                        onclick: move |_| {
                            messages.write().clear();
                            clear_storage();
                        },
                        "Clear"
                    }
                }
            }

            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6 items-start",

                div { class: "flex flex-col gap-3",

                    if messages.read().is_empty() {
                        div { class: "rounded-lg border border-border bg-muted/30 p-4 text-sm text-muted",
                            p { class: "mb-1 font-medium text-foreground", "Conversational mode — try a back-and-forth:" }
                            ul { class: "list-disc list-inside space-y-1",
                                li { "\"Suggest some late-night jazz piano albums\"" }
                                li { "\"Of those, which is the most relaxed?\" (referring to the prior list)" }
                                li { "\"Play it on the kitchen zone\"" }
                                li { "\"Actually, queue something more upbeat next\"" }
                            }
                        }
                    }

                    div { class: "flex flex-col gap-3 min-h-[200px]",
                        for msg in messages.read().iter() {
                            {
                                let text = msg.text.clone();
                                let suggestions = msg.suggestions.clone();
                                match msg.role {
                                    Role::User => rsx! {
                                        div {
                                            class: "self-end max-w-[90%] rounded-2xl rounded-br-sm bg-primary text-primary-foreground px-4 py-2 text-sm",
                                            "{text}"
                                        }
                                    },
                                    Role::Assistant => rsx! {
                                        div { class: "self-start max-w-[90%] flex flex-col gap-2",
                                            div {
                                                class: "rounded-2xl rounded-bl-sm bg-muted px-4 py-3 ai-prose",
                                                dangerous_inner_html: "{text}",
                                            }
                                            if !suggestions.is_empty() {
                                                div { class: "flex flex-col gap-1 ml-2",
                                                    for sug in suggestions.iter() {
                                                        {
                                                            let sug = sug.clone();
                                                            let play_msg = play_message_for(&sug);
                                                            let label = match &sug.artist {
                                                                Some(a) if !a.is_empty() => format!("{} — {}", sug.title, a),
                                                                _ => sug.title.clone(),
                                                            };
                                                            let is_loading = *loading.read();
                                                            rsx! {
                                                                div {
                                                                    class: "flex items-center gap-2 rounded-md border border-border bg-background/50 px-2 py-1.5 text-sm",
                                                                    button {
                                                                        class: "btn-primary px-2 py-0.5 text-xs disabled:opacity-50",
                                                                        disabled: is_loading,
                                                                        onclick: move |_| do_send_text(play_msg.clone(), messages, loading, selected_zone, speech),
                                                                        "▶ Play"
                                                                    }
                                                                    span { class: "truncate", "{label}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
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

                        if *loading.read() {
                            div { class: "self-start flex gap-1 px-4 py-2",
                                span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce [animation-delay:-0.3s]" }
                                span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce [animation-delay:-0.15s]" }
                                span { class: "h-2 w-2 rounded-full bg-muted-foreground animate-bounce" }
                            }
                        }
                    }

                    div { class: "flex gap-2 sticky bottom-4 mt-2",
                        textarea {
                            class: "input flex-1 resize-none text-sm",
                            rows: "2",
                            placeholder: if *listening.read() { "Listening… (speak now)" } else { "Continue the conversation…" },
                            value: "{input}",
                            disabled: *loading.read() || *listening.read(),
                            oninput: move |e| input.set(e.value()),
                            onkeydown: on_keydown,
                        }
                        {
                            let is_listening = *listening.read();
                            let mic_disabled = !*stt_supported.read() || (*loading.read() && !is_listening);
                            let title_text = if !*stt_supported.read() {
                                "Voice input not supported in this browser"
                            } else if is_listening {
                                "Listening — click to cancel"
                            } else {
                                "Click to speak"
                            };
                            rsx! {
                                button {
                                    class: if is_listening {
                                        "self-end px-3 py-2 text-sm rounded-md bg-red-600 text-white animate-pulse"
                                    } else {
                                        "self-end px-3 py-2 text-sm rounded-md border border-border hover:bg-muted disabled:opacity-40"
                                    },
                                    disabled: mic_disabled,
                                    title: "{title_text}",
                                    onclick: on_mic,
                                    if is_listening { "■" } else { "🎤" }
                                }
                            }
                        }
                        button {
                            class: "btn-primary self-end px-4 py-2 text-sm disabled:opacity-50",
                            disabled: *loading.read() || input.read().trim().is_empty(),
                            onclick: send,
                            if *loading.read() { "…" } else { "Send" }
                        }
                    }
                }

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
