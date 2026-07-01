//! # trapfall-proto
//!
//! Wire types and protocol definitions for TrapFall.
//! Shared across all crates — no business logic here.

use serde::{Deserialize, Serialize};

// ── Enums ────────────────────────────────────────────────────────────────

/// Issue grouping status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueStatus {
    Unresolved,
    Resolved,
    Ignored,
    Regression,
}

impl Default for IssueStatus {
    fn default() -> Self {
        Self::Unresolved
    }
}

/// Log level severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Fatal,
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}

impl Default for Level {
    fn default() -> Self {
        Self::Error
    }
}

// ── Fingerprint ──────────────────────────────────────────────────────────

/// Deterministic issue fingerprint (blake3-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    /// blake3 hash in hex (64 chars).
    pub hash: String,
    /// The components that were hashed (exception type, function, filename).
    pub components: Vec<String>,
}

impl Fingerprint {
    /// Create a fingerprint from components.
    pub fn new(hash: String, components: Vec<String>) -> Self {
        Self { hash, components }
    }
}

// ── Stack Trace ──────────────────────────────────────────────────────────

/// A single frame in a stack trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub filename: Option<String>,
    pub function: Option<String>,
    pub module: Option<String>,
    pub lineno: Option<u32>,
    pub colno: Option<u32>,
    pub abs_path: Option<String>,
    #[serde(default)]
    pub in_app: bool,
    pub pre_context: Option<Vec<String>>,
    pub context_line: Option<String>,
    pub post_context: Option<Vec<String>>,
}

/// Exception data within an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exception {
    #[serde(rename = "type")]
    pub exc_type: String,
    pub value: Option<String>,
    pub stacktrace: Option<Stacktrace>,
}

/// A stack trace (list of frames).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stacktrace {
    pub frames: Vec<StackFrame>,
}

// ── Breadcrumb ───────────────────────────────────────────────────────────

/// A breadcrumb event leading up to an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumb {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub bc_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub level: Level,
}

// ── Event (Inbound) ─────────────────────────────────────────────────────

/// An inbound error event from a Sentry SDK.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    #[serde(default)]
    pub level: Level,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub server_name: Option<String>,
    #[serde(default)]
    pub breadcrumbs: Breadcrumbs,
    pub exception: Option<ExceptionValues>,
    #[serde(default, deserialize_with = "deserialize_message", alias = "Message")]
    pub message: Option<String>,
    #[serde(default)]
    pub tags: serde_json::Value,
    #[serde(default)]
    pub extra: serde_json::Value,
    #[serde(rename = "contexts", default)]
    pub contexts: serde_json::Value,
    pub timestamp: Option<String>,
}

/// Deserialize a Sentry event `message` field that may arrive either as a
/// plain string (minimal SDKs) or as the `Message` interface object
/// `{ "formatted": "...", "message": "..." }` (modern SDKs incl. Dart/Flutter).
///
/// We collapse both forms into the formatted display string, preferring
/// `formatted`, then `message`, then falling back to `None`.
/// Without this, a `message` object silently fails whole-event
/// deserialization (serde "invalid type: map, expected a string") and the
/// event is dropped.
pub fn deserialize_message<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|v| match v {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Object(map) => map
            .get("formatted")
            .and_then(|f| f.as_str().map(str::to_string))
            .or_else(|| map.get("message").and_then(|m| m.as_str().map(str::to_string))),
        _ => None,
    }))
}

/// Wrapper for exception values (Sentry format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionValues {
    pub values: Vec<Exception>,
}

/// Wrapper for breadcrumbs (Sentry format).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Breadcrumbs {
    pub values: Vec<Breadcrumb>,
}

// ── Issue (Stored) ──────────────────────────────────────────────────────

/// A grouped issue in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    pub fingerprint: String,
    pub title: String,
    pub culprit: Option<String>,
    #[serde(default)]
    pub status: IssueStatus,
    pub level: Level,
    pub count: i64,
    pub user_count: i64,
    pub first_seen: String,
    pub last_seen: String,
}

// ── Wire Protocol (Internal) ────────────────────────────────────────────

/// Internal event passed through the ingest channel.
#[derive(Debug, Clone)]
pub struct IngestEvent {
    pub project_id: String,
    pub fingerprint: Fingerprint,
    pub event: Event,
    pub received_at: String,
}

