//! SQLite backend — implements [`Database`] using `sqlx::SqlitePool`.
//!
//! All SQLite-specific SQL lives here. Phase 1 is a mechanical extraction
//! of the queries previously scattered across `store.rs`, `auth.rs`, and
//! `search.rs`. Zero behavior change.

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};
use trapfall_proto::{AlertRule, Issue, IssueStatus, Level, Project, StoredEvent};

use crate::common::*;
use crate::{Database, StoredSession, StoredUser};

// ── Backend handle ─────────────────────────────────────────────────────

/// SQLite backend wrapping a connection pool.
#[derive(Clone)]
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

// ── Trait impl ─────────────────────────────────────────────────────────

#[async_trait]
impl Database for SqliteBackend {
    fn sqlite_pool(&self) -> Result<&SqlitePool> {
        Ok(&self.pool)
    }

    async fn run_migrations(&self) -> Result<()> {
        crate::run_sqlite_migrations(&self.pool).await
    }

    // ── Projects ───────────────────────────────────────────────────────

    async fn create_project(&self, slug: &str, name: &str) -> Result<Project> {
        self.create_project_with_host(slug, name, "localhost:3000").await
    }

    async fn create_project_with_host(&self, slug: &str, name: &str, host: &str) -> Result<Project> {
        let id = new_id();
        let dsn = generate_dsn_with(host, &id);
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

        Ok(Project { id, slug: slug.to_string(), name: name.to_string(), dsn, created_at: now, archived_at: None })
    }

