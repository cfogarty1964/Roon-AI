//! Windows SMTC (System Media Transport Controls) bridge.
//!
//! Lets the keyboard's media keys (Play/Pause/Next/Previous) drive playback on
//! the active Roon zone — regardless of which app is in the foreground. Roon
//! AI registers as a Windows media session; Windows routes media-key presses
//! to our handler, and we dispatch to the most-recently-updated playing zone.
//!
//! The "active zone" is resolved at each command:
//! 1. Most-recently-updated zone in `playing` state (preferred)
//! 2. Most-recently-updated zone overall (fallback when nothing is playing)
//! 3. First Roon zone alphabetically (last-resort fallback)
//!
//! Metadata (title / artist / album / cover) is pushed to Windows whenever a
//! `NowPlayingChanged` or `ZoneUpdated` event fires for the active zone, so
//! the volume-overlay tile matches what's actually playing.
//!
//! Compiled only on Windows; non-Windows builds skip this module entirely
//! (see `lib.rs`). Cross-platform support (Linux MPRIS, macOS NowPlaying)
//! could come later — souvlaki already speaks all three; we just need to
//! relax the cfg gate and test.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig,
};
use tokio::sync::mpsc;

use crate::adapters::roon::RoonAdapter;
use crate::adapters::upnp::UPnPAdapter;
use crate::aggregator::ZoneAggregator;
use crate::bus::events::PlaybackState;
use crate::bus::{BusEvent, SharedBus, Zone};

/// Spawn the SMTC integration. Returns immediately; all work happens in
/// background tasks. If SMTC initialisation fails (e.g. running as a
/// service, no desktop session) the function logs a warning and returns
/// without spawning — the rest of the app keeps working unchanged.
///
/// `hwnd_isize` is a window handle (cast from `*mut c_void`) that souvlaki
/// uses to host its SMTC session. Souvlaki on Windows requires a real
/// HWND — we obtain one from a hidden tao window created in `run_tray()`
/// on the main thread (Windows GUI windows have to be created on the
/// thread that owns the message pump). The handle is passed across as
/// `isize` because raw pointers aren't `Send`; we cast back inside the
/// SMTC thread before passing it to souvlaki.
pub fn spawn_smtc(
    bus: SharedBus,
    aggregator: Arc<ZoneAggregator>,
    roon: Arc<RoonAdapter>,
    upnp: Arc<UPnPAdapter>,
    hwnd_isize: isize,
    runtime: tokio::runtime::Handle,
) {
    // souvlaki's MediaControls is created on a normal thread (not tokio).
    // It owns a hidden window with a message loop on Windows. We move it
    // into a dedicated thread to keep the lifecycle clean.
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<MediaControlEvent>();
    let (meta_tx, meta_rx) = std::sync::mpsc::channel::<SmtcCommand>();

    // Background thread owns the MediaControls instance. Receives metadata
    // updates via std::sync::mpsc and forwards SMTC button presses through
    // the tokio mpsc to the async dispatch task below.
    std::thread::Builder::new()
        .name("roon-ai-smtc".into())
        .spawn(move || {
            run_smtc_thread(event_tx, meta_rx, hwnd_isize);
        })
        .ok();

    // Bus subscriber → forwards relevant events to the SMTC thread as
    // SmtcCommand::SetMetadata / SetPlayback. Spawned on the server's
    // tokio runtime since this code runs on the main thread (which has
    // no runtime of its own).
    let aggregator_for_bus = aggregator.clone();
    let bus_for_subscriber = bus.clone();
    let meta_tx_for_bus = meta_tx.clone();
    runtime.spawn(async move {
        run_bus_to_smtc(bus_for_subscriber, aggregator_for_bus, meta_tx_for_bus).await;
    });

    // SMTC events → resolve active zone → dispatch to adapter.
    runtime.spawn(async move {
        while let Some(evt) = event_rx.recv().await {
            handle_smtc_event(evt, &aggregator, &roon, &upnp).await;
        }
    });
}

/// Commands sent from async-land to the SMTC thread (which owns the
/// MediaControls instance and isn't a tokio task).
enum SmtcCommand {
    SetMetadata {
        title: String,
        artist: String,
        album: String,
        cover_url: Option<String>,
    },
    SetPlayback(MediaPlayback),
}

