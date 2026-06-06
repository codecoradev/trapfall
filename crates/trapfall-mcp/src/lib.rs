//! MCP server — stdio transport, AI agent tools.
//!
//! Implements JSON-RPC 2.0 over stdin/stdout for Model Context Protocol.
//! Spawned by AI agents as subprocess. No auth needed (process isolation).

use std::io::{BufRead, Write};

use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use trapfall_proto::IssueStatus;

use trapfall_core::Store;

/// Run the MCP server, reading JSON-RPC from stdin and writing to stdout.
pub async fn run_server(pool: SqlitePool) -> anyhow::Result<()> {
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

        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "error": {"code": -32700, "message": format!("Parse error: {e}")},
                    "id": null
                });
                writeln!(stdout, "{}", resp)?;
                stdout.flush()?;
                continue;
            }
        };

        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));

        let result = handle_request(method, params, &pool).await;

        let response = match result {
            Ok(val) => json!({
                "jsonrpc": "2.0",
                "result": val,
                "id": id
            }),
            Err(code_msg) => json!({
                "jsonrpc": "2.0",
                "error": {"code": -32603, "message": code_msg},
                "id": id
            }),
        };

        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }

    Ok(())
}

async fn handle_request(
    method: &str,
    params: Value,
    pool: &SqlitePool,
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
            let tool_name = params
                .get("name")
                .and_then(|n| n.as_str())
                .ok_or("missing tool name")?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            call_tool(tool_name, args, pool).await
        }
        "ping" => Ok(json!({})),
        _ => Err(format!("Unknown method: {method}")),
    }
}

