//! Zones listing page component.
//!
//! Shows all available zones using Dioxus resources.

use crate::app::api::{NowPlaying, Zone, ZonesResponse};
use crate::app::components::{Layout, VolumeControlsCompact};
use crate::app::sse::{use_sse, SseEvent};
use dioxus::prelude::*;
use std::collections::HashMap;

/// Control request body
#[derive(Clone, serde::Serialize)]
struct ControlRequest {
    zone_id: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f64>,
}

/// Fetch now playing for all zones
async fn fetch_all_now_playing(zones: &[Zone]) -> HashMap<String, NowPlaying> {
    let mut np_map = HashMap::new();
    for zone in zones {
        let url = format!(
            "/now_playing?zone_id={}",
            urlencoding::encode(&zone.zone_id)
        );
        if let Ok(np) = crate::app::api::fetch_json::<NowPlaying>(&url).await {
            np_map.insert(zone.zone_id.clone(), np);
        }
    }
    np_map
}

/// Fetch now playing for a single zone by ID
async fn fetch_zone_now_playing(zone_id: &str) -> Option<NowPlaying> {
    let url = format!("/now_playing?zone_id={}", urlencoding::encode(zone_id));
    crate::app::api::fetch_json::<NowPlaying>(&url).await.ok()
}

/// Zones listing page component.
#[component]
pub fn Zones() -> Element {
    let sse = use_sse();

    // Load zones resource
    let mut zones = use_resource(|| async {
        crate::app::api::fetch_json::<ZonesResponse>("/zones")
            .await
            .ok()
    });

    // Now playing state (populated after zones load and refreshed on SSE events)
    let mut now_playing = use_signal(HashMap::<String, NowPlaying>::new);

    // Track zones list for now_playing refresh
    let zones_list_signal = use_memo(move || {
        zones
            .read()
            .clone()
            .flatten()
            .map(|r| r.zones)
            .unwrap_or_default()
    });

    // Load now playing for each zone when zones change
    use_effect(move || {
        let zone_list = zones_list_signal();
        if !zone_list.is_empty() {
            spawn(async move {
                let np_map = fetch_all_now_playing(&zone_list).await;
                now_playing.set(np_map);
            });
        }
    });

    // Refresh on SSE events
    use_effect(move || {
        let _ = (sse.event_count)();
        let event = (sse.last_event)();

        // Refresh zones list on structural changes
        if matches!(
            event.as_ref(),
            Some(
                SseEvent::ZoneUpdated { .. }
                    | SseEvent::ZoneRemoved { .. }
                    | SseEvent::RoonConnected
                    | SseEvent::RoonDisconnected
            )
        ) {
            zones.restart();
        }

        // Refresh now_playing on playback/volume changes
        if let Some(ref evt) = event {
            match evt {
                SseEvent::NowPlayingChanged { .. } | SseEvent::ZoneUpdated { .. } => {
                    if let Some(zone_id) = evt.zone_id() {
                        let zone_id = zone_id.to_string();
                        spawn(async move {
                            if let Some(np) = fetch_zone_now_playing(&zone_id).await {
                                now_playing.with_mut(|map| {
                                    map.insert(zone_id, np);
                                });
                            }
                        });
                    }
                }
                SseEvent::VolumeChanged { .. } => {
                    let zone_list = zones_list_signal();
                    if !zone_list.is_empty() {
                        spawn(async move {
                            let np_map = fetch_all_now_playing(&zone_list).await;
                            now_playing.with_mut(|map| {
                                for (k, v) in np_map {
                                    map.insert(k, v);
                                }
                            });
                        });
                    }
                }
                _ => {}
            }
        }
    });

    // Control handler
    let control = move |(zone_id, action): (String, String)| {
        spawn(async move {
            let req = ControlRequest {
                zone_id,
                action,
                value: None,
            };
            if let Err(e) = crate::app::api::post_json_no_response("/control", &req).await {
                #[cfg(target_arch = "wasm32")]
                web_sys::console::warn_1(&format!("Control request failed: {e}").into());
                #[cfg(not(target_arch = "wasm32"))]
                tracing::warn!("Control request failed: {e}");
            }
        });
    };

    let is_loading = zones.read().is_none();
    let zones_list = zones
        .read()
        .clone()
        .flatten()
        .map(|r| r.zones)
        .unwrap_or_default();
    let np_map = now_playing();

    // Group zones by source protocol
    let grouped_zones: Vec<(String, Vec<Zone>)> = {
        let mut groups: std::collections::HashMap<String, Vec<Zone>> =
            std::collections::HashMap::new();
        for zone in zones_list.iter() {
            let source = zone.source.clone().unwrap_or_else(|| "Other".to_string());
            groups.entry(source).or_default().push(zone.clone());
        }
        for zones in groups.values_mut() {
            zones.sort_by(|a, b| a.zone_name.cmp(&b.zone_name));
        }
        let priority = |s: &str| -> i32 {
            match s.to_lowercase().as_str() {
                "roon" => 0,
                "upnp" => 1,
                _ => 2,
            }
        };
        let mut result: Vec<_> = groups.into_iter().collect();
        result.sort_by(|a, b| priority(&a.0).cmp(&priority(&b.0)));
        result
    };

    let content = if is_loading {
        rsx! {
            div { class: "card p-6", aria_busy: "true", "Loading zones..." }
        }
    } else if zones_list.is_empty() {
        rsx! {
            div { class: "card p-6", "No zones available. Check that adapters are connected." }
        }
    } else {
        rsx! {
            for (source, group_zones) in grouped_zones {
                div { class: "mb-8",
                    h3 { class: "text-lg font-semibold mb-4 text-muted", "{source}" }
                    div { class: "grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3",
                        for zone in group_zones {
                            ZoneCard {
                                key: "{zone.zone_id}",
                                zone: zone.clone(),
                                now_playing: np_map.get(&zone.zone_id).cloned(),
                                on_control: control,
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        Layout {
            title: "Zones".to_string(),
            nav_active: "zones".to_string(),

            h1 { class: "text-2xl font-bold mb-6", "Zones" }

            section { id: "zones",
                {content}
            }
        }
    }
}

/// Zone card component
#[component]
fn ZoneCard(
    zone: Zone,
    now_playing: Option<NowPlaying>,
    on_control: EventHandler<(String, String)>,
) -> Element {
    let zone_id = zone.zone_id.clone();
    let zone_id_prev = zone_id.clone();
    let zone_id_play = zone_id.clone();
    let zone_id_next = zone_id.clone();
    let zone_id_vol_down = zone_id.clone();
    let zone_id_vol_up = zone_id.clone();

    let np = now_playing.as_ref();
    let is_playing = np.map(|n| n.is_playing).unwrap_or(false);

    // Extract volume info for component
    let volume = np.and_then(|n| n.volume);
    let volume_type = np.and_then(|n| n.volume_type.clone());
    let volume_step = np.and_then(|n| n.volume_step);

    // Album art URL with cache-busting image_key
    let base_image_url = np.and_then(|n| n.image_url.clone()).unwrap_or_default();
    let image_key = np.and_then(|n| n.image_key.clone());
    let image_url = if let Some(key) = image_key {
        let sep = if base_image_url.contains('?') { "&" } else { "?" };
        format!("{}{}k={}", base_image_url, sep, key)
    } else {
        base_image_url
    };
    let has_image = !image_url.is_empty();

    let (track, artist) = np
        .map(|n| {
            if n.line1.as_deref().unwrap_or("Idle") != "Idle" {
                (
                    n.line1.clone().unwrap_or_default(),
                    n.line2.clone().unwrap_or_default(),
                )
            } else {
                (String::new(), String::new())
            }
        })
        .unwrap_or_default();

    rsx! {
        article { class: "zone-card",
            div { class: "flex gap-3 sm:gap-5 items-start overflow-hidden",
                if has_image {
                    img {
                        src: "{image_url}",
                        alt: "Album art",
                        class: "w-16 h-16 sm:w-24 sm:h-24 object-cover rounded-lg bg-elevated flex-shrink-0"
                    }
                } else {
                    div { class: "w-16 h-16 sm:w-24 sm:h-24 rounded-lg bg-elevated flex items-center justify-center text-muted text-2xl sm:text-3xl flex-shrink-0",
                        "♪"
                    }
                }

                div { class: "flex-1 min-w-0",
                    h3 { class: "flex items-center gap-2 mb-2 text-base font-semibold",
                        span { class: "truncate", "{zone.zone_name}" }
                    }

                    if !track.is_empty() {
                        p { class: "font-medium text-sm truncate mb-1", "{track}" }
                        p { class: "text-sm text-muted truncate", "{artist}" }
                    } else {
                        p { class: "text-sm text-muted", "Nothing playing" }
                    }
                }
            }

            div { class: "flex flex-wrap items-center gap-2 mt-4",
                button {
                    class: "btn btn-ghost",
                    "aria-label": "Previous track",
                    onclick: move |_| on_control.call((zone_id_prev.clone(), "previous".to_string())),
                    svg { class: "w-5 h-5", fill: "currentColor", view_box: "0 0 24 24",
                        path { d: "M6 6h2v12H6zm3.5 6l8.5 6V6z" }
                    }
                }
                button {
                    class: "btn btn-primary",
                    "aria-label": if is_playing { "Pause" } else { "Play" },
                    onclick: move |_| on_control.call((zone_id_play.clone(), "play_pause".to_string())),
                    if is_playing {
                        svg { class: "w-5 h-5", fill: "currentColor", view_box: "0 0 24 24",
                            path { d: "M6 19h4V5H6v14zm8-14v14h4V5h-4z" }
                        }
                    } else {
                        svg { class: "w-5 h-5", fill: "currentColor", view_box: "0 0 24 24",
                            path { d: "M8 5v14l11-7z" }
                        }
                    }
                }
                button {
                    class: "btn btn-ghost",
                    "aria-label": "Next track",
                    onclick: move |_| on_control.call((zone_id_next.clone(), "next".to_string())),
                    svg { class: "w-5 h-5", fill: "currentColor", view_box: "0 0 24 24",
                        path { d: "M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z" }
                    }
                }

                VolumeControlsCompact {
                    volume: volume,
                    volume_type: volume_type,
                    volume_step: volume_step,
                    on_vol_down: move |_| on_control.call((zone_id_vol_down.clone(), "vol_down".to_string())),
                    on_vol_up: move |_| on_control.call((zone_id_vol_up.clone(), "vol_up".to_string())),
                }
            }
        }
    }
}
