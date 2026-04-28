//! HTTP API handlers

use crate::adapters::roon::RoonAdapter;
use crate::adapters::upnp::UPnPAdapter;
use crate::adapters::Startable;
use crate::aggregator::ZoneAggregator;
use crate::bus::SharedBus;
use crate::coordinator::AdapterCoordinator;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub roon: Arc<RoonAdapter>,
    pub upnp: Arc<UPnPAdapter>,
    pub bus: SharedBus,
    pub aggregator: Arc<ZoneAggregator>,
    pub coordinator: Arc<AdapterCoordinator>,
    pub startable_adapters: Arc<Vec<Arc<dyn Startable>>>,
    pub start_time: Instant,
    /// Cancellation token for graceful shutdown (terminates SSE streams)
    pub shutdown: CancellationToken,
    /// Count of active SSE connections (for shutdown diagnostics)
    pub sse_connections: Arc<AtomicUsize>,
    /// Anthropic API key for AI chat (None = feature disabled)
    pub anthropic_api_key: Option<String>,
    /// OpenAI API key for cloud TTS (None = TTS endpoint returns 503)
    pub openai_api_key: Option<String>,
}

impl AppState {
    pub fn new(
        roon: Arc<RoonAdapter>,
        upnp: Arc<UPnPAdapter>,
        bus: SharedBus,
        aggregator: Arc<ZoneAggregator>,
        coordinator: Arc<AdapterCoordinator>,
        startable_adapters: Vec<Arc<dyn Startable>>,
        start_time: Instant,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            roon,
            upnp,
            bus,
            aggregator,
            coordinator,
            startable_adapters: Arc::new(startable_adapters),
            start_time,
            shutdown,
            sse_connections: Arc::new(AtomicUsize::new(0)),
            anthropic_api_key: None,
            openai_api_key: None,
        }
    }

    pub fn with_anthropic_key(mut self, key: Option<String>) -> Self {
        self.anthropic_api_key = key;
        self
    }

    pub fn with_openai_key(mut self, key: Option<String>) -> Self {
        self.openai_api_key = key;
        self
    }

    /// Get the count of active SSE connections
    pub fn active_sse_connections(&self) -> usize {
        self.sse_connections.load(Ordering::Relaxed)
    }

}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Generic zones response wrapper - clients expect {zones: [...]}
#[derive(Serialize)]
pub struct ZonesWrapper<T: Serialize> {
    pub zones: Vec<T>,
}

/// General status response
#[derive(Serialize)]
pub struct StatusResponse {
    pub service: &'static str,
    pub version: &'static str,
    pub git_sha: &'static str,
    pub uptime_secs: u64,
    pub roon_connected: bool,
    pub upnp_devices: usize,
    pub bus_subscribers: usize,
}

/// GET /status - Service health check
pub async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    let roon_status = state.roon.get_status().await;
    let upnp_status = state.upnp.get_status().await;

    Json(StatusResponse {
        service: "roon-ai",
        version: env!("ROON_AI_VERSION"),
        git_sha: env!("ROON_AI_GIT_SHA"),
        uptime_secs: state.start_time.elapsed().as_secs(),
        roon_connected: roon_status.connected,
        upnp_devices: upnp_status.renderer_count,
        bus_subscribers: state.bus.subscriber_count(),
    })
}

// =============================================================================
// Roon handlers
// =============================================================================

/// GET /roon/status - Roon connection status
pub async fn roon_status_handler(
    State(state): State<AppState>,
) -> Json<crate::adapters::roon::RoonStatus> {
    Json(state.roon.get_status().await)
}

/// GET /roon/zones - List all Roon zones
pub async fn roon_zones_handler(
    State(state): State<AppState>,
) -> Json<ZonesWrapper<crate::adapters::roon::Zone>> {
    Json(ZonesWrapper {
        zones: state.roon.get_zones().await,
    })
}

/// GET /zones - Unified zone list across all adapters (used by the web UI)
pub async fn zones_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let zones = state.aggregator.get_zones().await;
    Json(serde_json::json!({ "zones": zones }))
}

/// GET /roon/zone/:zone_id - Get specific zone
pub async fn roon_zone_handler(
    State(state): State<AppState>,
    Path(zone_id): Path<String>,
) -> impl IntoResponse {
    match state.roon.get_zone(&zone_id).await {
        Some(zone) => (StatusCode::OK, Json(zone)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Zone not found: {}", zone_id),
            }),
        )
            .into_response(),
    }
}

