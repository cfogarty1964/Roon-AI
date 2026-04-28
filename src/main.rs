//! Roon AI - Rust Implementation
//!
//! A natural-language Roon control bridge with AI chat and voice control.

// In release builds on Windows, hide the console window so the binary runs
// silently in the background. Logs go to a rolling file in the data dir.
// Debug builds keep the console for development.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Server-only: full server implementation
#[cfg(feature = "server")]
mod server {
    use roon_ai::{
        adapters, aggregator, api, app, bus, config, coordinator, embedded, mcp,
    };

    // Import Startable trait for adapter lifecycle methods
    use adapters::Startable;

    // Import load_app_settings for checking adapter enabled state
    use api::load_app_settings;

    use anyhow::Result;
    use axum::{
        response::{IntoResponse, Redirect},
        routing::{delete, get, post},
        Router,
    };
    use dioxus::prelude::DioxusRouterExt;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::signal;
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;
    use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

    /// Legacy redirect: /control -> /ui/zones
    async fn control_redirect() -> impl IntoResponse {
        Redirect::to("/ui/zones")
    }

    /// Legacy redirect: /admin -> /settings
    async fn settings_redirect() -> impl IntoResponse {
        Redirect::to("/settings")
    }

    pub async fn run(external_shutdown: oneshot::Receiver<()>) -> Result<()> {
        tracing::info!(
            "Starting Roon AI v{} ({})",
            env!("ROON_AI_VERSION"),
            env!("ROON_AI_GIT_SHA")
        );

        // Log embedded assets status (ADR 002)
        if embedded::has_embedded_assets() {
            let assets = embedded::list_embedded_assets();
            tracing::info!(
                "Embedded WASM assets: {} files (single-binary mode)",
                assets.len()
            );
            tracing::debug!("Embedded files: {:?}", assets);
        } else {
            tracing::info!("No embedded WASM assets (development mode, use dx serve)");
        }

        // Load configuration
        let config = config::load_config()?;
        tracing::info!("Configuration loaded, port: {}", config.port);

        // Issue #76: Migrate config files to state/ subdirectory
        config::migrate_config_to_subdir();

        // Migrate Node.js config files if present (seamless Docker image swap)
        config::migrate_nodejs_configs();

        // Create event bus
        let bus = bus::create_bus();
        tracing::info!("Event bus initialized");

        // Load app settings and create adapter coordinator (single source of truth for lifecycle)
        let app_settings = load_app_settings();
        let coord = Arc::new(coordinator::AdapterCoordinator::new(bus.clone()));
        coord.register_from_settings(&app_settings.adapters).await;
        tracing::info!("Adapter coordinator initialized");

        // Construct base URL for display in Roon Settings → Extensions
        let base_url = format!(
            "http://{}:{}",
            gethostname::gethostname().to_string_lossy(),
            config.port
        );

        // =========================================================================
        // Create all adapter instances (needed for API handlers regardless of state)
        // =========================================================================

        // Roon adapter - coordinator handles starting based on enabled state
        let roon = Arc::new(adapters::roon::RoonAdapter::new_configured(
            bus.clone(),
            base_url.clone(),
        ));

        // UPnP adapter
        let upnp = Arc::new(adapters::upnp::UPnPAdapter::new(bus.clone()));

        // =========================================================================
        // Start enabled adapters (single codepath using coordinator)
        // =========================================================================

        // Build list of startable adapters
        let startable_adapters: Vec<Arc<dyn adapters::Startable>> = vec![
            roon.clone(),
            upnp.clone(),
        ];

        // Single loop to start all enabled adapters
        coord.start_all_enabled(&startable_adapters).await;

        // Initialize ZoneAggregator for unified zone state
        let zone_aggregator = Arc::new(aggregator::ZoneAggregator::new(bus.clone()));
        let aggregator_for_spawn = zone_aggregator.clone();
        tokio::spawn(async move {
            aggregator_for_spawn.run().await;
        });
        tracing::info!("ZoneAggregator started");

        // Clone Roon adapter for shutdown access (cheap - just Arc clones)
        let roon_for_shutdown = roon.clone();

        // Create shutdown token for graceful SSE termination (fixes #73)
        let shutdown_token = CancellationToken::new();

        // Resolve Anthropic API key (env var takes precedence over TOML)
        let anthropic_key = config::resolve_anthropic_api_key(&config);
        if anthropic_key.is_some() {
            tracing::info!("AI chat enabled (Anthropic API key found)");
        } else {
            tracing::info!("AI chat disabled (set ANTHROPIC_API_KEY to enable)");
        }

        // Build application state (clone Arcs so we can access adapters for shutdown)
        let state = api::AppState::new(
            roon,
            upnp.clone(),
            bus.clone(),
            zone_aggregator,
            coord.clone(),
            startable_adapters.clone(),
            Instant::now(),
            shutdown_token.clone(),
        )
        .with_anthropic_key(anthropic_key);

        // Clone state for shutdown diagnostics
        let state_for_shutdown = state.clone();

        // Create MCP extension (state for MCP handlers)
        let mcp_extension = mcp::create_mcp_extension(state.clone());

        // Build API routes
        let router = Router::new()
            // Health check
            .route("/status", get(api::status_handler))
            // Roon routes
            .route("/roon/status", get(api::roon_status_handler))
            .route("/roon/zones", get(api::roon_zones_handler))
            .route("/roon/zone/{zone_id}", get(api::roon_zone_handler))
            .route("/roon/control", post(api::roon_control_handler))
            .route("/roon/volume", post(api::roon_volume_handler))
            .route("/roon/image", get(api::roon_image_handler))
            // Roon Browse routes
            .route("/roon/search", get(api::roon_search_handler))
            .route("/roon/play", post(api::roon_play_handler))
            .route("/roon/play_item", post(api::roon_play_item_handler))
            .route("/roon/browse", post(api::roon_browse_handler))
            .route("/roon/browse/load", post(api::roon_browse_load_handler))
            .route("/roon/browse/status", get(api::roon_browse_status_handler))
            // UPnP routes
            .route("/upnp/status", get(api::upnp_status_handler))
            .route("/upnp/zones", get(api::upnp_zones_handler))
            .route(
                "/upnp/zone/{zone_id}/now_playing",
                get(api::upnp_now_playing_handler),
            )
            .route("/upnp/control", post(api::upnp_control_handler))
            // App settings API
            .route("/api/settings", get(api::api_settings_get_handler))
            .route("/api/settings", post(api::api_settings_post_handler))
            // AI chat
            .route("/api/ai/chat", post(api::ai_chat_handler))
            .route("/api/ai/chat/stream", post(api::ai_chat_stream_handler))
            // Event stream (SSE)
            .route("/events", get(api::events_handler))
            // Zones JSON for the web UI
            .route("/zones", get(api::zones_handler))
            // Legacy redirects
            .route("/control", get(control_redirect))
            .route("/admin", get(settings_redirect))
            // Embedded WASM/JS assets (ADR 002: serve from memory, no disk extraction)
            .route("/assets/{*path}", get(embedded::serve_embedded_asset))
            // Embedded static files (favicon, CSS, images)
            .route(
                "/favicon.ico",
                get(|| embedded::serve_static_file(axum::extract::Path("favicon.ico".to_string()))),
            )
            .route(
                "/apple-touch-icon.png",
                get(|| {
                    embedded::serve_static_file(axum::extract::Path(
                        "apple-touch-icon.png".to_string(),
                    ))
                }),
            )
            .route(
                "/tailwind.css",
                get(|| {
                    embedded::serve_static_file(axum::extract::Path("tailwind.css".to_string()))
                }),
            )
            .route(
                "/dx-components-theme.css",
                get(|| {
                    embedded::serve_static_file(axum::extract::Path(
                        "dx-components-theme.css".to_string(),
                    ))
                }),
            )
            // MCP routes (same port as main app)
            .route("/mcp", get(mcp::handle_mcp_get))
            .route("/mcp", post(mcp::handle_mcp_post))
            .route("/mcp", delete(mcp::handle_mcp_delete))
            // Middleware
            .layer(mcp_extension)
            .layer(CorsLayer::permissive())
            .layer(CompressionLayer::new())
            .layer(TraceLayer::new_for_http())
            .with_state(state);

        // ADR 002: Embedded assets mode - SSR with injected bootstrap scripts
        // serve_api_application() provides SSR + server functions, but no static assets
        // Our middleware injects the bootstrap scripts (from embedded index.html) into SSR HTML
        // This enables WASM hydration without requiring a public/ directory at runtime
        let router = if embedded::has_embedded_assets() {
            if let Some(bootstrap) = embedded::extract_bootstrap_snippet() {
                tracing::info!("Using embedded SSR mode (bootstrap scripts will be injected)");
                tracing::debug!("Bootstrap snippet:\n{}", bootstrap);
                router
                    .serve_api_application(dioxus::server::ServeConfig::new(), app::App)
                    .layer(embedded::InjectDioxusBootstrapLayer::new(bootstrap))
            } else {
                tracing::warn!(
                    "Embedded assets found but no bootstrap scripts - falling back to SPA"
                );
                router
                    .serve_api_application(dioxus::server::ServeConfig::new(), app::App)
                    .fallback(embedded::serve_index_html)
            }
        } else {
            tracing::info!("Using SSR mode (no embedded assets, use dx serve for development)");
            // Standard SSR mode for development
            router.serve_dioxus_application(dioxus::server::ServeConfig::new(), app::App)
        };

        // Start server with graceful shutdown
        let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
        tracing::info!("Listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;

        // Create shutdown future that races signal/external/token cancel (fixes #73)
        let graceful_shutdown = {
            let token = shutdown_token.clone();
            let state = state_for_shutdown.clone();
            async move {
                shutdown_signal(external_shutdown).await;

                // Cancel SSE streams BEFORE Axum starts waiting for connections
                token.cancel();

                // Log active SSE connections for diagnostics
                let active = state.active_sse_connections();
                if active > 0 {
                    tracing::info!(
                        "Cancelling {} active SSE connection(s) for graceful shutdown",
                        active
                    );
                }
            }
        };

        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(graceful_shutdown)
        .await?;

        // Cleanup: publish ShuttingDown event and stop adapters
        tracing::info!("Shutting down adapters...");

        // Publish ShuttingDown event for any bus listeners
        bus.publish(bus::BusEvent::ShuttingDown {
            reason: Some("User requested shutdown".to_string()),
        });

        // Give listeners a moment to react to ShuttingDown
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Stop adapters
        roon_for_shutdown.stop().await;
        upnp.stop().await;
        tracing::info!("Shutdown complete");

        Ok(())
    }

    /// Wait for any shutdown signal: Ctrl+C, SIGTERM, or external (tray Quit).
    #[allow(clippy::expect_used)] // Signal handlers must succeed for graceful shutdown
    async fn shutdown_signal(external: oneshot::Receiver<()>) {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down..."),
            _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
            _ = external => tracing::info!("External shutdown requested (tray Quit), shutting down..."),
        }
    }
}