    async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects WHERE slug = ?",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn get_project_by_id(&self, id: &str) -> Result<Option<Project>> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn get_project_by_dsn_key(&self, sentry_key: &str) -> Result<Option<Project>> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects WHERE dsn_key = ?",
        )
        .bind(sentry_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn rotate_dsn(&self, project_id: &str) -> Result<String> {
        let project = self.get_project_by_id(project_id).await?.ok_or_else(|| anyhow::anyhow!("Project not found"))?;
        let host = project
            .dsn
            .split('@')
            .nth(1)
            .map(|s| s.split('/').next().unwrap_or("localhost:3000"))
            .unwrap_or("localhost:3000");
        let new_dsn = generate_dsn_with(host, project_id);
        let new_dsn_key = extract_dsn_key(&new_dsn);
        sqlx::query("UPDATE projects SET dsn = ?, dsn_key = ? WHERE id = ?")
            .bind(&new_dsn)
            .bind(&new_dsn_key)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(new_dsn)
    }

    async fn archive_project(&self, project_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE projects SET archived_at = ? WHERE id = ?")
            .bind(&now)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn unarchive_project(&self, project_id: &str) -> Result<()> {
        sqlx::query("UPDATE projects SET archived_at = NULL WHERE id = ?").bind(project_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_project(&self, project_id: &str) -> Result<bool> {
        // Atomic: all-or-nothing. Partial deletes would leave orphaned rows
        // (e.g. project gone but events remain).
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM events WHERE project_id = ?").bind(project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM issues WHERE project_id = ?").bind(project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM alert_history WHERE project_id = ?").bind(project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM alert_rules WHERE project_id = ?").bind(project_id).execute(&mut *tx).await?;
        let result = sqlx::query("DELETE FROM projects WHERE id = ?").bind(project_id).execute(&mut *tx).await?;
        let affected = result.rows_affected() > 0;
        tx.commit().await?;
        Ok(affected)
    }

    async fn update_project(&self, project_id: &str, name: &str) -> Result<Project> {
        sqlx::query("UPDATE projects SET name = ? WHERE id = ?")
            .bind(name)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        self.get_project_by_id(project_id).await?.ok_or_else(|| anyhow::anyhow!("Project not found after update"))
    }

    async fn set_project_webhook(&self, project_slug: &str, webhook_url: &str) -> Result<()> {
        sqlx::query("UPDATE projects SET webhook_url = ? WHERE slug = ?")
            .bind(webhook_url)
            .bind(project_slug)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Issues ─────────────────────────────────────────────────────────

    async fn upsert_issue(
        &self,
        project_id: &str,
        fingerprint: &str,
        title: &str,
        culprit: Option<&str>,
        level: Level,
    ) -> Result<Issue> {
        let level_str = level_to_str(level);
        let now = chrono::Utc::now().to_rfc3339();

        // Atomic upsert via ON CONFLICT — avoids the SELECT-then-UPDATE race
        // where two concurrent ingests could both read count=N and both write
        // N+1 (lost update). The unique constraint on (project_id, fingerprint)
        // makes this safe.
        let row = sqlx::query_as::<_, IssueRow>(
            "INSERT INTO issues (id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen) \n             VALUES (?, ?, ?, ?, ?, 'unresolved', ?, 1, 0, ?, ?) \n             ON CONFLICT(project_id, fingerprint) DO UPDATE SET \n                 count = count + 1,\n                 last_seen = excluded.last_seen,\n                 level = excluded.level\n             RETURNING id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen",
        )
        .bind(new_id())
        .bind(project_id)
        .bind(fingerprint)
        .bind(title)
        .bind(culprit)
        .bind(&level_str)
        .bind(&now)
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;

        Ok(Issue {
            id: row.id,
            project_id: row.project_id,
            fingerprint: row.fingerprint,
            title: row.title,
            culprit: row.culprit,
            status: str_to_status(&row.status),
            level,
            count: row.count,
            user_count: row.user_count,
            first_seen: row.first_seen,
            last_seen: row.last_seen,
        })
    }

    async fn get_issue(&self, issue_id: &str) -> Result<Option<Issue>> {
        let row = sqlx::query_as::<_, IssueRow>(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen FROM issues WHERE id = ?",
        )
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn list_issues(&self, project_id: &str, limit: i64, offset: i64) -> Result<Vec<Issue>> {
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

    async fn list_issues_filtered(
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

    async fn count_issues(&self, project_id: &str, status: Option<&str>, level: Option<&str>) -> Result<i64> {
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

    async fn set_issue_status(&self, issue_id: &str, status: IssueStatus) -> Result<()> {
        let status_str = status_to_str(status);
        sqlx::query("UPDATE issues SET status = ? WHERE id = ?")
            .bind(&status_str)
            .bind(issue_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Events ─────────────────────────────────────────────────────────

    async fn insert_event(&self, issue_id: &str, project_id: &str, event_data: &str) -> Result<String> {
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

    async fn list_events(&self, issue_id: &str, limit: i64, offset: i64) -> Result<Vec<StoredEvent>> {
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

    async fn count_events(&self, issue_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE issue_id = ?")
            .bind(issue_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    // ── Transactions ───────────────────────────────────────────────────

    async fn insert_transaction(&self, project_id: &str, transaction: &trapfall_proto::Transaction) -> Result<String> {
        let id = new_id();
        let duration_ms = (transaction.timestamp - transaction.start_timestamp) * 1000.0;
        let status_str = span_status_to_str(trapfall_proto::SpanStatus::Ok);
        let data = serde_json::to_string(transaction).unwrap_or_else(|_| "{}".to_string());
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO transactions (id, project_id, name, release, environment, duration_ms, status, data, received_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(&transaction.transaction)
        .bind(&transaction.release)
        .bind(&transaction.environment)
        .bind(duration_ms)
        .bind(&status_str)
        .bind(&data)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        for span in &transaction.spans {
            let span_id = new_id();
            let start_offset_ms = (span.start_timestamp - transaction.start_timestamp) * 1000.0;
            let span_duration_ms = (span.timestamp - span.start_timestamp) * 1000.0;
            let span_status = span_status_to_str(span.status);
            let span_data = serde_json::to_string(span).unwrap_or_else(|_| "{}".to_string());
            sqlx::query(
                "INSERT INTO transaction_spans (id, transaction_id, span_id, trace_id, parent_span_id, op, description, start_offset_ms, duration_ms, status, data) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&span_id)
            .bind(&id)
            .bind(&span.span_id)
            .bind(&span.trace_id)
            .bind(&span.parent_span_id)
            .bind(&span.op)
            .bind(&span.description)
            .bind(start_offset_ms)
            .bind(span_duration_ms)
            .bind(&span_status)
            .bind(&span_data)
            .execute(&self.pool)
            .await?;
        }

        Ok(id)
    }

    async fn list_transactions(
        &self,
        project_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::common::TransactionRow>> {
        let rows = sqlx::query_as::<_, crate::common::TransactionRow>(
            "SELECT * FROM transactions WHERE project_id = ? ORDER BY received_at DESC LIMIT ? OFFSET ?",
        )
        .bind(project_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn get_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<Option<(crate::common::TransactionRow, Vec<crate::common::SpanRow>)>> {
        let tx_row = sqlx::query_as::<_, crate::common::TransactionRow>("SELECT * FROM transactions WHERE id = ?")
            .bind(transaction_id)
            .fetch_optional(&self.pool)
            .await?;

        match tx_row {
            Some(row) => {
                let spans = sqlx::query_as::<_, crate::common::SpanRow>(
                    "SELECT * FROM transaction_spans WHERE transaction_id = ?",
                )
                .bind(transaction_id)
                .fetch_all(&self.pool)
                .await?;
                Ok(Some((row, spans)))
            }
            None => Ok(None),
        }
    }

    async fn count_transactions(&self, project_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transactions WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    // ── Release Health ────────────────────────────────────────────────

    async fn insert_release_health(
        &self,
        project_id: &str,
        aggregates: &trapfall_proto::SessionAggregates,
    ) -> Result<usize> {
        let now = chrono::Utc::now().to_rfc3339();
        let mut count = 0usize;

        for item in &aggregates.aggregates {
            let id = new_id();
            sqlx::query(
                "INSERT INTO release_health (id, project_id, release, environment, started_at, distinct_id, exited, errored, abnormal, crashed, received_at)                  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(project_id)
            .bind(&aggregates.attributes.release)
            .bind(&aggregates.attributes.environment)
            .bind(&item.started)
            .bind(&item.distinct_id)
            .bind(item.exited as i64)
            .bind(item.errored as i64)
            .bind(item.abnormal as i64)
            .bind(item.crashed as i64)
            .bind(&now)
            .execute(&self.pool)
            .await?;
            count += 1;
        }

        Ok(count)
    }

    async fn get_crash_rate(&self, project_id: &str, release: Option<&str>, env: Option<&str>) -> Result<Option<f64>> {
        let mut sql = String::from(
            "SELECT SUM(crashed), SUM(exited + errored + abnormal + crashed)              FROM release_health WHERE project_id = ?",
        );
        if release.is_some() {
            sql.push_str(" AND release = ?");
        }
        if env.is_some() {
            sql.push_str(" AND environment = ?");
        }

        let mut q = sqlx::query_as::<_, (i64, i64)>(&sql).bind(project_id);
        if let Some(r) = release {
            q = q.bind(r);
        }
        if let Some(e) = env {
            q = q.bind(e);
        }

        let row = q.fetch_optional(&self.pool).await?;

        match row {
            Some((crashed, total)) if total > 0 => Ok(Some((crashed as f64 / total as f64) * 100.0)),
            _ => Ok(None),
        }
    }

    async fn list_release_health(
        &self,
        project_id: &str,
        release: Option<&str>,
        env: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::common::ReleaseHealthRow>> {
        let mut sql = String::from(
            "SELECT id, project_id, release, environment, started_at, distinct_id, exited, errored, abnormal, crashed, received_at              FROM release_health WHERE project_id = ?",
        );
        if release.is_some() {
            sql.push_str(" AND release = ?");
        }
        if env.is_some() {
            sql.push_str(" AND environment = ?");
        }
        sql.push_str(" ORDER BY received_at DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query_as::<_, crate::common::ReleaseHealthRow>(&sql).bind(project_id);
        if let Some(r) = release {
            q = q.bind(r);
        }
        if let Some(e) = env {
            q = q.bind(e);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows)
    }

    async fn count_release_health(&self, project_id: &str, release: Option<&str>, env: Option<&str>) -> Result<i64> {
        let mut sql = String::from("SELECT COUNT(*) FROM release_health WHERE project_id = ?");
        if release.is_some() {
            sql.push_str(" AND release = ?");
        }
        if env.is_some() {
            sql.push_str(" AND environment = ?");
        }

        let mut q = sqlx::query_scalar::<_, i64>(&sql).bind(project_id);
        if let Some(r) = release {
            q = q.bind(r);
        }
        if let Some(e) = env {
            q = q.bind(e);
        }

        let count = q.fetch_one(&self.pool).await?;
        Ok(count)
    }

    // ── Alert Rules ────────────────────────────────────────────────────

    async fn create_alert_rule(
        &self,
        project_id: &str,
        name: &str,
        conditions: &str,
        action_type: &str,
        action_config: &str,
        cooldown_seconds: i64,
    ) -> Result<AlertRule> {
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

        Ok(AlertRule {
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

    async fn list_alert_rules(&self, project_id: &str) -> Result<Vec<AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at FROM alert_rules WHERE project_id = ? ORDER BY created_at",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_alert_rule(&self, rule_id: &str) -> Result<Option<AlertRule>> {
        let row = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at FROM alert_rules WHERE id = ?",
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_alert_rule(&self, rule_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM alert_rules WHERE id = ?").bind(rule_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn toggle_alert_rule(&self, rule_id: &str, enabled: bool) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE alert_rules SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled)
            .bind(&now)
            .bind(rule_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_enabled_rules_for_project(&self, project_id: &str) -> Result<Vec<AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at FROM alert_rules WHERE project_id = ? AND enabled = 1",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // ── Alert History ───────────────────────────────────────────────────

    async fn insert_alert_history(&self, rule_id: &str, project_id: &str, issue_id: &str) -> Result<String> {
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

    async fn mark_alert_sent(&self, history_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE alert_history SET status = 'sent', sent_at = ?, attempts = attempts + 1 WHERE id = ?")
            .bind(&now)
            .bind(history_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_alert_failed(&self, history_id: &str, error: &str) -> Result<()> {
        sqlx::query("UPDATE alert_history SET status = 'failed', last_error = ?, attempts = attempts + 1 WHERE id = ?")
            .bind(error)
            .bind(history_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Search ─────────────────────────────────────────────────────────

    async fn search_issues(
        &self,
        query: &str,
        project_id: Option<&str>,
        status: Option<&str>,
        level: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Issue>> {
        let pattern = format!("%{}%", escape_like_sqlite(query));

        let sql_base = "SELECT id, project_id, fingerprint, title, culprit, status, level, \
             count, user_count, first_seen, last_seen FROM issues WHERE (title LIKE ? ESCAPE '!' OR culprit LIKE ? ESCAPE '!')";

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

        let where_ext = if conds.is_empty() { String::new() } else { format!(" AND {}", conds.join(" AND ")) };
        let full_sql = format!("{sql_base}{where_ext} ORDER BY last_seen DESC LIMIT ? OFFSET ?");

        let mut q = sqlx::query(&full_sql);
        for b in &bindings {
            q = q.bind(b);
        }
        q = q.bind(limit).bind(offset);

        let rows = q.fetch_all(&self.pool).await?;

        let mut issues = Vec::new();
        for row in rows {
            let id: String = row.try_get("id")?;
            let pid: String = row.try_get("project_id")?;
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
                project_id: pid,
                fingerprint,
                title,
                culprit,
                status: serde_json::from_value(serde_json::Value::String(status_str))
                    .unwrap_or(IssueStatus::Unresolved),
                level: serde_json::from_value(serde_json::Value::String(level_str)).unwrap_or(Level::Error),
                count,
                user_count,
                first_seen,
                last_seen,
            });
        }

        Ok(issues)
    }

    async fn count_search_issues(
        &self,
        query: &str,
        project_id: Option<&str>,
        status: Option<&str>,
        level: Option<&str>,
    ) -> Result<i64> {
        let pattern = format!("%{}%", escape_like_sqlite(query));

        let sql_base = "SELECT COUNT(*) FROM issues WHERE (title LIKE ? ESCAPE '!' OR culprit LIKE ? ESCAPE '!')";

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

        let where_ext = if conds.is_empty() { String::new() } else { format!(" AND {}", conds.join(" AND ")) };
        let full_sql = format!("{sql_base}{where_ext}");

        let mut q = sqlx::query_scalar::<_, i64>(&full_sql);
        for b in &bindings {
            q = q.bind(b);
        }

        let count = q.fetch_one(&self.pool).await?;
        Ok(count)
    }

    // ── Metrics ────────────────────────────────────────────────────────

    async fn count_table(&self, table: &str) -> Result<i64> {
        let allowed = ["issues", "events", "projects", "alert_rules", "alert_history"];
        if !allowed.contains(&table) {
            return Ok(0);
        }
        let query = format!("SELECT COUNT(*) as count FROM {table}");
        let row: (i64,) = sqlx::query_as(&query).fetch_one(&self.pool).await.unwrap_or((0,));
        Ok(row.0)
    }

    // ── Retention ──────────────────────────────────────────────────────

    async fn purge_old_events(&self, days: i64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM events WHERE received_at < datetime('now', ?)")
            .bind(format!("-{days} days"))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn purge_orphan_issues(&self) -> Result<()> {
        sqlx::query("DELETE FROM issues WHERE id NOT IN (SELECT DISTINCT issue_id FROM events)")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn purge_stale_auth_attempts(&self) -> Result<()> {
        sqlx::query("DELETE FROM auth_attempts WHERE created_at < datetime('now', '-30 days')")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Alert Cooldown ─────────────────────────────────────────────────

    async fn is_rule_cooling_down(&self, rule_id: &str, cooldown_seconds: i64) -> Result<bool> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT created_at FROM alert_history WHERE rule_id = ? AND status = 'sent' ORDER BY created_at DESC LIMIT 1",
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((last_fired,)) = row {
            if let Ok(fired_time) = chrono::DateTime::parse_from_rfc3339(&last_fired) {
                let elapsed =
                    chrono::Utc::now().signed_duration_since(fired_time.with_timezone(&chrono::Utc)).num_seconds();
                return Ok(elapsed < cooldown_seconds);
            }
        }

        Ok(false)
    }

    // ── Health ─────────────────────────────────────────────────────────

    async fn ping(&self) -> Result<bool> {
        let ok: i64 = sqlx::query_scalar("SELECT 1").fetch_one(&self.pool).await?;
        Ok(ok == 1)
    }

    // ── Attachments ───────────────────────────────────────────────────

    async fn insert_attachment(&self, row: &crate::common::AttachmentRow) -> Result<String> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO attachments (id, event_id, project_id, filename, content_type, attachment_type, size_bytes, disk_path, created_at)              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&row.event_id)
        .bind(&row.project_id)
        .bind(&row.filename)
        .bind(&row.content_type)
        .bind(&row.attachment_type)
        .bind(row.size_bytes)
        .bind(&row.disk_path)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    async fn list_attachments_by_event(&self, event_id: &str) -> Result<Vec<crate::common::AttachmentRow>> {
        let rows = sqlx::query_as::<_, crate::common::AttachmentRow>(
            "SELECT id, event_id, project_id, filename, content_type, attachment_type, size_bytes, disk_path, created_at              FROM attachments WHERE event_id = ? ORDER BY created_at",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn get_attachment(&self, id: &str) -> Result<Option<crate::common::AttachmentRow>> {
        let row = sqlx::query_as::<_, crate::common::AttachmentRow>(
            "SELECT id, event_id, project_id, filename, content_type, attachment_type, size_bytes, disk_path, created_at              FROM attachments WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn delete_attachment(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM attachments WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Auth ───────────────────────────────────────────────────────────

    async fn has_users(&self) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_optional(&self.pool).await?;
        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    async fn create_user(&self, email: &str, name: &str, password_hash: &str) -> Result<()> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, created_at) VALUES (?, ?, ?, ?, 'admin', ?)",
        )
        .bind(&id)
        .bind(email)
        .bind(name)
        .bind(password_hash)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<StoredUser>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, name, password_hash, role, created_at FROM users WHERE email = ?",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn get_user_by_id(&self, id: &str) -> Result<Option<StoredUser>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, name, password_hash, role, created_at FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_session(&self, user_id: &str, token: &str, expires_at: &str) -> Result<()> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO sessions (id, user_id, token, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(user_id)
            .bind(token)
            .bind(expires_at)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_session(&self, token: &str) -> Result<Option<StoredSession>> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, user_id, token, expires_at, created_at FROM sessions WHERE token = ?",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_session(&self, token: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token = ?").bind(token).execute(&self.pool).await?;
        Ok(())
    }

    async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at < ?").bind(&now).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn record_auth_attempt(&self, email: &str, ip: &str, success: bool) -> Result<()> {
        let id = new_id();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO auth_attempts (id, email, ip, success, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(email)
            .bind(ip)
            .bind(success)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn count_failed_attempts_email(&self, email: &str, minutes: i64) -> Result<i64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM auth_attempts WHERE email = ? AND success = 0 AND created_at > ?")
                .bind(email)
                .bind(&cutoff)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(c,)| c).unwrap_or(0))
    }

    async fn count_failed_attempts_ip(&self, ip: &str, minutes: i64) -> Result<i64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM auth_attempts WHERE ip = ? AND success = 0 AND created_at > ?")
                .bind(ip)
                .bind(&cutoff)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(c,)| c).unwrap_or(0))
    }

    // ── Raw event fetch ────────────────────────────────────────────────

    async fn get_event_raw(&self, event_id: &str) -> Result<Option<StoredEvent>> {
        let row = sqlx::query_as::<_, EventRow>(
            "SELECT id, issue_id, project_id, data, received_at FROM events WHERE id = ?",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_backend() -> SqliteBackend {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(4).connect("sqlite::memory:").await.unwrap();
        sqlx::query(include_str!("../../trapfalld/migrations/20260606000001_initial.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../trapfalld/migrations/20260606000002_alert_rules.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../trapfalld/migrations/20260612000001_project_archive.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../trapfalld/migrations/20260613000001_transactions.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../trapfalld/migrations/20260627000001_release_health.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../trapfalld/migrations/20260627000002_attachments.sql"))
            .execute(&pool)
            .await
            .unwrap();
        SqliteBackend::new(pool)
    }

    #[tokio::test]
    async fn test_create_and_lookup_project() {
        let db = open_backend().await;
        let project = db.create_project("app", "App").await.unwrap();
        assert_eq!(project.slug, "app");

        let found = db.get_project_by_slug("app").await.unwrap();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn test_upsert_issue_increments_count() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        let issue = db.upsert_issue(&project.id, "fp1", "Boom", None, Level::Error).await.unwrap();
        assert_eq!(issue.count, 1);

        let issue2 = db.upsert_issue(&project.id, "fp1", "Boom", None, Level::Error).await.unwrap();
        assert_eq!(issue2.count, 2);
        assert_eq!(issue2.id, issue.id);
    }

    /// Verify the atomic ON CONFLICT upsert doesn't lose increments under
    /// concurrent writes. With the old SELECT-then-UPDATE path this would
    /// race and land on a count well below N; the atomic upsert should be
    /// lossless.
    #[tokio::test]
    async fn test_upsert_issue_concurrent_no_lost_updates() {
        let db = std::sync::Arc::new(open_backend().await);
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        const N: usize = 50;
        let mut handles = Vec::with_capacity(N);
        for _ in 0..N {
            let db = db.clone();
            let pid = project.id.clone();
            handles.push(tokio::spawn(
                async move { db.upsert_issue(&pid, "fp-shared", "Boom", None, Level::Error).await },
            ));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }

        let issue = db.list_issues(&project.id, 100, 0).await.unwrap().pop().expect("issue must exist");
        assert_eq!(issue.count, N as i64, "concurrent upserts must not lose increments");
    }

    /// `delete_project` must be atomic: if any child-row DELETE fails, no
    /// rows should be removed. We verify the happy path fully clears every
    /// related table.
    #[tokio::test]
    async fn test_delete_project_removes_all_related_rows() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        let issue = db.upsert_issue(&project.id, "fp1", "Boom", None, Level::Error).await.unwrap();
        db.insert_event(&issue.id, &project.id, "{}").await.unwrap();

        let deleted = db.delete_project(&project.id).await.unwrap();
        assert!(deleted, "project should be deleted");

        // Project + its issues + events should all be gone.
        assert!(db.get_project_by_id(&project.id).await.unwrap().is_none());
        assert!(db.list_issues(&project.id, 100, 0).await.unwrap().is_empty());
        assert!(db.list_events(&issue.id, 100, 0).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_search_and_count() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();
        db.upsert_issue(&project.id, "fp1", "DatabaseError: connection lost", None, Level::Error).await.unwrap();
        db.upsert_issue(&project.id, "fp2", "AuthError: bad token", None, Level::Warning).await.unwrap();

        let results = db.search_issues("Database", None, None, None, 10, 0).await.unwrap();
        assert_eq!(results.len(), 1);

        let count = db.count_search_issues("error", None, None, None).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_count_table_whitelist() {
        let db = open_backend().await;
        assert_eq!(db.count_table("projects").await.unwrap(), 0);
        assert_eq!(db.count_table("DROP TABLE users").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_ping() {
        let db = open_backend().await;
        assert!(db.ping().await.unwrap());
    }

    // ── Pagination edge cases (#221) ───────────────────────────────────

    /// Helper: seed a project with `n` unique issues and return the project ID.
    async fn seed_project_with_issues(db: &SqliteBackend, slug: &str, n: usize) -> String {
        db.create_project(slug, slug).await.unwrap();
        let project = db.get_project_by_slug(slug).await.unwrap().unwrap();
        for i in 0..n {
            db.upsert_issue(&project.id, &format!("fp-{i}"), &format!("Error {i}"), None, Level::Error).await.unwrap();
        }
        project.id
    }

    /// `page=1` (offset=0): standard first page returns exactly `limit`
    /// results when enough data exists, ordered by last_seen DESC.
    #[tokio::test]
    async fn test_pagination_first_page() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 15).await;

        let page = db.list_issues(&pid, 5, 0).await.unwrap();
        assert_eq!(page.len(), 5, "first page with limit=5 should return exactly 5 issues");
    }

    /// `page=0` should be clamped to page 1 by the caller. At the DB layer
    /// this translates to `offset=0`. Verify that a caller simulating
    /// `page.max(1)` logic gets offset=0 and identical results to page=1.
    #[tokio::test]
    async fn test_pagination_page_zero_clamped_to_offset_zero() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 10).await;

        // Simulate server-layer clamping: page = page.max(1), offset = (page-1)*limit
        let page_raw: i64 = 0;
        let page_clamped = page_raw.max(1);
        let limit: i64 = 5;
        let offset = (page_clamped - 1) * limit;

        let result = db.list_issues(&pid, limit, offset).await.unwrap();
        let first_page = db.list_issues(&pid, limit, 0).await.unwrap();

        assert_eq!(page_clamped, 1, "page=0 should be clamped to 1");
        assert_eq!(offset, 0, "offset should be 0 after clamping");
        assert_eq!(result.len(), 5);
        assert_eq!(
            result.iter().map(|i| &i.id).collect::<Vec<_>>(),
            first_page.iter().map(|i| &i.id).collect::<Vec<_>>(),
            "page=0 clamped should return the same results as page=1"
        );
    }

    /// Requesting a page number beyond available data returns an empty vec,
    /// not an error. With 15 issues and limit=5, page=10 (offset=45) is
    /// well past the end.
    #[tokio::test]
    async fn test_pagination_beyond_total_returns_empty() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 15).await;

        let result = db.list_issues(&pid, 5, 45).await.unwrap();
        assert!(result.is_empty(), "offset beyond total should return empty vec, not error");

        // Also test the exact boundary: offset == total returns empty.
        let boundary = db.list_issues(&pid, 5, 15).await.unwrap();
        assert!(boundary.is_empty(), "offset == total should return empty vec");
    }

    /// `per_page=0` (limit=0): SQLite returns zero rows without error.
    /// This documents the DB-layer contract — the server clamps
    /// per_page to a minimum via `per_page.min(100)` but doesn't enforce
    /// a floor, so we verify limit=0 is safe at the DB level.
    #[tokio::test]
    async fn test_pagination_limit_zero_returns_empty() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 10).await;

        let result = db.list_issues(&pid, 0, 0).await.unwrap();
        assert!(result.is_empty(), "limit=0 should return empty vec without error");
    }

    /// Large `per_page` (limit=100, the server max) returns all available
    /// results without error when total < limit.
    #[tokio::test]
    async fn test_pagination_large_limit_returns_all() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 12).await;

        let result = db.list_issues(&pid, 100, 0).await.unwrap();
        assert_eq!(result.len(), 12, "limit=100 with 12 issues should return all 12");

        // Also verify limit larger than total (e.g. 1000)
        let huge = db.list_issues(&pid, 1000, 0).await.unwrap();
        assert_eq!(huge.len(), 12, "limit=1000 with 12 issues should return all 12");
    }

    /// Consistency: the sum of issues across all paginated pages must
    /// equal `count_issues(project_id)`. This verifies no rows are
    /// duplicated or dropped across page boundaries.
    #[tokio::test]
    async fn test_pagination_total_consistent_with_count() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 15).await;

        let total = db.count_issues(&pid, None, None).await.unwrap();
        assert_eq!(total, 15, "count_issues should report 15");

        // Paginate through all issues with limit=6: pages → [6, 6, 3]
        let limit: i64 = 6;
        let mut collected = Vec::new();
        let mut offset: i64 = 0;
        loop {
            let page = db.list_issues(&pid, limit, offset).await.unwrap();
            if page.is_empty() {
                break;
            }
            collected.extend(page);
            offset += limit;
        }

        assert_eq!(collected.len(), 15, "pagination should collect exactly 15 issues");
        assert_eq!(collected.len() as i64, total, "sum of pages must equal count_issues");

        // Verify no duplicate IDs across pages.
        let mut ids: Vec<_> = collected.iter().map(|i| i.id.clone()).collect();
        ids.sort();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 15, "no duplicate issue IDs across pages");
    }

    /// Pagination with filtered queries: `list_issues_filtered` and
    /// `count_issues` with a level filter must be consistent across pages.
    #[tokio::test]
    async fn test_pagination_filtered_consistency() {
        let db = open_backend().await;
        let pid = seed_project_with_issues(&db, "pager", 10).await;
        // Add some warning-level issues
        for i in 0..5 {
            db.upsert_issue(&pid, &format!("fp-warn-{i}"), &format!("Warning {i}"), None, Level::Warning)
                .await
                .unwrap();
        }

        let total_errors = db.count_issues(&pid, None, Some("error")).await.unwrap();
        assert_eq!(total_errors, 10);

        let total_warnings = db.count_issues(&pid, None, Some("warning")).await.unwrap();
        assert_eq!(total_warnings, 5);

        // Paginate through error issues only
        let limit: i64 = 4;
        let mut collected = Vec::new();
        let mut offset: i64 = 0;
        loop {
            let page = db.list_issues_filtered(&pid, None, Some("error"), limit, offset).await.unwrap();
            if page.is_empty() {
                break;
            }
            collected.extend(page);
            offset += limit;
        }
        assert_eq!(collected.len(), 10);
        assert!(collected.iter().all(|i| matches!(i.level, Level::Error)));
    }

    // ── Transaction tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_insert_and_get_transaction() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        let tx = trapfall_proto::Transaction {
            event_id: "e1".into(),
            level: trapfall_proto::Level::Info,
            transaction: "GET /api/health".into(),
            start_timestamp: 1703894474.296,
            timestamp: 1703894474.891,
            release: Some("0.2.0".into()),
            environment: Some("production".into()),
            spans: vec![trapfall_proto::Span {
                span_id: "sp1".into(),
                trace_id: "tr1".into(),
                parent_span_id: None,
                op: Some("http.server".into()),
                description: Some("handle".into()),
                start_timestamp: 1703894474.296,
                timestamp: 1703894474.891,
                status: trapfall_proto::SpanStatus::Ok,
                tags: None,
                data: None,
            }],
            contexts: None,
            request: None,
            tags: None,
            extra: None,
        };

        let tx_id = db.insert_transaction(&project.id, &tx).await.unwrap();
        let (row, spans) = db.get_transaction(&tx_id).await.unwrap().unwrap();
        assert_eq!(row.name, "GET /api/health");
        assert_eq!(row.duration_ms.floor() as i64, 595);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].op.as_deref(), Some("http.server"));
    }

    #[tokio::test]
    async fn test_list_transactions_with_pagination() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        for i in 0..5 {
            let tx = trapfall_proto::Transaction {
                event_id: format!("e{i}"),
                level: trapfall_proto::Level::Info,
                transaction: format!("GET /api/{i}"),
                start_timestamp: 100.0,
                timestamp: 200.0,
                release: None,
                environment: None,
                spans: vec![],
                contexts: None,
                request: None,
                tags: None,
                extra: None,
            };
            db.insert_transaction(&project.id, &tx).await.unwrap();
        }

        let count = db.count_transactions(&project.id).await.unwrap();
        assert_eq!(count, 5);

        let page1 = db.list_transactions(&project.id, 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = db.list_transactions(&project.id, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);
    }

    #[tokio::test]
    async fn test_get_transaction_not_found() {
        let db = open_backend().await;
        let result = db.get_transaction("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    // ── Release Health tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_insert_and_query_release_health() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        let aggregates = trapfall_proto::SessionAggregates {
            aggregates: vec![
                trapfall_proto::SessionAggregateItem {
                    started: "2026-01-01T00:00:00Z".into(),
                    distinct_id: Some("user1".into()),
                    exited: 90,
                    errored: 5,
                    abnormal: 2,
                    crashed: 3,
                },
                trapfall_proto::SessionAggregateItem {
                    started: "2026-01-02T00:00:00Z".into(),
                    distinct_id: None,
                    exited: 50,
                    errored: 1,
                    abnormal: 0,
                    crashed: 0,
                },
            ],
            attributes: trapfall_proto::SessionAttributes {
                release: "1.0.0".into(),
                environment: Some("production".into()),
                ip_address: None,
                user_agent: None,
            },
        };

        let count = db.insert_release_health(&project.id, &aggregates).await.unwrap();
        assert_eq!(count, 2);

        let rows = db.list_release_health(&project.id, Some("1.0.0"), Some("production"), 10, 0).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].release, "1.0.0");
        assert_eq!(rows[0].environment.as_deref(), Some("production"));

        let total = db.count_release_health(&project.id, None, None).await.unwrap();
        assert_eq!(total, 2);

        let filtered = db.count_release_health(&project.id, Some("1.0.0"), Some("production")).await.unwrap();
        assert_eq!(filtered, 2);

        // Filter for non-existent release
        let none = db.count_release_health(&project.id, Some("2.0.0"), None).await.unwrap();
        assert_eq!(none, 0);
    }

    #[tokio::test]
    async fn test_crash_rate_calculation() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        let aggregates = trapfall_proto::SessionAggregates {
            aggregates: vec![trapfall_proto::SessionAggregateItem {
                started: "2026-01-01T00:00:00Z".into(),
                distinct_id: None,
                exited: 90,
                errored: 5,
                abnormal: 2,
                crashed: 3,
            }],
            attributes: trapfall_proto::SessionAttributes {
                release: "1.0.0".into(),
                environment: None,
                ip_address: None,
                user_agent: None,
            },
        };

        db.insert_release_health(&project.id, &aggregates).await.unwrap();

        // crash_rate = 3 / (90+5+2+3) * 100 = 3.0
        let rate = db.get_crash_rate(&project.id, None, None).await.unwrap().unwrap();
        assert!((rate - 3.0).abs() < 0.01, "expected ~3.0, got {rate}");

        // No data for a different project should return None
        let none = db.get_crash_rate("nonexistent", None, None).await.unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_release_health_pagination() {
        let db = open_backend().await;
        db.create_project("app", "App").await.unwrap();
        let project = db.get_project_by_slug("app").await.unwrap().unwrap();

        for i in 0..5 {
            let aggregates = trapfall_proto::SessionAggregates {
                aggregates: vec![trapfall_proto::SessionAggregateItem {
                    started: format!("2026-01-{:02}T00:00:00Z", i + 1),
                    distinct_id: None,
                    exited: 10,
                    errored: 0,
                    abnormal: 0,
                    crashed: 0,
                }],
                attributes: trapfall_proto::SessionAttributes {
                    release: "1.0.0".into(),
                    environment: Some("production".into()),
                    ip_address: None,
                    user_agent: None,
                },
            };
            db.insert_release_health(&project.id, &aggregates).await.unwrap();
        }

        let page1 = db.list_release_health(&project.id, None, None, 2, 0).await.unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = db.list_release_health(&project.id, None, None, 2, 2).await.unwrap();
        assert_eq!(page2.len(), 2);

        let page3 = db.list_release_health(&project.id, None, None, 2, 4).await.unwrap();
        assert_eq!(page3.len(), 1);

        let beyond = db.list_release_health(&project.id, None, None, 2, 5).await.unwrap();
        assert!(beyond.is_empty());
    }

    // ── Attachment tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_insert_and_query_attachment() {
        use crate::common::AttachmentRow;

        let db = open_backend().await;
        db.create_project("attproj", "Att Project").await.unwrap();
        let project = db.get_project_by_slug("attproj").await.unwrap().unwrap();

        let row = AttachmentRow {
            id: String::new(), // generated by backend
            event_id: "event-1".to_string(),
            project_id: project.id.clone(),
            filename: "screenshot.png".to_string(),
            content_type: Some("image/png".to_string()),
            attachment_type: Some("attachment".to_string()),
            size_bytes: 12345,
            disk_path: "data/attachments/proj-1/ab/abc123".to_string(),
            created_at: String::new(), // generated by backend
        };

        let id = db.insert_attachment(&row).await.unwrap();
        assert!(!id.is_empty());

        // Query by event
        let attachments = db.list_attachments_by_event("event-1").await.unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].id, id);
        assert_eq!(attachments[0].filename, "screenshot.png");
        assert_eq!(attachments[0].size_bytes, 12345);
        assert_eq!(attachments[0].content_type.as_deref(), Some("image/png"));

        // Query by id
        let found = db.get_attachment(&id).await.unwrap().unwrap();
        assert_eq!(found.filename, "screenshot.png");

        // Nonexistent
        let none = db.get_attachment("nonexistent").await.unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_delete_attachment() {
        use crate::common::AttachmentRow;

        let db = open_backend().await;
        db.create_project("delproj", "Del Project").await.unwrap();
        let project = db.get_project_by_slug("delproj").await.unwrap().unwrap();

        let row = AttachmentRow {
            id: String::new(),
            event_id: "event-del".to_string(),
            project_id: project.id,
            filename: "file.txt".to_string(),
            content_type: None,
            attachment_type: None,
            size_bytes: 42,
            disk_path: "data/attachments/del/d4/d4d4".to_string(),
            created_at: String::new(),
        };

        let id = db.insert_attachment(&row).await.unwrap();

        // Verify it exists
        let found = db.get_attachment(&id).await.unwrap();
        assert!(found.is_some());

        // Delete
        let deleted = db.delete_attachment(&id).await.unwrap();
        assert!(deleted);

        // Verify gone
        let gone = db.get_attachment(&id).await.unwrap();
        assert!(gone.is_none());

        // Delete again → false
        let again = db.delete_attachment(&id).await.unwrap();
        assert!(!again);
    }
}