/// Control request body
#[derive(Deserialize)]
pub struct ControlRequest {
    pub zone_id: String,
    pub action: String,
}

/// POST /roon/control - Control playback
pub async fn roon_control_handler(
    State(state): State<AppState>,
    Json(req): Json<ControlRequest>,
) -> impl IntoResponse {
    match state.roon.control(&req.zone_id, &req.action).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Volume request body (f32 for fractional step support)
#[derive(Deserialize)]
pub struct VolumeRequest {
    /// Zone ID (also accepts output_id for backwards compatibility)
    #[serde(alias = "output_id")]
    pub zone_id: String,
    pub value: f32,
    #[serde(default)]
    pub relative: bool,
}

/// POST /roon/volume - Change volume
pub async fn roon_volume_handler(
    State(state): State<AppState>,
    Json(req): Json<VolumeRequest>,
) -> impl IntoResponse {
    match state
        .roon
        .change_volume(&req.zone_id, req.value, req.relative)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Query params for image request
#[derive(Deserialize)]
pub struct ImageQuery {
    pub image_key: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

/// GET /roon/image - fetch album art
pub async fn roon_image_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ImageQuery>,
) -> impl IntoResponse {
    match state
        .roon
        .get_image(&params.image_key, params.width, params.height)
        .await
    {
        Ok(image_data) => {
            let headers = [(
                axum::http::header::CONTENT_TYPE,
                image_data
                    .content_type
                    .parse()
                    .unwrap_or(axum::http::HeaderValue::from_static("image/jpeg")),
            )];
            (StatusCode::OK, headers, image_data.data).into_response()
        }
        Err(e) => {
            tracing::warn!("Image fetch failed: {}", e);
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response()
        }
    }
}

// =============================================================================
// Roon Browse handlers
// =============================================================================

/// Query params for search request
#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub zone_id: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Search source: "library" (default), "tidal", or "qobuz"
    #[serde(default)]
    pub source: Option<String>,
}

/// Request body for play action
#[derive(Deserialize)]
pub struct PlayRequest {
    pub query: String,
    pub zone_id: String,
    /// Source: "library" (default), "tidal", or "qobuz"
    #[serde(default)]
    pub source: Option<String>,
    /// Action: "play" (default), "queue", or "radio"
    #[serde(default)]
    pub action: Option<String>,
}

/// Search result item (simplified from roon_api::browse::Item)
#[derive(Serialize)]
pub struct SearchResultItem {
    pub title: String,
    pub subtitle: Option<String>,
    pub item_key: Option<String>,
    pub hint: Option<String>,
}

impl From<roon_api::browse::Item> for SearchResultItem {
    fn from(item: roon_api::browse::Item) -> Self {
        use roon_api::browse::ItemHint;
        Self {
            title: item.title,
            subtitle: item.subtitle,
            item_key: item.item_key,
            hint: item.hint.map(|h| {
                match h {
                    ItemHint::None => "none",
                    ItemHint::Action => "action",
                    ItemHint::ActionList => "action_list",
                    ItemHint::List => "list",
                    ItemHint::Header => "header",
                }
                .to_string()
            }),
        }
    }
}

/// GET /roon/search - Search the Roon library, TIDAL, or Qobuz
pub async fn roon_search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    use crate::adapters::roon::SearchSource;

    if !state.roon.is_browse_connected().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Roon Browse not connected".to_string(),
            }),
        )
            .into_response();
    }

    let source = match params.source.as_deref() {
        Some("tidal") => SearchSource::Tidal,
        Some("qobuz") => SearchSource::Qobuz,
        _ => SearchSource::Library,
    };

    match state
        .roon
        .search(&params.q, params.zone_id.as_deref(), params.limit, source)
        .await
    {
        Ok(items) => {
            let results: Vec<SearchResultItem> = items.into_iter().map(|i| i.into()).collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "results": results })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// POST /roon/play - Search and play music
