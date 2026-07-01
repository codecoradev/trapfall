//! Store — thin facade over a [`Database`] backend.
//!
//! All SQL lives in `trapfall-db` backends. This struct delegates every
//! method to the underlying `dyn Database`, providing a stable API for
//! callers (server, digest, mcp).

use std::sync::Arc;

use anyhow::Result;
use trapfall_db::Database;
use trapfall_db::common::ReleaseHealthRow;
use trapfall_proto::{Issue, IssueStatus, Level, Project, SessionAggregates, StoredEvent};

#[derive(Clone)]
pub struct Store {
    db: Arc<dyn Database>,
}

impl Store {
    /// Wrap any `Database` backend into a `Store`.
    pub fn new(db: Arc<dyn Database>) -> Self {
        Self { db }
    }

    /// Access the underlying trait object.
    pub fn backend(&self) -> &dyn Database {
        self.db.as_ref()
    }

    // ── Projects ────────────────────────────────────────────────────────

    pub async fn create_project(&self, slug: &str, name: &str) -> Result<Project> {
        self.db.create_project(slug, name).await
    }

    pub async fn create_project_with_host(&self, slug: &str, name: &str, host: &str) -> Result<Project> {
        self.db.create_project_with_host(slug, name, host).await
    }

    pub async fn get_project_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        self.db.get_project_by_slug(slug).await
    }

    pub async fn get_project_by_id(&self, id: &str) -> Result<Option<Project>> {
        self.db.get_project_by_id(id).await
    }

    pub async fn get_project_by_dsn_key(&self, sentry_key: &str) -> Result<Option<Project>> {
        self.db.get_project_by_dsn_key(sentry_key).await
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        self.db.list_projects().await
    }

    pub async fn rotate_dsn(&self, project_id: &str) -> Result<String> {
        self.db.rotate_dsn(project_id).await
    }

    pub async fn archive_project(&self, project_id: &str) -> Result<()> {
        self.db.archive_project(project_id).await
    }

    pub async fn unarchive_project(&self, project_id: &str) -> Result<()> {
        self.db.unarchive_project(project_id).await
    }

    pub async fn delete_project(&self, project_id: &str) -> Result<bool> {
        self.db.delete_project(project_id).await
    }

    pub async fn update_project(&self, project_id: &str, name: &str) -> Result<Project> {
        self.db.update_project(project_id, name).await
    }

    pub async fn set_project_webhook(&self, slug: &str, url: &str) -> Result<()> {
        self.db.set_project_webhook(slug, url).await
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
        self.db.upsert_issue(project_id, fingerprint, title, culprit, level).await
    }

    pub async fn get_issue(&self, issue_id: &str) -> Result<Option<Issue>> {
        self.db.get_issue(issue_id).await
    }

    pub async fn list_issues(&self, project_id: &str, limit: i64, offset: i64) -> Result<Vec<Issue>> {
        self.db.list_issues(project_id, limit, offset).await
    }

    pub async fn list_issues_filtered(
        &self,
        project_id: &str,
        status: Option<&str>,
        level: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Issue>> {
        self.db.list_issues_filtered(project_id, status, level, limit, offset).await
    }

    pub async fn count_issues(&self, project_id: &str, status: Option<&str>, level: Option<&str>) -> Result<i64> {
        self.db.count_issues(project_id, status, level).await
    }

    pub async fn set_issue_status(&self, issue_id: &str, status: IssueStatus) -> Result<()> {
        self.db.set_issue_status(issue_id, status).await
    }

    // ── Events ──────────────────────────────────────────────────────────

    pub async fn insert_event(&self, issue_id: &str, project_id: &str, event_data: &str) -> Result<String> {
        self.db.insert_event(issue_id, project_id, event_data).await
    }

    pub async fn list_events(&self, issue_id: &str, limit: i64, offset: i64) -> Result<Vec<StoredEvent>> {
        self.db.list_events(issue_id, limit, offset).await
    }

    pub async fn count_events(&self, issue_id: &str) -> Result<i64> {
        self.db.count_events(issue_id).await
    }

    // ── Transactions ──────────────────────────────────────────────────────

    pub async fn list_transactions(
        &self,
        project_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<trapfall_db::common::TransactionRow>> {
        self.db.list_transactions(project_id, limit, offset).await
    }

    pub async fn get_transaction(
        &self,
        transaction_id: &str,
    ) -> Result<Option<(trapfall_db::common::TransactionRow, Vec<trapfall_db::common::SpanRow>)>> {
        self.db.get_transaction(transaction_id).await
    }

    pub async fn count_transactions(&self, project_id: &str) -> Result<i64> {
        self.db.count_transactions(project_id).await
    }

    // ── Release Health ──────────────────────────────────────────────────

    pub async fn insert_release_health(&self, project_id: &str, aggregates: &SessionAggregates) -> Result<usize> {
        self.db.insert_release_health(project_id, aggregates).await
    }

    pub async fn get_crash_rate(
        &self,
        project_id: &str,
        release: Option<&str>,
        env: Option<&str>,
    ) -> Result<Option<f64>> {
        self.db.get_crash_rate(project_id, release, env).await
    }

    pub async fn list_release_health(
        &self,
        project_id: &str,
        release: Option<&str>,
        env: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReleaseHealthRow>> {
        self.db.list_release_health(project_id, release, env, limit, offset).await
    }

    pub async fn count_release_health(
        &self,
        project_id: &str,
        release: Option<&str>,
        env: Option<&str>,
    ) -> Result<i64> {
        self.db.count_release_health(project_id, release, env).await
    }

    pub async fn list_environments(&self, project_id: &str) -> Result<Vec<String>> {
        self.db.list_environments(project_id).await
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
        self.db.create_alert_rule(project_id, name, conditions, action_type, action_config, cooldown_seconds).await
    }

    pub async fn list_alert_rules(&self, project_id: &str) -> Result<Vec<trapfall_proto::AlertRule>> {
        self.db.list_alert_rules(project_id).await
    }

    pub async fn get_alert_rule(&self, rule_id: &str) -> Result<Option<trapfall_proto::AlertRule>> {
        self.db.get_alert_rule(rule_id).await
    }

    pub async fn delete_alert_rule(&self, rule_id: &str) -> Result<bool> {
        self.db.delete_alert_rule(rule_id).await
    }

    pub async fn toggle_alert_rule(&self, rule_id: &str, enabled: bool) -> Result<()> {
        self.db.toggle_alert_rule(rule_id, enabled).await
    }

    pub async fn get_enabled_rules_for_project(&self, project_id: &str) -> Result<Vec<trapfall_proto::AlertRule>> {
        self.db.get_enabled_rules_for_project(project_id).await
    }

    // ── Attachments ────────────────────────────────────────────────────

    pub async fn insert_attachment(&self, row: &trapfall_db::common::AttachmentRow) -> Result<String> {
        self.db.insert_attachment(row).await
    }

    pub async fn list_attachments_by_event(&self, event_id: &str) -> Result<Vec<trapfall_db::common::AttachmentRow>> {
        self.db.list_attachments_by_event(event_id).await
    }

    pub async fn get_attachment(&self, id: &str) -> Result<Option<trapfall_db::common::AttachmentRow>> {
        self.db.get_attachment(id).await
    }

    pub async fn delete_attachment(&self, id: &str) -> Result<bool> {
        self.db.delete_attachment(id).await
    }

    // ── Alert History ───────────────────────────────────────────────────

    pub async fn insert_alert_history(&self, rule_id: &str, project_id: &str, issue_id: &str) -> Result<String> {
        self.db.insert_alert_history(rule_id, project_id, issue_id).await
    }

    pub async fn mark_alert_sent(&self, history_id: &str) -> Result<()> {
        self.db.mark_alert_sent(history_id).await
    }

    pub async fn mark_alert_failed(&self, history_id: &str, error: &str) -> Result<()> {
        self.db.mark_alert_failed(history_id, error).await
    }
}

// ── Helper retained for `auth.rs` tests ────────────────────────────────

/// Extract DSN key from DSN URL: `https://{key}@host/path` → `{key}`.
#[cfg(test)]
fn extract_dsn_key(dsn: &str) -> String {
    dsn.split('@').next().unwrap_or("").trim_start_matches("https://").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dsn_key() {
        assert_eq!(extract_dsn_key("https://abc123@trapfall.example.com/1"), "abc123");
        assert_eq!(extract_dsn_key("https://key456@localhost:9090/42"), "key456");
        assert_eq!(extract_dsn_key("malformed"), "malformed");
        assert_eq!(extract_dsn_key(""), "");
    }

    #[tokio::test]
    async fn test_rotate_dsn_updates_dsn_key() {
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        {
            trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        }
        let store = Store::new(backend);

        let project = store.create_project("test", "Test Project").await.unwrap();
        let original_dsn = project.dsn.clone();
        let original_key = extract_dsn_key(&original_dsn);

        let found = store.get_project_by_dsn_key(&original_key).await.unwrap();
        assert!(found.is_some());

        let new_dsn = store.rotate_dsn(&project.id).await.unwrap();
        assert_ne!(new_dsn, original_dsn);

        let new_key = extract_dsn_key(&new_dsn);
        assert_ne!(new_key, original_key);

        let old_lookup = store.get_project_by_dsn_key(&original_key).await.unwrap();
        assert!(old_lookup.is_none(), "Old DSN key should be revoked after rotation");

        let new_lookup = store.get_project_by_dsn_key(&new_key).await.unwrap();
        assert!(new_lookup.is_some(), "New DSN key should work after rotation");
    }
}
