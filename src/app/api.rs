//! Client-side API functions for fetching data.
//!
//! These functions use Dioxus server functions to fetch data
//! without causing SSR deadlocks.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// =============================================================================
// Status Types
// =============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AppStatus {
    pub version: String,
    #[serde(default)]
    pub git_sha: String,
    pub uptime_secs: u64,
    pub bus_subscribers: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct RoonStatus {
    pub connected: bool,
    pub core_name: Option<String>,
    pub core_version: Option<String>,
}

// =============================================================================
// Settings Types
// =============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AdapterSettings {
    pub roon: bool,
    pub upnp: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub adapters: AdapterSettings,
}

// =============================================================================
// Zone Types
// =============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Zone {
    pub zone_id: String,
    pub zone_name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub dsp: Option<ZoneDsp>,
    /// State string from the server's bus zone — "playing", "paused", "stopped", etc.
    #[serde(default)]
    pub state: Option<String>,
    /// Currently playing track on this zone (subset of fields).
    #[serde(default)]
    pub now_playing: Option<ZoneNowPlaying>,
    /// Volume + scale info for the zone's primary output. Used by the
    /// now-playing banner's slider on the conversational page.
    #[serde(default)]
    pub volume_control: Option<ZoneVolumeControl>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ZoneDsp {
    pub r#type: Option<String>,
}

/// Subset of the bus's NowPlaying that the AI page consumes for context.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ZoneNowPlaying {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub image_key: Option<String>,
}

/// Subset of the bus's VolumeControl needed by the banner slider. The bus's
/// `scale` is a `VolumeScale` enum that serializes as a snake_case string
/// ("number", "decibels", "incremental"); we just stash it as a string here.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ZoneVolumeControl {
    #[serde(default)]
    pub value: f32,
    #[serde(default)]
    pub min: f32,
    #[serde(default)]
    pub max: f32,
    #[serde(default)]
    pub step: f32,
    #[serde(default)]
    pub is_muted: bool,
    #[serde(default)]
    pub scale: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ZonesResponse {
    pub zones: Vec<Zone>,
}

// =============================================================================
// Roon Browse Types
// =============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowseItem {
    pub title: String,
    pub subtitle: Option<String>,
    pub item_key: Option<String>,
    pub hint: Option<String>,
    pub image_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowseListInfo {
    pub title: String,
    pub count: u32,
    pub level: u32,
    pub subtitle: Option<String>,
    pub image_key: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowseResult {
    pub action: String,
    pub list: Option<BrowseListInfo>,
    pub session_key: String,
    pub items: Vec<BrowseItem>,
    pub is_error: Option<bool>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BrowseLoadResult {
    pub items: Vec<BrowseItem>,
}

// =============================================================================
// Client-side fetch helpers (for use in effects/resources)
// =============================================================================

/// Fetch JSON from a URL (client-side only)
#[cfg(target_arch = "wasm32")]
pub async fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> Result<T, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, Response};

    let window = web_sys::window().ok_or("No window")?;
    let opts = RequestInit::new();
    opts.set_method("GET");

    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{:?}", e))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: Response = resp_value.dyn_into().map_err(|_| "Not a Response")?;

    let json = JsFuture::from(resp.json().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{:?}", e))
}

/// SSR stub - returns error (should not be called during SSR)
#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_json<T: for<'de> Deserialize<'de>>(_url: &str) -> Result<T, String> {
    Err("fetch_json is only available in browser".to_string())
}

/// POST JSON to a URL (client-side only)
#[cfg(target_arch = "wasm32")]
pub async fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    url: &str,
    body: &T,
) -> Result<R, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Headers, Request, RequestInit, Response};

    let window = web_sys::window().ok_or("No window")?;

    let headers = Headers::new().map_err(|e| format!("{:?}", e))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{:?}", e))?;

    let body_str = serde_json::to_string(body).map_err(|e| e.to_string())?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_headers(&headers);
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{:?}", e))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: Response = resp_value.dyn_into().map_err(|_| "Not a Response")?;

    let json = JsFuture::from(resp.json().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("{:?}", e))
}

/// SSR stub - returns error (should not be called during SSR)
#[cfg(not(target_arch = "wasm32"))]
pub async fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    _url: &str,
    _body: &T,
) -> Result<R, String> {
    Err("post_json is only available in browser".to_string())
}

/// POST JSON without expecting response body
#[cfg(target_arch = "wasm32")]
pub async fn post_json_no_response<T: Serialize>(url: &str, body: &T) -> Result<(), String> {
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Headers, Request, RequestInit};

    let window = web_sys::window().ok_or("No window")?;

    let headers = Headers::new().map_err(|e| format!("{:?}", e))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{:?}", e))?;

    let body_str = serde_json::to_string(body).map_err(|e| e.to_string())?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_headers(&headers);
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_str));

    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{:?}", e))?;

    JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{:?}", e))?;

    Ok(())
}

/// SSR stub - returns error (should not be called during SSR)
#[cfg(not(target_arch = "wasm32"))]
pub async fn post_json_no_response<T: Serialize>(_url: &str, _body: &T) -> Result<(), String> {
    Err("post_json_no_response is only available in browser".to_string())
}

// =============================================================================
// AI Chat Types
// =============================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AiChatRequest {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<String>,
    /// Prior turns to send back to the server. Roles: "user" | "assistant".
    /// Used by the Conversational AI page; empty for the legacy /ai page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryTurn>,
    /// What's currently playing on the selected zone, if anything. Sent so
    /// Claude can resolve context-dependent commands like "skip this".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_track: Option<CurrentTrack>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CurrentTrack {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub album: Option<String>,
    #[serde(default)]
    pub is_playing: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct HistoryTurn {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AiChatResponse {
    #[serde(default)]
    pub response: String,
    /// Raw markdown of the assistant's reply (suggestions block stripped).
    /// Clients should store this and replay it as the assistant turn's `text`
    /// in the next request's `history`.
    #[serde(default)]
    pub response_markdown: String,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub suggestions: Vec<Suggestion>,
    /// Set when the server returns an error JSON
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
}

pub async fn ai_chat(req: AiChatRequest) -> Result<AiChatResponse, String> {
    post_json("/api/ai/chat", &req).await
}
