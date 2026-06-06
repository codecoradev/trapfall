//! # trapfalld — TrapFall daemon
//!
//! Main binary that runs the HTTP server, ingest pipeline, digest loop,
//! and serves the embedded SPA dashboard.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;

mod config;
mod digest;
mod metrics;
mod rate_limit;
mod retention;
mod server;
mod spa;

use config::Config;
use digest::DigestTask;
use server::AppState;

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

    // Start digest task
    let digest = DigestTask::new(pool.clone(), ingest_rx);
    let digest_handle = tokio::spawn(async move {
        if let Err(e) = digest.run().await {
            tracing::error!("Digest task failed: {e}");
        }
    });

    // Start retention task
    let retention_pool = pool.clone();
    let retention_handle = tokio::spawn(async move {
        retention::run_retention(retention_pool, None).await;
    });

    // Build app state
    let state = AppState {
        pool: pool.clone(),
        config,
        ingest_tx,
        rate_limiter: rate_limit::RateLimiter::default(),
    };

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(&cli.listen).await?;
    info!("Listening on {}", cli.listen);
    axum::serve(listener, server::router(state)).await?;

    digest_handle.abort();
    retention_handle.abort();
    Ok(())
}
