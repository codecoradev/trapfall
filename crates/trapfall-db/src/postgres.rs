//! Postgres backend — implements [`Database`] using `sqlx::PgPool`.
//!
//! Mirrors `SqliteBackend` logic with Postgres dialect adjustments:
//! numbered params (`$1`), `ILIKE`, `BOOLEAN`, `ON CONFLICT` upsert.
//!
//! All shared row types and helpers live in [`crate::common`].

#![cfg_attr(not(feature = "postgres"), allow(unused))]

use anyhow::Result;
use async_trait::async_trait;
use sqlx::postgres::PgPool;
use trapfall_proto::{AlertRule, Issue, IssueStatus, Level, Project, StoredEvent};

use crate::common::*;
use crate::{Database, StoredSession, StoredUser};

// ── Backend handle ─────────────────────────────────────────────────────

/// Postgres backend wrapping a connection pool.
#[derive(Clone)]
pub struct PostgresBackend {
    pool: PgPool,
}

impl PostgresBackend {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

// ── Trait impl ─────────────────────────────────────────────────────────

#[async_trait]
impl Database for PostgresBackend {
    fn sqlite_pool(&self) -> Result<&sqlx::SqlitePool> {
        Err(anyhow::anyhow!("PostgresBackend does not expose a SqlitePool"))
    }

    async fn run_migrations(&self) -> Result<()> {
        crate::run_postgres_migrations(&self.pool).await
    }

    // ── Projects ───────────────────────────────────────────────────────

    async fn create_project(&self, slug: &str, name: &str) -> Result<Project> {
        self.create_project_with_host(slug, name, "localhost:9090").await
    }

    async fn create_project_with_host(&self, slug: &str, name: &str, host: &str) -> Result<Project> {
        let id = new_id();
        let dsn = generate_dsn_with(host, &id);
        let dsn_key = extract_dsn_key(&dsn);
        let now = now_rfc3339();
        sqlx::query("INSERT INTO projects (id, slug, name, dsn_key, dsn, created_at) VALUES ($1, $2, $3, $4, $5, $6)")
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
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn get_project_by_id(&self, id: &str) -> Result<Option<Project>> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn get_project_by_dsn_key(&self, sentry_key: &str) -> Result<Option<Project>> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, slug, name, dsn, created_at, archived_at FROM projects WHERE dsn_key = $1",
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
        let host = extract_dsn_host(&project.dsn);
        let new_dsn = generate_dsn_with(&host, project_id);
        let new_dsn_key = extract_dsn_key(&new_dsn);
        sqlx::query("UPDATE projects SET dsn = $1, dsn_key = $2 WHERE id = $3")
            .bind(&new_dsn)
            .bind(&new_dsn_key)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(new_dsn)
    }

