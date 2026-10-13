//! MCP server — stdio transport, AI agent tools.
//!
//! Implements JSON-RPC 2.0 over stdin/stdout for Model Context Protocol.
//! Spawned by AI agents as subprocess. No auth needed (process isolation).

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use trapfall_core::Store;
use trapfall_proto::IssueStatus;

/// MCP protocol versions this server can speak, newest first.
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

// JSON-RPC 2.0 error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// A JSON-RPC protocol-level error.
#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }
}

fn error_response(id: Value, err: RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "error": { "code": err.code, "message": err.message }, "id": id })
}

/// Process a JSON-RPC message (single request, notification or batch).
///
/// Returns `None` when no response must be sent (notifications, or a batch
/// consisting solely of notifications). Used by both the stdin server loop
/// and integration tests.
pub async fn handle_message(input: &str, store: &Store) -> Option<String> {
    let msg: Value = match serde_json::from_str(input.trim()) {
        Ok(v) => v,
        Err(e) => {
            return Some(
                error_response(Value::Null, RpcError::new(PARSE_ERROR, format!("Parse error: {e}"))).to_string(),
            );
        }
    };

    match msg {
        Value::Array(items) => {
            if items.is_empty() {
                return Some(
                    error_response(Value::Null, RpcError::new(INVALID_REQUEST, "Invalid Request: empty batch"))
                        .to_string(),
                );
            }
            let mut responses = Vec::new();
            for item in items {
                if let Some(resp) = handle_single(item, store).await {
                    responses.push(resp);
                }
            }
            if responses.is_empty() { None } else { Some(Value::Array(responses).to_string()) }
        }
        other => handle_single(other, store).await.map(|v| v.to_string()),
    }
}

/// Handle one JSON-RPC message object. `None` means "send nothing".
async fn handle_single(msg: Value, store: &Store) -> Option<Value> {
    let Some(obj) = msg.as_object() else {
        return Some(error_response(Value::Null, RpcError::new(INVALID_REQUEST, "Invalid Request: not an object")));
    };

    // `id` must be a string, number or null; its absence marks a notification.
    let (id, is_notification) = match obj.get("id") {
        None => (Value::Null, true),
        Some(v @ (Value::String(_) | Value::Number(_) | Value::Null)) => (v.clone(), false),
        Some(_) => {
            return Some(error_response(
                Value::Null,
                RpcError::new(INVALID_REQUEST, "Invalid Request: id must be a string, number or null"),
            ));
        }
    };

    // A response from the peer (no method, has result/error) needs no reply.
    if !obj.contains_key("method") && (obj.contains_key("result") || obj.contains_key("error")) {
        return None;
    }

    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error_response(id, RpcError::new(INVALID_REQUEST, "Invalid Request: jsonrpc must be \"2.0\"")));
    }

    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return Some(error_response(id, RpcError::new(INVALID_REQUEST, "Invalid Request: method must be a string")));
    };

    let params = match obj.get("params") {
        None => json!({}),
        Some(p @ (Value::Object(_) | Value::Array(_))) => p.clone(),
        Some(_) => {
            return (!is_notification)
                .then(|| error_response(id, RpcError::invalid_params("params must be an object or array")));
        }
    };

    let result = handle_request(method, params, store).await;

    if is_notification {
        return None;
    }
    Some(match result {
        Ok(val) => json!({ "jsonrpc": "2.0", "result": val, "id": id }),
        Err(err) => error_response(id, err),
    })
}

/// Run the MCP server, reading newline-delimited JSON-RPC from stdin and
/// writing responses to stdout.
pub async fn run_server(store: Store) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_message(&line, &store).await {
            stdout.write_all(response.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

// ── JSON-RPC Dispatcher ────────────────────────────────────────────────

async fn handle_request(method: &str, params: Value, store: &Store) -> Result<Value, RpcError> {
    match method {
        "initialize" => {
            let requested = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing protocolVersion"))?;
            let version = SUPPORTED_PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == requested)
                .unwrap_or(&SUPPORTED_PROTOCOL_VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "trapfall", "version": env!("CARGO_PKG_VERSION") }
            }))
        }
        "tools/list" => Ok(json!({ "tools": tools_list() })),
        "tools/call" => {
            let tool_name = params
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing tool name"))?;
            let args = match params.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(a @ Value::Object(_)) => a.clone(),
                Some(_) => return Err(RpcError::invalid_params("arguments must be an object")),
            };
            if !tools_list().iter().any(|t| t["name"] == tool_name) {
                return Err(RpcError::invalid_params(format!("Unknown tool: {tool_name}")));
            }
            // Tool execution failures are reported in-band (`isError`), not as protocol errors.
            Ok(match call_tool(tool_name, args, store).await {
                Ok(v) => v,
                Err(msg) => json!({ "content": [{ "type": "text", "text": msg }], "isError": true }),
            })
        }
        "ping" => Ok(json!({})),
        _ => Err(RpcError::new(METHOD_NOT_FOUND, format!("Unknown method: {method}"))),
    }
}

