//! Error-path and edge-case tests for the SQLite backend.
//!
//! Covers scenarios that the happy-path suite (`common.rs`) does not
//! exercise: FK violations, non-existent IDs, empty results, cascade
//! completeness, and upsert idempotency.
//!
//! All tests use in-memory SQLite — no external dependencies.
//!
//! Related: issue #221 (PR 1 — trapfall-db error paths).

// `common.rs` is a shared module — not every test binary uses every helper.
#![allow(dead_code)]
#![cfg(feature = "sqlite")]

mod common;

use std::sync::Arc;
use trapfall_db::{Database, SqliteBackend};
use trapfall_proto::{IssueStatus, Level};

// ── Test harness ─────────────────────────────────────────────────────

async fn setup() -> Arc<dyn Database> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .expect("failed to open in-memory SQLite pool");
    trapfall_db::run_sqlite_migrations(&pool).await.expect("failed to run migrations");
    Arc::new(SqliteBackend::new(pool))
}

/// Enable foreign-key enforcement for the current connection pool.
///
/// SQLite has FKs defined in DDL but does not enforce them by default
/// unless `PRAGMA foreign_keys = ON` is set per connection.
async fn setup_with_fk() -> Arc<dyn Database> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(4)
        .connect("sqlite::memory:")
        .await
        .expect("failed to open in-memory SQLite pool");
    sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await.expect("failed to enable foreign_keys pragma");
    trapfall_db::run_sqlite_migrations(&pool).await.expect("failed to run migrations");
    Arc::new(SqliteBackend::new(pool))
}

// ── 1. Foreign-key violations ────────────────────────────────────────

/// `upsert_issue` with a non-existent `project_id` should return an error,
/// not silently create an orphan issue.
#[tokio::test]
async fn upsert_issue_nonexistent_project_returns_error() {
    let db = setup_with_fk().await;
    let result = db.upsert_issue("nonexistent-project-id", "fp-1", "Boom", Some("app.rs"), Level::Error).await;
    assert!(result.is_err(), "upsert_issue with a fake project_id must error, got: {:?}", result.ok());
}