// ===========================================================================
// Logging setup — file appender + optional stdout (debug builds only)
// ===========================================================================

#[cfg(feature = "server")]
fn setup_logging() -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    // Priority: RUST_LOG > LOG_LEVEL (legacy) > default
    let log_filter = std::env::var("RUST_LOG")
        .or_else(|_| std::env::var("LOG_LEVEL"))
        .unwrap_or_else(|_| "roon_ai=debug,tower_http=debug,roon_api=info".into());

    // File logging — rolling daily under <data_dir>/logs/
    let logs_dir = roon_ai::config::get_data_dir().join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "roon-ai.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    // Stdout layer is harmless when console is hidden (writes go nowhere); useful in debug builds
    let stdout_layer = tracing_subscriber::fmt::layer();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(&log_filter))
        .with(file_layer)
        .with(stdout_layer)
        .init();

    tracing::info!("Logs: {}", logs_dir.display());

    Ok(guard)
}

// ===========================================================================
// Windows-only: system tray icon with menu
// ===========================================================================

#[cfg(all(windows, feature = "server"))]
fn run_tray(shutdown_tx: tokio::sync::oneshot::Sender<()>) -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
        TrayIconBuilder,
    };

    let event_loop = EventLoopBuilder::new().build();

    // Build menu
    let menu = Menu::new();
    let about = MenuItem::new(
        format!("Roon AI v{}", env!("ROON_AI_VERSION")),
        false,
        None,
    );
    let open_ui = MenuItem::new("Open Web UI", true, None);
    let open_logs = MenuItem::new("Open Logs Folder", true, None);
    let quit = MenuItem::new("Quit", true, None);
    menu.append(&about)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&open_ui)?;
    menu.append(&open_logs)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit)?;

    // Load icon (embedded PNG)
    let icon = load_tray_icon()?;

    // Tray must outlive the event loop — bind to a let so it isn't dropped
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("Roon AI v{}", env!("ROON_AI_VERSION")))
        .with_icon(icon)
        .build()?;

    let menu_channel = MenuEvent::receiver();
    let open_ui_id = open_ui.id().clone();
    let open_logs_id = open_logs.id().clone();
    let quit_id = quit.id().clone();

    let logs_dir = roon_ai::config::get_data_dir().join("logs");
    let mut shutdown_tx_opt = Some(shutdown_tx);

    tracing::info!("System tray icon initialised");

    event_loop.run(move |_event, _, control_flow| {
        // Slow poll: 100 ms between menu-event checks. Negligible CPU.
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));

        while let Ok(menu_event) = menu_channel.try_recv() {
            if menu_event.id == open_ui_id {
                tracing::info!("Tray: Open Web UI");
                let _ = std::process::Command::new("cmd")
                    .args(["/c", "start", "", "http://localhost:8088"])
                    .spawn();
            } else if menu_event.id == open_logs_id {
                tracing::info!("Tray: Open Logs Folder ({})", logs_dir.display());
                let _ = std::process::Command::new("explorer").arg(&logs_dir).spawn();
            } else if menu_event.id == quit_id {
                tracing::info!("Tray: Quit");
                if let Some(tx) = shutdown_tx_opt.take() {
                    if tx.send(()).is_err() {
                        tracing::warn!("Server thread already exited; tray Quit had no receiver");
                    }
                }
                *control_flow = ControlFlow::Exit;
                return;
            }
        }
    });
}

