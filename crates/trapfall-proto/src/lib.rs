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
    pub message: Option<String>,
    #[serde(default)]
    pub tags: serde_json::Value,
    #[serde(default)]
    pub extra: serde_json::Value,
    #[serde(rename = "contexts", default)]
    pub contexts: serde_json::Value,
    pub timestamp: Option<String>,
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
}
