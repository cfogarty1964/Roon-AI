//! AI natural language music control via Anthropic Claude API.
//!
//! Accepts a free-text music request and an optional target zone, then runs an
//! agentic loop — calling Claude with four hi-fi tools until it produces a final
//! text reply.  No new crate dependencies: uses the `reqwest` client already
//! present in the server feature.

use crate::api::AppState;
use anyhow::{anyhow, Context, Result};
use pulldown_cmark::{html as cmark_html, Options, Parser as CmarkParser};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Public request / response types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AiChatRequest {
    pub message: String,
    pub zone_id: Option<String>,
    /// Prior turns of the conversation. Empty = first turn (legacy behaviour).
    /// Roles are "user" or "assistant"; text is plain markdown (no HTML, no
    /// suggestions sentinel block).
    #[serde(default)]
    pub history: Vec<HistoryTurn>,
    /// Current track on the selected zone (sent by the client). Lets Claude
    /// resolve "skip this", "more like this", "pause", etc. without the user
    /// having to type the title.
    #[serde(default)]
    pub current_track: Option<CurrentTrack>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryTurn {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CurrentTrack {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub is_playing: bool,
}

#[derive(Debug, Serialize)]
pub struct AiChatResponse {
    /// Rendered HTML for display in the assistant bubble.
    pub response: String,
    /// Raw markdown (suggestions block stripped) — clients should store this
    /// and send it back in the `history` field for the next turn.
    pub response_markdown: String,
    pub actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
}

// ============================================================================
// Anthropic API wire types (minimal — only what we need)
// ============================================================================

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    tools: Vec<Value>,
    messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    role: String,
    content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    stop_reason: String,
    content: Vec<ContentBlock>,
}

// ============================================================================
// Tool definitions (Claude API JSON schema format)
// ============================================================================

fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "list_zones",
            "description": "List all available playback zones (Roon, UPnP). Call this first to discover zone IDs when the user hasn't specified one.",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "search_music",
            "description": "Search for tracks, albums, or artists. Use source='tidal' or source='qobuz' for streaming services (Roon only). Returns matching titles and subtitles.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query, e.g. 'Mahler Symphony 5 Adagietto', 'Miles Davis Kind of Blue'"
                    },
                    "zone_id": {
                        "type": "string",
                        "description": "Zone ID for context — recommended when zone is known"
                    },
                    "source": {
                        "type": "string",
                        "enum": ["library", "tidal", "qobuz"],
                        "description": "Where to search. Defaults to library. Roon only."
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "play_music",
            "description": "Search for music and immediately play, queue, or start radio on a zone. For 'play similar to X' requests, use action='radio' on a Roon zone — Roon Radio will find similar tracks automatically.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to play, e.g. 'Adagietto Mahler 5', 'late night jazz piano'"
                    },
                    "zone_id": {
                        "type": "string",
                        "description": "Zone ID to play on (get from list_zones)"
                    },
                    "source": {
                        "type": "string",
                        "enum": ["library", "tidal", "qobuz"],
                        "description": "Where to search. Defaults to library."
                    },
                    "action": {
                        "type": "string",
                        "enum": ["play", "queue", "radio"],
                        "description": "play = replace queue and start; queue = add to end; radio = start Roon Radio seeded from this track (Roon only). Use 'radio' when user wants 'similar to' or 'more like'."
                    }
                },
                "required": ["query", "zone_id"]
            }
        }),
        json!({
            "name": "control_playback",
            "description": "Control playback on a zone: transport commands and volume.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "zone_id": {
                        "type": "string",
                        "description": "Zone ID to control"
                    },
                    "action": {
                        "type": "string",
                        "enum": ["play", "pause", "playpause", "next", "previous", "volume_set", "volume_up", "volume_down"],
                        "description": "Action to perform"
                    },
                    "value": {
                        "type": "number",
                        "description": "For volume_set: 0-100. For volume_up/down: step amount (default 5)."
                    }
                },
                "required": ["zone_id", "action"]
            }
        }),
    ]
}

// ============================================================================
// System prompt
// ============================================================================

