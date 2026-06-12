//! # trapfall-core
//!
//! Core logic — storage trait, config, auth, fingerprint.

pub mod auth;
pub mod fingerprint;
pub mod store;

pub use auth::{UserInfo, hash_password, verify_password};
pub use fingerprint::derive_fingerprint;
pub use store::Store;

use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::str::FromStr;
use uuid::Uuid;

/// Open a SQLite connection pool with WAL mode.
pub async fn open_pool(db_path: &str) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(db_path)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;

    Ok(pool)
}

/// Run all database migrations.
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::query(include_str!("../../trapfalld/migrations/20260606000001_initial.sql")).execute(pool).await?;
    sqlx::query(include_str!("../../trapfalld/migrations/20260606000002_alert_rules.sql")).execute(pool).await?;
    sqlx::query(include_str!("../../trapfalld/migrations/20260608000001_drop_api_keys.sql")).execute(pool).await?;
    // Add archived_at column to projects (idempotent)
    // SQLite doesn't support IF NOT EXISTS for ALTER TABLE,
    // so we check if the column exists first via pragma_table_info.
    let has_archived_at: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM pragma_table_info('projects') WHERE name = 'archived_at'")
            .fetch_one(pool)
            .await
            .unwrap_or(false);
    if !has_archived_at {
        sqlx::query("ALTER TABLE projects ADD COLUMN archived_at TEXT DEFAULT NULL").execute(pool).await?;
    }
    Ok(())
}

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a DSN with the given public base URL and project ID.
/// Format: `https://{key}@{host}/{project_id}`
pub fn generate_dsn_with(host: &str, project_id: &str) -> String {
    let key = Uuid::new_v4();
    format!("https://{key}@{host}/{project_id}")
}

/// Generate a DSN with placeholder host.
/// When creating projects via CLI (no request context), we use a generic DSN.
/// Note: project_id is set to "1" as placeholder — should be regenerated with real project ID.
pub fn generate_dsn() -> String {
    generate_dsn_with("localhost:3000", "1")
}