/// Messages broadcast from digest to WebSocket clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    IssueCreated { issue: Issue },
    IssueUpdated { issue: Issue },
    EventReceived { issue_id: String, event_id: String },
    TransactionReceived { transaction_id: String, name: String, duration_ms: f64 },
}

/// Messages from WebSocket client (minimal for now).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Subscribe { project_id: String },
    Unsubscribe { project_id: String },
}

// ── API Response Types ──────────────────────────────────────────────────

/// Paginated list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
}

/// Project info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub dsn: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
}

impl Project {
    /// Mask the secret key in this project's DSN for safe display in
    /// dashboard listings.
    ///
    /// Format: `https://{key8}...{key4}@{host}/{id}` — preserves the host +
    /// project id (so the user knows where the DSN points) while hiding the
    /// middle of the secret. Falls back to returning the DSN unchanged if it
    /// does not match the expected `https://<key>@<host>/<id>` shape.
    pub fn masked_dsn(&self) -> Project {
        let masked = mask_dsn_key(&self.dsn);
        Project { dsn: masked, ..self.clone() }
    }
}

/// Mask the secret portion of a Sentry-style DSN.
///
/// Accepts `https://<key>@<host>/<id>` and returns
/// `https://<key-first8>...<key-last4>@<host>/<id>`. Short keys (<12 chars)
/// are fully replaced with `...`. Non-matching input is returned unchanged.
pub fn mask_dsn_key(dsn: &str) -> String {
    // Expect shape: scheme://key@host/path
    let Some(scheme_slash) = dsn.find("://") else { return dsn.to_string() };
    let scheme_end = scheme_slash + 3;
    let Some(at_idx) = dsn[scheme_end..].find('@').map(|i| i + scheme_end) else {
        return dsn.to_string();
    };
    if at_idx <= scheme_end {
        return dsn.to_string();
    }
    let key = &dsn[scheme_end..at_idx];
    let rest = &dsn[at_idx..];
    let masked_key =
        if key.len() >= 12 { format!("{}...{}", &key[..8], &key[key.len() - 4..]) } else { "...".to_string() };
    // `dsn[..scheme_end]` is `https://`; `rest` is `@host/id`.
    format!("{}{}{}", &dsn[..scheme_end], masked_key, rest)
}

/// Stored event (full detail).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: String,
    pub issue_id: String,
    pub project_id: String,
    pub data: serde_json::Value,
    pub received_at: String,
}