/// `insert_event` with a non-existent `project_id` / `issue_id` should error.
#[tokio::test]
async fn insert_event_nonexistent_project_returns_error() {
    let db = setup_with_fk().await;
    let result = db.insert_event("nonexistent-issue-id", "nonexistent-project-id", r#"{"x":1}"#).await;
    assert!(result.is_err(), "insert_event with fake project_id + issue_id must error");
}

/// `insert_event` with a valid project but non-existent issue should error.
#[tokio::test]
async fn insert_event_valid_project_nonexistent_issue_returns_error() {
    let db = setup_with_fk().await;
    let project_id = common::create_project(&db, "fk-app", "FK App").await;

    let result = db.insert_event("nonexistent-issue-id", &project_id, r#"{"x":1}"#).await;
    assert!(result.is_err(), "insert_event with valid project but fake issue_id must error");
}

/// `create_alert_rule` with a non-existent `project_id` should error.
#[tokio::test]
async fn create_alert_rule_nonexistent_project_returns_error() {
    let db = setup_with_fk().await;
    let result = db
        .create_alert_rule(
            "nonexistent-project-id",
            "Rule for Ghost",
            r#"{"level":["error"]}"#,
            "webhook",
            r#"{"url":"https://hook.test"}"#,
            300,
        )
        .await;
    assert!(result.is_err(), "create_alert_rule with a fake project_id must error");
}

/// `insert_alert_history` with non-existent project / rule / issue should error.
#[tokio::test]
async fn insert_alert_history_nonexistent_fk_returns_error() {
    let db = setup_with_fk().await;
    let result = db.insert_alert_history("nonexistent-rule-id", "nonexistent-project-id", "nonexistent-issue-id").await;
    assert!(result.is_err(), "insert_alert_history with all-fake FKs must error");
}

// ── 2. Non-existent ID lookups ───────────────────────────────────────

/// `get_project_by_id` with a non-existent ID returns `None` (not an error).
#[tokio::test]
async fn get_project_by_nonexistent_id_returns_none() {
    let db = setup().await;
    let result = db.get_project_by_id("does-not-exist").await;
    assert!(result.is_ok(), "get_project_by_id should not error for missing ID");
    assert!(result.unwrap().is_none(), "get_project_by_id should return None for missing ID");
}

/// `get_project_by_slug` with a non-existent slug returns `None`.
#[tokio::test]
async fn get_project_by_nonexistent_slug_returns_none() {
    let db = setup().await;
    let result = db.get_project_by_slug("ghost-slug").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

/// `get_project_by_dsn_key` with a non-existent key returns `None`.
#[tokio::test]
async fn get_project_by_nonexistent_dsn_key_returns_none() {
    let db = setup().await;
    let result = db.get_project_by_dsn_key("00000000-0000-0000-0000-000000000000").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

/// `get_issue` with a non-existent ID returns `None`.
#[tokio::test]
async fn get_issue_nonexistent_id_returns_none() {
    let db = setup().await;
    let result = db.get_issue("does-not-exist").await;
    assert!(result.is_ok(), "get_issue should not error for missing ID");
    assert!(result.unwrap().is_none());
}

/// `get_event_raw` with a non-existent ID returns `None`.
#[tokio::test]
async fn get_event_raw_nonexistent_id_returns_none() {
    let db = setup().await;
    let result = db.get_event_raw("does-not-exist").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

/// `get_alert_rule` with a non-existent ID returns `None`.
#[tokio::test]
async fn get_alert_rule_nonexistent_id_returns_none() {
    let db = setup().await;
    let result = db.get_alert_rule("does-not-exist").await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

/// `delete_project` on a non-existent ID returns `false` (not an error).
#[tokio::test]
async fn delete_nonexistent_project_returns_false() {
    let db = setup().await;
    let result = db.delete_project("does-not-exist").await;
    assert!(result.is_ok(), "delete_project should not error for missing ID");
    assert!(!result.unwrap(), "delete_project should return false when nothing was deleted");
}

/// `delete_alert_rule` on a non-existent ID returns `false`.
#[tokio::test]
async fn delete_nonexistent_alert_rule_returns_false() {
    let db = setup().await;
    let result = db.delete_alert_rule("does-not-exist").await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

// ── 3. Empty result handling ─────────────────────────────────────────

/// `list_issues` on a project with zero issues returns an empty vec.
#[tokio::test]
async fn list_issues_on_empty_project_returns_empty_vec() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-issues", "Empty Issues").await;

    let result = db.list_issues(&project_id, 50, 0).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty(), "list_issues on empty project should return []");
}

/// `list_issues_filtered` on a project with zero issues returns empty vec.
#[tokio::test]
async fn list_issues_filtered_on_empty_project_returns_empty_vec() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-filtered", "Empty Filtered").await;

    let result = db.list_issues_filtered(&project_id, Some("unresolved"), Some("error"), 50, 0).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

/// `count_issues` on a project with zero issues returns 0.
#[tokio::test]
async fn count_issues_on_empty_project_returns_zero() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-count", "Empty Count").await;

    let count = db.count_issues(&project_id, None, None).await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0, "count_issues on empty project should be 0");
}

/// `count_issues` with a non-existent project_id returns 0 (no rows match).
#[tokio::test]
async fn count_issues_nonexistent_project_returns_zero() {
    let db = setup().await;
    let count = db.count_issues("does-not-exist", None, None).await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0);
}

/// `list_events` for an issue with zero events returns empty vec.
#[tokio::test]
async fn list_events_on_empty_issue_returns_empty_vec() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-events", "Empty Events").await;
    let issue = db.upsert_issue(&project_id, "fp-1", "Boom", None, Level::Error).await.expect("upsert_issue failed");

    let result = db.list_events(&issue.id, 50, 0).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty(), "list_events on an issue with no events should return []");
}

/// `count_events` for an issue with zero events returns 0.
#[tokio::test]
async fn count_events_on_empty_issue_returns_zero() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-evt-count", "Empty Event Count").await;
    let issue = db.upsert_issue(&project_id, "fp-1", "Boom", None, Level::Error).await.expect("upsert_issue failed");

    let count = db.count_events(&issue.id).await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0);
}

/// `count_events` with a non-existent issue_id returns 0.
#[tokio::test]
async fn count_events_nonexistent_issue_returns_zero() {
    let db = setup().await;
    let count = db.count_events("does-not-exist").await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0);
}

