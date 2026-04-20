use crate::app::api::{BrowseItem, BrowseLoadResult, BrowseResult, ZonesResponse};
use crate::app::components::Layout;
use crate::app::default_zone::use_default_zone;
use dioxus::prelude::*;

const ALPHA_THRESHOLD: u32 = 26;

// ---------------------------------------------------------------------------
// Request shapes
// ---------------------------------------------------------------------------

#[derive(Clone, serde::Serialize)]
struct BrowseRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    item_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zone_id: Option<String>,
    pop_all: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pop_levels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_key: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct BrowseLoadRequest {
    session_key: String,
    offset: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

#[derive(Clone, serde::Serialize)]
struct PlayItemRequest {
    item_key: String,
    zone_id: String,
    action: String,
}

// ---------------------------------------------------------------------------
// Page-local state
// ---------------------------------------------------------------------------

#[derive(Clone, Default, PartialEq)]
struct LibraryState {
    items: Vec<BrowseItem>,
    list_title: String,
    list_count: u32,
    level: u32,
    session_key: Option<String>,
    loading: bool,
    loading_all: bool,
    error: Option<String>,
}

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

#[component]
pub fn Library() -> Element {
    let mut state = use_signal(LibraryState::default);
    let mut selected_zone = use_signal(|| String::new());
    let mut play_msg = use_signal(|| None::<String>);
    let mut selected_letter = use_signal(|| None::<char>);
    let default_zone_ctx = use_default_zone();

    let zones = use_resource(|| async {
        crate::app::api::fetch_json::<ZonesResponse>("/zones")
            .await
            .ok()
            .map(|r| r.zones)
            .unwrap_or_default()
    });

    // Pre-select default zone if stored, otherwise fall back to first Roon zone
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

    // Load library root on mount (once); include zone_id if already selected so Roon
    // can associate the session with the target zone from the first browse call.
    use_effect(move || {
        let already_loaded = {
            let s = state.read();
            s.loading || s.session_key.is_some()
        };
        if !already_loaded {
            let zone = selected_zone.read().clone();
            spawn(async move {
                do_browse(
                    &mut state,
                    BrowseRequest {
                        item_key: None,
                        zone_id: if zone.is_empty() { None } else { Some(zone) },
                        pop_all: true,
                        pop_levels: None,
                        session_key: None,
                    },
                )
                .await;
            });
        }
    });

    // Auto-load all items in background for large lists so the alphabet filter is complete
    use_effect(move || {
        let (should_load, sess, offset) = {
            let s = state.read();
            let should = s.list_count >= ALPHA_THRESHOLD
                && !s.loading
                && !s.loading_all
                && (s.items.len() as u32) < s.list_count
                && s.session_key.is_some();
            (should, s.session_key.clone(), s.items.len())
        };
        if should_load {
            if let Some(sk) = sess {
                state.with_mut(|s| s.loading_all = true);
                spawn(async move {
                    load_all_remaining(&mut state, sk, offset).await;
                });
            }
        }
    });

    // ---------------------------------------------------------------------------
    // Derived values (computed before rsx! to avoid borrow issues)
    // ---------------------------------------------------------------------------

    let st = state.read().clone();
    let zone_list = zones.read().clone().unwrap_or_default();
    let items = st.items.clone();
    let is_large_list = st.list_count >= ALPHA_THRESHOLD;
    let letter = *selected_letter.read();

    // Which letters actually have loaded items
    let active_letters: std::collections::HashSet<char> = if is_large_list {
        items
            .iter()
            .filter_map(|i| {
                i.title.chars().next().map(|c| {
                    if c.is_ascii_alphabetic() {
                        c.to_ascii_uppercase()
                    } else {
                        '#'
                    }
                })
            })
            .collect()
    } else {
        std::collections::HashSet::new()
    };

    // Apply letter filter
    let displayed_items: Vec<BrowseItem> = if is_large_list {
        match letter {
            Some('#') => items
                .iter()
                .filter(|i| {
                    !i.title
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_alphabetic())
                        .unwrap_or(false)
                })
                .cloned()
                .collect(),
            Some(l) => items
                .iter()
                .filter(|i| {
                    i.title
                        .chars()
                        .next()
                        .map(|c| c.to_ascii_uppercase() == l)
                        .unwrap_or(false)
                })
                .cloned()
                .collect(),
            None => items.clone(),
        }
    } else {
        items.clone()
    };

    let displayed_empty = displayed_items.is_empty();

    // ---------------------------------------------------------------------------
    // Render
    // ---------------------------------------------------------------------------

    rsx! {
        Layout {
            title: "Library",
            nav_active: "library",

            // Page header
            div { class: "flex flex-wrap items-center justify-between gap-3 mb-4",
                h1 { class: "text-2xl font-semibold", "Library" }

                div { class: "flex items-center gap-2",
                    label { class: "text-sm text-muted", "Play to:" }
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

            // Play feedback
            if let Some(ref msg) = *play_msg.read() {
                div { class: "mb-3 text-sm text-green-600 dark:text-green-400", "{msg}" }
            }

            // Browse nav bar: back + title + loading-all progress
            div { class: "flex items-center gap-3 mb-3",
                if st.level > 0 {
                    button {
                        class: "btn-ghost flex items-center gap-1 text-sm",
                        onclick: move |_| {
                            selected_letter.set(None);
                            let sess = state.read().session_key.clone();
                            let zone = selected_zone.read().clone();
                            spawn(async move {
                                do_browse(
                                    &mut state,
                                    BrowseRequest {
                                        item_key: None,
                                        zone_id: if zone.is_empty() { None } else { Some(zone) },
                                        pop_all: false,
                                        pop_levels: Some(1),
                                        session_key: sess,
                                    },
                                )
                                .await;
                            });
                        },
                        svg { class: "w-4 h-4", fill: "none", view_box: "0 0 24 24", stroke: "currentColor", "stroke-width": "2",
                            path { "stroke-linecap": "round", "stroke-linejoin": "round", d: "M15 19l-7-7 7-7" }
                        }
                        "Back"
                    }
                }
                if !st.list_title.is_empty() {
                    span { class: "font-medium", "{st.list_title}" }
                    if st.list_count > 0 {
                        span { class: "text-sm text-muted ml-1", "({st.list_count})" }
                    }
                }
                if st.loading_all {
                    span { class: "text-xs text-muted ml-auto animate-pulse",
                        "Indexing {st.items.len()} of {st.list_count}…"
                    }
                }
            }

            // Alphabet selector bar (large lists only)
            if is_large_list && !st.loading {
                div { class: "flex flex-wrap gap-1 mb-4 select-none",
                    for letter_char in "ABCDEFGHIJKLMNOPQRSTUVWXYZ#".chars() {
                        {
                            let is_active = *selected_letter.read() == Some(letter_char);
                            let has_items = active_letters.contains(&letter_char);
                            let btn_class = if is_active {
                                "w-7 h-7 text-xs font-bold rounded bg-blue-500 text-white"
                            } else if has_items {
                                "w-7 h-7 text-xs rounded hover:bg-gray-200 dark:hover:bg-gray-700 cursor-pointer transition-colors"
                            } else {
                                "w-7 h-7 text-xs rounded text-gray-300 dark:text-gray-600"
                            };
                            rsx! {
                                button {
                                    class: "{btn_class}",
                                    disabled: !has_items,
                                    onclick: move |_| {
                                        if has_items {
                                            let cur = *selected_letter.read();
                                            selected_letter.set(
                                                if cur == Some(letter_char) { None } else { Some(letter_char) }
                                            );
                                        }
                                    },
                                    "{letter_char}"
                                }
                            }
                        }
                    }
                    if selected_letter.read().is_some() {
                        button {
                            class: "px-2 h-7 text-xs rounded bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 ml-1 transition-colors",
                            onclick: move |_| selected_letter.set(None),
                            "All"
                        }
                    }
                }
            }

            // Loading spinner
            if st.loading {
                div { class: "flex justify-center py-12",
                    svg { class: "animate-spin h-8 w-8 text-muted", fill: "none", view_box: "0 0 24 24",
                        circle { class: "opacity-25", cx: "12", cy: "12", r: "10", stroke: "currentColor", "stroke-width": "4" }
                        path { class: "opacity-75", fill: "currentColor", d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" }
                    }
                }
            }

            // Error
            if let Some(ref err) = st.error {
                div { class: "card p-4 text-red-600 dark:text-red-400", "{err}" }
            }

            // Items
            if !st.loading {
                if is_large_list {
                    // Multi-column grid for artists / composers / large lists
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 xl:grid-cols-5 gap-1",
                        for item in displayed_items {
                            {
                                let hint = item.hint.clone().unwrap_or_default();
                                let is_navigable = hint == "list" || hint == "action_list";
                                let is_playable = hint == "action";
                                let has_key = item.item_key.is_some();
                                let item_key = item.item_key.clone();
                                let title = item.title.clone();
                                let image_key = item.image_key.clone();

                                let image_url = image_key.as_ref().map(|k| {
                                    format!("/roon/image?image_key={}&width=80&height=80", urlencoding::encode(k))
                                });

                                let first_char = title.chars().next().unwrap_or('?').to_string();

                                let card_class = if has_key {
                                    "flex items-center gap-2 p-2 rounded-lg cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                                } else {
                                    "flex items-center gap-2 p-2 rounded-lg"
                                };

                                rsx! {
                                    div {
                                        class: "{card_class}",
                                        onclick: move |_| {
                                            if !has_key { return; }
                                            if is_navigable {
                                                selected_letter.set(None);
                                                let sess = state.read().session_key.clone();
                                                let key = item_key.clone();
                                                let zone = selected_zone.read().clone();
                                                spawn(async move {
                                                    do_browse(
                                                        &mut state,
                                                        BrowseRequest {
                                                            item_key: key,
                                                            zone_id: if zone.is_empty() { None } else { Some(zone) },
                                                            pop_all: false,
                                                            pop_levels: None,
                                                            session_key: sess,
                                                        },
                                                    )
                                                    .await;
                                                });
                                            } else if is_playable {
                                                let zone = selected_zone.read().clone();
                                                if zone.is_empty() {
                                                    play_msg.set(Some("Select a zone first".to_string()));
                                                    return;
                                                }
                                                let key = item_key.clone().unwrap_or_default();
                                                let sess = state.read().session_key.clone();
                                                spawn(async move {
                                                    do_action_item(&mut state, key, zone, sess, &mut play_msg).await;
                                                });
                                            }
                                        },
                                        // Thumbnail or initial letter
                                        div { class: "w-10 h-10 flex-shrink-0 rounded overflow-hidden bg-gray-100 dark:bg-gray-800",
                                            if let Some(ref url) = image_url {
                                                img { src: "{url}", class: "w-full h-full object-cover", loading: "lazy" }
                                            } else {
                                                div { class: "w-full h-full flex items-center justify-center text-muted text-sm font-semibold",
                                                    "{first_char}"
                                                }
                                            }
                                        }
                                        // Name
                                        p { class: "text-sm font-medium leading-tight line-clamp-2 flex-1 min-w-0", "{title}" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Single-column list for small / non-alphabetic lists
                    div { class: "divide-y divide-gray-100 dark:divide-gray-800",
                        for item in displayed_items {
                            {
                                let hint = item.hint.clone().unwrap_or_default();
                                let is_navigable = hint == "list" || hint == "action_list";
                                let is_playable = hint == "action";
                                let is_header = hint == "header";
                                let has_key = item.item_key.is_some();
                                let item_key = item.item_key.clone();
                                let title = item.title.clone();
                                let subtitle = item.subtitle.clone();
                                let image_key = item.image_key.clone();

                                let image_url = image_key.as_ref().map(|k| {
                                    format!("/roon/image?image_key={}&width=64&height=64", urlencoding::encode(k))
                                });

                                let row_class = if is_header {
                                    "px-1 py-2"
                                } else if has_key {
                                    "flex items-center gap-3 py-2 px-1 rounded cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
                                } else {
                                    "flex items-center gap-3 py-2 px-1 rounded"
                                };

                                rsx! {
                                    div {
                                        class: "{row_class}",
                                        onclick: move |_| {
                                            if !has_key || is_header { return; }
                                            if is_navigable {
                                                selected_letter.set(None);
                                                let sess = state.read().session_key.clone();
                                                let key = item_key.clone();
                                                let zone = selected_zone.read().clone();
                                                spawn(async move {
                                                    do_browse(
                                                        &mut state,
                                                        BrowseRequest {
                                                            item_key: key,
                                                            zone_id: if zone.is_empty() { None } else { Some(zone) },
                                                            pop_all: false,
                                                            pop_levels: None,
                                                            session_key: sess,
                                                        },
                                                    )
                                                    .await;
                                                });
                                            } else if is_playable {
                                                let zone = selected_zone.read().clone();
                                                if zone.is_empty() {
                                                    play_msg.set(Some("Select a zone first".to_string()));
                                                    return;
                                                }
                                                let key = item_key.clone().unwrap_or_default();
                                                let sess = state.read().session_key.clone();
                                                spawn(async move {
                                                    do_action_item(&mut state, key, zone, sess, &mut play_msg).await;
                                                });
                                            }
                                        },

                                        if is_header {
                                            span { class: "text-xs font-semibold uppercase text-muted tracking-wide", "{title}" }
                                        } else {
                                            div { class: "w-10 h-10 flex-shrink-0 rounded overflow-hidden bg-gray-100 dark:bg-gray-800",
                                                if let Some(ref url) = image_url {
                                                    img { src: "{url}", class: "w-full h-full object-cover", loading: "lazy" }
                                                } else {
                                                    div { class: "w-full h-full flex items-center justify-center text-muted",
                                                        svg { class: "w-5 h-5", fill: "currentColor", view_box: "0 0 20 20",
                                                            path { d: "M18 3a1 1 0 00-1.196-.98l-10 2A1 1 0 006 5v9.114A4.369 4.369 0 005 14c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V7.82l8-1.6v5.894A4.37 4.37 0 0015 12c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V3z" }
                                                        }
                                                    }
                                                }
                                            }

                                            div { class: "flex-1 min-w-0",
                                                p { class: "text-sm font-medium truncate", "{title}" }
                                                if let Some(ref sub) = subtitle {
                                                    p { class: "text-xs text-muted truncate", "{sub}" }
                                                }
                                            }

                                            if is_navigable {
                                                svg { class: "w-4 h-4 text-muted flex-shrink-0", fill: "none", view_box: "0 0 24 24", stroke: "currentColor", "stroke-width": "2",
                                                    path { "stroke-linecap": "round", "stroke-linejoin": "round", d: "M9 5l7 7-7 7" }
                                                }
                                            } else if is_playable {
                                                svg { class: "w-4 h-4 text-blue-500 flex-shrink-0", fill: "currentColor", view_box: "0 0 20 20",
                                                    path { d: "M10 18a8 8 0 100-16 8 8 0 000 16zM9.555 7.168A1 1 0 008 8v4a1 1 0 001.555.832l3-2a1 1 0 000-1.664l-3-2z" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Empty state
                if displayed_empty && st.error.is_none() {
                    div { class: "text-center py-12 text-muted",
                        if letter.is_some() {
                            p { "No items for this letter." }
                        } else {
                            p { "No items — waiting for Roon browse service." }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Async helpers
// ---------------------------------------------------------------------------

async fn do_browse(state: &mut Signal<LibraryState>, req: BrowseRequest) {
    state.with_mut(|s| {
        s.loading = true;
        s.loading_all = false;
        s.error = None;
    });

    match crate::app::api::post_json::<BrowseRequest, BrowseResult>("/roon/browse", &req).await {
        Ok(result) => {
            state.with_mut(|s| {
                s.items = result.items;
                s.list_title = result
                    .list
                    .as_ref()
                    .map(|l| l.title.clone())
                    .unwrap_or_default();
                s.list_count = result.list.as_ref().map(|l| l.count).unwrap_or(0);
                s.level = result.list.as_ref().map(|l| l.level).unwrap_or(0);
                s.session_key = Some(result.session_key);
                s.loading = false;
                if result.is_error == Some(true) {
                    s.error = result.message.or(Some("Browse error".to_string()));
                }
            });
        }
        Err(e) => {
            state.with_mut(|s| {
                s.loading = false;
                s.error = Some(e);
            });
        }
    }
}

async fn load_all_remaining(state: &mut Signal<LibraryState>, session_key: String, mut offset: usize) {
    loop {
        // Bail if the user navigated away (session changed) before we started or between batches.
        let still_current = state.read().session_key.as_deref() == Some(session_key.as_str());
        if !still_current {
            break;
        }

        let req = BrowseLoadRequest {
            session_key: session_key.clone(),
            offset,
            count: Some(100),
        };
        match crate::app::api::post_json::<BrowseLoadRequest, BrowseLoadResult>(
            "/roon/browse/load",
            &req,
        )
        .await
        {
            Ok(result) => {
                let loaded = result.items.len();
                if loaded == 0 {
                    break;
                }
                offset += loaded;
                let done = state.with_mut(|s| {
                    // Only append if we're still on the same browse level.
                    if s.session_key.as_deref() != Some(session_key.as_str()) {
                        return true; // triggers break
                    }
                    s.items.extend(result.items);
                    (s.items.len() as u32) >= s.list_count
                });
                if done {
                    break;
                }
            }
            Err(_) => {
                // Silently discard errors from stale background loads — the user has
                // almost certainly navigated away and the session_key is no longer valid.
                break;
            }
        }
    }
    state.with_mut(|s| {
        if s.session_key.as_deref() == Some(session_key.as_str()) {
            s.loading_all = false;
        }
    });
}

// Execute a Roon browse "action" item (Shuffle, Start Radio, Play Now, etc.)
// These must be triggered via /roon/browse with zone_id — not via /roon/play_item.
// After the call: if Roon returns a new list, navigate into it; if it returns a
// message/none, show feedback and leave the current list unchanged.
async fn do_action_item(
    state: &mut Signal<LibraryState>,
    item_key: String,
    zone_id: String,
    session_key: Option<String>,
    msg: &mut Signal<Option<String>>,
) {
    let req = BrowseRequest {
        item_key: Some(item_key),
        zone_id: Some(zone_id),
        pop_all: false,
        pop_levels: None,
        session_key,
    };
    match crate::app::api::post_json::<BrowseRequest, crate::app::api::BrowseResult>(
        "/roon/browse",
        &req,
    )
    .await
    {
        Ok(result) => {
            if result.action == "list" {
                // Roon returned a sub-menu — navigate into it
                state.with_mut(|s| {
                    s.items = result.items;
                    s.list_title = result.list.as_ref().map(|l| l.title.clone()).unwrap_or_default();
                    s.list_count = result.list.as_ref().map(|l| l.count).unwrap_or(0);
                    s.level = result.list.as_ref().map(|l| l.level).unwrap_or(0);
                    s.session_key = Some(result.session_key);
                    s.loading_all = false;
                });
            } else {
                // Action executed — show Roon's message or a generic confirmation
                let feedback = result.message.unwrap_or_else(|| "Playing".to_string());
                msg.set(Some(feedback));
            }
        }
        Err(e) => msg.set(Some(format!("Failed: {e}"))),
    }
}