// ── Alert Rule Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub enabled: bool,
    pub conditions: serde_json::Value,
    pub action_type: String,
    pub action_config: serde_json::Value,
    pub cooldown_seconds: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAlertRule {
    pub name: String,
    pub conditions: serde_json::Value,
    pub action_type: Option<String>,
    pub action_config: Option<serde_json::Value>,
    pub cooldown_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertHistory {
    pub id: String,
    pub rule_id: String,
    pub project_id: String,
    pub issue_id: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub sent_at: Option<String>,
}

// ── Transaction (Inbound) ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    #[default]
    Ok,
    DeadlineExceeded,
    Cancelled,
    UnknownError,
    InternalError,
    ResourceExhausted,
    Unauthenticated,
    Unavailable,
    AlreadyExists,
    PermissionDenied,
    NotFound,
    FailedPrecondition,
    Aborted,
    OutOfRange,
    Unimplemented,
    DataLoss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: String,
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start_timestamp: f64,
    pub timestamp: f64,
    #[serde(default)]
    pub status: SpanStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub event_id: String,
    #[serde(default)]
    pub level: Level,
    pub transaction: String,
    pub start_timestamp: f64,
    pub timestamp: f64,
    pub release: Option<String>,
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contexts: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

// ── Session Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Ok,
    Exited,
    Crashed,
    Abnormal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAttributes {
    pub release: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUpdate {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct_id: Option<String>,
    pub init: bool,
    pub started: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    pub status: SessionStatus,
    pub errors: u64,
    pub attributes: SessionAttributes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAggregateItem {
    pub started: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct_id: Option<String>,
    pub exited: u32,
    pub errored: u32,
    pub abnormal: u32,
    pub crashed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAggregates {
    pub aggregates: Vec<SessionAggregateItem>,
    pub attributes: SessionAttributes,
}

// ── Attachment (binary payload) ──

/// Binary attachment carried in a Sentry envelope item.
/// Attachments are NOT JSON — they use length-delimited binary payloads.
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Original filename from the envelope header.
    pub filename: String,
    /// MIME type (e.g. "image/png", "text/plain").
    pub content_type: Option<String>,
    /// Sentry attachment type (e.g. "event.attachment", "event.minidump").
    pub attachment_type: Option<String>,
    /// Raw binary payload bytes.
    pub data: Vec<u8>,
}

// ── Envelope Parsing Result ──

/// Parsed result from a Sentry envelope — may contain multiple item types.
#[derive(Debug, Clone, Default)]
pub struct ParsedEnvelope {
    pub events: Vec<Event>,
    pub transactions: Vec<Transaction>,
    pub session_updates: Vec<SessionUpdate>,
    pub session_aggregates: Vec<SessionAggregates>,
    pub attachments: Vec<Attachment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_status_default_is_unresolved() {
        assert_eq!(IssueStatus::default(), IssueStatus::Unresolved);
    }

    #[test]
    fn level_default_is_error() {
        assert_eq!(Level::default(), Level::Error);
    }

    #[test]
    fn fingerprint_new() {
        let fp = Fingerprint::new("abc123".to_string(), vec!["TypeError".into(), "main".into()]);
        assert_eq!(fp.hash, "abc123");
        assert_eq!(fp.components.len(), 2);
    }

    #[test]
    fn serde_issue_status() {
        let status = IssueStatus::Resolved;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"resolved\"");
        let back: IssueStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }

    #[test]
    fn serde_event_minimal() {
        let event = Event {
            event_id: "abc123".to_string(),
            level: Level::Error,
            platform: Some("rust".to_string()),
            release: None,
            environment: None,
            server_name: None,
            breadcrumbs: Breadcrumbs::default(),
            exception: None,
            message: Some("something broke".to_string()),
            tags: serde_json::Value::Null,
            extra: serde_json::Value::Null,
            contexts: serde_json::Value::Null,
            timestamp: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(back.event_id, "abc123");
        assert_eq!(back.platform.unwrap(), "rust");
    }

    #[test]
    fn serde_server_message() {
        let msg = ServerMessage::IssueCreated {
            issue: Issue {
                id: "i1".into(),
                project_id: "p1".into(),
                fingerprint: "fp1".into(),
                title: "TypeError".into(),
                culprit: Some("main.rs:42".into()),
                status: IssueStatus::Unresolved,
                level: Level::Error,
                count: 1,
                user_count: 1,
                first_seen: "2026-01-01T00:00:00Z".into(),
                last_seen: "2026-01-01T00:00:00Z".into(),
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"IssueCreated\""));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::IssueCreated { issue } => {
                assert_eq!(issue.id, "i1");
            }
            _ => panic!("wrong variant"),
        }
    }
    #[test]
    fn serde_transaction_received() {
        let msg = ServerMessage::TransactionReceived {
            transaction_id: "tx1".into(),
            name: "GET /api/health".into(),
            duration_ms: 595.0,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""type":"TransactionReceived""#));
        let back: ServerMessage = serde_json::from_str(&json).unwrap();
        match back {
            ServerMessage::TransactionReceived { transaction_id, name, duration_ms } => {
                assert_eq!(transaction_id, "tx1");
                assert_eq!(name, "GET /api/health");
                assert_eq!(duration_ms, 595.0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn serde_client_message() {
        let msg = ClientMessage::Subscribe { project_id: "p1".into() };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"Subscribe\""));
        let back: ClientMessage = serde_json::from_str(&json).unwrap();
        match back {
            ClientMessage::Subscribe { project_id } => {
                assert_eq!(project_id, "p1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn serde_list_response() {
        let resp = ListResponse { data: vec![1i64, 2, 3], total: 3, page: 1, per_page: 20 };
        let json = serde_json::to_string(&resp).unwrap();
        let back: ListResponse<i64> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data.len(), 3);
        assert_eq!(back.total, 3);
    }

    #[test]
    fn mask_dsn_key_masks_middle_of_secret() {
        let dsn = "https://3167cebd-abfb-4ee4-ae6a-fab02204366b@trapfall.example.com/proj-id";
        let masked = mask_dsn_key(dsn);
        assert_eq!(masked, "https://3167cebd...366b@trapfall.example.com/proj-id");
        // Original must be unchanged (masking is non-destructive).
        assert_ne!(masked, dsn);
    }

    #[test]
    fn mask_dsn_key_handles_short_keys() {
        // Key shorter than 12 chars collapses to "...".
        let dsn = "https://abc@host/id";
        assert_eq!(mask_dsn_key(dsn), "https://...@host/id");
    }

    #[test]
    fn mask_dsn_key_passthrough_non_dsn_input() {
        // No `@` -> return unchanged.
        assert_eq!(mask_dsn_key("not-a-dsn"), "not-a-dsn");
        // `@` before scheme end -> return unchanged.
        assert_eq!(mask_dsn_key("weird@input"), "weird@input");
    }

    #[test]
    fn project_masked_dsn_returns_clone_with_masked_field() {
        let project = Project {
            id: "proj-1".into(),
            slug: "app".into(),
            name: "App".into(),
            dsn: "https://3167cebd-abfb-4ee4-ae6a-fab02204366b@host/p1".into(),
            created_at: "2026-06-19T00:00:00Z".into(),
            archived_at: None,
        };
        let masked = project.masked_dsn();
        assert_eq!(masked.id, project.id);
        assert_eq!(masked.slug, project.slug);
        assert_eq!(masked.dsn, "https://3167cebd...366b@host/p1");
        // Original must still contain the full key.
        assert!(project.dsn.contains("abfb-4ee4-ae6a-fab02204366b"));
    }

    // ── Extended edge cases (#221) ─────────────────────────────────

    #[test]
    fn mask_dsn_key_very_long_key() {
        // Key much longer than 32 chars — only first 8 and last 4 survive.
        let key = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJ";
        let dsn = format!("https://{key}@errors.example.com/proj-long");
        let masked = mask_dsn_key(&dsn);
        assert_eq!(masked, "https://abcdefgh...GHIJ@errors.example.com/proj-long");
        // Middle bytes must not leak.
        assert!(!masked.contains("ijklmnop"));
    }

    #[test]
    fn mask_dsn_key_exactly_12_chars() {
        // Boundary: key of exactly 12 chars takes the >= 12 path.
        let dsn = "https://abcdefghijkl@host/id"; // key = "abcdefghijkl" (12 chars)
        let masked = mask_dsn_key(dsn);
        assert_eq!(masked, "https://abcdefgh...ijkl@host/id");
    }

    #[test]
    fn mask_dsn_key_empty_key() {
        // Edge: key between `://` and `@` is empty string (at_idx == scheme_end).
        // The masking function guards against this and returns the DSN unchanged.
        let dsn = "https://@host/id";
        let masked = mask_dsn_key(dsn);
        // Guard `at_idx <= scheme_end` triggers → passthrough.
        assert_eq!(masked, dsn, "empty key falls through unchanged due to guard");
    }

    #[test]
    fn mask_dsn_key_no_scheme_passthrough() {
        // Input without `://` is returned as-is.
        assert_eq!(mask_dsn_key("just-a-string"), "just-a-string");
        assert_eq!(mask_dsn_key(""), "");
    }

    #[test]
    fn mask_dsn_key_preserves_host_and_project() {
        // Regardless of key length, host + project_id must be intact.
        // We extract the suffix starting from '@' in the *original* DSN and
        // check it appears identically in the masked output.
        let cases = [
            "https://abc@sub.domain.example.com:8443/org/proj-1",
            "https://verylongkeyhere1234567890@localhost:3000/42",
            "https://k@host/id",
        ];
        for dsn in cases {
            let masked = mask_dsn_key(dsn);
            let suffix = &dsn[dsn.find('@').unwrap()..];
            assert!(masked.ends_with(suffix), "masked must preserve host/path, got {masked} for {dsn}");
        }
    }

    #[test]
    fn serde_span_status_roundtrip() {
        let status = SpanStatus::Ok;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""ok""#);
        let back: SpanStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SpanStatus::Ok);
    }

    #[test]
    fn serde_span_minimal() {
        let span = Span {
            span_id: "s1".into(),
            trace_id: "t1".into(),
            parent_span_id: None,
            op: Some("db.sql.select".into()),
            description: None,
            start_timestamp: 100.0,
            timestamp: 150.0,
            status: SpanStatus::Ok,
            tags: None,
            data: None,
        };
        let json = serde_json::to_string(&span).unwrap();
        let back: Span = serde_json::from_str(&json).unwrap();
        assert_eq!(back.span_id, "s1");
        assert_eq!(back.trace_id, "t1");
        assert_eq!(back.op.unwrap(), "db.sql.select");
        assert!(back.parent_span_id.is_none());
    }

    #[test]
    fn serde_transaction_with_spans() {
        let tx = Transaction {
            event_id: "e1".into(),
            level: Level::Info,
            transaction: "POST /api/feedback".into(),
            start_timestamp: 1703894474.296,
            timestamp: 1703894474.891,
            release: Some("rungu@0.2.0".into()),
            environment: Some("production".into()),
            spans: vec![Span {
                span_id: "sp1".into(),
                trace_id: "tr1".into(),
                parent_span_id: None,
                op: Some("http.server".into()),
                description: Some("handle request".into()),
                start_timestamp: 1703894474.296,
                timestamp: 1703894474.891,
                status: SpanStatus::Ok,
                tags: None,
                data: None,
            }],
            contexts: None,
            request: None,
            tags: None,
            extra: None,
        };
        let json = serde_json::to_string(&tx).unwrap();
        let back: Transaction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.transaction, "POST /api/feedback");
        assert_eq!(back.spans.len(), 1);
        assert_eq!(back.spans[0].op.clone().unwrap(), "http.server");
    }

    #[test]
    fn parsed_envelope_default_is_empty() {
        let env = ParsedEnvelope::default();
        assert!(env.attachments.is_empty());
        assert!(env.events.is_empty());
        assert!(env.transactions.is_empty());
        assert!(env.session_updates.is_empty());
        assert!(env.session_aggregates.is_empty());
    }

    #[test]
    fn serde_transaction_minimal() {
        let tx_json = r#"{"event_id":"e1","level":"info","transaction":"GET /","start_timestamp":1.0,"timestamp":2.0}"#;
        let tx: Transaction = serde_json::from_str(tx_json).unwrap();
        assert_eq!(tx.event_id, "e1");
        assert!(tx.spans.is_empty());
        assert!(tx.release.is_none());
    }
    #[test]
    fn serde_session_status() {
        let s = SessionStatus::Crashed;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"crashed\"");
        let back: SessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SessionStatus::Crashed);
    }

    #[test]
    fn serde_session_update() {
        let session = SessionUpdate {
            session_id: "sess-123".into(),
            distinct_id: Some("user-42".into()),
            init: true,
            started: "2026-06-27T10:00:00Z".into(),
            duration: Some(59500.0),
            status: SessionStatus::Ok,
            errors: 0,
            attributes: SessionAttributes {
                release: "myapp@1.0.0".into(),
                environment: Some("production".into()),
                ip_address: Some("1.2.3.4".into()),
                user_agent: Some("Mozilla/5.0".into()),
            },
        };
        let json = serde_json::to_string(&session).unwrap();
        let back: SessionUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_id, "sess-123");
        assert_eq!(back.status, SessionStatus::Ok);
        assert_eq!(back.attributes.release, "myapp@1.0.0");
        assert_eq!(back.errors, 0);
    }

    #[test]
    fn serde_session_aggregates() {
        let agg = SessionAggregates {
            aggregates: vec![
                SessionAggregateItem {
                    started: "2026-06-27T10:00:00Z".into(),
                    distinct_id: None,
                    exited: 90,
                    errored: 5,
                    abnormal: 2,
                    crashed: 3,
                },
                SessionAggregateItem {
                    started: "2026-06-27T11:00:00Z".into(),
                    distinct_id: None,
                    exited: 80,
                    errored: 10,
                    abnormal: 1,
                    crashed: 9,
                },
            ],
            attributes: SessionAttributes {
                release: "myapp@1.0.0".into(),
                environment: Some("production".into()),
                ip_address: None,
                user_agent: None,
            },
        };
        let json = serde_json::to_string(&agg).unwrap();
        let back: SessionAggregates = serde_json::from_str(&json).unwrap();
        assert_eq!(back.aggregates.len(), 2);
        assert_eq!(back.aggregates[0].crashed, 3);
        assert_eq!(back.aggregates[1].exited, 80);
    }
}
