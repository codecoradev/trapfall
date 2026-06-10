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
    Ok(())
}

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a DSN with the given public base URL.
/// Format: `https://{key}@{host}/{project_id}`
pub fn generate_dsn_with(host: &str) -> String {
    let key = Uuid::new_v4();
    format!("https://{key}@{host}/1")
}

/// Generate a DSN with placeholder host.
/// When creating projects via CLI (no request context), we use a generic DSN.
pub fn generate_dsn() -> String {
    generate_dsn_with("localhost:3000")
}