    async fn archive_project(&self, project_id: &str) -> Result<()> {
        let now = now_rfc3339();
        sqlx::query("UPDATE projects SET archived_at = $1 WHERE id = $2")
            .bind(&now)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn unarchive_project(&self, project_id: &str) -> Result<()> {
        sqlx::query("UPDATE projects SET archived_at = NULL WHERE id = $1")
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_project(&self, project_id: &str) -> Result<bool> {
        // Atomic: all-or-nothing. Partial deletes would leave orphaned rows.
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM events WHERE project_id = $1").bind(project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM issues WHERE project_id = $1").bind(project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM alert_history WHERE project_id = $1").bind(project_id).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM alert_rules WHERE project_id = $1").bind(project_id).execute(&mut *tx).await?;
        let result = sqlx::query("DELETE FROM projects WHERE id = $1").bind(project_id).execute(&mut *tx).await?;
        let affected = result.rows_affected() > 0;
        tx.commit().await?;
        Ok(affected)
    }

    async fn update_project(&self, project_id: &str, name: &str) -> Result<Project> {
        sqlx::query("UPDATE projects SET name = $1 WHERE id = $2")
            .bind(name)
            .bind(project_id)
            .execute(&self.pool)
            .await?;
        self.get_project_by_id(project_id).await?.ok_or_else(|| anyhow::anyhow!("Project not found after update"))
    }

    async fn set_project_webhook(&self, project_slug: &str, webhook_url: &str) -> Result<()> {
        sqlx::query("UPDATE projects SET webhook_url = $1 WHERE slug = $2")
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
        let id = new_id();
        let now = now_rfc3339();

        // INSERT ... ON CONFLICT DO UPDATE — returns the row in both cases.
        let row = sqlx::query_as::<_, IssueRow>(
            "INSERT INTO issues (id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen) \
             VALUES ($1, $2, $3, $4, $5, 'unresolved', $6, 1, 0, $7, $7) \
             ON CONFLICT (project_id, fingerprint) DO UPDATE SET \
                 count = issues.count + 1, last_seen = $7, level = $6 \
             RETURNING id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen",
        )
        .bind(&id)
        .bind(project_id)
        .bind(fingerprint)
        .bind(title)
        .bind(culprit)
        .bind(&level_str)
        .bind(&now)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    async fn get_issue(&self, issue_id: &str) -> Result<Option<Issue>> {
        let row = sqlx::query_as::<_, IssueRow>(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen FROM issues WHERE id = $1",
        )
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn list_issues(&self, project_id: &str, limit: i64, offset: i64) -> Result<Vec<Issue>> {
        let rows = sqlx::query_as::<_, IssueRow>(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen \
             FROM issues WHERE project_id = $1 ORDER BY last_seen DESC LIMIT $2 OFFSET $3",
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
             count, user_count, first_seen, last_seen FROM issues WHERE project_id = $1",
        );
        let mut idx = 2usize;
        if status.is_some() {
            sql.push_str(&format!(" AND status = ${idx}"));
            idx += 1;
        }
        if level.is_some() {
            sql.push_str(&format!(" AND level = ${idx}"));
            idx += 1;
        }
        sql.push_str(&format!(" ORDER BY last_seen DESC LIMIT ${idx} OFFSET ${}", idx + 1));

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
        let mut sql = String::from("SELECT COUNT(*) FROM issues WHERE project_id = $1");
        let mut idx = 2usize;
        if status.is_some() {
            sql.push_str(&format!(" AND status = ${idx}"));
            idx += 1;
        }
        if level.is_some() {
            sql.push_str(&format!(" AND level = ${idx}"));
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
        sqlx::query("UPDATE issues SET status = $1 WHERE id = $2")
            .bind(&status_str)
            .bind(issue_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ── Events ─────────────────────────────────────────────────────────

    async fn insert_event(&self, issue_id: &str, project_id: &str, event_data: &str) -> Result<String> {
        let id = new_id();
        let now = now_rfc3339();
        sqlx::query("INSERT INTO events (id, issue_id, project_id, data, received_at) VALUES ($1, $2, $3, $4, $5)")
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
            "SELECT id, issue_id, project_id, data, received_at FROM events \
             WHERE issue_id = $1 ORDER BY received_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(issue_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count_events(&self, issue_id: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE issue_id = $1")
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
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO transactions (id, project_id, name, release, environment, duration_ms, status, data, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
                "INSERT INTO transaction_spans (id, transaction_id, span_id, trace_id, parent_span_id, op, description, start_offset_ms, duration_ms, status, data) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
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
            "SELECT * FROM transactions WHERE project_id = $1 ORDER BY received_at DESC LIMIT $2 OFFSET $3",
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
        let tx_row = sqlx::query_as::<_, crate::common::TransactionRow>("SELECT * FROM transactions WHERE id = $1")
            .bind(transaction_id)
            .fetch_optional(&self.pool)
            .await?;

        match tx_row {
            Some(row) => {
                let spans = sqlx::query_as::<_, crate::common::SpanRow>(
                    "SELECT * FROM transaction_spans WHERE transaction_id = $1",
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
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transactions WHERE project_id = $1")
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
        let now = now_rfc3339();
        let mut count = 0usize;

        for item in &aggregates.aggregates {
            let id = new_id();
            sqlx::query(
                "INSERT INTO release_health (id, project_id, release, environment, started_at, distinct_id, exited, errored, abnormal, crashed, received_at)                  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
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
        let row: Option<(i64, i64)> = if release.is_some() || env.is_some() {
            let mut sql = String::from(
                "SELECT SUM(crashed), SUM(exited + errored + abnormal + crashed)                  FROM release_health WHERE project_id = $1",
            );
            let mut idx = 2usize;
            if release.is_some() {
                sql.push_str(&format!(" AND release = ${idx}"));
                idx += 1;
            }
            if env.is_some() {
                sql.push_str(&format!(" AND environment = ${idx}"));
            }
            let mut q = sqlx::query_as::<_, (i64, i64)>(&sql).bind(project_id);
            if let Some(r) = release {
                q = q.bind(r);
            }
            if let Some(e) = env {
                q = q.bind(e);
            }
            q.fetch_optional(&self.pool).await?
        } else {
            sqlx::query_as::<_, (i64, i64)>(
                "SELECT SUM(crashed), SUM(exited + errored + abnormal + crashed)                  FROM release_health WHERE project_id = $1",
            )
            .bind(project_id)
            .fetch_optional(&self.pool)
            .await?
        };

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
            "SELECT id, project_id, release, environment, started_at, distinct_id, exited, errored, abnormal, crashed, received_at              FROM release_health WHERE project_id = $1",
        );
        let mut idx = 2usize;
        if let Some(_r) = release {
            sql.push_str(&format!(" AND release = ${idx}"));
            idx += 1;
        }
        if let Some(_e) = env {
            sql.push_str(&format!(" AND environment = ${idx}"));
            idx += 1;
        }
        sql.push_str(&format!(" ORDER BY received_at DESC LIMIT ${idx} OFFSET ${}", idx + 1));

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
        let mut sql = String::from("SELECT COUNT(*) FROM release_health WHERE project_id = $1");
        let mut idx = 2usize;
        if let Some(_r) = release {
            sql.push_str(&format!(" AND release = ${idx}"));
            idx += 1;
        }
        if let Some(_e) = env {
            sql.push_str(&format!(" AND environment = ${idx}"));
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
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO alert_rules (id, project_id, name, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
        )
        .bind(&id)
        .bind(project_id)
        .bind(name)
        .bind(conditions)
        .bind(action_type)
        .bind(action_config)
        .bind(cooldown_seconds)
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
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at \
             FROM alert_rules WHERE project_id = $1 ORDER BY created_at",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_alert_rule(&self, rule_id: &str) -> Result<Option<AlertRule>> {
        let row = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at \
             FROM alert_rules WHERE id = $1",
        )
        .bind(rule_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_alert_rule(&self, rule_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM alert_rules WHERE id = $1").bind(rule_id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn toggle_alert_rule(&self, rule_id: &str, enabled: bool) -> Result<()> {
        let now = now_rfc3339();
        sqlx::query("UPDATE alert_rules SET enabled = $1, updated_at = $2 WHERE id = $3")
            .bind(enabled)
            .bind(&now)
            .bind(rule_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_enabled_rules_for_project(&self, project_id: &str) -> Result<Vec<AlertRule>> {
        let rows = sqlx::query_as::<_, AlertRuleRow>(
            "SELECT id, project_id, name, enabled, conditions, action_type, action_config, cooldown_seconds, created_at, updated_at \
             FROM alert_rules WHERE project_id = $1 AND enabled = TRUE",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // ── Alert History ───────────────────────────────────────────────────

    async fn insert_alert_history(&self, rule_id: &str, project_id: &str, issue_id: &str) -> Result<String> {
        let id = new_id();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO alert_history (id, rule_id, project_id, issue_id, status, attempts, created_at) \
             VALUES ($1, $2, $3, $4, 'pending', 0, $5)",
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
        let now = now_rfc3339();
        sqlx::query("UPDATE alert_history SET status = 'sent', sent_at = $1, attempts = attempts + 1 WHERE id = $2")
            .bind(&now)
            .bind(history_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_alert_failed(&self, history_id: &str, error: &str) -> Result<()> {
        sqlx::query(
            "UPDATE alert_history SET status = 'failed', last_error = $1, attempts = attempts + 1 WHERE id = $2",
        )
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
        let pattern = format!("%{}%", escape_ilike_postgres(query));

        let mut sql = String::from(
            "SELECT id, project_id, fingerprint, title, culprit, status, level, \
             count, user_count, first_seen, last_seen FROM issues \
             WHERE (title ILIKE $1 ESCAPE '\\' OR culprit ILIKE $1 ESCAPE '\\')",
        );
        let mut idx = 2usize;
        let mut conds: Vec<String> = Vec::new();
        if project_id.is_some() {
            conds.push(format!("project_id = ${idx}"));
            idx += 1;
        }
        if status.is_some() {
            conds.push(format!("status = ${idx}"));
            idx += 1;
        }
        if level.is_some() {
            conds.push(format!("level = ${idx}"));
            idx += 1;
        }
        if !conds.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conds.join(" AND "));
        }
        sql.push_str(&format!(" ORDER BY last_seen DESC LIMIT ${idx} OFFSET ${}", idx + 1));

        let mut q = sqlx::query_as::<_, IssueRow>(&sql).bind(&pattern);
        if let Some(pid) = project_id {
            q = q.bind(pid);
        }
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

    async fn count_search_issues(
        &self,
        query: &str,
        project_id: Option<&str>,
        status: Option<&str>,
        level: Option<&str>,
    ) -> Result<i64> {
        let pattern = format!("%{}%", escape_ilike_postgres(query));

        let mut sql = String::from(
            "SELECT COUNT(*) FROM issues WHERE (title ILIKE $1 ESCAPE '\\' OR culprit ILIKE $1 ESCAPE '\\')",
        );
        let mut idx = 2usize;
        let mut conds: Vec<String> = Vec::new();
        if project_id.is_some() {
            conds.push(format!("project_id = ${idx}"));
            idx += 1;
        }
        if status.is_some() {
            conds.push(format!("status = ${idx}"));
            idx += 1;
        }
        if level.is_some() {
            conds.push(format!("level = ${idx}"));
        }
        if !conds.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conds.join(" AND "));
        }

        let mut q = sqlx::query_scalar::<_, i64>(&sql).bind(&pattern);
        if let Some(pid) = project_id {
            q = q.bind(pid);
        }
        if let Some(s) = status {
            q = q.bind(s);
        }
        if let Some(l) = level {
            q = q.bind(l);
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
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let result = sqlx::query("DELETE FROM events WHERE received_at < $1").bind(&cutoff).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn purge_orphan_issues(&self) -> Result<()> {
        sqlx::query("DELETE FROM issues WHERE id NOT IN (SELECT DISTINCT issue_id FROM events)")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn purge_stale_auth_attempts(&self) -> Result<()> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
        sqlx::query("DELETE FROM auth_attempts WHERE created_at < $1").bind(&cutoff).execute(&self.pool).await?;
        Ok(())
    }

    // ── Alert Cooldown ─────────────────────────────────────────────────

    async fn is_rule_cooling_down(&self, rule_id: &str, cooldown_seconds: i64) -> Result<bool> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT created_at FROM alert_history WHERE rule_id = $1 AND status = 'sent' ORDER BY created_at DESC LIMIT 1",
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
        let ok: i16 = sqlx::query_scalar("SELECT 1").fetch_one(&self.pool).await?;
        Ok(ok == 1)
    }

    // ── Attachments ───────────────────────────────────────────────────

    async fn insert_attachment(&self, row: &crate::common::AttachmentRow) -> Result<String> {
        let id = new_id();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO attachments (id, event_id, project_id, filename, content_type, attachment_type, size_bytes, disk_path, created_at)              VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
            "SELECT id, event_id, project_id, filename, content_type, attachment_type, size_bytes, disk_path, created_at              FROM attachments WHERE event_id = $1 ORDER BY created_at",
        )
        .bind(event_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn get_attachment(&self, id: &str) -> Result<Option<crate::common::AttachmentRow>> {
        let row = sqlx::query_as::<_, crate::common::AttachmentRow>(
            "SELECT id, event_id, project_id, filename, content_type, attachment_type, size_bytes, disk_path, created_at              FROM attachments WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn delete_attachment(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM attachments WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Auth ───────────────────────────────────────────────────────────

    async fn has_users(&self) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_optional(&self.pool).await?;
        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    async fn create_user(&self, email: &str, name: &str, password_hash: &str) -> Result<()> {
        let id = new_id();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO users (id, email, name, password_hash, role, created_at) VALUES ($1, $2, $3, $4, 'admin', $5)",
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
            "SELECT id, email, name, password_hash, role, created_at FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn get_user_by_id(&self, id: &str) -> Result<Option<StoredUser>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, name, password_hash, role, created_at FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_session(&self, user_id: &str, token: &str, expires_at: &str) -> Result<()> {
        let id = new_id();
        let now = now_rfc3339();
        sqlx::query("INSERT INTO sessions (id, user_id, token, expires_at, created_at) VALUES ($1, $2, $3, $4, $5)")
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
            "SELECT id, user_id, token, expires_at, created_at FROM sessions WHERE token = $1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn delete_session(&self, token: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token = $1").bind(token).execute(&self.pool).await?;
        Ok(())
    }

    async fn cleanup_expired_sessions(&self) -> Result<u64> {
        let now = now_rfc3339();
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at < $1").bind(&now).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn record_auth_attempt(&self, email: &str, ip: &str, success: bool) -> Result<()> {
        let id = new_id();
        let now = now_rfc3339();
        sqlx::query("INSERT INTO auth_attempts (id, email, ip, success, created_at) VALUES ($1, $2, $3, $4, $5)")
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
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM auth_attempts WHERE email = $1 AND success = FALSE AND created_at > $2",
        )
        .bind(email)
        .bind(&cutoff)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(c,)| c).unwrap_or(0))
    }

    async fn count_failed_attempts_ip(&self, ip: &str, minutes: i64) -> Result<i64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339();
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT COUNT(*) FROM auth_attempts WHERE ip = $1 AND success = FALSE AND created_at > $2")
                .bind(ip)
                .bind(&cutoff)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(c,)| c).unwrap_or(0))
    }

    // ── Raw event fetch ────────────────────────────────────────────────

    async fn get_event_raw(&self, event_id: &str) -> Result<Option<StoredEvent>> {
        let row = sqlx::query_as::<_, EventRow>(
            "SELECT id, issue_id, project_id, data, received_at FROM events WHERE id = $1",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }
}
