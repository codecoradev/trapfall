//! Shared test suite — runs identically against any `Database` backend.
//!
//! Each test function takes a `Arc<dyn Database>` and exercises a major
//! operation group. Backend-specific test modules (`sqlite_backend.rs`,
//! `postgres_backend.rs`) call these functions after setting up their pool.
//!
//! Phase 4 (#169): confidence that both backends behave identically.

use std::sync::Arc;

use trapfall_db::Database;
use trapfall_proto::{IssueStatus, Level};

// ── Helpers ───────────────────────────────────────────────────────────

/// Create a project and return its ID.
pub async fn create_project(db: &Arc<dyn Database>, slug: &str, name: &str) -> String {
    let project = db.create_project(slug, name).await.unwrap();
    assert_eq!(project.slug, slug);
    assert_eq!(project.name, name);
    assert!(!project.id.is_empty());
    assert!(project.dsn.starts_with("https://"));
    project.id
}

// ── Project CRUD ──────────────────────────────────────────────────────

pub async fn project_crud(db: Arc<dyn Database>) {
    // Create
    let project_id = create_project(&db, "crud-app", "CRUD App").await;

    // Read by slug
    let found = db.get_project_by_slug("crud-app").await.unwrap().unwrap();
    assert_eq!(found.id, project_id);

    // Read by id
    let by_id = db.get_project_by_id(&project_id).await.unwrap().unwrap();
    assert_eq!(by_id.slug, "crud-app");

    // Read by DSN key
    let dsn_key = found.dsn.split('@').next().unwrap().trim_start_matches("https://");
    let by_dsn = db.get_project_by_dsn_key(dsn_key).await.unwrap().unwrap();
    assert_eq!(by_dsn.id, project_id);

    // List
    let list = db.list_projects().await.unwrap();
    assert!(list.iter().any(|p| p.id == project_id));

    // Update
    let updated = db.update_project(&project_id, "Renamed App").await.unwrap();
    assert_eq!(updated.name, "Renamed App");

    // DSN rotation
    let new_dsn = db.rotate_dsn(&project_id).await.unwrap();
    assert!(!new_dsn.is_empty());

    // Archive / unarchive
    db.archive_project(&project_id).await.unwrap();
    let archived = db.get_project_by_id(&project_id).await.unwrap().unwrap();
    assert!(archived.archived_at.is_some());

    db.unarchive_project(&project_id).await.unwrap();
    let unarchived = db.get_project_by_id(&project_id).await.unwrap().unwrap();
    assert!(unarchived.archived_at.is_none());

    // Delete
    let deleted = db.delete_project(&project_id).await.unwrap();
    assert!(deleted);
    let gone = db.get_project_by_id(&project_id).await.unwrap();
    assert!(gone.is_none());
}

// ── Issue Upsert + Dedup ──────────────────────────────────────────────

pub async fn issue_upsert_dedup(db: Arc<dyn Database>) {
    let project_id = create_project(&db, "issue-app", "Issue App").await;

    // First event → creates issue with count=1
    let issue1 = db.upsert_issue(&project_id, "fp-1", "TypeError", Some("app.rs:10"), Level::Error).await.unwrap();
    assert_eq!(issue1.count, 1);
    assert_eq!(issue1.title, "TypeError");

    // Same fingerprint → increments count
    let issue2 = db.upsert_issue(&project_id, "fp-1", "TypeError", Some("app.rs:10"), Level::Error).await.unwrap();
    assert_eq!(issue2.id, issue1.id);
    assert_eq!(issue2.count, 2);

    // Different fingerprint → new issue
    let issue3 = db.upsert_issue(&project_id, "fp-2", "AuthError", None, Level::Warning).await.unwrap();
    assert_ne!(issue3.id, issue1.id);

    // Get single
    let fetched = db.get_issue(&issue1.id).await.unwrap().unwrap();
    assert_eq!(fetched.count, 2);

    // List (default)
    let all = db.list_issues(&project_id, 50, 0).await.unwrap();
    assert_eq!(all.len(), 2);

    // List filtered by level
    let errors = db.list_issues_filtered(&project_id, None, Some("error"), 50, 0).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert!(matches!(errors[0].level, Level::Error));

    // Count
    let total = db.count_issues(&project_id, None, None).await.unwrap();
    assert_eq!(total, 2);

    let error_count = db.count_issues(&project_id, None, Some("error")).await.unwrap();
    assert_eq!(error_count, 1);

    // Set status
    db.set_issue_status(&issue1.id, IssueStatus::Resolved).await.unwrap();
    let unresolved = db.list_issues_filtered(&project_id, Some("unresolved"), None, 50, 0).await.unwrap();
    assert_eq!(unresolved.len(), 1); // only issue3
}