// ── Tool Definitions ───────────────────────────────────────────────────

fn tools_list() -> Vec<Value> {
    vec![
        tool(
            "list_issues",
            "List error issues for a project, optionally filtered by status and level",
            json!({
                "type": "object",
                "properties": {
                    "project_slug": { "type": "string", "description": "Project slug" },
                    "status": { "type": "string", "enum": ["unresolved","resolved","ignored","regression"], "description": "Filter by status" },
                    "level": { "type": "string", "enum": ["fatal","error","warning","info","debug","trace"], "description": "Filter by level" },
                    "limit": { "type": "integer", "description": "Max results (default 50)" }
                },
                "required": ["project_slug"]
            }),
        ),
        tool(
            "get_issue",
            "Get a specific issue by ID",
            json!({
                "type": "object",
                "properties": { "issue_id": { "type": "string", "description": "Issue ID" } },
                "required": ["issue_id"]
            }),
        ),
        tool(
            "get_event",
            "Get a specific event with full stacktrace data",
            json!({
                "type": "object",
                "properties": { "event_id": { "type": "string", "description": "Event ID" } },
                "required": ["event_id"]
            }),
        ),
        tool(
            "set_status",
            "Set the status of an issue (resolve, ignore, unresolve)",
            json!({
                "type": "object",
                "properties": {
                    "issue_id": { "type": "string", "description": "Issue ID" },
                    "status": { "type": "string", "enum": ["resolved","ignored","unresolved","regression"] }
                },
                "required": ["issue_id", "status"]
            }),
        ),
        tool(
            "search_issues",
            "Search issues by substring in title or culprit",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "project_slug": { "type": "string", "description": "Optional project filter" },
                    "limit": { "type": "integer", "description": "Max results (default 20)" }
                },
                "required": ["query"]
            }),
        ),
        tool("list_projects", "List all projects", json!({ "type": "object", "properties": {} })),
        tool(
            "get_project",
            "Get project details by slug",
            json!({
                "type": "object",
                "properties": { "slug": { "type": "string", "description": "Project slug" } },
                "required": ["slug"]
            }),
        ),
        tool(
            "get_project_stats",
            "Get issue statistics for a project",
            json!({
                "type": "object",
                "properties": { "project_slug": { "type": "string", "description": "Project slug" } },
                "required": ["project_slug"]
            }),
        ),
        tool(
            "list_alert_rules",
            "List alert rules for a project",
            json!({
                "type": "object",
                "properties": { "project_slug": { "type": "string", "description": "Project slug" } },
                "required": ["project_slug"]
            }),
        ),
        tool(
            "list_events",
            "List events for a specific issue",
            json!({
                "type": "object",
                "properties": {
                    "issue_id": { "type": "string", "description": "Issue ID" },
                    "limit": { "type": "integer", "description": "Max results (default 20)" }
                },
                "required": ["issue_id"]
            }),
        ),
        tool(
            "rotate_dsn",
            "Rotate the DSN key for a project",
            json!({
                "type": "object",
                "properties": { "project_slug": { "type": "string", "description": "Project slug" } },
                "required": ["project_slug"]
            }),
        ),
        tool("healthcheck", "Check if the TrapFall server is healthy", json!({ "type": "object", "properties": {} })),
    ]
}

/// Helper to build a tool definition.
fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

// ── Tool Dispatcher ────────────────────────────────────────────────────

async fn call_tool(name: &str, args: Value, store: &Store) -> Result<Value, String> {
    match name {
        "list_issues" => tool_list_issues(args, store).await,
        "get_issue" => tool_get_issue(args, store).await,
        "get_event" => tool_get_event(args, store).await,
        "set_status" => tool_set_status(args, store).await,
        "search_issues" => tool_search_issues(args, store).await,
        "list_projects" => tool_list_projects(store).await,
        "get_project" => tool_get_project(args, store).await,
        "get_project_stats" => tool_get_project_stats(args, store).await,
        "list_alert_rules" => tool_list_alert_rules(args, store).await,
        "list_events" => tool_list_events(args, store).await,
        "rotate_dsn" => tool_rotate_dsn(args, store).await,
        "healthcheck" => tool_healthcheck(store).await,
        _ => Err(format!("Unknown tool: {name}")),
    }
}

