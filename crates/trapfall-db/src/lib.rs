//! # trapfall-db
//!
//! Database trait abstraction layer for TrapFall.
//!
//! Provides a generic [`Database`] trait that isolates backend-specific
//! SQL from business logic. Currently only SQLite is implemented; Postgres
//! will land in Phase 3 (#168).
//!
//! ## Connection factory
//!
//! Use [`open_database`] to create a backend from a connection URL:
//!
//! ```text
//! sqlite:./trapfall.db       → SqliteBackend
//! postgres://user@host/db    → PostgresBackend (requires `postgres` feature)
//! ```
//!
//! See epic #171 for the full multi-backend roadmap.

pub mod common;
pub mod error;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "postgres")]
pub mod postgres;

pub use error::DbError;

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteBackend;

#[cfg(feature = "postgres")]
pub use postgres::PostgresBackend;

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use trapfall_proto::{AlertRule, Issue, IssueStatus, Level, Project, StoredEvent};

// ── Connection factory ────────────────────────────────────────────────

/// Open a database connection from a URL scheme.
///
/// Detects the URL scheme and instantiates the appropriate backend:
///
/// | Scheme | Backend | Feature flag |
/// |--------|---------|--------------|
/// | `sqlite:` | [`SqliteBackend`] | `sqlite` (default) |
/// | `postgres:` / `postgresql:` | `PostgresBackend` | `postgres` (optional) |
///
/// # Errors
///
/// - `DbError::Backend` if the scheme is not recognised.
/// - `DbError::Backend` if `postgres:` is used but the `postgres` Cargo
///   feature is not enabled.
pub async fn open_database(url: &str) -> Result<Arc<dyn Database>> {
    let lower = url.to_ascii_lowercase();

    if lower.starts_with("sqlite:") {
        #[cfg(feature = "sqlite")]
        {
            let pool = open_sqlite_pool(url).await?;
            return Ok(Arc::new(SqliteBackend::new(pool)));
        }
        #[cfg(not(feature = "sqlite"))]
        {
            return Err(DbError::Backend("sqlite: URL given but `sqlite` feature is not enabled".into()).into());
        }
    }

    if lower.starts_with("postgres:") || lower.starts_with("postgresql:") {
        #[cfg(feature = "postgres")]
        {
            let pool = open_postgres_pool(url).await?;
            return Ok(Arc::new(PostgresBackend::new(pool)));
        }
        #[cfg(not(feature = "postgres"))]
        {
            return Err(DbError::Backend(
                "postgres: URL given but `postgres` Cargo feature is not enabled. \
                 Build with `--features postgres` to enable."
                    .into(),
            )
            .into());
        }
    }

    Err(DbError::Backend(format!(
        "unrecognised database URL scheme: {url:?} \
         (expected `sqlite:` or `postgres:`)"
    ))
    .into())
}

/// Resolve a database URL into a connection string suitable for
/// [`open_database`].
///
/// Accepts both bare paths (`./trapfall.db`) and scheme-prefixed URLs
/// (`sqlite:./trapfall.db`). Bare paths default to SQLite.
pub fn normalise_url(url: &str) -> String {
    if url.contains(':') && !url.starts_with('.') && !url.starts_with('/') {
        url.to_string()
    } else {
        format!("sqlite:{url}")
    }
}

// ── SQLite pool helper ────────────────────────────────────────────────

/// Open a SQLite connection pool with WAL mode (mirrors `trapfall_core::open_pool`).
#[cfg(feature = "sqlite")]
async fn open_sqlite_pool(url: &str) -> Result<sqlx::SqlitePool> {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
    use std::str::FromStr;

    let options = SqliteConnectOptions::from_str(url)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new().max_connections(4).connect_with(options).await?;
    Ok(pool)
}

/// Run all SQLite database migrations (schema setup).
///
/// Idempotent — safe to call on every startup.
#[cfg(feature = "sqlite")]
pub async fn run_sqlite_migrations(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(include_str!("../../trapfalld/migrations/20260606000001_initial.sql")).execute(pool).await?;
    sqlx::query(include_str!("../../trapfalld/migrations/20260606000002_alert_rules.sql")).execute(pool).await?;
    sqlx::query(include_str!("../../trapfalld/migrations/20260608000001_drop_api_keys.sql")).execute(pool).await?;
    // Add archived_at column to projects (idempotent).
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
    sqlx::query(include_str!("../../trapfalld/migrations/20260613000001_transactions.sql")).execute(pool).await?;
    sqlx::query(include_str!("../../trapfalld/migrations/20260627000001_release_health.sql")).execute(pool).await?;
    Ok(())
}

// ── Postgres pool + migrations ────────────────────────────────────────

/// Open a Postgres connection pool.
#[cfg(feature = "postgres")]
async fn open_postgres_pool(url: &str) -> Result<sqlx::PgPool> {
    use sqlx::postgres::PgPoolOptions;
    let pool = PgPoolOptions::new().max_connections(8).connect(url).await?;
    Ok(pool)
}

