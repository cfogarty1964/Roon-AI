//! AI natural language music control via Anthropic Claude API.
//!
//! Accepts a free-text music request and an optional target zone, then runs an
//! agentic loop — calling Claude with four hi-fi tools until it produces a final
//! text reply.  No new crate dependencies: uses the `reqwest` client already
//! present in the server feature.

use crate::api::AppState;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ============================================================================
// Public request / response types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AiChatRequest {
    pub message: String,
    pub zone_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AiChatResponse {
    pub response: String,
    pub actions: Vec<String>,
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
            "description": "List all available playback zones (Roon, LMS, OpenHome, UPnP). Call this first to discover zone IDs when the user hasn't specified one.",
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

fn system_prompt(preferred_zone: Option<&str>) -> String {
    let zone_hint = preferred_zone
        .map(|z| format!("\n\nThe user has pre-selected zone: `{}`. Use this zone unless they say otherwise.", z))
        .unwrap_or_default();

    format!(
        "You are an AI assistant controlling a hi-fi audio system. \
You have access to tools that let you discover playback zones and control music playback. \
\n\nWhen a user asks to play music, always confirm which zone you used and what you queued or started. \
When a user asks for music 'similar to' or 'like' a specific piece, use action='radio' on a Roon zone \
to seed Roon Radio from that track — Roon will automatically find similar music. \
If the user doesn't specify a zone, call list_zones first and pick the most appropriate one, or ask. \
Keep replies concise and friendly — one or two sentences confirming what you did.{}",
        zone_hint
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

    async fn call(&self, messages: Vec<Message>, preferred_zone: Option<&str>) -> Result<AnthropicResponse> {
        let body = AnthropicRequest {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 1024,
            system: system_prompt(preferred_zone),
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

            if zone_id.is_some_and(|z| z.starts_with("lms:")) {
                match state.lms.search(query, zone_id, Some(8)).await {
                    Ok(results) => format_search_results_lms(results),
                    Err(e) => format!("Search error: {}", e),
                }
            } else {
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
        }

        "play_music" => {
            let query = input["query"].as_str().unwrap_or("");
            let zone_id = input["zone_id"].as_str().unwrap_or("");
            let source_str = input["source"].as_str().unwrap_or("library");
            let action_str = input["action"].as_str().unwrap_or("play");

            if zone_id.starts_with("lms:") {
                use crate::adapters::lms::LmsPlayAction;
                if action_str == "radio" {
                    return "Radio mode is only supported on Roon zones.".to_string();
                }
                let action = LmsPlayAction::parse(Some(action_str));
                match state.lms.search_and_play(query, zone_id, action).await {
                    Ok(msg) => msg,
                    Err(e) => format!("Play error: {}", e),
                }
            } else {
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
        }

        "control_playback" => {
            let zone_id = input["zone_id"].as_str().unwrap_or("");
            let action = input["action"].as_str().unwrap_or("");
            let value = input["value"].as_f64();

            match action {
                "volume_set" => {
                    let v = value.unwrap_or(50.0);
                    let result = if zone_id.starts_with("lms:") {
                        state.lms.change_volume(zone_id, v as f32, false).await
                    } else {
                        state.roon.change_volume(zone_id, v as f32, false).await
                    };
                    match result {
                        Ok(()) => format!("Volume set to {}", v),
                        Err(e) => format!("Volume error: {}", e),
                    }
                }
                "volume_up" => {
                    let delta = value.unwrap_or(5.0);
                    let result = if zone_id.starts_with("lms:") {
                        state.lms.change_volume(zone_id, delta as f32, true).await
                    } else {
                        state.roon.change_volume(zone_id, delta as f32, true).await
                    };
                    match result {
                        Ok(()) => "Volume increased".to_string(),
                        Err(e) => format!("Volume error: {}", e),
                    }
                }
                "volume_down" => {
                    let delta = -(value.unwrap_or(5.0));
                    let result = if zone_id.starts_with("lms:") {
                        state.lms.change_volume(zone_id, delta as f32, true).await
                    } else {
                        state.roon.change_volume(zone_id, delta as f32, true).await
                    };
                    match result {
                        Ok(()) => "Volume decreased".to_string(),
                        Err(e) => format!("Volume error: {}", e),
                    }
                }
                _ => {
                    // Map to backend action string
                    let backend = match action {
                        "playpause" => "play_pause",
                        other => other,
                    };
                    let result = if zone_id.starts_with("lms:") {
                        state.lms.control(zone_id, backend, None).await
                    } else if zone_id.starts_with("openhome:") {
                        state.openhome.control(zone_id, backend, None).await
                    } else if zone_id.starts_with("upnp:") {
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

fn format_search_results_lms(results: Vec<crate::adapters::lms::LmsSearchResult>) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    results
        .iter()
        .map(|r| {
            let detail = match (&r.artist, &r.album) {
                (Some(a), Some(al)) => format!(" — {} / {}", a, al),
                (Some(a), None) => format!(" — {}", a),
                _ => String::new(),
            };
            format!("{}{}", r.title, detail)
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
    let preferred_zone = request.zone_id.as_deref();

    let mut messages: Vec<Message> = vec![Message {
        role: "user".to_string(),
        content: MessageContent::Text(request.message.clone()),
    }];

    let mut actions: Vec<String> = Vec::new();
    let mut final_text = String::new();

    // Cap at 10 iterations to prevent runaway tool chains
    for _ in 0..10 {
        let resp = client.call(messages.clone(), preferred_zone).await?;

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

    Ok(AiChatResponse {
        response: final_text,
        actions,
    })
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
