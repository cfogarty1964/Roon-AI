use crate::app::api::{AiChatRequest, CurrentTrack, HistoryTurn, Suggestion, Zone, ZonesResponse};
use crate::app::components::Layout;
use crate::app::default_zone::use_default_zone;
use crate::app::sse::use_sse;
use crate::app::voice_context::use_voice;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// Multi-conversation storage:
//   - INDEX_KEY    → JSON Vec<ConversationMeta> (id + title for each chat)
//   - MSG_KEY_PREFIX + id → JSON Vec<ChatMessage> for that conversation
// On first hydrate after upgrade, the legacy `roon-ai-conversation` key is
// migrated into a single conversation entry with the title "Conversation".
#[allow(dead_code)]
const INDEX_KEY: &str = "roon-ai-conversations-index";
#[allow(dead_code)]
const MSG_KEY_PREFIX: &str = "roon-ai-conversation-";
#[allow(dead_code)]
const LEGACY_STORAGE_KEY: &str = "roon-ai-conversation";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct ConversationMeta {
    id: String,
    title: String,
}

/// Ordered fragment of a streaming assistant reply. Text deltas and tool
/// calls are interleaved in the order they arrive so the UI can render
/// inline `⚡ tool` pills exactly where the agent paused. In-memory only —
/// not persisted, since once streaming completes the bubble switches to the
/// server's rendered HTML.
#[derive(Clone, PartialEq)]
enum StreamPart {
    Text(String),
    Tool(String),
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct ChatMessage {
    role: Role,
    /// Display text. For Assistant this is rendered HTML once streaming
    /// finishes; while streaming it's plain text accumulating from deltas.
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
    /// True while the assistant turn is mid-stream. Renders as plain text
    /// (whitespace-preserving). Once the `done` event arrives, this flips to
    /// false and `text` becomes the rendered HTML body.
    #[serde(default)]
    streaming: bool,
    /// Interleaved text/tool fragments captured during the stream. Skipped
    /// from serialization — only consulted while `streaming == true`.
    #[serde(skip)]
    stream_parts: Vec<StreamPart>,
}

/// Streaming events from the server, deserialised from each SSE message's
/// JSON `data` payload. Mirrors `crate::ai::StreamEvent` server-side.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentEvent {
    Text { text: String },
    Tool { summary: String },
    Done {
        #[serde(default)]
        response: String,
        #[serde(default)]
        response_markdown: String,
        #[serde(default)]
        suggestions: Vec<Suggestion>,
    },
    Error { message: String },
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
    current_track: Signal<Option<CurrentTrack>>,
) {
    if msg.is_empty() || *loading.read() {
        return;
    }
    let zone = {
        let z = selected_zone.read().clone();
        if z.is_empty() { None } else { Some(z) }
    };
    let history = build_history(&messages.read());
    let track_snapshot = current_track.read().clone();

    // Push the user turn + an in-progress streaming assistant bubble.
    messages.write().push(ChatMessage {
        role: Role::User,
        text: msg.clone(),
        markdown: String::new(),
        actions: vec![],
        suggestions: vec![],
        streaming: false,
        stream_parts: vec![],
    });
    messages.write().push(ChatMessage {
        role: Role::Assistant,
        text: String::new(),
        markdown: String::new(),
        actions: vec![],
        suggestions: vec![],
        streaming: true,
        stream_parts: vec![],
    });
    let in_progress_idx = messages.read().len() - 1;

    loading.set(true);

    let req = AiChatRequest {
        message: msg,
        zone_id: zone,
        history,
        current_track: track_snapshot,
    };
    let req_json = match serde_json::to_value(&req) {
        Ok(v) => v,
        Err(e) => {
            messages.write().clear_streaming(in_progress_idx, Role::Error, format!("encode error: {}", e));
            loading.set(false);
            return;
        }
    };

    spawn(async move {
        let mut eval = dioxus::document::eval(STREAM_CONSUMER_JS);
        // Send the request body to the JS side.
        let _ = eval.send(req_json);

        let mut spoken_markdown: Option<String> = None;

        loop {
            match eval.recv::<AgentEvent>().await {
                Ok(AgentEvent::Text { text }) => {
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        m.text.push_str(&text);
                        // Coalesce consecutive text deltas into one segment
                        // so the inline render doesn't fragment a sentence.
                        match m.stream_parts.last_mut() {
                            Some(StreamPart::Text(t)) => t.push_str(&text),
                            _ => m.stream_parts.push(StreamPart::Text(text)),
                        }
                    }
                }
                Ok(AgentEvent::Tool { summary }) => {
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        m.actions.push(summary.clone());
                        m.stream_parts.push(StreamPart::Tool(summary));
                    }
                }
                Ok(AgentEvent::Done { response, response_markdown, suggestions }) => {
                    if *speech.speak_enabled.read() {
                        spoken_markdown = Some(response_markdown.clone());
                    }
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        m.text = response;
                        m.markdown = response_markdown;
                        m.suggestions = suggestions;
                        m.streaming = false;
                    }
                    break;
                }
                Ok(AgentEvent::Error { message }) => {
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        m.role = Role::Error;
                        m.text = message;
                        m.streaming = false;
                    }
                    break;
                }
                Err(_) => {
                    // Stream ended without a Done event (network drop / parse failure).
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        if m.streaming {
                            m.role = Role::Error;
                            m.text = "Stream ended unexpectedly.".to_string();
                            m.streaming = false;
                        }
                    }
                    break;
                }
            }
        }
        loading.set(false);

        if let Some(md) = spoken_markdown {
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
                start_listening_task(messages, loading, selected_zone, speech, current_track);
            }
        }
    });
}