fn run_smtc_thread(
    event_tx: mpsc::UnboundedSender<MediaControlEvent>,
    meta_rx: std::sync::mpsc::Receiver<SmtcCommand>,
    hwnd_isize: isize,
) {
    // PlatformConfig.hwnd: Souvlaki on Windows hooks the SMTC dispatch onto
    // the window's WndProc so the caller has to provide a window handle and
    // (separately) keep that window's message pump running. We get the
    // handle from the hidden tao window created in `run_tray()`. Cast back
    // from isize → raw pointer here; this isn't unsafe per se (creating the
    // pointer is fine; souvlaki's internal use of it is what's `unsafe`).
    let config = PlatformConfig {
        dbus_name: "roon_ai",
        display_name: "Roon AI",
        hwnd: Some(hwnd_isize as *mut std::ffi::c_void),
    };

    let mut controls = match MediaControls::new(config) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "SMTC init failed: {:?} — media keys will not control Roon AI \
                (this is expected when running as a Windows service or without a desktop session).",
                e
            );
            return;
        }
    };

    if let Err(e) = controls.attach(move |evt: MediaControlEvent| {
        let _ = event_tx.send(evt);
    }) {
        tracing::warn!("SMTC attach failed: {:?}", e);
        return;
    }

    // Initial state — show "Roon AI" with no track until the first
    // NowPlayingChanged arrives.
    let _ = controls.set_metadata(MediaMetadata {
        title: Some("Roon AI"),
        artist: None,
        album: None,
        cover_url: None,
        duration: None,
    });
    let _ = controls.set_playback(MediaPlayback::Stopped);

    tracing::info!(
        "SMTC ready — keyboard media keys (Play/Pause/Next/Previous) now drive the active Roon zone"
    );

    // Block on commands. souvlaki spins its own message-pump thread for
    // Windows, so we only have to relay our own metadata updates here.
    while let Ok(cmd) = meta_rx.recv() {
        match cmd {
            SmtcCommand::SetMetadata {
                title,
                artist,
                album,
                cover_url,
            } => {
                let _ = controls.set_metadata(MediaMetadata {
                    title: if title.is_empty() { None } else { Some(&title) },
                    artist: if artist.is_empty() { None } else { Some(&artist) },
                    album: if album.is_empty() { None } else { Some(&album) },
                    cover_url: cover_url.as_deref(),
                    duration: None,
                });
            }
            SmtcCommand::SetPlayback(state) => {
                let _ = controls.set_playback(state);
            }
        }
    }
}