/// Run all Postgres database migrations (schema setup).
///
/// Idempotent — uses `CREATE TABLE IF NOT EXISTS`.
#[cfg(feature = "postgres")]
pub async fn run_postgres_migrations(pool: &sqlx::PgPool) -> Result<()> {
    sqlx::query(include_str!("../migrations/postgres/001_initial.sql")).execute(pool).await?;
    sqlx::query(include_str!("../migrations/postgres/002_alert_rules.sql")).execute(pool).await?;
    sqlx::query(include_str!("../migrations/postgres/003_transactions.sql")).execute(pool).await?;
    sqlx::query(include_str!("../migrations/postgres/004_release_health.sql")).execute(pool).await?;
    Ok(())
}

/// Generic database abstraction covering all storage operations used by TrapFall.
///
/// Every method maps 1:1 to an existing `Store` method — this is a mechanical
/// extraction, not a redesign. Implementations are free to use any backend
/// (SQLite, Postgres, …) as long as semantics match.
///
/// All methods are async and return `anyhow::Result<T>`.
#[async_trait]
pub trait Database: Send + Sync {
    // ── Projects ────────────────────────────────────────────────────────

    async fn create_project(&self, slug: &str, name: &str) -> Result<Project>;
    async fn create_project_with_host(&self, slug: &str, name: &str, host: &str) -> Result<Project>;
    async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>>;
    async fn get_project_by_id(&self, id: &str) -> Result<Option<Project>>;
    async fn get_project_by_dsn_key(&self, sentry_key: &str) -> Result<Option<Project>>;
    async fn list_projects(&self) -> Result<Vec<Project>>;
    async fn rotate_dsn(&self, project_id: &str) -> Result<String>;
    async fn archive_project(&self, project_id: &str) -> Result<()>;
    async fn unarchive_project(&self, project_id: &str) -> Result<()>;
    async fn delete_project(&self, project_id: &str) -> Result<bool>;
    async fn update_project(&self, project_id: &str, name: &str) -> Result<Project>;
    async fn set_project_webhook(&self, project_slug: &str, webhook_url: &str) -> Result<()>;

    // ── Issues ──────────────────────────────────────────────────────────