// ── Tool Implementations ───────────────────────────────────────────────

/// Resolve project slug to project, or return error string.
async fn resolve_slug(slug: &str, store: &Store) -> Result<trapfall_proto::Project, String> {
    store.get_project_by_slug(slug).await.map_err(|e| e.to_string())?.ok_or_else(|| "project not found".to_string())
}

/// Wrap a serializable value into MCP content response.
fn text_response(data: &impl serde::Serialize) -> Value {
    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(data).unwrap_or_default() }] })
}

/// Wrap a plain text string into MCP content response.
fn text_msg(msg: impl std::fmt::Display) -> Value {
    json!({ "content": [{ "type": "text", "text": msg.to_string() }] })
}

async fn tool_list_issues(args: Value, store: &Store) -> Result<Value, String> {
    let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
    let project = resolve_slug(slug, store).await?;

    let status = args.get("status").and_then(|v| v.as_str());
    let level = args.get("level").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

    let issues = store.list_issues_filtered(&project.id, status, level, limit, 0).await.map_err(|e| e.to_string())?;

    let result: Vec<Value> = issues
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "title": i.title,
                "status": i.status,
                "level": i.level,
                "count": i.count,
                "culprit": i.culprit,
                "last_seen": i.last_seen,
            })
        })
        .collect();

    Ok(text_response(&result))
}

async fn tool_get_issue(args: Value, store: &Store) -> Result<Value, String> {
    let id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
    let issue = store.get_issue(id).await.map_err(|e| e.to_string())?.ok_or("issue not found")?;
    Ok(text_response(&issue))
}

async fn tool_get_event(args: Value, store: &Store) -> Result<Value, String> {
    let event_id = args.get("event_id").and_then(|v| v.as_str()).ok_or("missing event_id")?;
    let event = store.backend().get_event_raw(event_id).await.map_err(|e| e.to_string())?.ok_or("event not found")?;

    Ok(text_response(&json!({
        "id": event.id,
        "issue_id": event.issue_id,
        "received_at": event.received_at,
        "event": event.data,
    })))
}

async fn tool_set_status(args: Value, store: &Store) -> Result<Value, String> {
    let issue_id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
    let status_str = args.get("status").and_then(|v| v.as_str()).ok_or("missing status")?;
    let status: IssueStatus =
        serde_json::from_value(json!(status_str)).map_err(|e: serde_json::Error| format!("invalid status: {e}"))?;
    store.set_issue_status(issue_id, status).await.map_err(|e| e.to_string())?;
    Ok(text_msg(format!("Issue {issue_id} status set to {status_str}")))
}

async fn tool_search_issues(args: Value, store: &Store) -> Result<Value, String> {
    let query = args.get("query").and_then(|v| v.as_str()).ok_or("missing query")?;
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 100);

    let project_id = if let Some(slug) = args.get("project_slug").and_then(|v| v.as_str()) {
        Some(resolve_slug(slug, store).await?.id)
    } else {
        None
    };

    let issues = store
        .backend()
        .search_issues(query, project_id.as_deref(), None, None, limit, 0)
        .await
        .map_err(|e| e.to_string())?;

    let results: Vec<Value> = issues
        .iter()
        .map(|i| {
            json!({
                "id": i.id,
                "title": i.title,
                "status": format!("{:?}", i.status).to_lowercase(),
                "level": format!("{:?}", i.level).to_lowercase(),
                "count": i.count,
                "last_seen": i.last_seen,
            })
        })
        .collect();

    Ok(text_response(&results))
}

async fn tool_list_projects(store: &Store) -> Result<Value, String> {
    let projects = store.list_projects().await.map_err(|e| e.to_string())?;
    let list: Vec<Value> = projects
        .iter()
        .map(|p| json!({ "id": p.id, "slug": p.slug, "name": p.name, "dsn": mask_dsn(&p.dsn) }))
        .collect();
    Ok(text_response(&list))
}

