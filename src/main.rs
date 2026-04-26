//! Unified Hi-Fi Control - Rust Implementation
//!
//! A natural-language Roon control bridge with AI chat and voice control.

// Server-only: full server implementation
#[cfg(feature = "server")]
mod server {
    use unified_hifi_control::{
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
    use tokio_util::sync::CancellationToken;
    use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    /// Legacy redirect: /control -> /ui/zones
    async fn control_redirect() -> impl IntoResponse {
        Redirect::to("/ui/zones")
    }

    /// Legacy redirect: /admin -> /settings
    async fn settings_redirect() -> impl IntoResponse {
        Redirect::to("/settings")
    }

    pub async fn run() -> Result<()> {
        // Initialize logging
        // Priority: RUST_LOG > LOG_LEVEL (legacy) > default
        let log_filter = std::env::var("RUST_LOG")
            .or_else(|_| std::env::var("LOG_LEVEL"))
            .unwrap_or_else(|_| "unified_hifi_control=debug,tower_http=debug,roon_api=info".into());

        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(&log_filter))
            .with(tracing_subscriber::fmt::layer())
            .init();

        tracing::info!(
            "Starting Roon AI v{} ({})",
            env!("UHC_VERSION"),
            env!("UHC_GIT_SHA")
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

        // Issue #76: Migrate config files to unified-hifi/ subdirectory
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

        // Create shutdown future that cancels token before graceful shutdown (fixes #73)
        let graceful_shutdown = {
            let token = shutdown_token.clone();
            let state = state_for_shutdown.clone();
            async move {
                shutdown_signal().await;

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

    /// Wait for shutdown signal (Ctrl+C or SIGTERM)
    #[allow(clippy::expect_used)] // Signal handlers must succeed for graceful shutdown
    async fn shutdown_signal() {
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
        }
    }
}

// Server entry point
#[cfg(feature = "server")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Handle --version and --help before starting server
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!(
            "unified-hifi-control {} ({})",
            env!("UHC_VERSION"),
            env!("UHC_GIT_SHA")
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "unified-hifi-control {} ({})",
            env!("UHC_VERSION"),
            env!("UHC_GIT_SHA")
        );
        println!();
        println!(
            "Natural-language Roon control bridge with AI chat and voice control."
        );
        println!();
        println!("USAGE:");
        println!("    unified-hifi-control [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("    -h, --help       Print help information");
        println!("    -V, --version    Print version information");
        println!();
        println!("ENVIRONMENT VARIABLES:");
        println!("    PORT             HTTP server port (default: 8088)");
        println!("    CONFIG_DIR       Configuration directory");
        println!("    LOG_LEVEL        Log level (debug, info, warn, error)");
        return Ok(());
    }

    server::run().await
}

// WASM entry point (client-side only)
#[cfg(all(not(feature = "server"), target_arch = "wasm32"))]
fn main() {
    use unified_hifi_control::app;
    dioxus::launch(app::App);
}

// Fallback for other configurations
#[cfg(all(not(feature = "server"), not(target_arch = "wasm32")))]
fn main() {
    eprintln!("This binary requires either the 'server' feature or wasm32 target.");
    std::process::exit(1);
}
