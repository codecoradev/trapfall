//! Shared types and helpers used by both SQLite and Postgres backends.
//!
//! Extracted to avoid duplication (~300 lines shared between backends).
//! Contains: row types for sqlx mapping, domain conversions, and
//! pure-logic helpers (ID generation, DSN, level/status mapping).

use trapfall_proto::{AlertRule, Issue, IssueStatus, Level, Project, StoredEvent};
use uuid::Uuid;

// ── Row types (sqlx mapping — identical for SQLite and Postgres) ──────

#[derive(sqlx::FromRow)]
pub struct ProjectRow {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub dsn: String,
    pub created_at: String,
    pub archived_at: Option<String>,
}

impl From<ProjectRow> for Project {
    fn from(r: ProjectRow) -> Self {
        Self { id: r.id, slug: r.slug, name: r.name, dsn: r.dsn, created_at: r.created_at, archived_at: r.archived_at }
    }
}

#[derive(sqlx::FromRow)]
pub struct IssueRow {
    pub id: String,
    pub project_id: String,
    pub fingerprint: String,
    pub title: String,
    pub culprit: Option<String>,
    pub status: String,
    pub level: String,
    pub count: i64,
    pub user_count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

impl From<IssueRow> for Issue {
    fn from(r: IssueRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            fingerprint: r.fingerprint,
            title: r.title,
            culprit: r.culprit,
            status: str_to_status(&r.status),
            level: str_to_level(&r.level),
            count: r.count,
            user_count: r.user_count,
            first_seen: r.first_seen,
            last_seen: r.last_seen,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct EventRow {
    pub id: String,
    pub issue_id: String,
    pub project_id: String,
    pub data: String,
    pub received_at: String,
}

impl From<EventRow> for StoredEvent {
    fn from(r: EventRow) -> Self {
        let data = serde_json::from_str(&r.data).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse event data JSON: {e}");
            serde_json::Value::Null
        });
        Self { id: r.id, issue_id: r.issue_id, project_id: r.project_id, data, received_at: r.received_at }
    }
}

#[derive(sqlx::FromRow)]
pub struct AlertRuleRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub enabled: bool,
    pub conditions: String,
    pub action_type: String,
    pub action_config: String,
    pub cooldown_seconds: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl From<AlertRuleRow> for AlertRule {
    fn from(r: AlertRuleRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id,
            name: r.name,
            enabled: r.enabled,
            conditions: serde_json::from_str(&r.conditions).unwrap_or(serde_json::Value::Object(Default::default())),
            action_type: r.action_type,
            action_config: serde_json::from_str(&r.action_config)
                .unwrap_or(serde_json::Value::Object(Default::default())),
            cooldown_seconds: r.cooldown_seconds,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
}

impl From<UserRow> for crate::StoredUser {
    fn from(r: UserRow) -> Self {
        Self {
            id: r.id,
            email: r.email,
            name: r.name,
            password_hash: r.password_hash,
            role: r.role,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct SessionRow {
    pub id: String,
    pub user_id: String,
    pub token: String,
    pub expires_at: String,
    pub created_at: String,
}

impl From<SessionRow> for crate::StoredSession {
    fn from(r: SessionRow) -> Self {
        Self { id: r.id, user_id: r.user_id, token: r.token, expires_at: r.expires_at, created_at: r.created_at }
    }
}

// ── Transaction + Span row types ─────────────────────────────────────

/// Stored transaction row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TransactionRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub duration_ms: f64,
    pub status: String,
    pub data: String,
    pub received_at: String,
}

/// Stored span row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SpanRow {
    pub id: String,
    pub transaction_id: String,
    pub span_id: String,
    pub trace_id: String,
    pub parent_span_id: Option<String>,
    pub op: Option<String>,
    pub description: Option<String>,
    pub start_offset_ms: f64,
    pub duration_ms: f64,
    pub status: Option<String>,
    pub data: String,
}

/// Stored release health row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReleaseHealthRow {
    pub id: String,
    pub project_id: String,
    pub release: String,
    pub environment: Option<String>,
    pub started_at: String,
    pub distinct_id: Option<String>,
    pub exited: i64,
    pub errored: i64,
    pub abnormal: i64,
    pub crashed: i64,
    pub received_at: String,
}

// ── Shared helpers ───────────────────────────────────────────────────

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a DSN with the given host and numeric project sequence.
///
/// Sentry JS SDKs require the DSN path to be an all-digit project id, so the
/// per-project `seq` (not the internal UUID) is rendered into the DSN.
/// Format: `https://{key}@{host}/{seq}`.
pub fn generate_dsn_with(host: &str, seq: i64) -> String {
    generate_dsn_with_scheme("https", host, seq)
}

/// Like [`generate_dsn_with`] but with an explicit URL scheme (`http`/`https`).
pub fn generate_dsn_with_scheme(scheme: &str, host: &str, seq: i64) -> String {
    let key = Uuid::new_v4();
    format!("{scheme}://{key}@{host}/{seq}")
}

/// Extract the DSN key from a full DSN URL (any scheme, e.g. `http://` or `https://`).
pub fn extract_dsn_key(dsn: &str) -> String {
    let userinfo = dsn.split('@').next().unwrap_or("");
    match userinfo.split_once("://") {
        Some((_, key)) => key.to_string(),
        None => userinfo.to_string(),
    }
}

/// Parse a path segment as a numeric project sequence (all ASCII digits).
///
/// Returns `None` for UUIDs and any other non-numeric input so callers can
/// fall back to legacy UUID resolution.
pub fn parse_project_seq(segment: &str) -> Option<i64> {
    if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    segment.parse().ok()
}

/// Extract the host from a DSN URL.
pub fn extract_dsn_host(dsn: &str) -> String {
    dsn.split('@')
        .nth(1)
        .map(|s| s.split('/').next().unwrap_or("localhost:9090"))
        .unwrap_or("localhost:9090")
        .to_string()
}

/// Current time as RFC3339 string.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ── Level / Status mapping ───────────────────────────────────────────

pub fn level_to_str(level: Level) -> String {
    match level {
        Level::Fatal => "fatal",
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Info => "info",
        Level::Debug => "debug",
        Level::Trace => "trace",
    }
    .to_string()
}

pub fn str_to_level(s: &str) -> Level {
    match s {
        "fatal" => Level::Fatal,
        "error" => Level::Error,
        "warning" => Level::Warning,
        "info" => Level::Info,
        "debug" => Level::Debug,
        "trace" => Level::Trace,
        _ => Level::Error,
    }
}

pub fn status_to_str(status: IssueStatus) -> String {
    match status {
        IssueStatus::Unresolved => "unresolved",
        IssueStatus::Resolved => "resolved",
        IssueStatus::Ignored => "ignored",
        IssueStatus::Regression => "regression",
    }
    .to_string()
}

pub fn str_to_status(s: &str) -> IssueStatus {
    match s {
        "unresolved" => IssueStatus::Unresolved,
        "resolved" => IssueStatus::Resolved,
        "ignored" => IssueStatus::Ignored,
        "regression" => IssueStatus::Regression,
        _ => IssueStatus::Unresolved,
    }
}

pub fn span_status_to_str(status: trapfall_proto::SpanStatus) -> String {
    match status {
        trapfall_proto::SpanStatus::Ok => "ok",
        trapfall_proto::SpanStatus::DeadlineExceeded => "deadline_exceeded",
        trapfall_proto::SpanStatus::Cancelled => "cancelled",
        trapfall_proto::SpanStatus::UnknownError => "unknown_error",
        trapfall_proto::SpanStatus::InternalError => "internal_error",
        trapfall_proto::SpanStatus::ResourceExhausted => "resource_exhausted",
        trapfall_proto::SpanStatus::Unauthenticated => "unauthenticated",
        trapfall_proto::SpanStatus::Unavailable => "unavailable",
        trapfall_proto::SpanStatus::AlreadyExists => "already_exists",
        trapfall_proto::SpanStatus::PermissionDenied => "permission_denied",
        trapfall_proto::SpanStatus::NotFound => "not_found",
        trapfall_proto::SpanStatus::FailedPrecondition => "failed_precondition",
        trapfall_proto::SpanStatus::Aborted => "aborted",
        trapfall_proto::SpanStatus::OutOfRange => "out_of_range",
        trapfall_proto::SpanStatus::Unimplemented => "unimplemented",
        trapfall_proto::SpanStatus::DataLoss => "data_loss",
    }
    .to_string()
}

/// Parse a level string from a serde_json Value (used in dynamic search queries).
pub fn level_from_string(s: &str) -> Level {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(Level::Error)
}

/// Parse a status string from a serde_json Value.
pub fn status_from_string(s: &str) -> IssueStatus {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or(IssueStatus::Unresolved)
}

// ── Dynamic WHERE clause builder (shared by search + count) ───────────

/// Build a WHERE-clause extension from optional project/status/level filters.
///
/// Returns `(conditions, bindings)` where `conditions` is a Vec of SQL
/// fragments (e.g. `["project_id = ?"]`) and `bindings` is a Vec of
/// the corresponding values.
///
/// Callers append `conditions` to their base WHERE and bind
/// `bindings` in order before limit/offset.
pub fn build_filter_conds(
    project_id: Option<&str>,
    status: Option<&str>,
    level: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    let mut conds: Vec<String> = Vec::new();
    let mut bindings: Vec<String> = Vec::new();

    if let Some(pid) = project_id {
        conds.push("project_id = ?".into());
        bindings.push(pid.to_string());
    }
    if let Some(s) = status {
        conds.push("status = ?".into());
        bindings.push(s.to_string());
    }
    if let Some(l) = level {
        conds.push("level = ?".into());
        bindings.push(l.to_string());
    }

    (conds, bindings)
}

/// Join conditions into a SQL AND string (empty if no conditions).
pub fn join_conds(conds: &[String]) -> String {
    if conds.is_empty() { String::new() } else { format!(" AND {}", conds.join(" AND ")) }
}

// ── LIKE escaping (SQLite uses ! as escape char) ─────────────────────

/// Escape SQLite LIKE wildcard characters (`%`, `_`, `!`).
/// Uses `!` as ESCAPE character.
pub fn escape_like_sqlite(input: &str) -> String {
    input.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

/// Escape Postgres ILIKE wildcard characters (`%`, `_`, `\\`).
/// Uses `\\` as escape character (default for Postgres).
pub fn escape_ilike_postgres(input: &str) -> String {
    input.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

// ── Attachment row type ─────────────────────────────────────────────

/// Stored attachment metadata row (binary data lives on disk).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttachmentRow {
    pub id: String,
    pub event_id: String,
    pub project_id: String,
    pub filename: String,
    pub content_type: Option<String>,
    pub attachment_type: Option<String>,
    pub size_bytes: i64,
    pub disk_path: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsn_uses_numeric_project_id() {
        let dsn = generate_dsn_with("trapfall.example.com", 7);
        assert!(dsn.starts_with("https://"));
        assert!(dsn.ends_with("@trapfall.example.com/7"));
        let path = dsn.rsplit('/').next().unwrap();
        assert!(path.bytes().all(|b| b.is_ascii_digit()));
    }

    #[test]
    fn dsn_with_http_scheme() {
        let dsn = generate_dsn_with_scheme("http", "localhost:9090", 1);
        assert!(dsn.starts_with("http://"));
        assert!(dsn.ends_with("@localhost:9090/1"));
    }

    #[test]
    fn extract_key_handles_http_and_https() {
        assert_eq!(extract_dsn_key("https://abc@h.example/1"), "abc");
        assert_eq!(extract_dsn_key("http://abc@localhost:9090/1"), "abc");
        assert_eq!(extract_dsn_key("abc@h/1"), "abc");
        assert_eq!(extract_dsn_key(""), "");
    }

    #[test]
    fn extract_key_roundtrips_generated_dsn() {
        for scheme in ["http", "https"] {
            let dsn = generate_dsn_with_scheme(scheme, "h:1", 3);
            let key = extract_dsn_key(&dsn);
            assert!(Uuid::parse_str(&key).is_ok(), "key should be a bare uuid, got {key}");
        }
    }

    #[test]
    fn extract_host_unchanged_for_numeric_dsn() {
        assert_eq!(extract_dsn_host("http://k@localhost:9090/5"), "localhost:9090");
    }

    #[test]
    fn parse_seq_numeric_only() {
        assert_eq!(parse_project_seq("42"), Some(42));
        assert_eq!(parse_project_seq(""), None);
        assert_eq!(parse_project_seq("-1"), None);
        assert_eq!(parse_project_seq("2e3c789d-98cc-4cee-b0d1-6ed51cab436b"), None);
        assert_eq!(parse_project_seq("12abc"), None);
        assert_eq!(parse_project_seq("99999999999999999999999"), None);
    }
}