async fn tool_get_project(args: Value, store: &Store) -> Result<Value, String> {
    let slug = args.get("slug").and_then(|v| v.as_str()).ok_or("missing slug")?;
    let project = resolve_slug(slug, store).await?;
    Ok(text_response(&json!({
        "id": project.id,
        "slug": project.slug,
        "name": project.name,
        "dsn": mask_dsn(&project.dsn),
    })))
}

async fn tool_get_project_stats(args: Value, store: &Store) -> Result<Value, String> {
    let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
    let project = resolve_slug(slug, store).await?;

    let total = store.backend().count_issues(&project.id, None, None).await.map_err(|e| e.to_string())?;
    let unresolved =
        store.backend().count_issues(&project.id, Some("unresolved"), None).await.map_err(|e| e.to_string())?;
    let errors = store.backend().count_issues(&project.id, None, Some("error")).await.map_err(|e| e.to_string())?;
    let fatal = store.backend().count_issues(&project.id, None, Some("fatal")).await.map_err(|e| e.to_string())?;

    Ok(text_response(&json!({
        "project": project.slug,
        "total_issues": total,
        "unresolved": unresolved,
        "errors": errors,
        "fatal": fatal,
    })))
}

async fn tool_list_alert_rules(args: Value, store: &Store) -> Result<Value, String> {
    let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
    let project = resolve_slug(slug, store).await?;
    let rules = store.list_alert_rules(&project.id).await.map_err(|e| e.to_string())?;
    let list: Vec<Value> = rules
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "enabled": r.enabled,
                "conditions": r.conditions,
                "action_type": r.action_type,
                "cooldown_seconds": r.cooldown_seconds,
            })
        })
        .collect();
    Ok(text_response(&list))
}

async fn tool_list_events(args: Value, store: &Store) -> Result<Value, String> {
    let issue_id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 100);
    let events = store.list_events(issue_id, limit, 0).await.map_err(|e| e.to_string())?;
    let list: Vec<Value> =
        events.iter().map(|e| json!({ "id": e.id, "issue_id": e.issue_id, "received_at": e.received_at })).collect();
    Ok(text_response(&list))
}

async fn tool_rotate_dsn(args: Value, store: &Store) -> Result<Value, String> {
    let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
    let project = resolve_slug(slug, store).await?;
    let new_key = store.rotate_dsn(&project.id).await.map_err(|e| e.to_string())?;
    Ok(text_msg(format!("DSN rotated for {}. New key: {}", slug, abbreviate_key(&new_key))))
}

async fn tool_healthcheck(store: &Store) -> Result<Value, String> {
    let ok = store.backend().ping().await.map_err(|e| e.to_string())?;
    Ok(text_msg(format!("Healthy (db={})", if ok { 1 } else { 0 })))
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Mask DSN for MCP responses: show only first 8 and last 4 chars of key.
fn mask_dsn(dsn: &str) -> String {
    let at_pos = match dsn.find('@') {
        Some(p) => p,
        None => return "***masked***".to_string(),
    };
    let key = dsn[..at_pos].trim_start_matches("https://");
    let host = &dsn[at_pos + 1..];
    if key.chars().count() <= 12 {
        format!("https://***@{host}")
    } else {
        format!("https://{}@{host}", abbreviate_key(key))
    }
}

/// Abbreviate a key to its first 8 and last 4 characters (char-boundary safe).
/// Keys too short to abbreviate meaningfully are fully masked.
fn abbreviate_key(key: &str) -> String {
    let count = key.chars().count();
    if count <= 12 {
        return "***".to_string();
    }
    let head: String = key.chars().take(8).collect();
    let tail: String = key.chars().skip(count - 4).collect();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviate_key_handles_short_and_multibyte() {
        assert_eq!(abbreviate_key(""), "***");
        assert_eq!(abbreviate_key("abc"), "***");
        assert_eq!(abbreviate_key("0123456789abcdef"), "01234567...cdef");
        // Multibyte chars must not panic on slicing.
        assert_eq!(abbreviate_key("ééééééééééééééé"), "éééééééé...éééé");
    }

    #[test]
    fn mask_dsn_handles_edge_cases() {
        assert_eq!(mask_dsn("no-at-sign"), "***masked***");
        assert_eq!(mask_dsn("https://short@host/1"), "https://***@host/1");
        assert_eq!(mask_dsn("https://0123456789abcdef@host/1"), "https://01234567...cdef@host/1");
        assert_eq!(mask_dsn("https://éééééééééééééééé@host/1"), "https://éééééééé...éééé@host/1");
    }
}
