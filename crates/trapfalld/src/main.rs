//! # trapfalld — TrapFall daemon
//!
//! Main binary: CLI subcommands + HTTP server + ingest + digest + alerts.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use tracing::info;

use trapfall_core::Store;
use trapfalld::attachment_storage::AttachmentStorage;
use trapfalld::{AppState, Config, DigestTask, WsHub, spawn_alert_engine};

#[derive(Parser, Debug)]
#[command(name = "trapfall", version, about = "TrapFall error capture daemon")]
struct Cli {
    /// Database URL or path. Supports `sqlite:path.db` (default) and
    /// `postgres://...` (requires `postgres` feature).
    ///
    /// Can also be set via `TRAPFALL_DATABASE_URL` env var.
    #[arg(short, long, global = true, env = "TRAPFALL_DATABASE_URL", default_value = "trapfall.db")]
    db: String,

    /// Log level
    #[arg(short = 'L', long, global = true, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the HTTP server (default)
    Serve {
        /// HTTP listen address (default: 0.0.0.0:9090 or TRAPFALL_LISTEN env var)
        #[arg(short, long, env = "TRAPFALL_LISTEN", default_value = "0.0.0.0:9090")]
        listen: String,
    },
    /// List all projects
    ProjectList,
    /// Add a new project
    ProjectAdd {
        /// Project name
        name: String,
        /// Project slug (optional, auto-generated from name)
        #[arg(short, long)]
        slug: Option<String>,
    },
    /// Rotate DSN key for a project
    ProjectRotateDsn {
        /// Project slug
        slug: String,
    },
    /// Set webhook URL for a project
    ProjectSetWebhook {
        /// Project slug
        slug: String,
        /// Webhook URL
        url: String,
    },
    /// Health check (exit 0 if healthy)
    Healthcheck,
    /// Start MCP server (stdio JSON-RPC 2.0)
    Mcp,
    /// Database management (export, import, verify)
    Db {
        #[command(subcommand)]
        db_command: DbCommands,
    },
}