    async fn upsert_issue(
        &self,
        project_id: &str,
        fingerprint: &str,
        title: &str,
        culprit: Option<&str>,
        level: Level,
    ) -> Result<Issue>;
    async fn get_issue(&self, issue_id: &str) -> Result<Option<Issue>>;
    async fn list_issues(&self, project_id: &str, limit: i64, offset: i64) -> Result<Vec<Issue>>;
    async fn list_issues_filtered(
        &self,
        project_id: &str,
        status: Option<&str>,
        level: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Issue>>;
    async fn count_issues(&self, project_id: &str, status: Option<&str>, level: Option<&str>) -> Result<i64>;
    async fn set_issue_status(&self, issue_id: &str, status: IssueStatus) -> Result<()>;

    // ── Events ──────────────────────────────────────────────────────────

    async fn insert_event(&self, issue_id: &str, project_id: &str, event_data: &str) -> Result<String>;
    async fn list_events(&self, issue_id: &str, limit: i64, offset: i64) -> Result<Vec<StoredEvent>>;
    async fn count_events(&self, issue_id: &str) -> Result<i64>;

    // ── Transactions ──────────────────────────────────────────────────────

    async fn insert_transaction(&self, project_id: &str, transaction: &trapfall_proto::Transaction) -> Result<String>;
    async fn list_transactions(
        &self,
        project_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::common::TransactionRow>>;
    async fn get_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<Option<(crate::common::TransactionRow, Vec<crate::common::SpanRow>)>>;
    async fn count_transactions(&self, project_id: &str) -> Result<i64>;

    // ── Release Health ────────────────────────────────────────────────

    async fn insert_release_health(
        &self,
        project_id: &str,
        aggregates: &trapfall_proto::SessionAggregates,
    ) -> Result<usize>;
    async fn get_crash_rate(&self, project_id: &str, release: Option<&str>, env: Option<&str>) -> Result<Option<f64>>;
    async fn list_release_health(
        &self,
        project_id: &str,
        release: Option<&str>,
        env: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::common::ReleaseHealthRow>>;
    async fn count_release_health(&self, project_id: &str, release: Option<&str>, env: Option<&str>) -> Result<i64>;

    // ── Alert Rules ────────────────────────────────────────────────────

    async fn create_alert_rule(
        &self,
        project_id: &str,
        name: &str,
        conditions: &str,
        action_type: &str,
        action_config: &str,
        cooldown_seconds: i64,
    ) -> Result<AlertRule>;
    async fn list_alert_rules(&self, project_id: &str) -> Result<Vec<AlertRule>>;
    async fn get_alert_rule(&self, rule_id: &str) -> Result<Option<AlertRule>>;
    async fn delete_alert_rule(&self, rule_id: &str) -> Result<bool>;
    async fn toggle_alert_rule(&self, rule_id: &str, enabled: bool) -> Result<()>;
    async fn get_enabled_rules_for_project(&self, project_id: &str) -> Result<Vec<AlertRule>>;

    // ── Alert History ───────────────────────────────────────────────────

    async fn insert_alert_history(&self, rule_id: &str, project_id: &str, issue_id: &str) -> Result<String>;
    async fn mark_alert_sent(&self, history_id: &str) -> Result<()>;
    async fn mark_alert_failed(&self, history_id: &str, error: &str) -> Result<()>;

    // ── Search ─────────────────────────────────────────────────────────

    async fn search_issues(
        &self,
        query: &str,
        project_id: Option<&str>,
        status: Option<&str>,
        level: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Issue>>;
    async fn count_search_issues(
        &self,
        query: &str,
        project_id: Option<&str>,
        status: Option<&str>,
        level: Option<&str>,
    ) -> Result<i64>;

    // ── Metrics ────────────────────────────────────────────────────────

    /// Count rows in a whitelisted table (`issues`, `events`, `projects`,
    /// `alert_rules`, `alert_history`). Returns 0 for unknown tables.
    async fn count_table(&self, table: &str) -> Result<i64>;

    // ── Retention ──────────────────────────────────────────────────────

    /// Delete events older than `days` days. Returns count of deleted rows.
    async fn purge_old_events(&self, days: i64) -> Result<u64>;
    /// Delete orphaned issues (no remaining events). Best-effort.
    async fn purge_orphan_issues(&self) -> Result<()>;
    /// Delete auth attempts older than 30 days. Best-effort.
    async fn purge_stale_auth_attempts(&self) -> Result<()>;

    // ── Alert Cooldown ─────────────────────────────────────────────────

    /// Returns `true` if `rule_id` fired within the last `cooldown_seconds`.
    async fn is_rule_cooling_down(&self, rule_id: &str, cooldown_seconds: i64) -> Result<bool>;

    // ── Health ─────────────────────────────────────────────────────────

    /// Returns `true` if the backend answers a trivial query.
    async fn ping(&self) -> Result<bool>;

    // ── Auth ───────────────────────────────────────────────────────────

    async fn has_users(&self) -> Result<bool>;
    async fn create_user(&self, email: &str, name: &str, password_hash: &str) -> Result<()>;
    async fn get_user_by_email(&self, email: &str) -> Result<Option<StoredUser>>;
    async fn get_user_by_id(&self, id: &str) -> Result<Option<StoredUser>>;
    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<()>;
    async fn create_session(&self, user_id: &str, token: &str, expires_at: &str) -> Result<()>;
    async fn get_session(&self, token: &str) -> Result<Option<StoredSession>>;
    async fn delete_session(&self, token: &str) -> Result<()>;
    async fn cleanup_expired_sessions(&self) -> Result<u64>;
    async fn record_auth_attempt(&self, email: &str, ip: &str, success: bool) -> Result<()>;
    async fn count_failed_attempts_email(&self, email: &str, minutes: i64) -> Result<i64>;
    async fn count_failed_attempts_ip(&self, ip: &str, minutes: i64) -> Result<i64>;

    // ── Raw event fetch (MCP tool_get_event) ───────────────────────────

    /// Fetch a single event row by id, returning (id, issue_id, project_id,
    /// data_json, received_at). Used by MCP `get_event`.
    async fn get_event_raw(&self, event_id: &str) -> Result<Option<StoredEvent>>;

    // ── SQL helpers (non-trait) ───────────────────────────────────────

    /// Return a backend-specific opaque pool reference for consumers that
    /// still issue raw SQL (search, mcp, retention, metrics).
    ///
    /// Phase 1 keeps these consumers on raw `SqlitePool`; Phase 3 will
    /// route them through trait methods entirely.
    ///
    /// Returns the concrete `SqlitePool` for SQLite backends. Postgres
    /// backends should return an error — they are not expected to satisfy
    /// SQLite-only callers.
    fn sqlite_pool(&self) -> Result<&sqlx::SqlitePool>;

    /// Run all idempotent schema migrations for this backend.
    ///
    /// Dispatch is backend-specific — SQLite runs the embedded SQLite
    /// migrations, Postgres runs the Postgres ones. Callers (e.g. the daemon
    /// on startup) no longer need to know which backend they got back from
    /// [`open_database`].
    async fn run_migrations(&self) -> Result<()>;
}

// ── Auxiliary row types ────────────────────────────────────────────────

/// User row without auth-adjacent metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
}

/// Session row as stored in the backend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredSession {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub expires_at: String,
    pub created_at: String,
}
