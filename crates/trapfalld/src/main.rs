//! # trapfalld — TrapFall daemon
//!
//! Main binary: CLI subcommands + HTTP server + ingest + digest + alerts.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::sync::mpsc;
use tracing::info;

use trapfall_core::Store;
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

    // Init tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .init();

    // Resolve database URL: --db flag or TRAPFALL_DATABASE_URL env var.
    // Bare paths default to SQLite (e.g. "trapfall.db" -> "sqlite:trapfall.db").
    let db_url = trapfall_db::normalise_url(&cli.db);
    info!("Opening database: {db_url}");

    let backend = trapfall_db::open_database(&db_url).await?;
    {
        let pool = backend.sqlite_pool()?;
        trapfall_db::run_sqlite_migrations(pool).await?;
    }
    let store = trapfall_core::Store::new(backend);

    match cli.command.unwrap_or(Commands::Serve { listen: "0.0.0.0:9090".into() }) {
        Commands::Serve { listen } => run_server(store, listen).await,
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
        Commands::Db { db_command } => match db_command {
            DbCommands::Export { from, to } => trapfalld::migrate::export_database(&from, &to).await,
            DbCommands::Import { from, to } => trapfalld::migrate::import_database(&from, &to).await,
            DbCommands::Verify { url } => trapfalld::migrate::verify_database(&url).await,
        },
    }
}

async fn run_server(store: Store, listen: String) -> Result<()> {
    info!("TrapFall daemon starting");

    let secure_cookie =
        std::env::var("TRAPFALL_SECURE_COOKIE").map(|v| v.to_lowercase() != "false" && v != "0").unwrap_or(true);
    let cors_origins = std::env::var("TRAPFALL_CORS_ORIGINS")
        .map(|s| s.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect())
        .unwrap_or_default();
    let config = Config {
        db_path: std::path::PathBuf::from("trapfall.db"),
        listen_addr: listen.clone(),
        cors_origins,
        secure_cookie,
    };

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

    // Retention task
    let retention_handle = {
        let store_clone = store.clone();
        tokio::spawn(async move { trapfalld::retention::run_retention(&store_clone, None).await })
    };

    // App state
    let state =
        AppState { store, config, ingest_tx, rate_limiter: trapfalld::rate_limit::RateLimiter::default(), ws_hub };

    // HTTP server
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    info!("Listening on {listen}");
    axum::serve(listener, trapfalld::server::router(state)).await?;

    digest_handle.abort();
    retention_handle.abort();
    bridge_handle.abort();
    Ok(())
}
