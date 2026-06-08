//! MCP server — stdio transport, AI agent tools.
//!
//! Implements JSON-RPC 2.0 over stdin/stdout for Model Context Protocol.
//! Spawned by AI agents as subprocess. No auth needed (process isolation).

use std::io::{BufRead, Write};

use anyhow::Result;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use trapfall_proto::IssueStatus;

/// Process a single JSON-RPC message and return the response string.
/// Used by both the stdin server loop and integration tests.
pub async fn handle_message(input: &str, pool: &SqlitePool, store: &trapfall_core::Store) -> String {
    let msg: Value = match serde_json::from_str(input.trim()) {
        Ok(v) => v,
        Err(e) => {
            return serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "error": {"code": -32700, "message": format!("Parse error: {e}")},
                "id": null
            }))
            .unwrap();
        }
    };

    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));

    let result = handle_request(method, params, pool, store).await;

    match result {
        Ok(val) => json!({ "jsonrpc": "2.0", "result": val, "id": id }),
        Err(code_msg) => json!({ "jsonrpc": "2.0", "error": {"code": -32603, "message": code_msg}, "id": id }),
    }
    .to_string()
}

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run_server(pool: SqlitePool) -> Result<()> {
    let store = trapfall_core::Store::new(pool.clone());
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    let mut line = String::new();
    let stdin = stdin.lock();

    for result in stdin.lines() {
        line.clear();
        match result {
            Ok(l) => line.push_str(&l),
            Err(_) => break,
        }

        let response = handle_message(&line, &pool, &store).await;
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }

    Ok(())
}

// ── JSON-RPC Dispatcher ────────────────────────────────────────────────

async fn handle_request(
    method: &str,
    params: Value,
    pool: &SqlitePool,
    store: &trapfall_core::Store,
) -> Result<Value, String> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "trapfall", "version": env!("CARGO_PKG_VERSION") }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "tools/list" => Ok(json!({ "tools": tools_list() })),
        "tools/call" => {
            let tool_name = params.get("name").and_then(|n| n.as_str()).ok_or("missing tool name")?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            call_tool(tool_name, args, pool, store).await
        }
        "ping" => Ok(json!({})),
        _ => Err(format!("Unknown method: {method}")),
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
                    "status": { "type": "string", "enum": ["unresolved","resolved","ignored"], "description": "Filter by status" },
                    "level": { "type": "string", "enum": ["fatal","error","warning","info","debug"], "description": "Filter by level" },
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
                    "status": { "type": "string", "enum": ["resolved","ignored","unresolved"] }
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

async fn call_tool(name: &str, args: Value, pool: &SqlitePool, store: &trapfall_core::Store) -> Result<Value, String> {
    match name {
        "list_issues" => tool_list_issues(args, store).await,
        "get_issue" => tool_get_issue(args, store).await,
        "get_event" => tool_get_event(args, pool).await,
        "set_status" => tool_set_status(args, store).await,
        "search_issues" => tool_search_issues(args, pool, store).await,
        "list_projects" => tool_list_projects(store).await,
        "get_project" => tool_get_project(args, store).await,
        "get_project_stats" => tool_get_project_stats(args, pool, store).await,
        "list_alert_rules" => tool_list_alert_rules(args, store).await,
        "list_events" => tool_list_events(args, store).await,
        "rotate_dsn" => tool_rotate_dsn(args, store).await,
        "healthcheck" => tool_healthcheck(pool).await,
        _ => Err(format!("Unknown tool: {name}")),
    }
}

// ── Tool Implementations ───────────────────────────────────────────────