pub async fn roon_play_handler(
    State(state): State<AppState>,
    Json(req): Json<PlayRequest>,
) -> impl IntoResponse {
    use crate::adapters::roon::{PlayAction, SearchSource};

    if !state.roon.is_browse_connected().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Roon Browse not connected".to_string(),
            }),
        )
            .into_response();
    }

    let source = match req.source.as_deref() {
        Some("tidal") => SearchSource::Tidal,
        Some("qobuz") => SearchSource::Qobuz,
        _ => SearchSource::Library,
    };

    let action = PlayAction::parse(req.action.as_deref().unwrap_or("play"));

    match state
        .roon
        .search_and_play(&req.query, &req.zone_id, source, action)
        .await
    {
        Ok(message) => (
            StatusCode::OK,
            Json(serde_json::json!({ "message": message })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Play item request body
#[derive(Deserialize)]
pub struct PlayItemRequest {
    pub item_key: String,
    pub zone_id: String,
    #[serde(default)]
    pub action: Option<String>,
}

/// POST /roon/play_item - Play a specific item by its key
pub async fn roon_play_item_handler(
    State(state): State<AppState>,
    Json(req): Json<PlayItemRequest>,
) -> impl IntoResponse {
    use crate::adapters::roon::PlayAction;

    if !state.roon.is_browse_connected().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Roon Browse not connected".to_string(),
            }),
        )
            .into_response();
    }

    let action = PlayAction::parse(req.action.as_deref().unwrap_or("play"));

    match state
        .roon
        .play_item(&req.item_key, &req.zone_id, action)
        .await
    {
        Ok(message) => (
            StatusCode::OK,
            Json(serde_json::json!({ "message": message })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// Browse request body
#[derive(Deserialize)]
pub struct BrowseRequest {
    #[serde(default)]
    pub item_key: Option<String>,
    #[serde(default)]
    pub zone_id: Option<String>,
    #[serde(default)]
    pub pop_all: bool,
    /// Pop N levels back in the browse stack (for back navigation)
    #[serde(default)]
    pub pop_levels: Option<u32>,
    #[serde(default)]
    pub input: Option<String>,
    /// Session key for maintaining browse state across requests
    #[serde(default)]
    pub session_key: Option<String>,
}

/// Browse result converted to serializable format
#[derive(Serialize)]
pub struct BrowseResultResponse {
    pub action: String,
    pub list: Option<BrowseListInfo>,
    pub is_error: Option<bool>,
    pub message: Option<String>,
    /// Session key to use for subsequent browse calls
    pub session_key: String,
    /// Items at the current browse level
    pub items: Vec<BrowseItemResponse>,
}

#[derive(Serialize)]
pub struct BrowseListInfo {
    pub title: String,
    pub count: u32,
    pub level: u32,
    pub subtitle: Option<String>,
    pub image_key: Option<String>,
}

#[derive(Serialize)]
pub struct BrowseItemResponse {
    pub title: String,
    pub subtitle: Option<String>,
    pub item_key: Option<String>,
    pub hint: Option<String>,
    pub image_key: Option<String>,
}

/// POST /roon/browse - Browse the Roon library hierarchy
pub async fn roon_browse_handler(
    State(state): State<AppState>,
    Json(req): Json<BrowseRequest>,
) -> impl IntoResponse {
    use roon_api::browse::{BrowseOpts, ItemHint, LoadOpts};

    if !state.roon.is_browse_connected().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Roon Browse not connected".to_string(),
            }),
        )
            .into_response();
    }

    // Use provided session_key or generate a new one
    let session_key = req.session_key.unwrap_or_else(|| {
        format!(
            "browse_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });

    let opts = BrowseOpts {
        item_key: req.item_key,
        zone_or_output_id: req.zone_id,
        pop_all: req.pop_all,
        pop_levels: req.pop_levels,
        input: req.input,
        multi_session_key: Some(session_key.clone()),
        ..Default::default()
    };

    // Browse to the level
    let browse_result = match state.roon.browse(opts).await {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
                .into_response()
        }
    };

    // Load items at this level
    let items = if let Some(ref list) = browse_result.list {
        if list.count > 0 {
            let load_opts = LoadOpts {
                multi_session_key: Some(session_key.clone()),
                count: Some(50), // Load up to 50 items
                ..Default::default()
            };
            match state.roon.load(load_opts).await {
                Ok(load_result) => load_result
                    .items
                    .into_iter()
                    .map(|item| {
                        let hint_str = item.hint.map(|h| match h {
                            ItemHint::Action => "action",
                            ItemHint::ActionList => "action_list",
                            ItemHint::List => "list",
                            ItemHint::Header => "header",
                            ItemHint::None => "none",
                        });
                        BrowseItemResponse {
                            title: item.title,
                            subtitle: item.subtitle,
                            item_key: item.item_key,
                            hint: hint_str.map(|s| s.to_string()),
                            image_key: item.image_key,
                        }
                    })
                    .collect(),
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Browse load error: {}", e),
                        }),
                    )
                        .into_response();
                }
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    use roon_api::browse::Action;
    let action_str = match browse_result.action {
        Action::None => "none",
        Action::Message => "message",
        Action::List => "list",
        Action::ReplaceItem => "replace_item",
        Action::RemoveItem => "remove_item",
    };

    let response = BrowseResultResponse {
        action: action_str.to_string(),
        list: browse_result.list.map(|l| BrowseListInfo {
            title: l.title,
            count: l.count as u32,
            level: l.level,
            subtitle: l.subtitle,
            image_key: l.image_key,
        }),
        is_error: browse_result.is_error,
        message: browse_result.message,
        session_key,
        items,
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// GET /roon/browse/status - Check if browse service is connected
pub async fn roon_browse_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let connected = state.roon.is_browse_connected().await;
    Json(serde_json::json!({
        "connected": connected
    }))
}

/// Load request body for paginating an existing browse session
#[derive(Deserialize)]
pub struct BrowseLoadRequest {
    pub session_key: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub count: Option<usize>,
}

/// POST /roon/browse/load - Load more items from an existing browse session
pub async fn roon_browse_load_handler(
    State(state): State<AppState>,
    Json(req): Json<BrowseLoadRequest>,
) -> impl IntoResponse {
    use roon_api::browse::{ItemHint, LoadOpts};

    if !state.roon.is_browse_connected().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Roon Browse not connected".to_string(),
            }),
        )
            .into_response();
    }

    let load_opts = LoadOpts {
        multi_session_key: Some(req.session_key.clone()),
        offset: req.offset,
        count: req.count.or(Some(50)),
        ..Default::default()
    };

    match state.roon.load(load_opts).await {
        Ok(load_result) => {
            let items: Vec<BrowseItemResponse> = load_result
                .items
                .into_iter()
                .map(|item| {
                    let hint_str = item.hint.map(|h| match h {
                        ItemHint::Action => "action",
                        ItemHint::ActionList => "action_list",
                        ItemHint::List => "list",
                        ItemHint::Header => "header",
                        ItemHint::None => "none",
                    });
                    BrowseItemResponse {
                        title: item.title,
                        subtitle: item.subtitle,
                        item_key: item.item_key,
                        hint: hint_str.map(|s| s.to_string()),
                        image_key: item.image_key,
                    }
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({ "items": items }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// =============================================================================
// SSE Events
// =============================================================================

/// GET /events - Server-Sent Events stream
/// Guard that decrements SSE connection count on drop
struct SseConnectionGuard {
    counter: Arc<AtomicUsize>,
}

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        let prev = self.counter.fetch_sub(1, Ordering::Relaxed);
        tracing::debug!("SSE connection closed ({} remaining)", prev - 1);
    }
}

pub async fn events_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Track this connection
    let count = state.sse_connections.fetch_add(1, Ordering::Relaxed) + 1;
    tracing::debug!("SSE connection opened ({} active)", count);

    let guard = SseConnectionGuard {
        counter: state.sse_connections.clone(),
    };
    let shutdown = state.shutdown.clone();
    let rx = state.bus.subscribe();

    // Create stream that terminates on shutdown
    // Use futures::StreamExt::take_until via UFCS (tokio_stream doesn't have it)
    let base_stream = BroadcastStream::new(rx);
    let with_shutdown =
        futures::StreamExt::take_until(base_stream, async move { shutdown.cancelled().await });

    let stream = with_shutdown
        .filter_map(|result| match result {
            Ok(event) => {
                // Serialize event to JSON
                match serde_json::to_string(&event) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(_) => None,
                }
            }
            Err(_) => None, // Skip lagged messages
        })
        // Use map + flatten to attach guard lifetime to stream
        // When stream ends, guard is dropped (decrementing counter)
        .map(move |item| {
            let _ = &guard; // Keep guard alive while stream produces items
            item
        });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}

// =============================================================================
// UPnP handlers
// =============================================================================

/// GET /upnp/status - UPnP discovery status
pub async fn upnp_status_handler(
    State(state): State<AppState>,
) -> Json<crate::adapters::upnp::UPnPStatus> {
    Json(state.upnp.get_status().await)
}

/// GET /upnp/zones - List all discovered UPnP renderers
pub async fn upnp_zones_handler(
    State(state): State<AppState>,
) -> Json<ZonesWrapper<crate::adapters::upnp::UPnPZone>> {
    Json(ZonesWrapper {
        zones: state.upnp.get_zones().await,
    })
}

/// GET /upnp/zone/:zone_id/now_playing - Get now playing for renderer
pub async fn upnp_now_playing_handler(
    State(state): State<AppState>,
    Path(zone_id): Path<String>,
) -> impl IntoResponse {
    match state.upnp.get_now_playing(&zone_id).await {
        Some(np) => (StatusCode::OK, Json(np)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Renderer not found: {}", zone_id),
            }),
        )
            .into_response(),
    }
}