/// Subscribes to the bus and forwards now-playing / state changes to the
/// SMTC thread. Maintains an "active zone" cache so we only push metadata
/// when the relevant zone changes.
async fn run_bus_to_smtc(
    bus: SharedBus,
    aggregator: Arc<ZoneAggregator>,
    meta_tx: std::sync::mpsc::Sender<SmtcCommand>,
) {
    let mut rx = bus.subscribe();
    let mut last_active_id: Option<String> = None;

    // Periodic refresh too — the active zone can change without any single
    // bus event firing (e.g. when a ZoneUpdated for zone B makes B the
    // most-recent-playing instead of A).
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            evt = rx.recv() => {
                match evt {
                    Ok(BusEvent::NowPlayingChanged { .. })
                    | Ok(BusEvent::ZoneUpdated { .. })
                    | Ok(BusEvent::ZoneDiscovered { .. })
                    | Ok(BusEvent::ZoneRemoved { .. }) => {
                        push_active_zone(&aggregator, &meta_tx, &mut last_active_id).await;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            _ = tick.tick() => {
                push_active_zone(&aggregator, &meta_tx, &mut last_active_id).await;
            }
        }
    }
}

async fn push_active_zone(
    aggregator: &Arc<ZoneAggregator>,
    meta_tx: &std::sync::mpsc::Sender<SmtcCommand>,
    last_active_id: &mut Option<String>,
) {
    let zones = aggregator.get_zones().await;
    let Some(active) = pick_active_zone(&zones) else {
        return;
    };
    let id = active.zone_id.clone();
    // Always re-push metadata + playback state on any zone change. (A more
    // surgical approach would skip if neither title nor playback state
    // changed since last push, but the cost of a redundant SMTC call is
    // negligible.)
    let title = active
        .now_playing
        .as_ref()
        .map(|np| np.title.clone())
        .unwrap_or_default();
    let artist = active
        .now_playing
        .as_ref()
        .map(|np| np.artist.clone())
        .unwrap_or_default();
    let album = active
        .now_playing
        .as_ref()
        .map(|np| np.album.clone())
        .unwrap_or_default();
    let cover_url = active.now_playing.as_ref().and_then(|np| {
        np.image_key.as_ref().map(|k| {
            // The SMTC tile loads the cover via http(s) — point it at our
            // own /roon/image proxy. Windows fetches it on the user's
            // machine, so localhost works.
            format!(
                "https://localhost:8088/roon/image?image_key={}&width=400&height=400",
                urlencoding::encode(k)
            )
        })
    });

    let _ = meta_tx.send(SmtcCommand::SetMetadata {
        title,
        artist,
        album,
        cover_url,
    });

    let playback = match active.state {
        PlaybackState::Playing => MediaPlayback::Playing { progress: None },
        PlaybackState::Paused => MediaPlayback::Paused { progress: None },
        _ => MediaPlayback::Stopped,
    };
    let _ = meta_tx.send(SmtcCommand::SetPlayback(playback));

    *last_active_id = Some(id);
}

/// Pick the "active zone" for SMTC — the one whose state we expose and the
/// one media-key presses dispatch to. Order: most-recently-updated playing,
/// then most-recently-updated overall, then first Roon zone alphabetically.
fn pick_active_zone(zones: &[Zone]) -> Option<&Zone> {
    let playing = zones
        .iter()
        .filter(|z| z.state == PlaybackState::Playing)
        .max_by_key(|z| z.last_updated);
    if let Some(z) = playing {
        return Some(z);
    }
    let any = zones.iter().max_by_key(|z| z.last_updated);
    if let Some(z) = any {
        return Some(z);
    }
    zones.iter().find(|z| z.zone_id.starts_with("roon:"))
}

async fn handle_smtc_event(
    evt: MediaControlEvent,
    aggregator: &Arc<ZoneAggregator>,
    roon: &Arc<RoonAdapter>,
    upnp: &Arc<UPnPAdapter>,
) {
    let zones = aggregator.get_zones().await;
    let Some(active) = pick_active_zone(&zones) else {
        tracing::debug!("SMTC event {:?} ignored — no zones available", evt);
        return;
    };
    let zone_id = active.zone_id.clone();

    let action = match evt {
        MediaControlEvent::Play => "play",
        MediaControlEvent::Pause => "pause",
        MediaControlEvent::Toggle => "play_pause",
        MediaControlEvent::Next => "next",
        MediaControlEvent::Previous => "previous",
        MediaControlEvent::Stop => "stop",
        // Volume / Seek / Quit / Raise / Open are not currently surfaced
        // by SMTC for our use case; ignore them.
        _ => {
            tracing::debug!("SMTC event {:?} ignored (unsupported)", evt);
            return;
        }
    };

    tracing::info!("SMTC {:?} → zone {} action={}", evt, zone_id, action);

    let result = if zone_id.starts_with("roon:") {
        roon.control(&zone_id, action).await
    } else if zone_id.starts_with("upnp:") {
        // UPnP's control signature has an extra Option<i32> for volume
        // step actions; we never send those from SMTC, so None is fine.
        upnp.control(&zone_id, action, None).await
    } else {
        Err(anyhow::anyhow!("unknown zone prefix: {}", zone_id))
    };

    if let Err(e) = result {
        tracing::warn!("SMTC dispatch to {} failed: {:?}", zone_id, e);
    }
}

// Suppress an unused-import warning when the bus has no PlaybackState in
// some future cfg combination — keep the bus types fully imported.
#[allow(dead_code)]
fn _link_check(_: PlaybackState) -> Result<()> {
    Ok(())
}