#[cfg(all(windows, feature = "server"))]
fn load_tray_icon() -> anyhow::Result<tray_icon::Icon> {
    let bytes = include_bytes!("../public/hifi-logo.png");
    let img = image::load_from_memory(bytes)?.to_rgba8();
    let (width, height) = img.dimensions();
    let rgba = img.into_raw();
    let icon = tray_icon::Icon::from_rgba(rgba, width, height)?;
    Ok(icon)
}

// ===========================================================================
// Server entry point
// ===========================================================================

#[cfg(feature = "server")]
fn main() -> anyhow::Result<()> {
    // Handle --version and --help before doing anything else.
    // Note: with windows_subsystem = "windows" in release builds, stdout has
    // no console attached, so these flags only show output in debug builds or
    // when launched from a terminal that hasn't detached.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "roon-ai {} ({})",
            env!("ROON_AI_VERSION"),
            env!("ROON_AI_GIT_SHA")
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "roon-ai {} ({})",
            env!("ROON_AI_VERSION"),
            env!("ROON_AI_GIT_SHA")
        );
        println!();
        println!("Natural-language Roon control bridge with AI chat and voice control.");
        println!();
        println!("USAGE:");
        println!("    roon-ai [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("    -h, --help       Print help information");
        println!("    -V, --version    Print version information");
        println!();
        println!("ENVIRONMENT VARIABLES:");
        println!("    PORT             HTTP server port (default: 8088)");
        println!("    CONFIG_DIR       Configuration directory");
        println!("    LOG_LEVEL        Log level (debug, info, warn, error)");
        println!();
        println!("Logs: <data_dir>/logs/roon-ai.log.<DATE>");
        println!("Quit (Windows): right-click the system tray icon → Quit");
        return Ok(());
    }

    // Set up logging — must happen before anything that uses tracing.
    // The guard keeps the non-blocking writer flushing; drop = drain & close.
    let _log_guard = setup_logging()?;

    // Channel: tray "Quit" → server graceful shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn the tokio runtime + axum server on a worker thread.
    // The main thread is reserved for the tray icon's event loop (Windows GUI
    // event loops require the main thread).
    let server_thread = std::thread::Builder::new()
        .name("roon-ai-server".to_string())
        .spawn(move || -> anyhow::Result<()> {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(server::run(shutdown_rx))
        })?;

    // On Windows: run the tray icon event loop on the main thread.
    // run_tray() blocks until the user picks Quit; it then sends on shutdown_tx
    // so the server can shut down gracefully.
    #[cfg(windows)]
    {
        if let Err(e) = run_tray(shutdown_tx) {
            tracing::error!("Tray icon failed: {} — running headless", e);
            // Fall through and wait on the server thread anyway.
            match server_thread.join() {
                Ok(result) => return result,
                Err(_) => return Err(anyhow::anyhow!("Server thread panicked")),
            }
        }
    }

    // On non-Windows: no tray. Just wait for the server to exit (Ctrl+C).
    // The shutdown_tx is dropped here, which means the server's external_shutdown
    // future resolves immediately on Drop — but tokio::oneshot Drop on the
    // sender is fine; the receiver returns Err(_) which our select!/match treats
    // as a no-op. We rely on Ctrl+C / SIGTERM for shutdown on non-Windows.
    #[cfg(not(windows))]
    {
        let _ = shutdown_tx; // intentionally drop; non-Windows uses signals
    }

    // Wait for the server thread to finish (it will, after shutdown completes).
    match server_thread.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow::anyhow!("Server thread panicked")),
    }
}

// WASM entry point (client-side only)
#[cfg(all(not(feature = "server"), target_arch = "wasm32"))]
fn main() {
    use roon_ai::app;
    dioxus::launch(app::App);
}

// Fallback for other configurations
#[cfg(all(not(feature = "server"), not(target_arch = "wasm32")))]
fn main() {
    eprintln!("This binary requires either the 'server' feature or wasm32 target.");
    std::process::exit(1);
}