/// UPnP control request
#[derive(Deserialize)]
pub struct UPnPControlRequest {
    pub zone_id: String,
    pub action: String,
    #[serde(default)]
    pub value: Option<i32>,
}

/// POST /upnp/control - Control UPnP renderer
pub async fn upnp_control_handler(
    State(state): State<AppState>,
    Json(req): Json<UPnPControlRequest>,
) -> impl IntoResponse {
    match state
        .upnp
        .control(&req.zone_id, &req.action, req.value)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

// =============================================================================
// App settings handlers
// =============================================================================

/// App settings for UI preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub adapters: AdapterSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterSettings {
    #[serde(default = "default_true")]
    pub roon: bool,
    #[serde(default)]
    pub upnp: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            adapters: AdapterSettings {
                roon: true,
                upnp: false,
            },
        }
    }
}

const APP_SETTINGS_FILE: &str = "app-settings.json";

fn settings_path() -> std::path::PathBuf {
    crate::config::get_config_file_path(APP_SETTINGS_FILE)
}

/// Load app settings from disk.
/// Reads from the config subdirectory first, falls back to the root for legacy files.
pub fn load_app_settings() -> AppSettings {
    match crate::config::read_config_file(APP_SETTINGS_FILE) {
        Some(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse app settings: {}", e);
            AppSettings::default()
        }),
        None => AppSettings::default(),
    }
}