fn system_prompt(preferred_zone: Option<&str>, current_track: Option<&CurrentTrack>) -> String {
    let zone_hint = preferred_zone
        .map(|z| format!("\n\nThe user has pre-selected zone: `{}`. Use this zone unless they say otherwise.", z))
        .unwrap_or_default();

    let track_hint = current_track
        .and_then(|t| {
            let title = t.title.as_deref().unwrap_or("").trim();
            if title.is_empty() {
                return None;
            }
            let mut s = format!("\n\nThe selected zone is currently {} '{}'", if t.is_playing { "playing" } else { "stopped on" }, title);
            if let Some(a) = t.artist.as_deref().filter(|a| !a.is_empty()) {
                s.push_str(&format!(" by {}", a));
            }
            if let Some(al) = t.album.as_deref().filter(|a| !a.is_empty()) {
                s.push_str(&format!(" (from '{}')", al));
            }
            s.push_str(". When the user says 'skip this', 'pause', 'more like this', 'what is this', etc. — they are referring to this track. Use it as implicit context.");
            Some(s)
        })
        .unwrap_or_default();

    format!(
        "You are an AI assistant controlling a hi-fi audio system. \
You have access to tools that let you discover playback zones and control music playback. \
\n\nWhen a user asks to play music, always confirm which zone you used and what you queued or started. \
When a user asks for music 'similar to' or 'like' a specific piece, use action='radio' on a Roon zone \
to seed Roon Radio from that track — Roon will automatically find similar music. \
If the user doesn't specify a zone, call list_zones first and pick the most appropriate one, or ask. \
Keep replies concise and friendly — one or two sentences confirming what you did. \
\n\nWhenever your reply lists or recommends specific tracks, pieces, or albums the user could play \
(whether or not you are playing one right now), end your reply with a machine-readable block in this exact format, \
on its own lines, after a blank line:\n\n\
<<<SUGGESTIONS>>>\n\
[{{\"title\": \"Piece or track title\", \"artist\": \"Composer or performer\", \"album\": \"Album (optional)\"}}]\n\
<<<END_SUGGESTIONS>>>\n\n\
Rules for the block: valid JSON array only; include between 1 and 10 items; omit the block entirely if you are not recommending specific pieces. \
Do not mention the block in the prose. Use it only for recommendations the user could act on — not for confirming a play you just executed.{}{}",
        zone_hint,
        track_hint
    )
}

// ============================================================================
// Anthropic HTTP client
// ============================================================================

struct AnthropicClient {
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicClient {
    fn new(api_key: String) -> Self {
        Self {
            api_key,
            http: reqwest::Client::new(),
        }
    }

    async fn call(&self, messages: Vec<Message>, system: String) -> Result<AnthropicResponse> {
        let body = AnthropicRequest {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 1024,
            system,
            tools: tools(),
            messages,
        };

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to reach Anthropic API")?;

        let status = resp.status();
        let text = resp.text().await.context("Failed to read Anthropic response body")?;

        if !status.is_success() {
            return Err(anyhow!("Anthropic API error {}: {}", status, text));
        }

        serde_json::from_str::<AnthropicResponse>(&text)
            .with_context(|| format!("Failed to parse Anthropic response: {}", text))
    }

    /// Streaming variant. Calls `on_text_delta(text)` for every text fragment as
    /// it arrives. Returns the fully assembled `AnthropicResponse` once the
    /// stream completes (so the caller can inspect tool_use blocks and stop
    /// reason exactly like in the non-streaming path).
    async fn call_streaming(
        &self,
        messages: Vec<Message>,
        system: String,
        on_text_delta: &mut (dyn FnMut(&str) + Send),
    ) -> Result<AnthropicResponse> {
        let body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 1024,
            "system": system,
            "tools": tools(),
            "messages": messages,
            "stream": true,
        });