#[derive(Subcommand, Debug)]
enum DbCommands {
    /// Export all data from a database to JSONL format
    Export {
        /// Source database URL (e.g. sqlite:trapfall.db)
        #[arg(long)]
        from: String,
        /// Output JSONL file path
        #[arg(long)]
        to: String,
    },
    /// Import JSONL data into a database
    Import {
        /// Input JSONL file path
        #[arg(long)]
        from: String,
        /// Target database URL (e.g. postgres://...)
        #[arg(long)]
        to: String,
    },
    /// Verify database integrity (row counts)
    Verify {
        /// Database URL to verify
        #[arg(long)]
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init tracing — timestamps in the configured display timezone (UTC by default).
    tracing_subscriber::fmt()
        .with_timer(trapfalld::log_time::LocalTzTimer::from_env())
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .init();

    // For `db` subcommands, the database is specified via --from/--to/--url flags,
    // so we skip the global database init and early-return.
    if let Some(Commands::Db { db_command }) = cli.command {
        return match db_command {
            DbCommands::Export { from, to } => trapfalld::migrate::export_database(&from, &to).await,
            DbCommands::Import { from, to } => trapfalld::migrate::import_database(&from, &to).await,
            DbCommands::Verify { url } => trapfalld::migrate::verify_database(&url).await,
        };
    }

    // Normal commands: open global database.
    // If user didn't explicitly set --db or TRAPFALL_DATABASE_URL, use
    // the standardised data directory (~/.codecora/trapfall/).
    let db_str = if cli.db == "trapfall.db" {
        let dir = Config::default_data_dir();
        Config::ensure_data_dir(&dir).expect("failed to create data directory");
        Config::default_db_path()
    } else {
        cli.db.clone()
    };
    let db_url = trapfall_db::normalise_url(&db_str);
    info!("Opening database: {db_url}");
    let backend = trapfall_db::open_database(&db_url).await?;
    // Run idempotent schema migrations for whichever backend we got back.
    // Dispatch happens inside the backend (trait method) so the daemon does
    // not need to know whether it is talking to SQLite or Postgres.
    backend.run_migrations().await?;
    let store = trapfall_core::Store::new(backend);

    match cli.command.unwrap_or(Commands::Serve { listen: "0.0.0.0:9090".into() }) {
        Commands::Serve { listen } => run_server(store, listen, cli.db.clone()).await,
        Commands::ProjectList => {
            let projects = store.list_projects().await?;
            if projects.is_empty() {
                println!("No projects found.");
            } else {
                let header = format!("{:<36} {:<20} {:<10} {}", "ID", "SLUG", "NAME", "DSN");
                println!("{header}");
                for p in &projects {
                    println!("{} {:<20} {:<10} {}...{}", p.id, p.slug, p.name, &p.dsn[..8], &p.dsn[p.dsn.len() - 4..]);
                }
            }
            Ok(())
        }
        Commands::ProjectAdd { name, slug } => {
            let slug = slug.unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let project = store.create_project(&slug, &name).await?;
            println!("Project created: {} ({})", project.name, project.slug);
            println!("DSN: {}", project.dsn);
            Ok(())
        }
        Commands::ProjectRotateDsn { slug } => {
            let project =
                store.get_project_by_slug(&slug).await?.ok_or_else(|| anyhow::anyhow!("project not found"))?;
            let new_key = store.rotate_dsn(&project.id).await?;
            println!("DSN rotated for {}: {}...{}", slug, &new_key[..8], &new_key[new_key.len() - 4..]);
            Ok(())
        }
        Commands::ProjectSetWebhook { slug, url } => {
            store.set_project_webhook(&slug, &url).await?;
            println!("Webhook set for {slug}: {url}");
            Ok(())
        }
        Commands::Healthcheck => {
            let ok = store.backend().ping().await?;
            if ok {
                println!("Healthy");
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
        Commands::Mcp => trapfall_mcp::run_server(store).await,
        Commands::Db { .. } => unreachable!("db commands handled via early return"),
    }
}

async fn run_server(store: Store, listen: String, db_url: String) -> Result<()> {
    info!("TrapFall daemon starting");

    let config = Config::from_env(&db_url, &listen);
    info!(
        "Config: db={}, listen={}, secure_cookie={}, cors_origins={}, public_url={}, timezone={}",
        config.db_path.display(),
        config.listen_addr,
        config.secure_cookie,
        if config.cors_origins.is_empty() { "<all>" } else { "<restricted>" },
        config.dsn_host().unwrap_or_else(|| "<unset, will use Host header>".into()),
        config.timezone,
    );

    // Channel: ingest → digest
    let (ingest_tx, ingest_rx) = mpsc::channel::<trapfall_proto::IngestEvent>(1024);

    // WebSocket hub
    let ws_hub = WsHub::new(256);
    let (ws_broadcast_tx, mut ws_broadcast_rx) = mpsc::unbounded_channel::<trapfall_proto::ServerMessage>();

    // Alert engine
    let alert_tx = spawn_alert_engine(store.clone(), 256);

    // Digest task
    let digest = DigestTask::new(store.clone(), ingest_rx).with_ws_sender(ws_broadcast_tx).with_alert_sender(alert_tx);
    let digest_handle = tokio::spawn(async move {
        if let Err(e) = digest.run().await {
            tracing::error!("Digest task failed: {e}");
        }
    });

    // WS bridge
    let hub_clone = ws_hub.clone();
    let bridge_handle = tokio::spawn(async move {
        while let Some(msg) = ws_broadcast_rx.recv().await {
            hub_clone.send(msg);
        }
    });

    // Retention task — configurable via TRAPFALL_RETENTION_DAYS
    let retention_handle = {
        let store_clone = store.clone();
        let days = config.retention_days;
        tokio::spawn(async move { trapfalld::retention::run_retention(&store_clone, Some(days)).await })
    };

    // App state
    let storage = AttachmentStorage::new(None);
    let state = AppState {
        store,
        config,
        ingest_tx,
        rate_limiter: trapfalld::rate_limit::RateLimiter::default(),
        ws_hub,
        storage: std::sync::Arc::new(storage),
    };

    // HTTP server
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    info!("Listening on {listen}");
    // Serving the router consumes `state` (and with it the only `ingest_tx`),
    // so once graceful shutdown completes the digest channel is closed and the
    // digest task drains its queue and flushes the final batch before exiting.
    // Open connections (e.g. long-lived WebSockets) can stall graceful shutdown,
    // so give them a bounded grace period and then drop the server regardless.
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    let stop_rx = stop.clone();
    let serve = axum::serve(listener, trapfalld::server::router(state)).with_graceful_shutdown(async move {
        stop_rx.notified().await;
    });
    let mut server = Box::pin(std::future::IntoFuture::into_future(serve));
    tokio::select! {
        res = &mut server => {
            if let Err(e) = res {
                tracing::error!("HTTP server error: {e}");
            }
        }
        _ = async {
            shutdown_signal().await;
            stop.notify_one();
            tokio::time::sleep(HTTP_GRACE_TIMEOUT).await;
        } => tracing::warn!("HTTP graceful shutdown timed out after {HTTP_GRACE_TIMEOUT:?}; dropping open connections"),
    }
    // Drop the server (and the router state holding `ingest_tx`) so the digest channel closes.
    drop(server);

    info!("HTTP server stopped, draining digest pipeline");
    retention_handle.abort();
    match tokio::time::timeout(DIGEST_DRAIN_TIMEOUT, digest_handle).await {
        Ok(Ok(())) => info!("Digest pipeline drained"),
        Ok(Err(e)) => tracing::error!("Digest task join error: {e}"),
        Err(_) => {
            tracing::error!("Digest drain timed out after {DIGEST_DRAIN_TIMEOUT:?}; remaining events may be lost")
        }
    }
    // The digest task owned the ws sender; once it exits the bridge ends on its own.
    bridge_handle.abort();
    Ok(())
}

/// Upper bound on how long shutdown waits for open HTTP connections to finish.
const HTTP_GRACE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Upper bound on how long shutdown waits for the digest task to flush.
const DIGEST_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Resolves when SIGINT (Ctrl-C) or SIGTERM is received.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl-C handler: {e}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("Shutdown signal received");
}
