use crate::app::api::{AiChatRequest, CurrentTrack, HistoryTurn, RecentTrack, Suggestion, Zone, ZonesResponse};
use crate::app::components::Layout;
use crate::app::default_zone::use_default_zone;
use crate::app::sse::use_sse;
use crate::app::voice_context::use_voice;
use crate::app::wake_word_context::use_wake_word;
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
/// JSON `data` payload. Mirrors `crate::ai::StreamEvent` server-side, plus
/// one client-only variant (`SpeechComplete`) emitted by STREAM_CONSUMER_JS
/// after the per-sentence TTS queue drains.
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
    /// Sent by STREAM_CONSUMER_JS after `Done` AND the TTS audio queue has
    /// fully drained. The Rust loop uses this to gate continuous-mode
    /// listening restart so the mic doesn't open over still-playing speech.
    SpeechComplete,
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
    recent_tracks: Signal<Vec<RecentTrack>>,
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
    let recent_snapshot = recent_tracks.read().clone();

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
        recent_tracks: recent_snapshot,
    };
    let req_json = match serde_json::to_value(&req) {
        Ok(v) => v,
        Err(e) => {
            messages.write().clear_streaming(in_progress_idx, Role::Error, format!("encode error: {}", e));
            loading.set(false);
            return;
        }
    };

    // Snapshot speech enable + voice at request-start so STREAM_CONSUMER_JS
    // sees a stable choice for the duration of this turn (toggling Speak mid-
    // stream would otherwise leave a half-finished TTS session orphaned).
    let speak_enabled = *speech.speak_enabled.read();
    let voice_str = speech.selected_voice.read().clone();
    let consumer_payload = serde_json::json!({
        "request": req_json,
        "speak": speak_enabled,
        "voice": voice_str,
    });

    spawn(async move {
        let mut eval = dioxus::document::eval(STREAM_CONSUMER_JS);
        let _ = eval.send(consumer_payload);

        let mut done_seen = false;

        loop {
            match eval.recv::<AgentEvent>().await {
                Ok(AgentEvent::Text { text }) => {
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        m.text.push_str(&text);
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
                    let mut msgs = messages.write();
                    if let Some(m) = msgs.get_mut(in_progress_idx) {
                        m.text = response;
                        m.markdown = response_markdown;
                        m.suggestions = suggestions;
                        m.streaming = false;
                    }
                    done_seen = true;
                    // Don't break — wait for SpeechComplete so we know the
                    // audio queue has drained before re-opening the mic.
                    // The bubble itself is already finalised here.
                    loading.set(false);
                }
                Ok(AgentEvent::SpeechComplete) => {
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
                    // Stream ended without any closing event (network drop /
                    // parse failure / eval channel closed).
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

        if done_seen && *speech.continuous.read() {
            start_listening_task(messages, loading, selected_zone, speech, current_track, recent_tracks);
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
///
/// Per-sentence TTS is woven into this same loop: when `speak` is enabled, each
/// text delta is appended to a sentence buffer; complete sentences are
/// dispatched to `RoonSpeech.queueTtsChunk()` immediately so the spoken reply
/// starts (and overlaps with) the streaming text. After the server's `done`
/// event we close the TTS session, await drain, then send a `speech_complete`
/// event so the Rust side knows it's safe to restart listening (continuous
/// mode) without speaking over a still-playing utterance.
const STREAM_CONSUMER_JS: &str = r#"
const payload = await dioxus.recv();
const req = payload && payload.request ? payload.request : payload;
const speakEnabled = !!(payload && payload.speak);
const voice = (payload && typeof payload.voice === "string") ? payload.voice : "";

// Sentence buffer for per-sentence TTS. Detected via a regex that splits on
// terminal punctuation (.!?) followed by whitespace AND a likely sentence-start
// (uppercase / quote / paren). Avoids false positives on '3.14 pies' but tolerates
// occasional false positives on abbreviations like 'Mr. Smith' (no perceptible
// damage — TTS just briefly pauses where a human wouldn't).
const SENTENCE_END = /[.!?](?:["')\]]+)?\s+(?=[A-Z"'(À-ɏ]|$)/;
// Avoid speaking the suggestions sentinel itself if a token boundary lands on it.
const SUGGESTIONS_OPEN = "<<<SUGGESTIONS>>>";
const FORCE_FLUSH_LEN = 220; // force a chunk after this many chars even without a boundary
let sentenceBuf = "";
let suppressTts = false; // flips true once we encounter the suggestions block

if (speakEnabled) {
    try { window.RoonSpeech.startTtsSession(voice); } catch (e) {}
}

function _flushSentenceBuf(force) {
    if (!speakEnabled || suppressTts) return;
    while (true) {
        if (!sentenceBuf) return;
        // If we see the start of the suggestions sentinel, stop speaking from here
        // on — the rest of the text is JSON not meant for TTS.
        const sIdx = sentenceBuf.indexOf(SUGGESTIONS_OPEN);
        if (sIdx === 0) { suppressTts = true; sentenceBuf = ""; return; }
        const m = SENTENCE_END.exec(sentenceBuf);
        if (m) {
            const cut = m.index + m[0].length;
            const sentence = sentenceBuf.slice(0, cut);
            sentenceBuf = sentenceBuf.slice(cut);
            if (sIdx > -1 && sIdx < cut) {
                // Sentinel hit before the boundary — speak only what came before it
                try { window.RoonSpeech.queueTtsChunk(sentence.slice(0, sIdx)); } catch (e) {}
                suppressTts = true;
                sentenceBuf = "";
                return;
            }
            try { window.RoonSpeech.queueTtsChunk(sentence); } catch (e) {}
            continue;
        }
        if (force || sentenceBuf.length >= FORCE_FLUSH_LEN) {
            // No sentence boundary in the (long enough) buffer — speak it as-is
            // to bound latency. Try to break on a comma/space if possible.
            let cut = sentenceBuf.length;
            if (!force) {
                const commaIdx = sentenceBuf.lastIndexOf(", ");
                const spaceIdx = sentenceBuf.lastIndexOf(" ");
                cut = commaIdx > 50 ? commaIdx + 1 : (spaceIdx > 50 ? spaceIdx + 1 : sentenceBuf.length);
            }
            const chunk = sentenceBuf.slice(0, cut);
            sentenceBuf = sentenceBuf.slice(cut);
            try { window.RoonSpeech.queueTtsChunk(chunk); } catch (e) {}
            if (!sentenceBuf) return;
            // loop again in case force flush left more
            continue;
        }
        return;
    }
}

try {
    const response = await fetch('/api/ai/chat/stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(req)
    });
    if (!response.ok) {
        const t = await response.text();
        dioxus.send({ kind: 'error', message: `HTTP ${response.status}: ${t}` });
        if (speakEnabled) { try { await window.RoonSpeech.closeTtsSession(); } catch (e) {} }
        dioxus.send({ kind: 'speech_complete' });
        return;
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let sawDone = false;
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
                    let evt;
                    try { evt = JSON.parse(data); } catch (e) { continue; }
                    // Forward to Rust first so the bubble updates promptly
                    dioxus.send(evt);
                    // Per-sentence TTS dispatch
                    if (speakEnabled && evt && evt.kind === "text" && typeof evt.text === "string") {
                        sentenceBuf += evt.text;
                        _flushSentenceBuf(false);
                    }
                    if (evt && evt.kind === "done") {
                        sawDone = true;
                        if (speakEnabled) _flushSentenceBuf(true);
                    }
                }
            }
        }
    }
    // Stream closed. If we got Done, await TTS queue drain then signal complete.
    // If we never got Done (network drop), still signal complete so Rust loop
    // doesn't block forever.
    if (speakEnabled) {
        if (!sawDone) _flushSentenceBuf(true);
        try { await window.RoonSpeech.closeTtsSession(); } catch (e) {}
    }
    dioxus.send({ kind: 'speech_complete' });
} catch (e) {
    dioxus.send({ kind: 'error', message: String(e) });
    if (speakEnabled) { try { await window.RoonSpeech.closeTtsSession(); } catch (e2) {} }
    dioxus.send({ kind: 'speech_complete' });
}
"#;

/// JS module that wraps Picovoice Porcupine for browser-side wake-word
/// detection. Idempotent: re-running this script is safe if the module is
/// already installed.
///
/// **Fully scaffold-grade.** Until the user vendors three files into the
/// project's `public/wake-word/` directory the engine never initialises and
/// every method is a graceful no-op:
///   - `porcupine_web.iife.js` — Picovoice browser SDK (IIFE bundle from a
///     `@picovoice/porcupine-web` release)
///   - `pv_porcupine.wasm` — the engine WASM blob
///   - `Hey-Roon-AI_en.ppn` — the trained wake-word model the user generates
///     at <https://console.picovoice.ai/> (platform = WebAssembly)
///
/// `init(accessKey)` returns `true` only when all three are present, the
/// access key is valid, and the engine bootstraps successfully. `start()` /
/// `pause()` / `resume()` / `stop()` then drive the WebVoiceProcessor mic
/// subscription. Detections fire `RoonWake.onDetection()` if registered.
const WAKE_WORD_INSTALL_JS: &str = r#"
if (!window.RoonWake) {
    window.RoonWake = {
        ready: false,
        active: false,
        worker: null,
        onDetection: null,
        async _ensureSdkLoaded() {
            if (window.PorcupineWeb) return true;
            // Probe for the IIFE bundle. If 404, the user hasn't vendored.
            try {
                const head = await fetch('/wake-word/porcupine_web.iife.js', { method: 'HEAD' });
                if (!head.ok) return false;
            } catch (e) { return false; }
            // Inject script tag if not already injected.
            if (document.querySelector('script[data-roon-porcupine]')) {
                // Already injecting; wait briefly for it to settle.
                for (let i = 0; i < 30 && !window.PorcupineWeb; i++) {
                    await new Promise(r => setTimeout(r, 100));
                }
                return !!window.PorcupineWeb;
            }
            return await new Promise((resolve) => {
                const s = document.createElement('script');
                s.src = '/wake-word/porcupine_web.iife.js';
                s.dataset.roonPorcupine = '1';
                s.onload = () => resolve(!!window.PorcupineWeb);
                s.onerror = () => resolve(false);
                document.head.appendChild(s);
            });
        },
        async init(accessKey) {
            if (this.ready) return true;
            if (!accessKey) return false;
            const sdk = await this._ensureSdkLoaded();
            if (!sdk) return false;
            try {
                this.worker = await window.PorcupineWeb.PorcupineWorker.create(
                    accessKey,
                    [{ publicPath: '/wake-word/Hey-Roon-AI_en.ppn', label: 'roon-ai' }],
                    () => { try { if (this.onDetection) this.onDetection(); } catch (e) {} },
                    { publicPath: '/wake-word/pv_porcupine.wasm' }
                );
                this.ready = true;
                return true;
            } catch (e) {
                console.warn('Porcupine init failed:', e);
                return false;
            }
        },
        async start() {
            if (!this.ready || this.active) return this.active;
            try {
                await window.PorcupineWeb.WebVoiceProcessor.subscribe(this.worker);
                this.active = true;
                return true;
            } catch (e) { return false; }
        },
        async pause() {
            if (!this.active) return;
            try { await window.PorcupineWeb.WebVoiceProcessor.unsubscribe(this.worker); } catch (e) {}
            this.active = false;
        },
        async resume() {
            return this.start();
        },
        async stop() {
            if (this.active) {
                try { await window.PorcupineWeb.WebVoiceProcessor.unsubscribe(this.worker); } catch (e) {}
                this.active = false;
            }
            if (this.worker) {
                try { await this.worker.terminate(); } catch (e) {}
                this.worker = null;
            }
            this.ready = false;
            this.onDetection = null;
        }
    };
}
return true;
"#;

/// Long-running eval task: receives `{accessKey}` via `dioxus.recv()`, inits
/// Porcupine, then sends `{kind: "ready"}` and forwards each subsequent
/// detection as `{kind: "detected"}`. Sends `{kind: "failed", reason}` if
/// the engine can't initialise. The task is abandoned (eval dropped) when
/// the user disables the wake word — at which point a separate one-shot
/// eval calls `window.RoonWake.stop()` to halt the engine.
const WAKE_WORD_LISTEN_JS: &str = r#"
const cfg = await dioxus.recv();
const ok = await window.RoonWake.init(cfg && cfg.accessKey);
if (!ok) {
    dioxus.send({ kind: 'failed', reason: 'init failed (check assets in public/wake-word/ and your access key)' });
    return;
}
window.RoonWake.onDetection = () => { try { dioxus.send({ kind: 'detected' }); } catch (e) {} };
const started = await window.RoonWake.start();
if (!started) {
    dioxus.send({ kind: 'failed', reason: 'failed to acquire microphone' });
    return;
}
dioxus.send({ kind: 'ready' });
// Keep the eval alive so the detection callback stays valid. When the
// outer task is dropped (Rust drops the eval), this Promise never resolves
// — that's fine, the engine is still running on the JS side until
// window.RoonWake.stop() is called by the disable handler.
await new Promise(() => {});
"#;

/// Events from `WAKE_WORD_LISTEN_JS`.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WakeEvent {
    Ready,
    Detected,
    Failed { reason: String },
}

/// Start mic capture in a spawned task. On result, auto-submits via do_send_text.
fn start_listening_task(
    messages: Signal<Vec<ChatMessage>>,
    loading: Signal<bool>,
    selected_zone: Signal<String>,
    speech: SpeechCtx,
    current_track: Signal<Option<CurrentTrack>>,
    recent_tracks: Signal<Vec<RecentTrack>>,
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
                        do_send_text(trimmed, messages, loading, selected_zone, speech, current_track, recent_tracks);
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

    // ===== Streaming TTS session state (module-scoped so cancelSpeech() and
    // a new session always tear down the previous one cleanly). =====
    let _ttsActive = false;       // a session is open (between start and close)
    let _ttsClosed = false;       // close was called; awaiting drain
    let _ttsVoice = null;         // voiceName from Settings (browser name or "openai:<id>")
    let _ttsSeq = 0;              // submitted-chunk counter (drives ordering)
    let _ttsNextPlay = 0;         // OpenAI: next sequence to play
    let _ttsBlobs = new Map();    // OpenAI: seq -> blob (received but waiting in line)
    let _ttsPending = 0;          // outstanding chunks (fetch-in-flight OR queued OR speaking)
    let _ttsAudio = null;         // OpenAI: currently-playing Audio element
    let _ttsObjectUrls = new Set(); // OpenAI: object URLs to revoke on cancel
    let _ttsBrowserUtterances = []; // browser path: utterances still in queue/speaking
    let _ttsAbort = null;         // shared AbortController for OpenAI fetches in this session
    let _ttsCompleteResolve = null;
    let _ttsCompletePromise = null;

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

    function _isOpenAi(voiceName) {
        return typeof voiceName === "string" && voiceName.startsWith("openai:");
    }

    function _maybeFinishTts() {
        if (!_ttsActive) return;
        if (!_ttsClosed) return;
        if (_ttsPending > 0) return;
        // All chunks submitted, drained, audio finished → resolve.
        _ttsActive = false;
        _ttsClosed = false;
        const r = _ttsCompleteResolve;
        _ttsCompleteResolve = null;
        _ttsCompletePromise = null;
        if (r) r();
    }

    function _resetTtsState() {
        if (_ttsAbort) { try { _ttsAbort.abort(); } catch (e) {} _ttsAbort = null; }
        if (_ttsAudio) { try { _ttsAudio.pause(); _ttsAudio.src = ""; } catch (e) {} _ttsAudio = null; }
        for (const u of _ttsObjectUrls) { try { URL.revokeObjectURL(u); } catch (e) {} }
        _ttsObjectUrls.clear();
        _ttsBlobs.clear();
        _ttsBrowserUtterances.length = 0;
        try { window.speechSynthesis.cancel(); } catch (e) {}
        _ttsActive = false;
        _ttsClosed = false;
        _ttsPending = 0;
        _ttsSeq = 0;
        _ttsNextPlay = 0;
        if (_ttsCompleteResolve) { try { _ttsCompleteResolve(); } catch (e) {} _ttsCompleteResolve = null; }
        _ttsCompletePromise = null;
    }

    async function _openAiChunk(seq, text, voice) {
        const ctrl = _ttsAbort;
        let resp;
        try {
            resp = await fetch('/api/tts', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ text, voice }),
                signal: ctrl ? ctrl.signal : undefined,
            });
        } catch (e) {
            // aborted or network — drop the chunk, decrement, and try to drain
            _ttsPending--;
            _maybeFinishTts();
            return;
        }
        if (!_ttsActive) { _ttsPending--; _maybeFinishTts(); return; }
        if (!resp.ok) {
            try { const t = await resp.text(); console.warn('TTS error', resp.status, t); } catch (e) {}
            _ttsPending--;
            _maybeFinishTts();
            return;
        }
        let blob;
        try { blob = await resp.blob(); } catch (e) { _ttsPending--; _maybeFinishTts(); return; }
        if (!_ttsActive) { _ttsPending--; _maybeFinishTts(); return; }
        _ttsBlobs.set(seq, blob);
        _maybePlayNextOpenAi();
    }

    function _maybePlayNextOpenAi() {
        if (!_ttsActive) return;
        if (_ttsAudio) return;
        const blob = _ttsBlobs.get(_ttsNextPlay);
        if (!blob) {
            // The next-in-line chunk hasn't arrived yet. If we've drained
            // everything and there's nothing pending, finish.
            _maybeFinishTts();
            return;
        }
        _ttsBlobs.delete(_ttsNextPlay);
        const url = URL.createObjectURL(blob);
        _ttsObjectUrls.add(url);
        const audio = new Audio(url);
        _ttsAudio = audio;
        const onDone = () => {
            if (_ttsAudio === audio) _ttsAudio = null;
            if (_ttsObjectUrls.has(url)) {
                try { URL.revokeObjectURL(url); } catch (e) {}
                _ttsObjectUrls.delete(url);
            }
            _ttsNextPlay++;
            _ttsPending--;
            _maybePlayNextOpenAi();
        };
        audio.onended = onDone;
        audio.onerror = onDone;
        try { audio.play().catch(onDone); } catch (e) { onDone(); }
    }

    function _browserChunk(seq, text, voiceName) {
        const u = new SpeechSynthesisUtterance(text);
        u.lang = navigator.language || "en-US";
        if (voiceName) {
            const v = window.speechSynthesis.getVoices().find(v => v.name === voiceName);
            if (v) { u.voice = v; u.lang = v.lang; }
        }
        _ttsBrowserUtterances.push(u);
        const onDone = () => {
            const i = _ttsBrowserUtterances.indexOf(u);
            if (i >= 0) _ttsBrowserUtterances.splice(i, 1);
            _ttsPending--;
            _maybeFinishTts();
        };
        u.onend = onDone;
        u.onerror = onDone;
        try { window.speechSynthesis.speak(u); } catch (e) { onDone(); }
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
        // Streaming TTS API — used by STREAM_CONSUMER_JS for per-sentence
        // playback while a reply is generating.
        startTtsSession(voice) {
            _resetTtsState();
            _ttsActive = true;
            _ttsClosed = false;
            _ttsVoice = voice || null;
            _ttsSeq = 0;
            _ttsNextPlay = 0;
            _ttsBlobs = new Map();
            _ttsBrowserUtterances = [];
            _ttsObjectUrls = new Set();
            _ttsAbort = new AbortController();
            _ttsCompletePromise = new Promise(r => { _ttsCompleteResolve = r; });
        },
        queueTtsChunk(text) {
            if (!_ttsActive) return;
            const trimmed = String(text || "").trim();
            if (!trimmed) return;
            const seq = _ttsSeq++;
            _ttsPending++;
            const plain = plainify(trimmed);
            if (!plain) { _ttsPending--; return; }
            if (_isOpenAi(_ttsVoice)) {
                _openAiChunk(seq, plain, _ttsVoice.slice(7));
            } else {
                _browserChunk(seq, plain, _ttsVoice);
            }
        },
        closeTtsSession() {
            if (!_ttsActive) return Promise.resolve();
            _ttsClosed = true;
            const p = _ttsCompletePromise || Promise.resolve();
            _maybeFinishTts();
            return p;
        },
        // Legacy single-shot speak — used for any callers outside the
        // streaming path. Internally goes through the same queue so cancel
        // semantics are uniform.
        speak(md, voiceName) {
            this.startTtsSession(voiceName);
            this.queueTtsChunk(md);
            return this.closeTtsSession();
        },
        cancelSpeech() {
            _resetTtsState();
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
    recent_tracks: Signal<Vec<RecentTrack>>,
) {
    let msg = input.read().trim().to_string();
    if msg.is_empty() || *loading.read() {
        return;
    }
    input.set(String::new());
    do_send_text(msg, messages, loading, selected_zone, speech, current_track, recent_tracks);
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

/// Response shape from POST /roon/play (also covers Start Radio via
/// `action: "radio"`). Server returns `{ message: "Start Radio: <title>" }`.
#[derive(serde::Deserialize)]
struct PlayActionResponse {
    #[allow(dead_code)] // returned by server for diagnostics; we ignore on success
    message: String,
}

/// Start Roon Radio seeded by the currently-playing track. Reuses the
/// existing /roon/play endpoint with action="radio", which Roon translates
/// to the "Start Radio" item in every track action menu. The flash signal
/// carries human-readable feedback for the UI to display.
fn do_start_radio(
    zone_id: String,
    title: String,
    artist: Option<String>,
    mut flash: Signal<Option<String>>,
) {
    if zone_id.is_empty() || title.is_empty() {
        return;
    }
    // Search query: title plus artist when present, helps Roon disambiguate.
    let query = match &artist {
        Some(a) if !a.is_empty() => format!("{} {}", title, a),
        _ => title.clone(),
    };
    flash.set(Some(format!("📻 Starting radio from '{}'…", title)));
    spawn(async move {
        let body = serde_json::json!({
            "zone_id": zone_id,
            "query": query,
            "action": "radio",
            "source": "library",
        });
        match crate::app::api::post_json::<_, PlayActionResponse>("/roon/play", &body).await {
            Ok(_) => {
                flash.set(Some(format!("📻 Radio started from '{}'", title)));
            }
            Err(e) => {
                flash.set(Some(format!("Couldn't start radio: {}", e)));
            }
        }
    });
}

/// Set absolute volume on a Roon zone via `POST /roon/volume`. Only used by
/// the banner slider; UPnP zones don't expose an absolute-set endpoint and
/// hide the slider entirely. Errors are logged but not surfaced — SSE will
/// reflect the actual volume on the next update if the request was rejected.
fn do_volume_set(zone_id: String, value: f32) {
    if zone_id.is_empty() {
        return;
    }
    spawn(async move {
        let body = serde_json::json!({
            "zone_id": zone_id,
            "value": value,
            "relative": false,
        });
        if let Err(e) = crate::app::api::post_json_no_response("/roon/volume", &body).await {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(&format!("Volume set failed: {e}").into());
            #[cfg(not(target_arch = "wasm32"))]
            tracing::warn!("Volume set failed: {}", e);
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

    // Transient flash message for one-shot track actions in the now-playing
    // banner (currently used by Start Radio; designed to be reusable for
    // future single-shot actions). Stays visible until the next action or
    // until the user dismisses with the × button on the flash itself.
    let track_action_flash = use_signal(|| Option::<String>::None);

    // Album-art lightbox: when Some(url), render a native <dialog> popup over
    // the page. We use the browser's <dialog> element with showModal() rather
    // than a Tailwind position:fixed overlay because <dialog> renders in the
    // browser top layer and bypasses any transform/filter ancestor that would
    // otherwise turn position:fixed into position:absolute (the bug we hit on
    // the prior modal attempt — see HANDOFF 2026-04-28 fourth pass).
    let mut art_modal_url = use_signal(|| Option::<String>::None);

    // ✨ Similar — populated when the user clicks the banner's Similar button.
    // Cleared automatically whenever the now-playing track changes (different
    // title means stale recommendations) so we don't show suggestions seeded
    // off a track that's no longer playing.
    let mut similar_suggestions = use_signal(Vec::<Suggestion>::new);
    let mut similar_loading = use_signal(|| false);

    // Sync the <dialog>'s open state with our signal: showModal() when set to
    // Some, close() when set to None.
    use_effect(move || {
        let open = art_modal_url.read().is_some();
        let script = if open {
            "const d = document.getElementById('roon-art-modal'); if (d && !d.open) d.showModal(); return true;"
        } else {
            "const d = document.getElementById('roon-art-modal'); if (d && d.open) d.close(); return true;"
        };
        spawn(async move {
            let _ = dioxus::document::eval(script).join::<bool>().await;
        });
    });

    // Bridge native dialog 'close' events (fired by ESC) back into the signal
    // so a re-click of the same thumbnail re-opens. Single long-running eval
    // installed once on mount; the listener persists until page unload.
    use_effect(move || {
        spawn(async move {
            // Wait briefly for the dialog to be rendered into the DOM, then
            // install the listener. The empty Promise keeps the eval alive so
            // dioxus.send() retains a working channel back to Rust.
            let mut e = dioxus::document::eval(r#"
                let tries = 0;
                while (tries++ < 20) {
                    const d = document.getElementById('roon-art-modal');
                    if (d) {
                        d.addEventListener('close', () => { try { dioxus.send(true); } catch (err) {} });
                        break;
                    }
                    await new Promise(r => setTimeout(r, 50));
                }
                await new Promise(() => {});
            "#);
            loop {
                match e.recv::<bool>().await {
                    Ok(_) => art_modal_url.set(None),
                    Err(_) => break,
                }
            }
        });
    });

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
            // Install wake-word JS module too — idempotent, safe even if
            // assets aren't present (init will fail gracefully if so).
            let _ = dioxus::document::eval(WAKE_WORD_INSTALL_JS).join::<bool>().await;
        });
    });

    // Wake-word: shared context (toggle + access key live in Settings page).
    let wake_ctx = use_wake_word();

    // Spawn / shut down the Porcupine listener when the user toggles wake word
    // or pastes a new access key. When (enabled && key) flip true together,
    // run WAKE_WORD_LISTEN_JS and forward detections into wake_ctx.detected_count.
    // When either flips off, fire a one-shot stop on the JS side. The previous
    // listener task's eval is left to garbage-collect; window.RoonWake.stop()
    // cleared the detection callback, so any straggler events are no-ops.
    use_effect(move || {
        let enabled = *wake_ctx.enabled.read();
        let access_key = wake_ctx.access_key.read().clone();
        let mut status = wake_ctx.status;

        if !enabled {
            status.set("Off".into());
            spawn(async {
                let _ = dioxus::document::eval(
                    "if (window.RoonWake) await window.RoonWake.stop(); return true;",
                )
                .join::<bool>()
                .await;
            });
            return;
        }
        if access_key.is_empty() {
            status.set("Access key required".into());
            return;
        }

        status.set("Initialising…".into());
        let mut detected_count = wake_ctx.detected_count;
        spawn(async move {
            let mut eval = dioxus::document::eval(WAKE_WORD_LISTEN_JS);
            let _ = eval.send(serde_json::json!({ "accessKey": access_key }));
            loop {
                match eval.recv::<WakeEvent>().await {
                    Ok(WakeEvent::Ready) => {
                        status.set("Listening for 'Hey Roon AI'".into());
                    }
                    Ok(WakeEvent::Detected) => {
                        let next = *detected_count.peek() + 1;
                        detected_count.set(next);
                    }
                    Ok(WakeEvent::Failed { reason }) => {
                        status.set(format!("Failed: {}", reason));
                        break;
                    }
                    Err(_) => break,
                }
            }
        });
    });

    // Pause Porcupine while we're busy (a request is in flight or the mic is
    // open) so the wake word doesn't false-trigger from the AI's spoken reply
    // or from the user's STT capture itself. Resume when both go idle.
    use_effect(move || {
        if !*wake_ctx.enabled.read() { return; }
        let busy = *loading.read() || *listening.read();
        let cmd = if busy { "pause" } else { "resume" };
        let script = format!(
            "if (window.RoonWake) {{ try {{ await window.RoonWake.{}(); }} catch (e) {{}} }} return true;",
            cmd
        );
        spawn(async move {
            let _ = dioxus::document::eval(&script).join::<bool>().await;
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

    // Auto-title: two-phase.
    //   Phase 1: as soon as the first user message arrives, derive a substring
    //   title from it (instant, offline).
    //   Phase 2: once the first assistant reply has finished streaming, ask
    //   Claude Haiku for a 2-4 word title and replace the substring version.
    // Each phase only fires while the title is still its respective placeholder
    // ("New chat" → substring title → Haiku title), so we never overwrite a
    // user-edited (or Haiku-finalised) title.
    use_effect(move || {
        if !*hydrated.read() {
            return;
        }
        let id = current_id.read().clone();
        if id.is_empty() {
            return;
        }

        // Extract everything we need from the messages signal in an inner
        // scope so the read guard drops before we hit the spawned await
        // below (the await-in-lock lint catches anything else).
        let (first_user_text, first_assistant_md) = {
            let snapshot = messages.read();
            let first_user = snapshot.iter().find(|m| matches!(m.role, Role::User));
            let Some(first) = first_user else { return };
            let first_assistant = snapshot
                .iter()
                .find(|m| matches!(m.role, Role::Assistant) && !m.streaming && !m.markdown.is_empty());
            (
                first.text.clone(),
                first_assistant.map(|m| m.markdown.clone()),
            )
        };

        let substring_title = derive_title(&first_user_text);

        let current_title = conversations
            .read()
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone())
            .unwrap_or_default();

        // Phase 1: replace "New chat" with substring title for immediate feedback.
        if current_title == "New chat" {
            let mut idx = conversations.read().clone();
            for entry in idx.iter_mut() {
                if entry.id == id {
                    entry.title = substring_title.clone();
                    break;
                }
            }
            save_index(&idx);
            conversations.set(idx);
            return;
        }

        // Phase 2: if we still have the substring title AND the first assistant
        // reply has finished streaming, kick off Haiku for a nicer title.
        let Some(reply_md) = first_assistant_md else { return };
        if current_title != substring_title {
            return; // already replaced (Haiku ran or user edited)
        }

        let placeholder = substring_title.clone();
        let conv_id = id.clone();
        let mut conversations_handle = conversations;
        spawn(async move {
            let req = crate::app::api::TitleRequest {
                user_message: first_user_text,
                assistant_reply: reply_md,
            };
            let Ok(resp) = crate::app::api::ai_title(req).await else { return };
            let new_title = resp.title.trim().to_string();
            if new_title.is_empty() {
                return;
            }
            // Only replace if the title is STILL the substring placeholder
            // (in case the user edited it during the API round trip).
            let mut idx = conversations_handle.read().clone();
            let mut changed = false;
            for entry in idx.iter_mut() {
                if entry.id == conv_id && entry.title == placeholder {
                    entry.title = new_title.clone();
                    changed = true;
                    break;
                }
            }
            if changed {
                save_index(&idx);
                conversations_handle.set(idx);
            }
        });
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

    // Ring buffer of recently-played tracks (most recent first, capped at 3).
    // Populated when `current_track` transitions to a NEW title — the old track
    // is pushed onto the buffer so Claude has implicit "play more like that"
    // context without the user having to type the title.
    //
    // We subscribe to BOTH current_track and prev_track. Writing to prev_track
    // re-fires the effect once, but the early-return on equal titles prevents
    // any infinite loop. Saves us a `peek()` import.
    let mut prev_track: Signal<Option<CurrentTrack>> = use_signal(|| None);
    let mut recent_tracks: Signal<Vec<RecentTrack>> = use_signal(Vec::new);
    use_effect(move || {
        let now = current_track.read().clone();
        let prev = prev_track.read().clone();
        let now_title = now
            .as_ref()
            .and_then(|t| t.title.as_deref())
            .unwrap_or("")
            .trim()
            .to_string();
        let prev_title = prev
            .as_ref()
            .and_then(|t| t.title.as_deref())
            .unwrap_or("")
            .trim()
            .to_string();
        if now_title == prev_title {
            return;
        }
        if !prev_title.is_empty() {
            let entry = RecentTrack {
                title: prev_title.clone(),
                artist: prev.as_ref().and_then(|t| t.artist.clone()),
                album: prev.as_ref().and_then(|t| t.album.clone()),
            };
            let mut list = recent_tracks.read().clone();
            list.retain(|t| !t.title.eq_ignore_ascii_case(&entry.title));
            list.insert(0, entry);
            list.truncate(3);
            recent_tracks.set(list);
        }
        prev_track.set(now);
        // Track changed → seeded "Similar" recommendations are now stale.
        if !similar_suggestions.peek().is_empty() {
            similar_suggestions.set(Vec::new());
        }
    });

    // Wake-word detection → trigger STT (same code path as the manual mic
    // button). Watches the detected_count counter; each increment fires once.
    // The pause-while-busy effect above prevents detections from accumulating
    // during streaming or active mic capture, so we don't risk re-entering.
    use_effect(move || {
        let n = *wake_ctx.detected_count.read();
        if n == 0 { return; }
        if *loading.read() || *listening.read() { return; }
        start_listening_task(messages, loading, selected_zone, speech, current_track, recent_tracks);
    });

    let send = move |_: Event<MouseData>| do_send(input, messages, loading, selected_zone, speech, current_track, recent_tracks);

    let on_keydown = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter && !e.modifiers().shift() {
            do_send(input, messages, loading, selected_zone, speech, current_track, recent_tracks);
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
            start_listening_task(messages, loading, selected_zone, speech, current_track, recent_tracks);
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
            // Also exposes album-art thumbnail, ⏮ / ⏯ / ⏭ buttons (which hit
            // `/roon/control` directly — faster than routing through the agent),
            // and a volume slider for Roon zones (POST /roon/volume).
            {
                let track = current_track.read().clone();
                let zid = selected_zone.read().clone();
                // Pull the full Zone for image_key + volume_control. Driven by
                // the same SSE-refreshed `zones` resource as current_track, so
                // these stay in lockstep with the rest of the banner.
                let full_zone: Option<Zone> = zones
                    .read()
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .find(|z| z.zone_id == zid);
                let image_key = full_zone
                    .as_ref()
                    .and_then(|z| z.now_playing.as_ref())
                    .and_then(|np| np.image_key.clone());
                let vc = full_zone.as_ref().and_then(|z| z.volume_control.clone());
                let is_roon = zid.starts_with("roon:");
                rsx! {
                    // Track-action flash (e.g. Start Radio): persists until
                    // the next action or manual ×.
                    {
                        let flash_msg = track_action_flash.read().clone();
                        let mut flash = track_action_flash;
                        if let Some(msg) = flash_msg {
                            rsx! {
                                div {
                                    class: "mb-2 flex items-center justify-between gap-2 rounded-md border border-primary/30 bg-primary/10 px-3 py-1.5 text-sm text-primary",
                                    role: "status",
                                    "aria-live": "polite",
                                    span { class: "truncate", "{msg}" }
                                    button {
                                        class: "text-primary/70 hover:text-primary leading-none px-1",
                                        "aria-label": "Dismiss",
                                        onclick: move |_| flash.set(None),
                                        "×"
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                    if let Some(t) = track {
                        div { class: "mb-4 flex flex-col gap-2 rounded-lg border border-border bg-muted/40 px-3 py-2 text-sm",
                            // Top row: art, label, track text, transport buttons.
                            div { class: "flex items-center gap-3",
                                {
                                    if let Some(key) = image_key.as_ref() {
                                        let url = format!(
                                            "/roon/image?image_key={}&width=80&height=80",
                                            urlencoding::encode(key)
                                        );
                                        let full_url = format!(
                                            "/roon/image?image_key={}&width=1024&height=1024",
                                            urlencoding::encode(key)
                                        );
                                        rsx! {
                                            button {
                                                r#type: "button",
                                                title: "Open full-size",
                                                class: "flex-shrink-0 cursor-zoom-in",
                                                onclick: move |_| art_modal_url.set(Some(full_url.clone())),
                                                img {
                                                    src: "{url}",
                                                    alt: "Album art",
                                                    class: "w-10 h-10 object-cover rounded-md bg-muted hover:opacity-80 transition-opacity",
                                                }
                                            }
                                        }
                                    } else {
                                        rsx! {
                                            div {
                                                class: "w-10 h-10 rounded-md flex-shrink-0 bg-muted flex items-center justify-center text-muted-foreground",
                                                "♪"
                                            }
                                        }
                                    }
                                }
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
                                    let zid_prev = zid.clone();
                                    let zid_play = zid.clone();
                                    let zid_next = zid.clone();
                                    let zid_radio = zid.clone();
                                    let radio_title = t.title.clone().unwrap_or_default();
                                    let radio_artist = t.artist.clone();
                                    let is_playing = t.is_playing;
                                    let flash = track_action_flash;
                                    let radio_disabled = !is_roon || radio_title.is_empty();
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
                                            button {
                                                class: if radio_disabled {
                                                    "px-2 py-1 rounded-md text-base leading-none text-muted-foreground/40 cursor-not-allowed"
                                                } else {
                                                    "px-2 py-1 rounded-md hover:bg-primary/10 text-base leading-none"
                                                },
                                                "aria-label": "Start Roon Radio from this track",
                                                title: if radio_disabled { "Start Radio (Roon zones only)" } else { "Start Roon Radio seeded by this track" },
                                                disabled: radio_disabled,
                                                onclick: move |_| do_start_radio(
                                                    zid_radio.clone(),
                                                    radio_title.clone(),
                                                    radio_artist.clone(),
                                                    flash,
                                                ),
                                                "📻"
                                            }
                                            // ✨ Similar — Haiku suggests 3-5 tracks similar to the
                                            // current one. Suggestions render in a sub-row below the
                                            // banner; clicking ▶ Play on a row submits a chat turn
                                            // exactly like the assistant-suggestion rows do.
                                            {
                                                let sim_title = t.title.clone().unwrap_or_default();
                                                let sim_artist = t.artist.clone();
                                                let sim_album = t.album.clone();
                                                let sim_disabled = sim_title.is_empty() || *similar_loading.read();
                                                let is_loading = *similar_loading.read();
                                                rsx! {
                                                    button {
                                                        class: if sim_disabled {
                                                            "px-2 py-1 rounded-md text-base leading-none text-muted-foreground/40 cursor-not-allowed"
                                                        } else {
                                                            "px-2 py-1 rounded-md hover:bg-primary/10 text-base leading-none"
                                                        },
                                                        "aria-label": "Suggest similar tracks",
                                                        title: if is_loading { "Finding similar…" } else { "Suggest tracks similar to this one" },
                                                        disabled: sim_disabled,
                                                        onclick: move |_| {
                                                            let title = sim_title.clone();
                                                            let artist = sim_artist.clone();
                                                            let album = sim_album.clone();
                                                            similar_loading.set(true);
                                                            spawn(async move {
                                                                let req = crate::app::api::SimilarRequest { title, artist, album };
                                                                match crate::app::api::ai_similar(req).await {
                                                                    Ok(resp) => {
                                                                        let mapped: Vec<Suggestion> = resp.suggestions.into_iter().map(|s| Suggestion {
                                                                            title: s.title,
                                                                            artist: s.artist,
                                                                            album: s.album,
                                                                        }).collect();
                                                                        similar_suggestions.set(mapped);
                                                                    }
                                                                    Err(_) => {
                                                                        similar_suggestions.set(Vec::new());
                                                                    }
                                                                }
                                                                similar_loading.set(false);
                                                            });
                                                        },
                                                        if is_loading { "✨…" } else { "✨" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Bottom row: volume slider (Roon zones with a populated VolumeControl).
                            // UPnP zones expose only relative vol_up/vol_down via /upnp/control —
                            // no absolute-set endpoint — so we hide the slider for them.
                            {
                                if is_roon {
                                    if let Some(vc) = vc.as_ref() {
                                        let zid_vol = zid.clone();
                                        let unit = match vc.scale.as_deref() {
                                            Some("decibels") => " dB",
                                            _ => "",
                                        };
                                        let label = format!("{:.0}{}", vc.value, unit);
                                        rsx! {
                                            div { class: "flex items-center gap-3 px-1 pl-13",
                                                span { class: "text-xs text-muted", "🔊" }
                                                input {
                                                    r#type: "range",
                                                    class: "flex-1 accent-primary cursor-pointer",
                                                    min: "{vc.min}",
                                                    max: "{vc.max}",
                                                    step: "{vc.step}",
                                                    value: "{vc.value}",
                                                    "aria-label": "Volume",
                                                    oninput: move |e| {
                                                        if let Ok(v) = e.value().parse::<f32>() {
                                                            do_volume_set(zid_vol.clone(), v);
                                                        }
                                                    },
                                                }
                                                span { class: "text-xs text-muted tabular-nums w-12 text-right", "{label}" }
                                            }
                                        }
                                    } else {
                                        rsx! {}
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                            // ✨ Similar suggestions sub-row. Each row is a clickable
                            // ▶ Play button that submits a chat turn (so the AI handles
                            // the actual playback through its agent loop, the action
                            // shows in the tool log, and the same recommendation flow
                            // applies as for assistant-bubble suggestions).
                            {
                                let sims = similar_suggestions.read().clone();
                                if !sims.is_empty() {
                                    rsx! {
                                        div { class: "flex flex-col gap-1 px-1 pl-13 pt-1 border-t border-border/50",
                                            div { class: "flex items-center justify-between",
                                                span { class: "text-xs text-muted uppercase tracking-wider", "✨ Similar" }
                                                button {
                                                    class: "text-muted hover:text-foreground text-xs px-1",
                                                    title: "Dismiss",
                                                    onclick: move |_| similar_suggestions.set(Vec::new()),
                                                    "×"
                                                }
                                            }
                                            for sug in sims.iter() {
                                                {
                                                    let sug = sug.clone();
                                                    let play_msg = play_message_for(&sug);
                                                    let label = match &sug.artist {
                                                        Some(a) if !a.is_empty() => format!("{} — {}", sug.title, a),
                                                        _ => sug.title.clone(),
                                                    };
                                                    let is_loading = *loading.read();
                                                    rsx! {
                                                        div { class: "flex items-center gap-2 rounded-md bg-background/50 px-2 py-1 text-sm",
                                                            button {
                                                                class: "btn-primary px-2 py-0.5 text-xs disabled:opacity-50",
                                                                disabled: is_loading,
                                                                onclick: move |_| do_send_text(play_msg.clone(), messages, loading, selected_zone, speech, current_track, recent_tracks),
                                                                "▶ Play"
                                                            }
                                                            span { class: "truncate", "{label}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
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
                                let markdown = msg.markdown.clone();
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
                                            // Per-message replay button — only on finalised assistant
                                            // turns with non-empty markdown. Routes through the same
                                            // RoonSpeech session machinery as streaming TTS, so any
                                            // existing audio gets cancelled before this re-plays.
                                            if !is_streaming && !markdown.is_empty() {
                                                {
                                                    let md = markdown.clone();
                                                    let voice = voice_ctx.get();
                                                    rsx! {
                                                        div { class: "flex items-center gap-2 ml-2",
                                                            button {
                                                                class: "text-muted hover:text-foreground transition-colors text-xs px-2 py-0.5 rounded-md hover:bg-muted/50",
                                                                title: "Replay this reply aloud",
                                                                onclick: move |_| {
                                                                    let md = md.clone();
                                                                    let voice = voice.clone();
                                                                    spawn(async move {
                                                                        let md_json = serde_json::to_string(&md).unwrap_or_else(|_| "\"\"".into());
                                                                        let voice_json = if voice.is_empty() {
                                                                            "null".to_string()
                                                                        } else {
                                                                            serde_json::to_string(&voice).unwrap_or_else(|_| "null".into())
                                                                        };
                                                                        let script = format!(
                                                                            "if (window.RoonSpeech) await window.RoonSpeech.speak({}, {}); return true;",
                                                                            md_json, voice_json
                                                                        );
                                                                        let _ = dioxus::document::eval(&script).join::<bool>().await;
                                                                    });
                                                                },
                                                                "🔊 Replay"
                                                            }
                                                        }
                                                    }
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
                                                                        onclick: move |_| do_send_text(play_msg.clone(), messages, loading, selected_zone, speech, current_track, recent_tracks),
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

            // Album-art lightbox. Native <dialog> renders in the browser's
            // top layer; showModal/close are driven by the use_effect that
            // watches art_modal_url. Click anywhere (image, backdrop, ✕) or
            // press ESC to dismiss.
            // ESC closes the dialog natively but doesn't clear `art_modal_url`,
            // so a re-click of the same thumbnail might be a no-op (signal value
            // unchanged). Mitigated by also wiring a vanilla DOM 'close' event
            // listener via the use_effect above — see the showModal() script.
            dialog {
                id: "roon-art-modal",
                class: "p-0 bg-transparent border-none rounded-lg max-w-[95vw] max-h-[95vh] backdrop:bg-black/80 backdrop:backdrop-blur-sm",
                onclick: move |_| art_modal_url.set(None),
                if let Some(u) = (art_modal_url)() {
                    img {
                        src: "{u}",
                        alt: "Album art (full size)",
                        class: "block max-w-[95vw] max-h-[95vh] object-contain rounded-lg shadow-2xl cursor-zoom-out",
                    }
                }
            }
        }
    }
}