trait ChatMessagesExt {
    /// Replace the in-progress streaming turn at `idx` with a finalised one
    /// of `role` and `text`. Used for early-exit error paths.
    fn clear_streaming(&mut self, idx: usize, role: Role, text: String);
}

impl ChatMessagesExt for Vec<ChatMessage> {
    fn clear_streaming(&mut self, idx: usize, role: Role, text: String) {
        if let Some(m) = self.get_mut(idx) {
            m.role = role;
            m.text = text;
            m.streaming = false;
        }
    }
}

/// JS module that runs a fetch+stream against `/api/ai/chat/stream`, parses
/// SSE events, and forwards each event JSON to Rust via `dioxus.send`.
const STREAM_CONSUMER_JS: &str = r#"
const req = await dioxus.recv();
try {
    const response = await fetch('/api/ai/chat/stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(req)
    });
    if (!response.ok) {
        const t = await response.text();
        dioxus.send({ kind: 'error', message: `HTTP ${response.status}: ${t}` });
        return;
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let sep;
        while ((sep = buffer.indexOf('\n\n')) !== -1) {
            const raw = buffer.slice(0, sep);
            buffer = buffer.slice(sep + 2);
            for (const line of raw.split('\n')) {
                if (line.startsWith('data:')) {
                    const data = line.slice(5).trim();
                    if (!data) continue;
                    try {
                        dioxus.send(JSON.parse(data));
                    } catch (e) { /* skip malformed */ }
                }
            }
        }
    }
} catch (e) {
    dioxus.send({ kind: 'error', message: String(e) });
}
"#;

/// Start mic capture in a spawned task. On result, auto-submits via do_send_text.
fn start_listening_task(
    messages: Signal<Vec<ChatMessage>>,
    loading: Signal<bool>,
    selected_zone: Signal<String>,
    speech: SpeechCtx,
    current_track: Signal<Option<CurrentTrack>>,
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
                        do_send_text(trimmed, messages, loading, selected_zone, speech, current_track);
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
    current_track: Signal<Option<CurrentTrack>>,
) {
    let msg = input.read().trim().to_string();
    if msg.is_empty() || *loading.read() {
        return;
    }
    input.set(String::new());
    do_send_text(msg, messages, loading, selected_zone, speech, current_track);
}

fn play_message_for(s: &Suggestion) -> String {
    match (&s.artist, &s.album) {
        (Some(a), _) if !a.is_empty() => format!("Play \"{}\" by {}", s.title, a),
        (_, Some(al)) if !al.is_empty() => format!("Play \"{}\" from {}", s.title, al),
        _ => format!("Play \"{}\"", s.title),
    }
}

