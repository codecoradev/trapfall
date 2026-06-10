//! Store — CRUD operations for projects, issues, events.

use anyhow::Result;
use sqlx::SqlitePool;
use trapfall_proto::{Issue, IssueStatus, Level, Project, StoredEvent};

use crate::{generate_dsn, generate_dsn_with, new_id};

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ── Projects ────────────────────────────────────────────────────────

    pub async fn create_project(&self, slug: &str, name: &str) -> Result<Project> {
        self.create_project_with_host(slug, name, "localhost:3000").await
    }

    pub async fn create_project_with_host(&self, slug: &str, name: &str, host: &str) -> Result<Project> {
        let id = new_id();
        let dsn = generate_dsn_with(host);
        let dsn_key = extract_dsn_key(&dsn);
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO projects (id, slug, name, dsn_key, dsn, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(slug)
            .bind(name)
            .bind(&dsn_key)
            .bind(&dsn)
            .bind(&now)
            .execute(&self.pool)
            .await?;

        Ok(Project { id, slug: slug.to_string(), name: name.to_string(), dsn, created_at: now })
    }

    pub async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        let row =
            sqlx::query_as::<_, ProjectRow>("SELECT id, slug, name, dsn, created_at FROM projects WHERE slug = ?")
                .bind(slug)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(Into::into))
    }

    pub async fn get_project_by_dsn_key(&self, sentry_key: &str) -> Result<Option<Project>> {
        let row =
            sqlx::query_as::<_, ProjectRow>("SELECT id, slug, name, dsn, created_at FROM projects WHERE dsn_key = ?")
                .bind(sentry_key)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(Into::into))
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows =
            sqlx::query_as::<_, ProjectRow>("SELECT id, slug, name, dsn, created_at FROM projects ORDER BY created_at")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn rotate_dsn(&self, project_id: &str) -> Result<String> {
        let new_dsn = generate_dsn();
        let new_dsn_key = extract_dsn_key(&new_dsn);
        sqlx::query("UPDATE projects SET dsn = ?, dsn_key = ? WHERE id = ?")
            .bind(&new_dsn)
            .bind(&new_dsn_key)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(new_dsn)
    }

    // ── Issues ──────────────────────────────────────────────────────────

    pub async fn upsert_issue(
        &self,
        project_id: &str,
        fingerprint: &str,
        title: &str,
        culprit: Option<&str>,
        level: Level,
    ) -> Result<Issue> {
        let level_str = level_to_str(level);

        let existing = sqlx::query_as::<_, IssueRow>(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen FROM issues WHERE project_id = ? AND fingerprint = ?",
        )
        .bind(project_id)
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = existing {
            let new_count = row.count + 1;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE issues SET count = ?, last_seen = ?, level = ? WHERE id = ?")
                .bind(new_count)
                .bind(&now)
                .bind(&level_str)
                .bind(&row.id)
                .execute(&self.pool)
                .await?;

            Ok(Issue {
                id: row.id,
                project_id: row.project_id,
                fingerprint: row.fingerprint,
                title: row.title,
                culprit: row.culprit,
                status: str_to_status(&row.status),
                level,
                count: new_count,
                user_count: row.user_count,
                first_seen: row.first_seen,
                last_seen: now,
            })
        } else {
            let id = new_id();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO issues (id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen) VALUES (?, ?, ?, ?, ?, 'unresolved', ?, 1, 0, ?, ?)",
            )
            .bind(&id)
            .bind(project_id)
            .bind(fingerprint)
            .bind(title)
            .bind(culprit)
            .bind(&level_str)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;

            Ok(Issue {
                id,
                project_id: project_id.to_string(),
                fingerprint: fingerprint.to_string(),
                title: title.to_string(),
                culprit: culprit.map(|s| s.to_string()),
                status: IssueStatus::Unresolved,
                level,
                count: 1,
                user_count: 0,
                first_seen: now.clone(),
                last_seen: now,
            })
        }
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<Option<Issue>> {
        let row = sqlx::query_as::<_, IssueRow>(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen FROM issues WHERE id = ?",
        )
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    pub async fn list_issues(&self, project_id: &str, limit: i64, offset: i64) -> Result<Vec<Issue>> {
        let rows = sqlx::query_as::<_, IssueRow>(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen FROM issues WHERE project_id = ? ORDER BY last_seen DESC LIMIT ? OFFSET ?",
        )
        .bind(project_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// List issues with optional status and level filters (parameterized — no SQL injection risk).
    pub async fn list_issues_filtered(
        &self,
        project_id: &str,
        status: Option<&str>,
        level: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Issue>> {
        let mut sql = String::from(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, \
             count, user_count, first_seen, last_seen FROM issues WHERE project_id = ?",
        );
        if status.is_some() {
            sql.push_str(" AND status = ?");
        }
        if level.is_some() {
            sql.push_str(" AND level = ?");
        }
        sql.push_str(" ORDER BY last_seen DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query_as::<_, IssueRow>(&sql).bind(project_id);
        if let Some(s) = status {
            q = q.bind(s);
        }
        if let Some(l) = level {
            q = q.bind(l);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Count issues for a project with optional filters.
    pub async fn count_issues(&self, project_id: &str, status: Option<&str>, level: Option<&str>) -> Result<i64> {
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
        let count = q.fetch_one(&self.pool).await?;
        Ok(count)
    }

    pub async fn set_issue_status(&self, issue_id: &str, status: IssueStatus) -> Result<()> {
        let status_str = status_to_str(status);
        sqlx::query("UPDATE issues SET status = ? WHERE id = ?")
            .bind(&status_str)
            .bind(issue_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Events ──────────────────────────────────────────────────────────

    pub async fn insert_event(&self, issue_id: &str, project_id: &str, event_data: &str) -> Result<String> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO events (id, issue_id, project_id, data, received_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(issue_id)
            .bind(project_id)
            .bind(event_data)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    pub async fn list_events(&self, issue_id: &str, limit: i64, offset: i64) -> Result<Vec<StoredEvent>> {
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT id, issue_id, project_id, data, received_at FROM events WHERE issue_id = ? ORDER BY received_at DESC LIMIT ? OFFSET ?",
        )
        .bind(issue_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Count events for an issue.
    pub async fn count_events(&self, issue_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE issue_id = ?")
            .bind(issue_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    // ── Alert Rules ────────────────────────────────────────────────────

    pub async fn create_alert_rule(
        &self,
        project_id: &str,
        name: &str,
        conditions: &str,
        action_type: &str,
        action_config: &str,
        cooldown_seconds: i64,
    ) -> Result<trapfall_proto::AlertRule> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO alert_rules (id, project_id, name, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(name)
        .bind(conditions)
        .bind(action_type)
        .bind(action_config)
        .bind(cooldown_seconds)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(trapfall_proto::AlertRule {
            id,
            project_id: project_id.to_string(),
            name: name.to_string(),
            enabled: true,
            conditions: serde_json::from_str(conditions).unwrap_or(serde_json::Value::Object(Default::default())),
            action_type: action_type.to_string(),
            action_config: serde_json::from_str(action_config).unwrap_or(serde_json::Value::Object(Default::default())),
            cooldown_seconds,
            created_at: now.clone(),
            updated_at: now,
        })
    }

    pub async fn list_alert_rules(&self, project_id: &str) -> Result<Vec<trapfall_proto::AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at FROM alert_rules WHERE project_id = ? ORDER BY created_at",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_alert_rule(&self, rule_id: &str) -> Result<Option<trapfall_proto::AlertRule>> {
        let row = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at FROM alert_rules WHERE id = ?",
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn delete_alert_rule(&self, rule_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM alert_rules WHERE id = ?").bind(rule_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn toggle_alert_rule(&self, rule_id: &str, enabled: bool) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE alert_rules SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(&now)
            .bind(rule_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get all enabled rules for a project (used by rules engine).
    pub async fn get_enabled_rules_for_project(&self, project_id: &str) -> Result<Vec<trapfall_proto::AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at FROM alert_rules WHERE project_id = ? AND enabled = 1",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // ── Alert History ───────────────────────────────────────────────────

    pub async fn insert_alert_history(&self, rule_id: &str, project_id: &str, issue_id: &str) -> Result<String> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO alert_history (id, rule_id, project_id, issue_id, status, attempts, created_at) VALUES (?, ?, ?, ?, 'pending', 0, ?)",
        )
        .bind(&id)
        .bind(rule_id)
        .bind(project_id)
        .bind(issue_id)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn mark_alert_sent(&self, history_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE alert_history SET status = 'sent', sent_at = ?, attempts = attempts + 1 WHERE id = ?")
            .bind(&now)
            .bind(history_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_alert_failed(&self, history_id: &str, error: &str) -> Result<()> {
        sqlx::query("UPDATE alert_history SET status = 'failed', last_error = ?, attempts = attempts + 1 WHERE id = ?")
            .bind(error)
            .bind(history_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

// ── Row types (sqlx mapping) ────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    slug: String,
    name: String,
    dsn: String,
    created_at: String,
}

impl From<ProjectRow> for Project {
    fn from(r: ProjectRow) -> Self {
        Self { id: r.id, slug: r.slug, name: r.name, dsn: r.dsn, created_at: r.created_at }
    }
}

#[derive(sqlx::FromRow)]
struct IssueRow {
    id: String,
    project_id: String,
    fingerprint: String,
    title: String,
    culprit: Option<String>,
    status: String,
    level: String,
    count: i64,
    user_count: i64,
    first_seen: String,
    last_seen: String,
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
struct EventRow {
    id: String,
    issue_id: String,
    project_id: String,
    data: String,
    received_at: String,
}

impl From<EventRow> for StoredEvent {
    fn from(r: EventRow) -> Self {
        Self {
            id: r.id,
            issue_id: r.issue_id,
            project_id: r.project_id,
            data: serde_json::from_str(&r.data).unwrap_or(serde_json::Value::Null),
            received_at: r.received_at,
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct AlertRuleRow {
    id: String,
    project_id: String,
    name: String,
    enabled: bool,
    conditions: String,
    action_type: String,
    action_config: String,
    cooldown_seconds: i64,
    created_at: String,
    updated_at: String,
}

impl From<AlertRuleRow> for trapfall_proto::AlertRule {
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

/// Extract DSN key from DSN URL: `https://{key}@host/path` → `{key}`.
fn extract_dsn_key(dsn: &str) -> String {
    dsn.split('@').next().unwrap_or("").trim_start_matches("https://").to_string()
}

fn level_to_str(level: Level) -> String {
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

fn str_to_level(s: &str) -> Level {
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

fn status_to_str(status: IssueStatus) -> String {
    match status {
        IssueStatus::Unresolved => "unresolved",
        IssueStatus::Resolved => "resolved",
        IssueStatus::Ignored => "ignored",
        IssueStatus::Regression => "regression",
    }
    .to_string()
}

fn str_to_status(s: &str) -> IssueStatus {
    match s {
        "unresolved" => IssueStatus::Unresolved,
        "resolved" => IssueStatus::Resolved,
        "ignored" => IssueStatus::Ignored,
        "regression" => IssueStatus::Regression,
        _ => IssueStatus::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dsn_key() {
        assert_eq!(extract_dsn_key("https://abc123@trapfall.example.com/1"), "abc123");
        assert_eq!(extract_dsn_key("https://key456@localhost:9090/42"), "key456");
        // No @ → split returns whole string, trim_start_matches strips nothing
        assert_eq!(extract_dsn_key("malformed"), "malformed");
        assert_eq!(extract_dsn_key(""), "");
    }

    #[tokio::test]
    async fn test_rotate_dsn_updates_dsn_key() {
        let pool = crate::open_pool("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        let store = Store::new(pool);

        let project = store.create_project("test", "Test Project").await.unwrap();
        let original_dsn = project.dsn.clone();
        let original_key = extract_dsn_key(&original_dsn);

        // Verify original dsn_key matches
        let found = store.get_project_by_dsn_key(&original_key).await.unwrap();
        assert!(found.is_some());

        // Rotate
        let new_dsn = store.rotate_dsn(&project.id).await.unwrap();
        assert_ne!(new_dsn, original_dsn);

        let new_key = extract_dsn_key(&new_dsn);
        assert_ne!(new_key, original_key);

        // Old key should NOT find the project anymore
        let old_lookup = store.get_project_by_dsn_key(&original_key).await.unwrap();
        assert!(old_lookup.is_none(), "Old DSN key should be revoked after rotation");

        // New key should find the project
        let new_lookup = store.get_project_by_dsn_key(&new_key).await.unwrap();
        assert!(new_lookup.is_some(), "New DSN key should work after rotation");
    }
}