fn save_app_settings(settings: &AppSettings) -> bool {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(settings) {
        Ok(json) => match std::fs::write(&path, json) {
            Ok(()) => {
                tracing::info!("Saved app settings");
                true
            }
            Err(e) => {
                tracing::error!("Failed to save app settings: {}", e);
                false
            }
        },
        Err(e) => {
            tracing::error!("Failed to serialize app settings: {}", e);
            false
        }
    }
}

/// GET /api/settings - Get app settings
pub async fn api_settings_get_handler() -> impl IntoResponse {
    Json(load_app_settings())
}

/// POST /api/settings - Update app settings with dynamic adapter enable/disable
pub async fn api_settings_post_handler(
    State(state): State<AppState>,
    Json(new_settings): Json<AppSettings>,
) -> impl IntoResponse {
    // Load current settings to compare
    let old_settings = load_app_settings();

    // Save the new settings
    if !save_app_settings(&new_settings) {
        return Json(serde_json::json!({"ok": false, "error": "Failed to save settings"}));
    }

    // Compare adapter enabled states and start/stop as needed
    let old_adapters = &old_settings.adapters;
    let new_adapters = &new_settings.adapters;

    // Helper to process adapter state changes
    let adapters_list = state.startable_adapters.clone();
    let coord = state.coordinator.clone();

    // Check each adapter for state changes
    let adapter_changes: Vec<(&str, bool)> = vec![
        ("roon", old_adapters.roon != new_adapters.roon),
        ("upnp", old_adapters.upnp != new_adapters.upnp),
    ];

    for (name, changed) in adapter_changes {
        if !changed {
            continue;
        }

        // Get the new enabled state
        let now_enabled = match name {
            "roon" => new_adapters.roon,
            "upnp" => new_adapters.upnp,
            _ => continue,
        };

        // Update coordinator state
        coord.set_enabled(name, now_enabled).await;

        // Find the adapter and start/stop it
        if let Some(adapter) = adapters_list.iter().find(|a| a.name() == name) {
            if now_enabled {
                tracing::info!("Dynamically enabling adapter: {}", name);
                if adapter.can_start().await {
                    if let Err(e) = adapter.start().await {
                        tracing::warn!("Failed to start adapter {}: {}", name, e);
                    }
                }
            } else {
                tracing::info!("Dynamically disabling adapter: {}", name);
                adapter.stop().await;
            }
        }
    }

    Json(serde_json::json!({"ok": true}))
}

// ============================================================================
// AI chat handler
// ============================================================================

