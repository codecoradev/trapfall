//! Search module — LIKE + trigram substring matching for issues.
//!
//! Uses SQLite LIKE for substring matching on title + culprit columns.
//! Optimized for error messages (100-500 chars, <100K rows).

use sqlx::{Row, SqlitePool};
use trapfall_proto::{Issue, Level};

/// Search issues by substring query with optional filters.
pub async fn search_issues(
    pool: &SqlitePool,
    query: &str,
    project_id: Option<&str>,
    status: Option<&str>,
    level: Option<&str>,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<Issue>> {
    let pattern = format!("%{query}%");

    let mut sql = String::from(
        "SELECT id, project_id, fingerprint, title, culprit, status, level, \
         count, user_count, first_seen, last_seen \
         FROM issues WHERE (title LIKE ?1 OR culprit LIKE ?1)",
    );

    let mut param_idx = 2u32;

    if project_id.is_some() {
        sql.push_str(&format!(" AND project_id = ?{param_idx}"));
        param_idx += 1;
    }
    if status.is_some() {
        sql.push_str(&format!(" AND status = ?{param_idx}"));
        param_idx += 1;
    }
    if level.is_some() {
        sql.push_str(&format!(" AND level = ?{param_idx}"));
        param_idx += 1;
    }

    sql.push_str(" ORDER BY last_seen DESC LIMIT ? OFFSET ?");

    // Build with dynamic filters using raw query
    let sql_base = "SELECT id, project_id, fingerprint, title, culprit, status, level, \
         count, user_count, first_seen, last_seen FROM issues WHERE (title LIKE ? OR culprit LIKE ?)";

    let mut bindings: Vec<String> = vec![pattern.clone(), pattern];
    let mut conds: Vec<String> = Vec::new();

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

    let where_ext =
        if conds.is_empty() { String::new() } else { format!(" AND {}", conds.join(" AND ")) };

    let full_sql = format!("{sql_base}{where_ext} ORDER BY last_seen DESC LIMIT ? OFFSET ?");

    // Use raw query for dynamic SQL
    let mut q = sqlx::query(&full_sql);
    for b in &bindings {
        q = q.bind(b);
    }
    q = q.bind(limit).bind(offset);

    let rows = q.fetch_all(pool).await?;

    // Parse manually
    let mut issues = Vec::new();
    for row in rows {
        let id: String = row.try_get("id")?;
        let project_id: String = row.try_get("project_id")?;
        let fingerprint: String = row.try_get("fingerprint")?;
        let title: String = row.try_get("title")?;
        let culprit: Option<String> = row.try_get("culprit")?;
        let status_str: String = row.try_get("status")?;
        let level_str: String = row.try_get("level")?;
        let count: i64 = row.try_get("count")?;
        let user_count: i64 = row.try_get("user_count")?;
        let first_seen: String = row.try_get("first_seen")?;
        let last_seen: String = row.try_get("last_seen")?;

        issues.push(Issue {
            id,
            project_id,
            fingerprint,
            title,
            culprit,
            status: serde_json::from_value(serde_json::Value::String(status_str))
                .unwrap_or(trapfall_proto::IssueStatus::Unresolved),
            level: serde_json::from_value(serde_json::Value::String(level_str))
                .unwrap_or(Level::Error),
            count: count as i64,
            user_count: user_count as i64,
            first_seen,
            last_seen,
        });
    }

    Ok(issues)
}