        let mut resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to reach Anthropic API (stream)")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API error {}: {}", status, text));
        }

        // Accumulators for the assembled response.
        // `block_states` is indexed by the content_block index from Anthropic.
        let mut blocks: std::collections::BTreeMap<usize, BlockBuilder> =
            std::collections::BTreeMap::new();
        let mut stop_reason = String::new();

        // SSE parser: events are separated by "\n\n", lines within an event are
        // either `event: <name>` or `data: <json>`.
        let mut buffer = String::new();
        while let Some(chunk) = resp.chunk().await.context("Failed to read SSE chunk")? {
            let text = std::str::from_utf8(&chunk).unwrap_or("");
            buffer.push_str(text);
            while let Some(sep) = buffer.find("\n\n") {
                let raw = buffer[..sep].to_string();
                buffer.drain(..sep + 2);
                process_sse_event(&raw, &mut blocks, &mut stop_reason, on_text_delta);
            }
        }

        // Assemble final ContentBlock vec in index order.
        let content: Vec<ContentBlock> = blocks
            .into_values()
            .filter_map(|b| b.finalize())
            .collect();

        Ok(AnthropicResponse {
            stop_reason: if stop_reason.is_empty() {
                "end_turn".into()
            } else {
                stop_reason
            },
            content,
        })
    }
}

/// In-progress content block being assembled from streaming deltas.
enum BlockBuilder {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input_json: String,
    },
}

impl BlockBuilder {
    fn finalize(self) -> Option<ContentBlock> {
        match self {
            BlockBuilder::Text(text) => Some(ContentBlock::Text { text }),
            BlockBuilder::ToolUse { id, name, input_json } => {
                let input: Value = serde_json::from_str(&input_json).unwrap_or(Value::Null);
                Some(ContentBlock::ToolUse { id, name, input })
            }
        }
    }
}