/// `list_alert_rules` on a project with zero rules returns empty vec.
#[tokio::test]
async fn list_alert_rules_on_empty_project_returns_empty_vec() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-rules", "Empty Rules").await;

    let result = db.list_alert_rules(&project_id).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

/// `search_issues` on an empty project returns empty vec.
#[tokio::test]
async fn search_issues_on_empty_project_returns_empty_vec() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-search", "Empty Search").await;

    let result = db.search_issues("anything", Some(&project_id), None, None, 50, 0).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

/// `count_search_issues` on an empty project returns 0.
#[tokio::test]
async fn count_search_issues_on_empty_project_returns_zero() {
    let db = setup().await;
    let project_id = common::create_project(&db, "empty-search-count", "Empty Search Count").await;

    let count = db.count_search_issues("anything", Some(&project_id), None, None).await;
    assert!(count.is_ok());
    assert_eq!(count.unwrap(), 0);
}

// ── 4. delete_project cascade completeness ───────────────────────────

/// After `delete_project`, verify that:
/// - the project itself is gone
/// - issues referencing it are gone
/// - events referencing it are gone
/// - alert rules referencing it are gone
/// - alert history referencing it is gone
#[tokio::test]
async fn delete_project_cascades_to_all_related_tables() {
    let db = setup().await;
    let project_id = common::create_project(&db, "cascade-app", "Cascade App").await;

    // Populate issues + events
    let issue = db
        .upsert_issue(&project_id, "fp-cascade", "Boom", Some("app.rs"), Level::Error)
        .await
        .expect("upsert_issue failed");
    db.insert_event(&issue.id, &project_id, r#"{"i":1}"#).await.expect("insert_event failed");
    db.insert_event(&issue.id, &project_id, r#"{"i":2}"#).await.expect("insert_event failed");

    // Populate alert rule + alert history
    let rule = db
        .create_alert_rule(
            &project_id,
            "Cascade Rule",
            r#"{"level":["error"]}"#,
            "webhook",
            r#"{"url":"https://hook.test"}"#,
            300,
        )
        .await
        .expect("create_alert_rule failed");
    db.insert_alert_history(&rule.id, &project_id, &issue.id).await.expect("insert_alert_history failed");

    // Sanity: data exists before delete
    assert_eq!(db.count_issues(&project_id, None, None).await.unwrap(), 1);
    assert_eq!(db.count_events(&issue.id).await.unwrap(), 2);
    assert_eq!(db.list_alert_rules(&project_id).await.unwrap().len(), 1);

    // Delete the project
    let deleted = db.delete_project(&project_id).await.expect("delete_project failed");
    assert!(deleted, "delete_project should return true");

    // Project gone
    assert!(db.get_project_by_id(&project_id).await.unwrap().is_none());

    // Issues gone
    assert!(db.list_issues(&project_id, 100, 0).await.unwrap().is_empty());
    assert_eq!(db.count_issues(&project_id, None, None).await.unwrap(), 0);

    // Events gone (by project_id via the pool)
    let pool = db.sqlite_pool().expect("sqlite_pool failed");
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE project_id = ?")
        .bind(&project_id)
        .fetch_one(pool)
        .await
        .expect("count events query failed");
    assert_eq!(event_count, 0, "events should be deleted by cascade");

    // Alert rules gone
    assert!(db.list_alert_rules(&project_id).await.unwrap().is_empty());

    // Alert history gone
    let history_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_history WHERE project_id = ?")
        .bind(&project_id)
        .fetch_one(pool)
        .await
        .expect("count alert_history query failed");
    assert_eq!(history_count, 0, "alert_history should be deleted by cascade");
}

/// Deleting a project does NOT affect other projects' data.
#[tokio::test]
async fn delete_project_isolates_other_projects() {
    let db = setup().await;
    let proj_a = common::create_project(&db, "proj-a", "Project A").await;
    let proj_b = common::create_project(&db, "proj-b", "Project B").await;

    let issue_a = db.upsert_issue(&proj_a, "fp-a", "Error A", None, Level::Error).await.unwrap();
    let issue_b = db.upsert_issue(&proj_b, "fp-b", "Error B", None, Level::Error).await.unwrap();
    db.insert_event(&issue_a.id, &proj_a, "{}").await.unwrap();
    db.insert_event(&issue_b.id, &proj_b, "{}").await.unwrap();

    // Delete proj_a
    db.delete_project(&proj_a).await.unwrap();

    // Proj_a data gone
    assert!(db.get_project_by_id(&proj_a).await.unwrap().is_none());
    assert!(db.list_issues(&proj_a, 100, 0).await.unwrap().is_empty());

    // Proj_b data intact
    assert!(db.get_project_by_id(&proj_b).await.unwrap().is_some());
    let b_issues = db.list_issues(&proj_b, 100, 0).await.unwrap();
    assert_eq!(b_issues.len(), 1);
    assert_eq!(b_issues[0].id, issue_b.id);
    assert_eq!(db.count_events(&issue_b.id).await.unwrap(), 1);
}

// ── 5. upsert_issue atomicity / idempotency ──────────────────────────

/// Calling `upsert_issue` twice with the same fingerprint must NOT create
/// a duplicate issue. The count should increment, but the row count stays 1.
#[tokio::test]
async fn upsert_issue_twice_same_fingerprint_increments_not_duplicates() {
    let db = setup().await;
    let project_id = common::create_project(&db, "dup-app", "Dup App").await;

    let issue1 = db
        .upsert_issue(&project_id, "fp-dup", "TypeError", Some("app.rs:10"), Level::Error)
        .await
        .expect("first upsert failed");

    let issue2 = db
        .upsert_issue(&project_id, "fp-dup", "TypeError", Some("app.rs:10"), Level::Error)
        .await
        .expect("second upsert failed");

    assert_eq!(issue1.id, issue2.id, "same fingerprint → same issue ID");
    assert_eq!(issue2.count, issue1.count + 1, "count must increment by exactly 1");

    // Only one issue row in the DB
    let all = db.list_issues(&project_id, 100, 0).await.unwrap();
    assert_eq!(all.len(), 1, "there must be exactly 1 issue, not 2");
    assert_eq!(all[0].count, 2);
}

/// Upserting the same fingerprint N times should result in count = N,
/// not a duplicate row.
#[tokio::test]
async fn upsert_issue_repeated_same_fingerprint_count_matches() {
    let db = setup().await;
    let project_id = common::create_project(&db, "repeat-app", "Repeat App").await;

    const N: usize = 20;
    let mut last_id = String::new();
    for _ in 0..N {
        let issue = db
            .upsert_issue(&project_id, "fp-repeat", "Boom", None, Level::Warning)
            .await
            .expect("upsert must not fail");
        last_id = issue.id;
    }

    let all = db.list_issues(&project_id, 100, 0).await.unwrap();
    assert_eq!(all.len(), 1, "repeated upsert must produce exactly 1 issue row");
    assert_eq!(all[0].id, last_id);
    assert_eq!(all[0].count, N as i64, "count must be exactly {N}");
}

/// Upserting different fingerprints creates separate issue rows.
#[tokio::test]
async fn upsert_issue_different_fingerprints_creates_separate_rows() {
    let db = setup().await;
    let project_id = common::create_project(&db, "multi-fp", "Multi FP").await;

    for i in 0..5 {
        db.upsert_issue(&project_id, &format!("fp-{i}"), &format!("Error {i}"), None, Level::Error)
            .await
            .expect("upsert must not fail");
    }

    let all = db.list_issues(&project_id, 100, 0).await.unwrap();
    assert_eq!(all.len(), 5, "5 different fingerprints → 5 separate issues");
    for issue in &all {
        assert_eq!(issue.count, 1, "each issue should have count=1");
    }
}

/// Upsert should update the `level` on conflict (excluded.level in the
/// ON CONFLICT clause).
#[tokio::test]
async fn upsert_issue_updates_level_on_conflict() {
    let db = setup().await;
    let project_id = common::create_project(&db, "level-change", "Level Change").await;

    // Insert at Error level
    let first =
        db.upsert_issue(&project_id, "fp-level", "Boom", None, Level::Error).await.expect("first upsert failed");
    assert!(matches!(first.level, Level::Error));

    // Re-upsert at Fatal level → level should be updated
    let second =
        db.upsert_issue(&project_id, "fp-level", "Boom", None, Level::Fatal).await.expect("second upsert failed");
    assert!(matches!(second.level, Level::Fatal), "level should be updated to Fatal");

    // Verify the stored level
    let fetched = db.get_issue(&first.id).await.unwrap().expect("issue should exist");
    assert!(matches!(fetched.level, Level::Fatal));
    assert_eq!(fetched.count, 2);
}

// ── 6. Pagination edge cases ─────────────────────────────────────────

/// `list_issues` with a limit larger than the result set returns all rows.
#[tokio::test]
async fn list_issues_limit_exceeds_result_set() {
    let db = setup().await;
    let project_id = common::create_project(&db, "page-app", "Page App").await;

    for i in 0..3 {
        db.upsert_issue(&project_id, &format!("fp-{i}"), &format!("Error {i}"), None, Level::Error)
            .await
            .expect("upsert failed");
    }

    // limit=100 but only 3 issues exist
    let all = db.list_issues(&project_id, 100, 0).await.unwrap();
    assert_eq!(all.len(), 3);
}

/// `list_issues` with an offset past the last row returns empty vec.
#[tokio::test]
async fn list_issues_offset_past_end_returns_empty() {
    let db = setup().await;
    let project_id = common::create_project(&db, "page-offset", "Page Offset").await;

    for i in 0..3 {
        db.upsert_issue(&project_id, &format!("fp-{i}"), &format!("Error {i}"), None, Level::Error)
            .await
            .expect("upsert failed");
    }

    let result = db.list_issues(&project_id, 10, 100).await.unwrap();
    assert!(result.is_empty(), "offset past the end should return []");
}

/// `list_events` with offset past end returns empty vec.
#[tokio::test]
async fn list_events_offset_past_end_returns_empty() {
    let db = setup().await;
    let project_id = common::create_project(&db, "evt-offset", "Event Offset").await;
    let issue = db.upsert_issue(&project_id, "fp-1", "Boom", None, Level::Error).await.expect("upsert failed");
    db.insert_event(&issue.id, &project_id, r#"{"i":1}"#).await.expect("insert failed");

    let result = db.list_events(&issue.id, 10, 100).await.unwrap();
    assert!(result.is_empty());
}

/// `list_issues` with limit=0 returns zero rows (not an error).
#[tokio::test]
async fn list_issues_limit_zero_returns_empty_vec() {
    let db = setup().await;
    let project_id = common::create_project(&db, "limit-zero", "Limit Zero").await;
    db.upsert_issue(&project_id, "fp-1", "Boom", None, Level::Error).await.expect("upsert failed");

    let result = db.list_issues(&project_id, 0, 0).await;
    assert!(result.is_ok(), "limit=0 should not error");
    assert!(result.unwrap().is_empty(), "limit=0 should return []");
}

// ── 7. set_issue_status on non-existent issue ────────────────────────

/// `set_issue_status` on a non-existent issue is a no-op UPDATE (0 rows
/// affected). It should not error — the contract is fire-and-forget.
#[tokio::test]
async fn set_issue_status_nonexistent_issue_is_noop() {
    let db = setup().await;
    let result = db.set_issue_status("does-not-exist", IssueStatus::Resolved).await;
    assert!(result.is_ok(), "set_issue_status on a missing issue should not error");
}

// ── 8. rotate_dsn on non-existent project ────────────────────────────

/// `rotate_dsn` on a non-existent project_id should return an error
/// (it calls `get_project_by_id` internally and expects Some).
#[tokio::test]
async fn rotate_dsn_nonexistent_project_returns_error() {
    let db = setup().await;
    let result = db.rotate_dsn("does-not-exist").await;
    assert!(result.is_err(), "rotate_dsn on a missing project should error");
}

// ── 9. update_project on non-existent project ────────────────────────

/// `update_project` on a non-existent project_id returns an error
/// (it re-reads the project after UPDATE and expects Some).
#[tokio::test]
async fn update_project_nonexistent_returns_error() {
    let db = setup().await;
    let result = db.update_project("does-not-exist", "New Name").await;
    assert!(result.is_err(), "update_project on a missing project should error");
}