pub async fn ai_chat_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::ai::AiChatRequest>,
) -> impl IntoResponse {
    match crate::ai::run_agent(req, &state).await {
        Ok(resp) => (StatusCode::OK, Json(serde_json::to_value(resp).unwrap_or_default())),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// Streaming variant — returns Server-Sent Events as the agent runs.
/// Each event's `data` is a JSON-serialised `StreamEvent` (text / tool / done /
/// error). The client appends text deltas to the in-progress assistant bubble
/// and finalises with the `done` event's metadata (suggestions, markdown).
pub async fn ai_chat_stream_handler(
    State(state): State<AppState>,
    Json(req): Json<crate::ai::AiChatRequest>,
) -> impl IntoResponse {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<crate::ai::StreamEvent>();

    tokio::spawn(async move {
        crate::ai::run_agent_streaming(req, state, tx).await;
    });

    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx).map(|event| {
        let json = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, std::convert::Infallible>(Event::default().data(json))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ============================================================================
// TTS handler — proxies to OpenAI's /v1/audio/speech for cloud TTS voices
// ============================================================================

#[derive(Deserialize)]
pub struct TtsRequest {
    pub text: String,
    pub voice: String,
}

const OPENAI_TTS_VOICES: &[&str] = &[
    "alloy", "echo", "fable", "onyx", "nova", "shimmer",
];

// OpenAI's TTS endpoint accepts up to 4096 chars per request. Cap below that
// so we never get a 400 back when the user holds a long conversation and the
// reply is unusually long. Truncation is fine — the spoken reply just stops
// where the cap hits, and the rendered HTML in the chat is unaffected.
const OPENAI_TTS_MAX_CHARS: usize = 4000;

/// POST /api/tts — body `{ text, voice }`, returns audio/mpeg (MP3) bytes.
/// Uses model `gpt-4o-mini-tts` (cheap + fast, ~$0.60 / 1M chars).
/// Returns 503 if no OpenAI key is configured, 400 on bad input, 502 if
/// OpenAI rejects the request.
pub async fn tts_handler(
    State(state): State<AppState>,
    Json(req): Json<TtsRequest>,
) -> axum::response::Response {
    use axum::body::Body;
    use axum::response::Response;

    fn json_err(status: StatusCode, msg: &str) -> axum::response::Response {
        Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "error": msg }).to_string(),
            ))
            .unwrap_or_else(|_| Response::new(Body::empty()))
    }

    let api_key = match state.openai_api_key.as_ref() {
        Some(k) => k.clone(),
        None => {
            return json_err(
                StatusCode::SERVICE_UNAVAILABLE,
                "OpenAI API key not configured",
            );
        }
    };

    let mut text = req.text.trim().to_string();
    if text.is_empty() {
        return json_err(StatusCode::BAD_REQUEST, "text is required");
    }
    if text.chars().count() > OPENAI_TTS_MAX_CHARS {
        // char-boundary safe truncation
        let cutoff = text
            .char_indices()
            .nth(OPENAI_TTS_MAX_CHARS)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        text.truncate(cutoff);
    }

    if !OPENAI_TTS_VOICES.contains(&req.voice.as_str()) {
        return json_err(
            StatusCode::BAD_REQUEST,
            "voice must be one of: alloy, echo, fable, onyx, nova, shimmer",
        );
    }

    let body = serde_json::json!({
        "model": "gpt-4o-mini-tts",
        "input": text,
        "voice": req.voice,
        "response_format": "mp3",
    });

    let client = reqwest::Client::new();
    let resp = match client
        .post("https://api.openai.com/v1/audio/speech")
        .bearer_auth(&api_key)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("TTS upstream error: {}", e);
            return json_err(StatusCode::BAD_GATEWAY, "failed to reach OpenAI");
        }
    };

    let status = resp.status();
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("TTS body read error: {}", e);
            return json_err(StatusCode::BAD_GATEWAY, "failed to read OpenAI response");
        }
    };

    if !status.is_success() {
        // Forward the upstream status with the upstream body so the client can
        // surface the real reason (e.g. 401 invalid key, 429 rate limited).
        let upstream_status = StatusCode::from_u16(status.as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let body_text = String::from_utf8_lossy(&bytes).into_owned();
        tracing::warn!("TTS upstream {} body: {}", upstream_status, body_text);
        return Response::builder()
            .status(upstream_status)
            .header("content-type", "application/json")
            .body(Body::from(body_text))
            .unwrap_or_else(|_| Response::new(Body::empty()));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "audio/mpeg")
        .header("cache-control", "no-store")
        .body(Body::from(bytes.to_vec()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