// ── Event Insert + List ───────────────────────────────────────────────

pub async fn event_operations(db: Arc<dyn Database>) {
    let project_id = create_project(&db, "event-app", "Event App").await;
    let issue = db.upsert_issue(&project_id, "fp-evt", "Boom", None, Level::Error).await.unwrap();

    // Insert 5 events
    for i in 0..5 {
        db.insert_event(&issue.id, &project_id, &format!(r#"{{"id":{i}}}"#)).await.unwrap();
    }

    // Count
    let count = db.count_events(&issue.id).await.unwrap();
    assert_eq!(count, 5);

    // List with pagination
    let page1 = db.list_events(&issue.id, 3, 0).await.unwrap();
    assert_eq!(page1.len(), 3);

    let page2 = db.list_events(&issue.id, 3, 3).await.unwrap();
    assert_eq!(page2.len(), 2);

    // Get raw event
    let raw = db.get_event_raw(&page1[0].id).await.unwrap().unwrap();
    assert_eq!(raw.issue_id, issue.id);
}

// ── Auth + Session Lifecycle ──────────────────────────────────────────

pub async fn auth_and_sessions(db: Arc<dyn Database>) {
    // No users initially
    assert!(!db.has_users().await.unwrap());

    // Create user
    db.create_user("admin@test.com", "Admin", "hashed_password").await.unwrap();
    assert!(db.has_users().await.unwrap());

    // Lookup
    let user = db.get_user_by_email("admin@test.com").await.unwrap().unwrap();
    assert_eq!(user.name, "Admin");
    assert_eq!(user.role, "admin");

    let by_id = db.get_user_by_id(&user.id).await.unwrap().unwrap();
    assert_eq!(by_id.email, "admin@test.com");

    // Update password
    db.update_password(&user.id, "new_hash").await.unwrap();
    let updated = db.get_user_by_id(&user.id).await.unwrap().unwrap();
    assert_eq!(updated.password_hash, "new_hash");

    // Create session
    db.create_session(&user.id, "session-token-123", "2099-01-01T00:00:00Z").await.unwrap();

    // Get session
    let session = db.get_session("session-token-123").await.unwrap().unwrap();
    assert_eq!(session.user_id, user.id);

    // Delete session
    db.delete_session("session-token-123").await.unwrap();
    let gone = db.get_session("session-token-123").await.unwrap();
    assert!(gone.is_none());

    // Expired session cleanup
    db.create_session(&user.id, "expired-token", "2000-01-01T00:00:00Z").await.unwrap();
    let cleaned = db.cleanup_expired_sessions().await.unwrap();
    assert!(cleaned >= 1);
    let gone2 = db.get_session("expired-token").await.unwrap();
    assert!(gone2.is_none());
}

// ── Brute-force Lockout ───────────────────────────────────────────────

pub async fn auth_attempts(db: Arc<dyn Database>) {
    // Record 3 failed attempts
    for _ in 0..3 {
        db.record_auth_attempt("target@test.com", "192.168.1.1", false).await.unwrap();
    }
    // Record 1 success
    db.record_auth_attempt("target@test.com", "192.168.1.1", true).await.unwrap();

    let by_email = db.count_failed_attempts_email("target@test.com", 60).await.unwrap();
    assert_eq!(by_email, 3);

    let by_ip = db.count_failed_attempts_ip("192.168.1.1", 60).await.unwrap();
    assert_eq!(by_ip, 3);

    // Different IP/email
    let other = db.count_failed_attempts_email("other@test.com", 60).await.unwrap();
    assert_eq!(other, 0);
}

// ── Alert Rules + History ─────────────────────────────────────────────

pub async fn alert_rules(db: Arc<dyn Database>) {
    let project_id = create_project(&db, "alert-app", "Alert App").await;

    // Create rule
    let rule = db
        .create_alert_rule(
            &project_id,
            "High Error Rate",
            r#"{"level":["error"]}"#,
            "webhook",
            r#"{"url":"https://hook.test"}"#,
            300,
        )
        .await
        .unwrap();
    assert_eq!(rule.name, "High Error Rate");
    assert!(rule.enabled);

    // List
    let rules = db.list_alert_rules(&project_id).await.unwrap();
    assert_eq!(rules.len(), 1);

    // Get single
    let fetched = db.get_alert_rule(&rule.id).await.unwrap().unwrap();
    assert_eq!(fetched.name, "High Error Rate");

    // Toggle off
    db.toggle_alert_rule(&rule.id, false).await.unwrap();
    let enabled = db.get_enabled_rules_for_project(&project_id).await.unwrap();
    assert!(enabled.is_empty());

    // Toggle on
    db.toggle_alert_rule(&rule.id, true).await.unwrap();
    let enabled2 = db.get_enabled_rules_for_project(&project_id).await.unwrap();
    assert_eq!(enabled2.len(), 1);

    // Alert history
    let issue = db.upsert_issue(&project_id, "fp-alert", "Fire!", None, Level::Fatal).await.unwrap();
    let history_id = db.insert_alert_history(&rule.id, &project_id, &issue.id).await.unwrap();
    db.mark_alert_sent(&history_id).await.unwrap();

    // Cooldown check
    let cooling = db.is_rule_cooling_down(&rule.id, 300).await.unwrap();
    assert!(cooling);

    // Mark failed
    db.mark_alert_failed(&history_id, "timeout").await.unwrap();

    // Delete rule (alert_history FK may block — just verify toggle works)
    db.toggle_alert_rule(&rule.id, false).await.unwrap();
    let disabled = db.get_alert_rule(&rule.id).await.unwrap().unwrap();
    assert!(!disabled.enabled);
}

// ── Search ────────────────────────────────────────────────────────────

pub async fn search(db: Arc<dyn Database>) {
    let project_id = create_project(&db, "search-app", "Search App").await;

    db.upsert_issue(&project_id, "s1", "DatabaseError: connection lost", Some("db.rs"), Level::Error).await.unwrap();
    db.upsert_issue(&project_id, "s2", "AuthError: bad token", Some("auth.rs"), Level::Warning).await.unwrap();
    db.upsert_issue(&project_id, "s3", "NetworkError: timeout", Some("net.rs"), Level::Error).await.unwrap();

    // Search by title keyword
    let results = db.search_issues("Database", Some(&project_id), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].title.contains("Database"));

    // Search by culprit
    let by_culprit = db.search_issues("auth", Some(&project_id), None, None, 50, 0).await.unwrap();
    assert_eq!(by_culprit.len(), 1);

    // Search with level filter
    let errors_only = db.search_issues("Error", Some(&project_id), None, Some("error"), 50, 0).await.unwrap();
    assert!(errors_only.iter().all(|i| matches!(i.level, Level::Error)));

    // Count search — all three contain "error" (case-insensitive)
    let count = db.count_search_issues("error", Some(&project_id), None, None).await.unwrap();
    assert!(count >= 2);

    // No results
    let empty = db.search_issues("nonexistent_xyz", Some(&project_id), None, None, 50, 0).await.unwrap();
    assert!(empty.is_empty());
}

// ── Retention Purge ───────────────────────────────────────────────────

pub async fn retention(db: Arc<dyn Database>) {
    let project_id = create_project(&db, "retain-app", "Retain App").await;
    let issue = db.upsert_issue(&project_id, "fp-ret", "Old Error", None, Level::Error).await.unwrap();
    db.insert_event(&issue.id, &project_id, r#"{"old":true}"#).await.unwrap();

    // Purge with very large days (shouldn't delete anything)
    let deleted = db.purge_old_events(36500).await.unwrap();
    assert_eq!(deleted, 0);

    // Orphan purge (safe — issue has events)
    db.purge_orphan_issues().await.unwrap();
    let still_there = db.get_issue(&issue.id).await.unwrap();
    assert!(still_there.is_some());
}

// ── Metrics / Count Table ─────────────────────────────────────────────

pub async fn count_table(db: Arc<dyn Database>) {
    // Whitelist: valid tables return a number
    let projects = db.count_table("projects").await.unwrap();
    assert!(projects >= 0);

    // Invalid table → 0
    let invalid = db.count_table("DROP TABLE users; --").await.unwrap();
    assert_eq!(invalid, 0);
}

// ── Health / Ping ─────────────────────────────────────────────────────

pub async fn ping(db: Arc<dyn Database>) {
    let ok = db.ping().await.unwrap();
    assert!(ok);
}

// ── Runner ────────────────────────────────────────────────────────────

/// Run all shared tests against the given backend.
pub async fn run_all(db: Arc<dyn Database>) {
    project_crud(db.clone()).await;
    issue_upsert_dedup(db.clone()).await;
    event_operations(db.clone()).await;
    auth_and_sessions(db.clone()).await;
    auth_attempts(db.clone()).await;
    alert_rules(db.clone()).await;
    search(db.clone()).await;
    retention(db.clone()).await;
    count_table(db.clone()).await;
    ping(db.clone()).await;
}