/// Resolve project slug to project, or return error string.
async fn resolve_slug(slug: &str, store: &trapfall_core::Store) -> Result<trapfall_proto::Project, String> {
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

async fn tool_list_issues(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
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

async fn tool_get_issue(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
    let id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
    let issue = store.get_issue(id).await.map_err(|e| e.to_string())?.ok_or("issue not found")?;
    Ok(text_response(&issue))
}

async fn tool_get_event(args: Value, pool: &SqlitePool) -> Result<Value, String> {
    let event_id = args.get("event_id").and_then(|v| v.as_str()).ok_or("missing event_id")?;
    let row = sqlx::query("SELECT id, issue_id, project_id, data, received_at FROM events WHERE id = ?")
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("event not found")?;

    let id: String = row.try_get("id").map_err(|e| e.to_string())?;
    let issue_id: String = row.try_get("issue_id").map_err(|e| e.to_string())?;
    let data: String = row.try_get("data").map_err(|e| e.to_string())?;
    let received_at: String = row.try_get("received_at").map_err(|e| e.to_string())?;

    let event_data: Value = serde_json::from_str(&data).unwrap_or(json!({}));
    Ok(text_response(&json!({
        "id": id,
        "issue_id": issue_id,
        "received_at": received_at,
        "event": event_data
    })))
}

async fn tool_set_status(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
    let issue_id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
    let status_str = args.get("status").and_then(|v| v.as_str()).ok_or("missing status")?;
    let status: IssueStatus =
        serde_json::from_value(json!(status_str)).map_err(|e: serde_json::Error| format!("invalid status: {e}"))?;
    store.set_issue_status(issue_id, status).await.map_err(|e| e.to_string())?;
    Ok(text_msg(format!("Issue {issue_id} status set to {status_str}")))
}

async fn tool_search_issues(args: Value, pool: &SqlitePool, store: &trapfall_core::Store) -> Result<Value, String> {
    let query = args.get("query").and_then(|v| v.as_str()).ok_or("missing query")?;
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 100);

    let project_id = if let Some(slug) = args.get("project_slug").and_then(|v| v.as_str()) {
        resolve_slug(slug, store).await.ok().map(|p| p.id)
    } else {
        None
    };

    let issues = trapfall_search::search_issues(pool, query, project_id.as_deref(), None, None, limit, 0)
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

async fn tool_list_projects(store: &trapfall_core::Store) -> Result<Value, String> {
    let projects = store.list_projects().await.map_err(|e| e.to_string())?;
    let list: Vec<Value> = projects
        .iter()
        .map(|p| json!({ "id": p.id, "slug": p.slug, "name": p.name, "dsn": mask_dsn(&p.dsn) }))
        .collect();
    Ok(text_response(&list))
}

async fn tool_get_project(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
    let slug = args.get("slug").and_then(|v| v.as_str()).ok_or("missing slug")?;
    let project = resolve_slug(slug, store).await?;
    Ok(text_response(&json!({
        "id": project.id,
        "slug": project.slug,
        "name": project.name,
        "dsn": mask_dsn(&project.dsn),
    })))
}

async fn tool_get_project_stats(args: Value, pool: &SqlitePool, store: &trapfall_core::Store) -> Result<Value, String> {
    let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
    let project = resolve_slug(slug, store).await?;

    let total = count_issues(pool, &project.id, None, None).await?;
    let unresolved = count_issues(pool, &project.id, Some("unresolved"), None).await?;
    let errors = count_issues(pool, &project.id, None, Some("error")).await?;
    let fatal = count_issues(pool, &project.id, None, Some("fatal")).await?;

    Ok(text_response(&json!({
        "project": project.slug,
        "total_issues": total,
        "unresolved": unresolved,
        "errors": errors,
        "fatal": fatal,
    })))
}

/// Count issues for a project with optional status/level filters (parameterized).
async fn count_issues(
    pool: &SqlitePool,
    project_id: &str,
    status: Option<&str>,
    level: Option<&str>,
) -> Result<i64, String> {
    let mut sql = String::from("SELECT COUNT(*) FROM issues WHERE project_id = ?");
    if status.is_some() {
        sql.push_str(" AND status = ?");
    }
    if level.is_some() {
        sql.push_str(" AND level = ?");
    }

    let mut q = sqlx::query_scalar::<_, i64>(&sql).bind(project_id);
    if let Some(s) = status {
        q = q.bind(s);
    }
    if let Some(l) = level {
        q = q.bind(l);
    }
    q.fetch_one(pool).await.map_err(|e| e.to_string())
}

async fn tool_list_alert_rules(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
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

async fn tool_list_events(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
    let issue_id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 100);
    let events = store.list_events(issue_id, limit, 0).await.map_err(|e| e.to_string())?;
    let list: Vec<Value> =
        events.iter().map(|e| json!({ "id": e.id, "issue_id": e.issue_id, "received_at": e.received_at })).collect();
    Ok(text_response(&list))
}

async fn tool_rotate_dsn(args: Value, store: &trapfall_core::Store) -> Result<Value, String> {
    let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
    let project = resolve_slug(slug, store).await?;
    let new_key = store.rotate_dsn(&project.id).await.map_err(|e| e.to_string())?;
    Ok(text_msg(format!("DSN rotated for {}. New key: {}...{}", slug, &new_key[..8], &new_key[new_key.len() - 4..])))
}

async fn tool_healthcheck(pool: &SqlitePool) -> Result<Value, String> {
    let ok: i64 = sqlx::query_scalar("SELECT 1").fetch_one(pool).await.map_err(|e| e.to_string())?;
    Ok(text_msg(format!("Healthy (db={})", ok)))
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Mask DSN for MCP responses: show only first 8 and last 4 chars of key.
fn mask_dsn(dsn: &str) -> String {
    let at_pos = match dsn.find('@') {
        Some(p) => p,
        None => return "***masked***".to_string(),
    };
    let key = &dsn[..at_pos].trim_start_matches("https://");
    if key.len() <= 12 {
        format!("https://***@{}", &dsn[at_pos + 1..])
    } else {
        format!("https://{}...{}@{}", &key[..8], &key[key.len() - 4..], &dsn[at_pos + 1..])
    }
}
