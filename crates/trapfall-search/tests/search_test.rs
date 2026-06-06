use sqlx::SqlitePool;

async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::query(
        "CREATE TABLE issues (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            title TEXT NOT NULL,
            culprit TEXT,
            status TEXT NOT NULL DEFAULT 'unresolved',
            level TEXT NOT NULL DEFAULT 'error',
            count INTEGER NOT NULL DEFAULT 1,
            user_count INTEGER NOT NULL DEFAULT 1,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn seed_issues(pool: &SqlitePool, project_id: &str) {
    let issues = [
        ("id-1", "TypeError: Cannot read property 'x' of undefined", "app.ts:10", "unresolved", "error"),
        ("id-2", "ReferenceError: foo is not defined", "utils.ts:22", "resolved", "error"),
        ("id-3", "Warning: Deprecated API used", "main.rs:5", "unresolved", "warning"),
        ("id-4", "Info: Application started", "lib.rs:1", "unresolved", "info"),
        ("id-5", "Error: Database connection failed", "db.ts:33", "unresolved", "error"),
    ];
    for (id, title, culprit, status, level) in &issues {
        sqlx::query(
            "INSERT INTO issues (id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen) \
             VALUES (?, ?, 'fp', ?, ?, ?, ?, 1, 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
        )
        .bind(id)
        .bind(project_id)
        .bind(title)
        .bind(culprit)
        .bind(status)
        .bind(level)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn search_by_title() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    let results = trapfall_search::search_issues(&pool, "TypeError", Some("proj-1"), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].title.contains("TypeError"));
}

#[tokio::test]
async fn search_by_culprit() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    let results = trapfall_search::search_issues(&pool, "db.ts", Some("proj-1"), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].culprit.as_ref().unwrap().contains("db.ts"));
}

#[tokio::test]
async fn search_with_status_filter() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    let results =
        trapfall_search::search_issues(&pool, "Error", Some("proj-1"), Some("unresolved"), None, 50, 0).await.unwrap();
    assert!(results.iter().all(|i| i.status == trapfall_proto::IssueStatus::Unresolved));
}

#[tokio::test]
async fn search_with_level_filter() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    let results =
        trapfall_search::search_issues(&pool, "e", Some("proj-1"), None, Some("warning"), 50, 0).await.unwrap();
    assert!(results.iter().all(|i| matches!(i.level, trapfall_proto::Level::Warning)));
}

#[tokio::test]
async fn search_no_results() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    let results =
        trapfall_search::search_issues(&pool, "nonexistent_xyz", Some("proj-1"), None, None, 50, 0).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn search_all_filters_combined() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    let results =
        trapfall_search::search_issues(&pool, "Error", Some("proj-1"), Some("unresolved"), Some("error"), 50, 0)
            .await
            .unwrap();
    assert!(results.iter().all(|i| {
        i.title.contains("Error")
            && i.status == trapfall_proto::IssueStatus::Unresolved
            && matches!(i.level, trapfall_proto::Level::Error)
    }));
}

#[tokio::test]
async fn search_limit_and_offset() {
    let pool = setup_db().await;
    seed_issues(&pool, "proj-1").await;

    // "Error" matches TypeError, ReferenceError, Error: Database
    let page1 = trapfall_search::search_issues(&pool, "Error", Some("proj-1"), None, None, 2, 0).await.unwrap();
    assert_eq!(page1.len(), 2);

    let page2 = trapfall_search::search_issues(&pool, "Error", Some("proj-1"), None, None, 2, 2).await.unwrap();
    assert!(page2.len() <= 1);
}

#[tokio::test]
async fn search_special_characters() {
    let pool = setup_db().await;
    sqlx::query(
        "INSERT INTO issues (id, project_id, fingerprint, title, culprit, status, level, count, user_count, first_seen, last_seen) \
         VALUES ('id-%', 'proj-1', 'fp', '100% CPU usage', 'app.rs', 'unresolved', 'error', 1, 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let results = trapfall_search::search_issues(&pool, "100%", Some("proj-1"), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
}