fn tools_list() -> Vec<Value> {
    vec![
        json!({
            "name": "list_issues",
            "description": "List error issues for a project, optionally filtered by status and level",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_slug": { "type": "string", "description": "Project slug" },
                    "status": { "type": "string", "enum": ["unresolved","resolved","ignored"], "description": "Filter by status" },
                    "level": { "type": "string", "enum": ["fatal","error","warning","info","debug"], "description": "Filter by level" },
                    "limit": { "type": "integer", "description": "Max results (default 50)" }
                },
                "required": ["project_slug"]
            }
        }),
        json!({
            "name": "get_issue",
            "description": "Get a specific issue by ID",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "issue_id": { "type": "string", "description": "Issue ID" }
                },
                "required": ["issue_id"]
            }
        }),
        json!({
            "name": "get_event",
            "description": "Get a specific event with full stacktrace data",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "event_id": { "type": "string", "description": "Event ID" }
                },
                "required": ["event_id"]
            }
        }),
        json!({
            "name": "set_status",
            "description": "Set the status of an issue (resolve, ignore, unresolve)",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "issue_id": { "type": "string", "description": "Issue ID" },
                    "status": { "type": "string", "enum": ["resolved","ignored","unresolved"] }
                },
                "required": ["issue_id", "status"]
            }
        }),
        json!({
            "name": "search_issues",
            "description": "Search issues by substring in title or culprit",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "project_slug": { "type": "string", "description": "Optional project filter" },
                    "limit": { "type": "integer", "description": "Max results (default 20)" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "list_projects",
            "description": "List all projects",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_project",
            "description": "Get project details by slug",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Project slug" }
                },
                "required": ["slug"]
            }
        }),
        json!({
            "name": "get_project_stats",
            "description": "Get issue statistics for a project",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_slug": { "type": "string", "description": "Project slug" }
                },
                "required": ["project_slug"]
            }
        }),
        json!({
            "name": "list_alert_rules",
            "description": "List alert rules for a project",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_slug": { "type": "string", "description": "Project slug" }
                },
                "required": ["project_slug"]
            }
        }),
        json!({
            "name": "list_events",
            "description": "List events for a specific issue",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "issue_id": { "type": "string", "description": "Issue ID" },
                    "limit": { "type": "integer", "description": "Max results (default 20)" }
                },
                "required": ["issue_id"]
            }
        }),
        json!({
            "name": "rotate_dsn",
            "description": "Rotate the DSN key for a project",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_slug": { "type": "string", "description": "Project slug" }
                },
                "required": ["project_slug"]
            }
        }),
        json!({
            "name": "healthcheck",
            "description": "Check if the TrapFall server is healthy",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

async fn call_tool(
    name: &str,
    args: Value,
    pool: &SqlitePool,
) -> Result<Value, String> {
    let store = Store::new(pool.clone());

    match name {
        "list_issues" => {
            let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
            let project = store
                .get_project_by_slug(slug)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("project not found")?;

            let mut query = format!(
                "SELECT id, project_id, fingerprint, title, culprit, status, level, \
                 count, user_count, first_seen, last_seen FROM issues WHERE project_id = ?"
            );
            let mut bindings: Vec<String> = vec![project.id];

            if let Some(status) = args.get("status").and_then(|v| v.as_str()) {
                query.push_str(" AND status = ?");
                bindings.push(status.to_string());
            }
            if let Some(level) = args.get("level").and_then(|v| v.as_str()) {
                query.push_str(" AND level = ?");
                bindings.push(level.to_string());
            }

            let limit = args
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(50);
            query.push_str(" ORDER BY last_seen DESC LIMIT ?");
            bindings.push(limit.to_string());

            let mut q = sqlx::query(&query);
            for b in &bindings {
                q = q.bind(b);
            }
            let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;

            let mut issues = Vec::new();
            for row in rows {
                let title: String = row.try_get("title").map_err(|e| e.to_string())?;
                let id: String = row.try_get("id").map_err(|e| e.to_string())?;
                let status: String = row.try_get("status").map_err(|e| e.to_string())?;
                let level: String = row.try_get("level").map_err(|e| e.to_string())?;
                let count: i64 = row.try_get("count").map_err(|e| e.to_string())?;
                let last_seen: String = row.try_get("last_seen").map_err(|e| e.to_string())?;
                let culprit: Option<String> = row.try_get("culprit").map_err(|e| e.to_string())?;

                issues.push(json!({
                    "id": id,
                    "title": title,
                    "status": status,
                    "level": level,
                    "count": count,
                    "culprit": culprit,
                    "last_seen": last_seen,
                }));
            }

            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&issues).unwrap_or_default() }] }))
        }
        "get_issue" => {
            let id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
            let issue = store.get_issue(id).await.map_err(|e| e.to_string())?.ok_or("issue not found")?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&issue).unwrap_or_default() }] }))
        }
        "get_event" => {
            let event_id = args.get("event_id").and_then(|v| v.as_str()).ok_or("missing event_id")?;
            // Query events table for specific event
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
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({
                "id": id,
                "issue_id": issue_id,
                "received_at": received_at,
                "event": event_data
            })).unwrap_or_default() }] }))
        }
        "set_status" => {
            let issue_id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
            let status_str = args.get("status").and_then(|v| v.as_str()).ok_or("missing status")?;
            let status: IssueStatus = serde_json::from_value(json!(status_str))
                .map_err(|e: serde_json::Error| format!("invalid status: {e}"))?;
            store.set_issue_status(issue_id, status).await.map_err(|e| e.to_string())?;
            Ok(json!({ "content": [{ "type": "text", "text": format!("Issue {} status set to {}", issue_id, status_str) }] }))
        }
        "search_issues" => {
            let query = args.get("query").and_then(|v| v.as_str()).ok_or("missing query")?;
            let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

            let project_id = if let Some(slug) = args.get("project_slug").and_then(|v| v.as_str()) {
                store.get_project_by_slug(slug).await.map_err(|e| e.to_string())?.map(|p| p.id)
            } else {
                None
            };

            let issues = trapfall_search::search_issues(
                pool,
                query,
                project_id.as_deref(),
                None,
                None,
                limit,
                0,
            )
            .await
            .map_err(|e| e.to_string())?;

            let results: Vec<Value> = issues.iter().map(|i| json!({
                "id": i.id,
                "title": i.title,
                "status": format!("{:?}", i.status).to_lowercase(),
                "level": format!("{:?}", i.level).to_lowercase(),
                "count": i.count,
                "last_seen": i.last_seen,
            })).collect();

            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap_or_default() }] }))
        }
        "list_projects" => {
            let projects = store.list_projects().await.map_err(|e| e.to_string())?;
            let list: Vec<Value> = projects.iter().map(|p| json!({
                "id": p.id,
                "slug": p.slug,
                "name": p.name,
                "dsn": p.dsn_public,
            })).collect();
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&list).unwrap_or_default() }] }))
        }
        "get_project" => {
            let slug = args.get("slug").and_then(|v| v.as_str()).ok_or("missing slug")?;
            let project = store.get_project_by_slug(slug).await.map_err(|e| e.to_string())?.ok_or("project not found")?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({
                "id": project.id,
                "slug": project.slug,
                "name": project.name,
                "dsn": project.dsn_public,
            })).unwrap_or_default() }] }))
        }
        "get_project_stats" => {
            let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
            let project = store.get_project_by_slug(slug).await.map_err(|e| e.to_string())?.ok_or("project not found")?;

            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = ?")
                .bind(&project.id)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;
            let unresolved: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = ? AND status = 'unresolved'")
                .bind(&project.id)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;
            let errors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = ? AND level = 'error'")
                .bind(&project.id)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;
            let fatal: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = ? AND level = 'fatal'")
                .bind(&project.id)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;

            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&json!({
                "project": project.slug,
                "total_issues": total,
                "unresolved": unresolved,
                "errors": errors,
                "fatal": fatal,
            })).unwrap_or_default() }] }))
        }
        "list_alert_rules" => {
            let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
            let project = store.get_project_by_slug(slug).await.map_err(|e| e.to_string())?.ok_or("project not found")?;
            let rules = store.list_alert_rules(&project.id).await.map_err(|e| e.to_string())?;
            let list: Vec<Value> = rules.iter().map(|r| json!({
                "id": r.id,
                "name": r.name,
                "enabled": r.enabled,
                "conditions": r.conditions,
                "action_type": r.action_type,
                "cooldown_seconds": r.cooldown_seconds,
            })).collect();
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&list).unwrap_or_default() }] }))
        }
        "list_events" => {
            let issue_id = args.get("issue_id").and_then(|v| v.as_str()).ok_or("missing issue_id")?;
            let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
            let events = store.list_events(issue_id, limit, 0).await.map_err(|e| e.to_string())?;
            let list: Vec<Value> = events.iter().map(|e| json!({
                "id": e.id,
                "issue_id": e.issue_id,
                "received_at": e.received_at,
            })).collect();
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&list).unwrap_or_default() }] }))
        }
        "rotate_dsn" => {
            let slug = args.get("project_slug").and_then(|v| v.as_str()).ok_or("missing project_slug")?;
            let project = store.get_project_by_slug(slug).await.map_err(|e| e.to_string())?.ok_or("project not found")?;
            let new_key = store.rotate_dsn(&project.id).await.map_err(|e| e.to_string())?;
            Ok(json!({ "content": [{ "type": "text", "text": format!("DSN rotated for {}. New key: {}...{}", slug, &new_key[..8], &new_key[new_key.len()-4..]) }] }))
        }
        "healthcheck" => {
            let ok = sqlx::query_scalar("SELECT 1")
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "content": [{ "type": "text", "text": format!("Healthy (db={})", ok) }] }))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}
