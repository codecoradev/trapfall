//! # trapfalld — TrapFall daemon
//!
//! Main binary that runs the HTTP server, ingest pipeline, digest loop,
//! and serves the embedded SPA dashboard.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;

use trapfalld::{AppState, Config, DigestTask, WsHub};

#[derive(Parser, Debug)]
#[command(name = "trapfall", version, about = "TrapFall error capture daemon")]
struct Cli {
    /// Database path (SQLite)
    #[arg(short, long, default_value = "trapfall.db")]
    db: PathBuf,

    /// HTTP listen address
    #[arg(short, long, default_value = "0.0.0.0:9090")]
    listen: String,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .init();

    info!("TrapFall daemon starting");

    // Load config
    let config = Config { db_path: cli.db.clone(), listen_addr: cli.listen.clone() };

    // Open database
    let pool = trapfall_core::open_pool(&format!("sqlite:{}", cli.db.display())).await?;
    trapfall_core::run_migrations(&pool).await?;
    info!("Database ready: {}", cli.db.display());

    // Channel: ingest → digest
    let (ingest_tx, ingest_rx) = mpsc::channel::<trapfall_proto::IngestEvent>(1024);

    // WebSocket hub for real-time updates
    let ws_hub = WsHub::new(256);
    let (ws_broadcast_tx, mut ws_broadcast_rx) = mpsc::unbounded_channel::<trapfall_proto::ServerMessage>();

    // Start digest task with WS notifications
    let digest = DigestTask::new(pool.clone(), ingest_rx).with_ws_sender(ws_broadcast_tx);
    let digest_handle = tokio::spawn(async move {
        if let Err(e) = digest.run().await {
            tracing::error!("Digest task failed: {e}");
        }
    });

    // Bridge: mpsc from digest → broadcast to WS clients
    let hub_clone = ws_hub.clone();
    let bridge_handle = tokio::spawn(async move {
        while let Some(msg) = ws_broadcast_rx.recv().await {
            hub_clone.send(msg);
        }
    });

    // Start retention task
    let retention_pool = pool.clone();
    let retention_handle = tokio::spawn(async move {
        trapfalld::retention::run_retention(retention_pool, None).await;
    });

    // Build app state
    let state = AppState {
        pool: pool.clone(),
        config,
        ingest_tx,
        rate_limiter: trapfalld::rate_limit::RateLimiter::default(),
        ws_hub,
    };

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    info!("Listening on {}", cli.listen);
    axum::serve(listener, trapfalld::server::router(state)).await?;

    digest_handle.abort();
    retention_handle.abort();
    bridge_handle.abort();
    Ok(())
}