fn process_sse_event(
    raw: &str,
    blocks: &mut std::collections::BTreeMap<usize, BlockBuilder>,
    stop_reason: &mut String,
    on_text_delta: &mut (dyn FnMut(&str) + Send),
) {
    // Find the data: line. Anthropic always sends data:; event: is informational.
    let data_line = raw.lines().find_map(|l| l.strip_prefix("data:").map(str::trim));
    let Some(data) = data_line else { return };
    if data.is_empty() {
        return;
    }
    let Ok(v): std::result::Result<Value, _> = serde_json::from_str(data) else { return };
    let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match kind {
        "content_block_start" => {
            let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let block = v.get("content_block");
            let block_type = block
                .and_then(|b| b.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match block_type {
                "text" => {
                    blocks.insert(idx, BlockBuilder::Text(String::new()));
                }
                "tool_use" => {
                    let id = block
                        .and_then(|b| b.get("id"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .and_then(|b| b.get("name"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    blocks.insert(
                        idx,
                        BlockBuilder::ToolUse {
                            id,
                            name,
                            input_json: String::new(),
                        },
                    );
                }
                _ => {}
            }
        }
        "content_block_delta" => {
            let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let delta = v.get("delta");
            let delta_type = delta
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match delta_type {
                "text_delta" => {
                    let text = delta
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    if let Some(BlockBuilder::Text(buf)) = blocks.get_mut(&idx) {
                        buf.push_str(text);
                    }
                    on_text_delta(text);
                }
                "input_json_delta" => {
                    let partial = delta
                        .and_then(|d| d.get("partial_json"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    if let Some(BlockBuilder::ToolUse { input_json, .. }) = blocks.get_mut(&idx) {
                        input_json.push_str(partial);
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(reason) = v
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|s| s.as_str())
            {
                *stop_reason = reason.to_string();
            }
        }
        // message_start, content_block_stop, message_stop, ping → ignore
        _ => {}
    }
}

// ============================================================================
// Tool execution — routes each tool call to the appropriate adapter method
// ============================================================================

async fn execute_tool(name: &str, input: &Value, state: &AppState) -> String {
    match name {
        "list_zones" => {
            let zones = state.aggregator.get_zones().await;
            if zones.is_empty() {
                return "No zones are currently available.".to_string();
            }
            let list: Vec<String> = zones
                .iter()
                .map(|z| format!("{} (id: {}, state: {})", z.zone_name, z.zone_id, z.state))
                .collect();
            list.join("\n")
        }

        "search_music" => {
            let query = input["query"].as_str().unwrap_or("");
            let zone_id = input["zone_id"].as_str();
            let source_str = input["source"].as_str().unwrap_or("library");
            use crate::adapters::roon::SearchSource;
            let source = match source_str {
                "tidal" => SearchSource::Tidal,
                "qobuz" => SearchSource::Qobuz,
                _ => SearchSource::Library,
            };
            match state.roon.search(query, zone_id, Some(8), source).await {
                Ok(results) => format_search_results_roon(results),
                Err(e) => format!("Search error: {}", e),
            }
        }

        "play_music" => {
            let query = input["query"].as_str().unwrap_or("");
            let zone_id = input["zone_id"].as_str().unwrap_or("");
            let source_str = input["source"].as_str().unwrap_or("library");
            let action_str = input["action"].as_str().unwrap_or("play");
            use crate::adapters::roon::{PlayAction, SearchSource};
            let source = match source_str {
                "tidal" => SearchSource::Tidal,
                "qobuz" => SearchSource::Qobuz,
                _ => SearchSource::Library,
            };
            let action = PlayAction::parse(action_str);
            match state.roon.search_and_play(query, zone_id, source, action).await {
                Ok(msg) => msg,
                Err(e) => format!("Play error: {}", e),
            }
        }

        "control_playback" => {
            let zone_id = input["zone_id"].as_str().unwrap_or("");
            let action = input["action"].as_str().unwrap_or("");
            let value = input["value"].as_f64();

            match action {
                "volume_set" => {
                    let v = value.unwrap_or(50.0);
                    match state.roon.change_volume(zone_id, v as f32, false).await {
                        Ok(()) => format!("Volume set to {}", v),
                        Err(e) => format!("Volume error: {}", e),
                    }
                }
                "volume_up" => {
                    let delta = value.unwrap_or(5.0);
                    match state.roon.change_volume(zone_id, delta as f32, true).await {
                        Ok(()) => "Volume increased".to_string(),
                        Err(e) => format!("Volume error: {}", e),
                    }
                }
                "volume_down" => {
                    let delta = -(value.unwrap_or(5.0));
                    match state.roon.change_volume(zone_id, delta as f32, true).await {
                        Ok(()) => "Volume decreased".to_string(),
                        Err(e) => format!("Volume error: {}", e),
                    }
                }
                _ => {
                    let backend = match action {
                        "playpause" => "play_pause",
                        other => other,
                    };
                    let result = if zone_id.starts_with("upnp:") {
                        state.upnp.control(zone_id, backend, None).await
                    } else {
                        state.roon.control(zone_id, backend).await
                    };
                    match result {
                        Ok(()) => format!("Action '{}' executed on {}", action, zone_id),
                        Err(e) => format!("Control error: {}", e),
                    }
                }
            }
        }

        unknown => format!("Unknown tool: {}", unknown),
    }
}

fn format_search_results_roon(results: Vec<roon_api::browse::Item>) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    results
        .iter()
        .map(|r| match &r.subtitle {
            Some(sub) => format!("{} — {}", r.title, sub),
            None => r.title.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// Agentic loop — the public entry point
// ============================================================================

pub async fn run_agent(request: AiChatRequest, state: &AppState) -> Result<AiChatResponse> {
    let api_key = state
        .anthropic_api_key
        .clone()
        .ok_or_else(|| anyhow!("ANTHROPIC_API_KEY is not set. Set the env var or add api_key to [ai] in unified-hifi-control.toml."))?;

    let client = AnthropicClient::new(api_key);
    let system = system_prompt(request.zone_id.as_deref(), request.current_track.as_ref());

    let mut messages: Vec<Message> = Vec::with_capacity(request.history.len() + 1);
    for turn in &request.history {
        let role = match turn.role.as_str() {
            "user" | "assistant" => turn.role.clone(),
            _ => continue, // skip "error" or other non-Claude roles
        };
        messages.push(Message {
            role,
            content: MessageContent::Text(turn.text.clone()),
        });
    }
    messages.push(Message {
        role: "user".to_string(),
        content: MessageContent::Text(request.message.clone()),
    });

    let mut actions: Vec<String> = Vec::new();
    let mut final_text = String::new();

    // Cap at 10 iterations to prevent runaway tool chains
    for _ in 0..10 {
        let resp = client.call(messages.clone(), system.clone()).await?;

        if resp.stop_reason == "end_turn" {
            // Extract the text reply
            for block in &resp.content {
                if let ContentBlock::Text { text } = block {
                    final_text = text.clone();
                    break;
                }
            }
            break;
        }

        if resp.stop_reason == "tool_use" {
            // Append assistant message with the tool_use blocks
            messages.push(Message {
                role: "assistant".to_string(),
                content: MessageContent::Blocks(resp.content.clone()),
            });

            // Execute each tool and collect results
            let mut result_blocks: Vec<ContentBlock> = Vec::new();
            for block in &resp.content {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    tracing::debug!("AI tool call: {} {:?}", name, input);
                    let result = execute_tool(name, input, state).await;
                    actions.push(format!("{}({})", name, summarise_input(input)));
                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: result,
                    });
                }
            }

            // Append tool results as a user turn
            messages.push(Message {
                role: "user".to_string(),
                content: MessageContent::Blocks(result_blocks),
            });
        } else {
            // Unexpected stop reason — grab any text we have and bail
            for block in &resp.content {
                if let ContentBlock::Text { text } = block {
                    final_text = text.clone();
                    break;
                }
            }
            break;
        }
    }

    if final_text.is_empty() {
        final_text = "Done.".to_string();
    }

    let (clean_text, suggestions) = extract_suggestions(&final_text);

    Ok(AiChatResponse {
        response: markdown_to_html(&clean_text),
        response_markdown: clean_text,
        actions,
        suggestions,
    })
}

// ============================================================================
// Streaming agent — emits live events while the agent runs
// ============================================================================

/// Events emitted by the streaming agent. Serialised as JSON over SSE.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Append `text` to the in-progress assistant bubble.
    Text { text: String },
    /// A tool was just invoked. `summary` is `name(arg=value, ...)`.
    Tool { summary: String },
    /// Stream completed. Includes both raw markdown (for replay context in
    /// the next request's history) and the rendered HTML (for the bubble).
    Done {
        response: String,
        response_markdown: String,
        suggestions: Vec<Suggestion>,
    },
    /// Fatal error. The client should display this and stop the stream.
    Error { message: String },
}

/// Streaming agentic loop. Consumes the same `AiChatRequest` as `run_agent`
/// but pushes incremental updates into `tx`. The caller wraps the receiver
/// in an SSE response.
///
/// Text deltas are streamed live as they arrive from Claude. Tool calls are
/// announced via `Tool` events as soon as they fire. The final `Done` event
/// includes the suggestions sentinel parse and the cleaned markdown for the
/// client to store as the assistant turn's text.
pub async fn run_agent_streaming(
    request: AiChatRequest,
    state: AppState,
    tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
) {
    if let Err(e) = run_agent_streaming_inner(request, state, &tx).await {
        let _ = tx.send(StreamEvent::Error {
            message: e.to_string(),
        });
    }
}

async fn run_agent_streaming_inner(
    request: AiChatRequest,
    state: AppState,
    tx: &tokio::sync::mpsc::UnboundedSender<StreamEvent>,
) -> Result<()> {
    let api_key = state
        .anthropic_api_key
        .clone()
        .ok_or_else(|| {
            anyhow!("ANTHROPIC_API_KEY is not set. Set the env var or add api_key to [ai] in unified-hifi-control.toml.")
        })?;

    let client = AnthropicClient::new(api_key);
    let system = system_prompt(request.zone_id.as_deref(), request.current_track.as_ref());

    let mut messages: Vec<Message> = Vec::with_capacity(request.history.len() + 1);
    for turn in &request.history {
        let role = match turn.role.as_str() {
            "user" | "assistant" => turn.role.clone(),
            _ => continue,
        };
        messages.push(Message {
            role,
            content: MessageContent::Text(turn.text.clone()),
        });
    }
    messages.push(Message {
        role: "user".to_string(),
        content: MessageContent::Text(request.message.clone()),
    });

    let mut full_text = String::new();
    // Once Claude writes the suggestions sentinel, we stop forwarding text to
    // the client (the JSON inside the block isn't user-visible content).
    let mut suggestions_emitted = false;

    for _ in 0..10 {
        // Stream this iteration. Text deltas go to the client AND get appended
        // to `full_text` so we can run extract_suggestions at the end.
        let mut delta_cb = |delta: &str| {
            let prev_len = full_text.len();
            full_text.push_str(delta);

            if suggestions_emitted {
                return;
            }

            if let Some(marker_pos) = full_text.find("<<<SUGGESTIONS>>>") {
                suggestions_emitted = true;
                if marker_pos > prev_len {
                    let bytes_in_delta = marker_pos - prev_len;
                    // Snap to a UTF-8 char boundary to avoid splitting a codepoint.
                    let safe_end = (0..=bytes_in_delta.min(delta.len()))
                        .rev()
                        .find(|&i| delta.is_char_boundary(i))
                        .unwrap_or(0);
                    if safe_end > 0 {
                        let _ = tx.send(StreamEvent::Text {
                            text: delta[..safe_end].to_string(),
                        });
                    }
                }
                return;
            }

            let _ = tx.send(StreamEvent::Text {
                text: delta.to_string(),
            });
        };

        let resp = client
            .call_streaming(messages.clone(), system.clone(), &mut delta_cb)
            .await?;

        if resp.stop_reason == "end_turn" {
            break;
        }

        if resp.stop_reason == "tool_use" {
            // Append assistant message with the tool_use blocks
            messages.push(Message {
                role: "assistant".to_string(),
                content: MessageContent::Blocks(resp.content.clone()),
            });

            let mut result_blocks: Vec<ContentBlock> = Vec::new();
            for block in &resp.content {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    tracing::debug!("AI tool call (stream): {} {:?}", name, input);
                    let result = execute_tool(name, input, &state).await;
                    let _ = tx.send(StreamEvent::Tool {
                        summary: format!("{}({})", name, summarise_input(input)),
                    });
                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: id.clone(),
                        content: result,
                    });
                }
            }

            messages.push(Message {
                role: "user".to_string(),
                content: MessageContent::Blocks(result_blocks),
            });
        } else {
            break;
        }
    }

    if full_text.is_empty() {
        full_text = "Done.".to_string();
    }

    let (clean_text, suggestions) = extract_suggestions(&full_text);

    let _ = tx.send(StreamEvent::Done {
        response: markdown_to_html(&clean_text),
        response_markdown: clean_text,
        suggestions,
    });

    Ok(())
}

/// Extract a `<<<SUGGESTIONS>>> ... <<<END_SUGGESTIONS>>>` block from Claude's
/// reply, parse the JSON array inside, and return the text with that block
/// removed. Silently drops the block on any parse failure so the user still
/// sees the prose.
fn extract_suggestions(text: &str) -> (String, Vec<Suggestion>) {
    const START: &str = "<<<SUGGESTIONS>>>";
    const END: &str = "<<<END_SUGGESTIONS>>>";

    let Some(start_idx) = text.find(START) else {
        return (text.to_string(), Vec::new());
    };
    let after_start = start_idx + START.len();
    let Some(end_rel) = text[after_start..].find(END) else {
        return (text.to_string(), Vec::new());
    };
    let end_idx = after_start + end_rel;
    let after_end = end_idx + END.len();

    let json_slice = text[after_start..end_idx].trim();
    let suggestions: Vec<Suggestion> = serde_json::from_str(json_slice).unwrap_or_default();

    let mut cleaned = String::with_capacity(text.len());
    cleaned.push_str(&text[..start_idx]);
    cleaned.push_str(&text[after_end..]);

    (cleaned.trim().to_string(), suggestions)
}

fn markdown_to_html(text: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = CmarkParser::new_ext(text, opts);
    let mut out = String::new();
    cmark_html::push_html(&mut out, parser);
    out
}

/// Produce a short human-readable summary of tool input for the actions log.
fn summarise_input(input: &Value) -> String {
    if let Some(obj) = input.as_object() {
        let parts: Vec<String> = obj
            .iter()
            .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or(&v.to_string())))
            .collect();
        parts.join(", ")
    } else {
        input.to_string()
    }
}
