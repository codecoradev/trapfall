//! # trapfalld — TrapFall daemon
//!
//! Main binary: CLI subcommands + HTTP server + ingest + digest + alerts.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;

use trapfalld::{AppState, Config, DigestTask, WsHub, spawn_alert_engine};

#[derive(Parser, Debug)]
#[command(name = "trapfall", version, about = "TrapFall error capture daemon")]
struct Cli {
    /// Database path (SQLite)
    #[arg(short, long, global = true, default_value = "trapfall.db")]
    db: PathBuf,

    /// Log level
    #[arg(short, long, global = true, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the HTTP server (default)
    Serve {
        /// HTTP listen address
        #[arg(short, long, default_value = "0.0.0.0:9090")]
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .init();

    let pool = trapfall_core::open_pool(&format!("sqlite:{}", cli.db.display())).await?;
    trapfall_core::run_migrations(&pool).await?;

    match cli.command.unwrap_or(Commands::Serve { listen: "0.0.0.0:9090".into() }) {
        Commands::Serve { listen } => run_server(pool, listen).await,
        Commands::ProjectList => {
            let store = trapfall_core::Store::new(pool);
            let projects = store.list_projects().await?;
            if projects.is_empty() {
                println!("No projects found.");
            } else {
                println!("{:<36} {:<20} {:<10} {}", "ID", "SLUG", "NAME", "DSN");
                for p in &projects {
                    println!("{} {:<20} {:<10} {}...{}", p.id, p.slug, p.name, &p.dsn[..8], &p.dsn[p.dsn.len() - 4..]);
                }
            }
            Ok(())
        }
        Commands::ProjectAdd { name, slug } => {
            let store = trapfall_core::Store::new(pool);
            let slug = slug.unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let project = store.create_project(&slug, &name).await?;
            println!("Project created: {} ({})", project.name, project.slug);
            println!("DSN: {}", project.dsn);
            Ok(())
        }
        Commands::ProjectRotateDsn { slug } => {
            let store = trapfall_core::Store::new(pool);
            let project =
                store.get_project_by_slug(&slug).await?.ok_or_else(|| anyhow::anyhow!("project not found"))?;
            let new_key = store.rotate_dsn(&project.id).await?;
            println!("DSN rotated for {}: {}...{}", slug, &new_key[..8], &new_key[new_key.len() - 4..]);
            Ok(())
        }
        Commands::ProjectSetWebhook { slug, url } => {
            let pool_clone = pool.clone();
            sqlx::query("UPDATE projects SET webhook_url = ? WHERE slug = ?")
                .bind(&url)
                .bind(&slug)
                .execute(&pool_clone)
                .await?;
            println!("Webhook set for {slug}: {url}");
            Ok(())
        }
        Commands::Healthcheck => {
            let ok: i64 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await?;
            if ok == 1 {
                println!("Healthy");
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
        Commands::Mcp => trapfall_mcp::run_server(pool).await,
    }
}

async fn run_server(pool: sqlx::SqlitePool, listen: String) -> Result<()> {
    info!("TrapFall daemon starting");

    let config = Config { db_path: std::path::PathBuf::from("trapfall.db"), listen_addr: listen.clone() };

    // Channel: ingest → digest
    let (ingest_tx, ingest_rx) = mpsc::channel::<trapfall_proto::IngestEvent>(1024);

    // WebSocket hub
    let ws_hub = WsHub::new(256);
    let (ws_broadcast_tx, mut ws_broadcast_rx) = mpsc::unbounded_channel::<trapfall_proto::ServerMessage>();

    // Alert engine
    let alert_tx = spawn_alert_engine(pool.clone(), 256);

    // Digest task
    let digest = DigestTask::new(pool.clone(), ingest_rx).with_ws_sender(ws_broadcast_tx).with_alert_sender(alert_tx);
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

    // Retention task
    let retention_pool = pool.clone();
    let retention_handle = tokio::spawn(async move { trapfalld::retention::run_retention(retention_pool, None).await });

    // App state
    let state = AppState {
        pool: pool.clone(),
        config,
        ingest_tx,
        rate_limiter: trapfalld::rate_limit::RateLimiter::default(),
        ws_hub,
    };

    // HTTP server
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    info!("Listening on {listen}");
    axum::serve(listener, trapfalld::server::router(state)).await?;

    digest_handle.abort();
    retention_handle.abort();
    bridge_handle.abort();
    Ok(())
}