/// Dispatch a transport command to the right adapter endpoint based on zone
/// prefix. Used by the now-playing banner buttons. Routes Roon zones to
/// `/roon/control` and UPnP zones to `/upnp/control`. Errors are logged but
/// not surfaced — SSE will reflect any failure as the UI state failing to
/// update.
fn do_transport(zone_id: String, action: &'static str) {
    if zone_id.is_empty() {
        return;
    }
    spawn(async move {
        let url = if zone_id.starts_with("upnp:") {
            "/upnp/control"
        } else {
            "/roon/control"
        };
        let body = serde_json::json!({ "zone_id": zone_id, "action": action });
        if let Err(e) = crate::app::api::post_json_no_response(url, &body).await {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(&format!("Transport {action} failed: {e}").into());
            #[cfg(not(target_arch = "wasm32"))]
            tracing::warn!("Transport {} failed: {}", action, e);
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn load_messages_from_storage(id: &str) -> Vec<ChatMessage> {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let key = format!("{}{}", MSG_KEY_PREFIX, id);
            if let Ok(Some(json)) = storage.get_item(&key) {
                if let Ok(parsed) = serde_json::from_str::<Vec<ChatMessage>>(&json) {
                    return parsed;
                }
            }
        }
    }
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_messages_from_storage(_id: &str) -> Vec<ChatMessage> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
fn save_messages_to_storage(id: &str, messages: &[ChatMessage]) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let key = format!("{}{}", MSG_KEY_PREFIX, id);
            if let Ok(json) = serde_json::to_string(messages) {
                let _ = storage.set_item(&key, &json);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_messages_to_storage(_id: &str, _messages: &[ChatMessage]) {}

#[cfg(target_arch = "wasm32")]
fn delete_conversation_storage(id: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let key = format!("{}{}", MSG_KEY_PREFIX, id);
            let _ = storage.remove_item(&key);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn delete_conversation_storage(_id: &str) {}

#[cfg(target_arch = "wasm32")]
fn load_index() -> Vec<ConversationMeta> {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(json)) = storage.get_item(INDEX_KEY) {
                if let Ok(parsed) = serde_json::from_str::<Vec<ConversationMeta>>(&json) {
                    return parsed;
                }
            }
        }
    }
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_index() -> Vec<ConversationMeta> {
    Vec::new()
}

#[cfg(target_arch = "wasm32")]
fn save_index(index: &[ConversationMeta]) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(json) = serde_json::to_string(index) {
                let _ = storage.set_item(INDEX_KEY, &json);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_index(_index: &[ConversationMeta]) {}

/// Migrate the legacy single-conversation key into a new index entry. Runs
/// once on first hydrate after the multi-conversation upgrade. Returns the
/// migrated id if anything was migrated.
#[cfg(target_arch = "wasm32")]
fn migrate_legacy() -> Option<String> {
    let window = web_sys::window()?;
    let storage = window.local_storage().ok().flatten()?;
    let legacy_json = storage.get_item(LEGACY_STORAGE_KEY).ok().flatten()?;
    if legacy_json.is_empty() || legacy_json == "[]" {
        let _ = storage.remove_item(LEGACY_STORAGE_KEY);
        return None;
    }
    let id = generate_conversation_id();
    let new_key = format!("{}{}", MSG_KEY_PREFIX, id);
    let _ = storage.set_item(&new_key, &legacy_json);
    let _ = storage.remove_item(LEGACY_STORAGE_KEY);
    Some(id)
}

#[cfg(not(target_arch = "wasm32"))]
fn migrate_legacy() -> Option<String> {
    None
}

/// Generate a new conversation id. Uses the JS Date.now() so we don't need
/// chrono on WASM. On non-wasm (SSR) the id never matters because we never
/// hit storage there.
#[cfg(target_arch = "wasm32")]
fn generate_conversation_id() -> String {
    let now_ms = js_sys::Date::now() as u64;
    format!("c{}", now_ms)
}

#[cfg(not(target_arch = "wasm32"))]
fn generate_conversation_id() -> String {
    "c0".to_string()
}

/// Derive a 2-4 word title from the first user message. Trim whitespace,
/// take up to ~40 chars, append … if truncated.
fn derive_title(first_user_message: &str) -> String {
    let trimmed = first_user_message.trim();
    if trimmed.is_empty() {
        return "New chat".to_string();
    }
    let mut out = String::new();
    let mut count = 0usize;
    for ch in trimmed.chars() {
        if count >= 40 {
            out.push('…');
            return out;
        }
        out.push(ch);
        count += 1;
    }
    out
}

#[component]
pub fn ConversationalAi() -> Element {
    let mut messages = use_signal(|| Vec::<ChatMessage>::new());
    let mut input = use_signal(|| String::new());
    let mut selected_zone = use_signal(|| String::new());
    let loading = use_signal(|| false);
    let default_zone_ctx = use_default_zone();

    // Multi-conversation state.
    let mut conversations = use_signal(|| Vec::<ConversationMeta>::new());
    let mut current_id = use_signal(|| String::new());
    // Tracks whether we've finished hydrating from storage; suppresses the
    // initial save-on-mount that would otherwise overwrite stored data with
    // an empty list before the load has a chance to populate it.
    let mut hydrated = use_signal(|| false);

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

    // Hydrate the conversation index + initial messages on mount. Migrates the
    // legacy single-key conversation if found. Effect runs exactly once because
    // it doesn't read any tracked signals.
    use_effect(move || {
        let mut idx = load_index();
        if idx.is_empty() {
            // First-run-after-upgrade: try migrating the legacy single key.
            if let Some(legacy_id) = migrate_legacy() {
                idx.push(ConversationMeta {
                    id: legacy_id,
                    title: "Conversation".to_string(),
                });
                save_index(&idx);
            } else {
                // Fresh install — create a starter entry so the dropdown is never empty.
                let id = generate_conversation_id();
                idx.push(ConversationMeta {
                    id,
                    title: "New chat".to_string(),
                });
                save_index(&idx);
            }
        }
        let initial_id = idx[0].id.clone();
        let initial_msgs = load_messages_from_storage(&initial_id);
        conversations.set(idx);
        current_id.set(initial_id);
        if !initial_msgs.is_empty() {
            messages.set(initial_msgs);
        }
        hydrated.set(true);
    });

    // Persist messages under the current conversation id whenever they change.
    // Suppressed until hydration finishes so we don't blow away stored data
    // with the empty initial signal.
    use_effect(move || {
        if !*hydrated.read() {
            return;
        }
        let id = current_id.read().clone();
        if id.is_empty() {
            return;
        }
        let snapshot = messages.read().clone();
        save_messages_to_storage(&id, &snapshot);
    });

    // Auto-title: when a conversation that's still titled "New chat" gets its
    // first user turn, derive a title from that message and persist it.
    use_effect(move || {
        if !*hydrated.read() {
            return;
        }
        let id = current_id.read().clone();
        if id.is_empty() {
            return;
        }
        let snapshot = messages.read();
        let first_user = snapshot.iter().find(|m| matches!(m.role, Role::User));
        let Some(first) = first_user else { return };
        let new_title = derive_title(&first.text);
        let mut idx = conversations.read().clone();
        let mut changed = false;
        for entry in idx.iter_mut() {
            if entry.id == id && entry.title == "New chat" {
                entry.title = new_title.clone();
                changed = true;
                break;
            }
        }
        if changed {
            save_index(&idx);
            conversations.set(idx);
        }
    });

    let mut zones = use_resource(|| async {
        crate::app::api::fetch_json::<ZonesResponse>("/zones")
            .await
            .ok()
            .map(|r| r.zones)
            .unwrap_or_default()
    });

    // Re-fetch zones whenever SSE indicates a now-playing / zone change so the
    // banner reflects fresh state.
    let sse = use_sse();
    let event_count = sse.event_count;
    use_effect(move || {
        let _ = event_count();
        if sse.should_refresh_zones() {
            zones.restart();
        }
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

    // Now-playing for the currently selected zone, derived from the latest
    // `/zones` payload. Updated reactively whenever zones or the zone selection
    // changes (which itself triggers on SSE events).
    let mut current_track: Signal<Option<CurrentTrack>> = use_signal(|| None);
    use_effect(move || {
        let list = zones.read().clone().unwrap_or_default();
        let zid = selected_zone.read().clone();
        let zone: Option<Zone> = list.iter().find(|z| z.zone_id == zid).cloned();
        let track = zone
            .as_ref()
            .and_then(|z| z.now_playing.clone())
            .filter(|np| !np.title.is_empty())
            .map(|np| {
                let is_playing = zone
                    .as_ref()
                    .and_then(|z| z.state.as_deref())
                    .map(|s| s.eq_ignore_ascii_case("playing"))
                    .unwrap_or(false);
                CurrentTrack {
                    title: Some(np.title),
                    artist: if np.artist.is_empty() { None } else { Some(np.artist) },
                    album: if np.album.is_empty() { None } else { Some(np.album) },
                    is_playing,
                }
            });
        current_track.set(track);
    });

    let send = move |_: Event<MouseData>| do_send(input, messages, loading, selected_zone, speech, current_track);

    let on_keydown = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter && !e.modifiers().shift() {
            do_send(input, messages, loading, selected_zone, speech, current_track);
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
            start_listening_task(messages, loading, selected_zone, speech, current_track);
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
                    // Conversation selector: dropdown of past chats + New + Delete.
                    {
                        let convs = conversations.read().clone();
                        let cur = current_id.read().clone();
                        let single = convs.len() <= 1;
                        rsx! {
                            div { class: "flex items-center gap-1",
                                if !convs.is_empty() {
                                    select {
                                        class: "input text-sm py-1 max-w-[14rem]",
                                        title: "Switch conversation",
                                        value: "{cur}",
                                        oninput: move |e| {
                                            let new_id = e.value();
                                            if new_id == *current_id.read() {
                                                return;
                                            }
                                            let loaded = load_messages_from_storage(&new_id);
                                            current_id.set(new_id);
                                            messages.set(loaded);
                                        },
                                        for c in convs.iter() {
                                            option {
                                                value: "{c.id}",
                                                selected: c.id == cur,
                                                "{c.title}"
                                            }
                                        }
                                    }
                                }
                                button {
                                    class: "btn btn-outline btn-sm",
                                    title: "New conversation",
                                    onclick: move |_| {
                                        let id = generate_conversation_id();
                                        let mut idx = conversations.read().clone();
                                        idx.insert(0, ConversationMeta {
                                            id: id.clone(),
                                            title: "New chat".to_string(),
                                        });
                                        save_index(&idx);
                                        conversations.set(idx);
                                        current_id.set(id);
                                        messages.set(Vec::new());
                                    },
                                    "+ New"
                                }
                                button {
                                    class: "btn btn-outline btn-sm disabled:opacity-40",
                                    title: if single {
                                        "Clears the current conversation"
                                    } else {
                                        "Delete this conversation"
                                    },
                                    disabled: messages.read().is_empty() && single,
                                    onclick: move |_| {
                                        let id = current_id.read().clone();
                                        if id.is_empty() {
                                            return;
                                        }
                                        let mut idx = conversations.read().clone();
                                        if idx.len() <= 1 {
                                            // Only one conversation — wipe its messages but
                                            // keep the entry (rename back to "New chat" so
                                            // auto-title can fire again).
                                            for entry in idx.iter_mut() {
                                                if entry.id == id {
                                                    entry.title = "New chat".to_string();
                                                    break;
                                                }
                                            }
                                            save_index(&idx);
                                            conversations.set(idx);
                                            messages.set(Vec::new());
                                            delete_conversation_storage(&id);
                                            return;
                                        }
                                        // Multiple conversations — drop this one, switch to next.
                                        idx.retain(|c| c.id != id);
                                        delete_conversation_storage(&id);
                                        let next_id = idx[0].id.clone();
                                        let next_msgs = load_messages_from_storage(&next_id);
                                        save_index(&idx);
                                        conversations.set(idx);
                                        current_id.set(next_id);
                                        messages.set(next_msgs);
                                    },
                                    if single { "Clear" } else { "Delete" }
                                }
                            }
                        }
                    }
                }
            }

            // Now-playing banner — shown when the selected zone has a track.
            // Gives Claude implicit context for "skip this", "more like this", etc.
            // Also exposes ⏮ / ⏯ / ⏭ buttons that hit `/roon/control` directly
            // (faster than routing the command through the agent loop).
            {
                let track = current_track.read().clone();
                rsx! {
                    if let Some(t) = track {
                        div { class: "mb-4 flex items-center gap-3 rounded-lg border border-border bg-muted/40 px-3 py-2 text-sm",
                            span { class: "text-xs text-muted uppercase tracking-wider",
                                if t.is_playing { "Now playing" } else { "On deck" }
                            }
                            div { class: "flex flex-col flex-1 min-w-0",
                                span { class: "font-medium truncate",
                                    "{t.title.clone().unwrap_or_default()}"
                                }
                                {
                                    let parts: Vec<String> = [t.artist.clone(), t.album.clone()]
                                        .into_iter()
                                        .flatten()
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                    if !parts.is_empty() {
                                        let line = parts.join(" — ");
                                        rsx! { span { class: "text-xs text-muted truncate", "{line}" } }
                                    } else {
                                        rsx! {}
                                    }
                                }
                            }
                            {
                                let zid_prev = selected_zone.read().clone();
                                let zid_play = selected_zone.read().clone();
                                let zid_next = selected_zone.read().clone();
                                let is_playing = t.is_playing;
                                rsx! {
                                    div { class: "flex items-center gap-1",
                                        button {
                                            class: "px-2 py-1 rounded-md hover:bg-muted text-base leading-none",
                                            "aria-label": "Previous track",
                                            title: "Previous",
                                            onclick: move |_| do_transport(zid_prev.clone(), "previous"),
                                            "⏮"
                                        }
                                        button {
                                            class: "px-2 py-1 rounded-md hover:bg-muted text-base leading-none",
                                            "aria-label": if is_playing { "Pause" } else { "Play" },
                                            title: if is_playing { "Pause" } else { "Play" },
                                            onclick: move |_| do_transport(zid_play.clone(), "play_pause"),
                                            if is_playing { "⏸" } else { "▶" }
                                        }
                                        button {
                                            class: "px-2 py-1 rounded-md hover:bg-muted text-base leading-none",
                                            "aria-label": "Next track",
                                            title: "Next",
                                            onclick: move |_| do_transport(zid_next.clone(), "next"),
                                            "⏭"
                                        }
                                    }
                                }
                            }
                        }
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
                                let is_streaming = msg.streaming;
                                match msg.role {
                                    Role::User => rsx! {
                                        div {
                                            class: "self-end max-w-[90%] rounded-2xl rounded-br-sm bg-primary text-primary-foreground px-4 py-2 text-sm",
                                            "{text}"
                                        }
                                    },
                                    Role::Assistant => rsx! {
                                        div { class: "self-start max-w-[90%] flex flex-col gap-2",
                                            if is_streaming {
                                                // Streaming render: walk the interleaved text/tool
                                                // parts so tool calls show up as inline ⚡ pills
                                                // exactly where the agent paused. Switches to
                                                // dangerous_inner_html on the Done event.
                                                div {
                                                    class: "rounded-2xl rounded-bl-sm bg-muted px-4 py-3 text-sm whitespace-pre-wrap",
                                                    {
                                                        let parts = msg.stream_parts.clone();
                                                        // Fall back to raw `text` for any in-flight
                                                        // stream that predates the parts vector
                                                        // (e.g. legacy localStorage hydration).
                                                        if parts.is_empty() && !text.is_empty() {
                                                            rsx! { "{text}" }
                                                        } else {
                                                            rsx! {
                                                                for part in parts.iter() {
                                                                    {
                                                                        match part {
                                                                            StreamPart::Text(t) => rsx! { "{t}" },
                                                                            StreamPart::Tool(s) => rsx! {
                                                                                span {
                                                                                    class: "inline-block mx-1 px-2 py-0.5 rounded-full bg-primary/10 text-primary text-xs font-mono align-baseline",
                                                                                    title: "Tool call",
                                                                                    "⚡ {s}"
                                                                                }
                                                                            },
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    span { class: "inline-block w-2 h-4 ml-0.5 bg-muted-foreground/60 animate-pulse align-middle" }
                                                }
                                            } else {
                                                div {
                                                    class: "rounded-2xl rounded-bl-sm bg-muted px-4 py-3 ai-prose",
                                                    dangerous_inner_html: "{text}",
                                                }
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
                                                                        onclick: move |_| do_send_text(play_msg.clone(), messages, loading, selected_zone, speech, current_track),
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

                        // The in-progress assistant bubble's pulsing cursor is
                        // now the loading indicator (see is_streaming branch above).
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
