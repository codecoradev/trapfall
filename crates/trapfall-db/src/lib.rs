//! # trapfall-db
//!
//! Database trait abstraction layer for TrapFall.
//!
//! Provides a generic [`Database`] trait that isolates backend-specific
//! SQL from business logic. Currently only SQLite is implemented; Postgres
//! will land in Phase 3 (#168).
//!
//! See epic #171 for the full multi-backend roadmap.

pub mod error;
pub mod sqlite;

pub use error::DbError;
pub use sqlite::SqliteBackend;

use anyhow::Result;
use async_trait::async_trait;
use trapfall_proto::{
    AlertRule, Issue, IssueStatus, Level, Project, StoredEvent,
};

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

    // ── Backend-specific escape hatch ──────────────────────────────────

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
