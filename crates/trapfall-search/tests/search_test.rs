use trapfall_core::Store;

async fn setup_store() -> Store {
    let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
    let pool = backend.sqlite_pool().unwrap();
    trapfall_db::run_sqlite_migrations(pool).await.unwrap();
    let store = Store::new(backend);
    store.create_project("proj-1", "Test Project").await.unwrap();
    store
}

async fn project_id(store: &Store) -> String {
    store.get_project_by_slug("proj-1").await.unwrap().unwrap().id
}

async fn seed_issues(store: &Store) {
    let pid = project_id(store).await;
    let issues = [
        ("TypeError: Cannot read property 'x' of undefined", Some("app.ts:10"), "unresolved", "error"),
        ("ReferenceError: foo is not defined", Some("utils.ts:22"), "resolved", "error"),
        ("Warning: Deprecated API used", Some("main.rs:5"), "unresolved", "warning"),
        ("Info: Application started", Some("lib.rs:1"), "unresolved", "info"),
        ("Error: Database connection failed", Some("db.ts:33"), "unresolved", "error"),
    ];
    for (title, culprit, status, level) in &issues {
        let level_enum = match *level {
            "warning" => trapfall_proto::Level::Warning,
            "info" => trapfall_proto::Level::Info,
            _ => trapfall_proto::Level::Error,
        };
        let issue = store.upsert_issue(&pid, title, title, culprit.as_deref(), level_enum).await.unwrap();
        if *status == "resolved" {
            store.set_issue_status(&issue.id, trapfall_proto::IssueStatus::Resolved).await.unwrap();
        }
    }
}

#[tokio::test]
async fn search_by_title() {
    let store = setup_store().await;
    seed_issues(&store).await;
    let pid = project_id(&store).await;

    let results = trapfall_search::search_issues(&store, "TypeError", Some(&pid), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].title.contains("TypeError"));
}

#[tokio::test]
async fn search_by_culprit() {
    let store = setup_store().await;
    seed_issues(&store).await;
    let pid = project_id(&store).await;

    let results = trapfall_search::search_issues(&store, "db.ts", Some(&pid), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].culprit.as_ref().unwrap().contains("db.ts"));
}

#[tokio::test]
async fn search_with_status_filter() {
    let store = setup_store().await;
    seed_issues(&store).await;
    let pid = project_id(&store).await;

    let results =
        trapfall_search::search_issues(&store, "Error", Some(&pid), Some("unresolved"), None, 50, 0).await.unwrap();
    assert!(results.iter().all(|i| i.status == trapfall_proto::IssueStatus::Unresolved));
}

#[tokio::test]
async fn search_no_results() {
    let store = setup_store().await;
    seed_issues(&store).await;
    let pid = project_id(&store).await;

    let results =
        trapfall_search::search_issues(&store, "nonexistent_xyz", Some(&pid), None, None, 50, 0).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn search_all_filters_combined() {
    let store = setup_store().await;
    seed_issues(&store).await;
    let pid = project_id(&store).await;

    let results = trapfall_search::search_issues(&store, "Error", Some(&pid), Some("unresolved"), Some("error"), 50, 0)
        .await
        .unwrap();
    assert!(
        results.iter().all(|i| { i.title.contains("Error") && i.status == trapfall_proto::IssueStatus::Unresolved })
    );
}

#[tokio::test]
async fn search_limit_and_offset() {
    let store = setup_store().await;
    seed_issues(&store).await;
    let pid = project_id(&store).await;

    // "Error" matches TypeError, ReferenceError, Error: Database
    let page1 = trapfall_search::search_issues(&store, "Error", Some(&pid), None, None, 2, 0).await.unwrap();
    assert_eq!(page1.len(), 2);

    let page2 = trapfall_search::search_issues(&store, "Error", Some(&pid), None, None, 2, 2).await.unwrap();
    assert!(page2.len() <= 1);
}

#[tokio::test]
async fn search_special_characters() {
    let store = setup_store().await;
    let pid = project_id(&store).await;

    let _ = store
        .backend()
        .upsert_issue(&pid, "fp-special", "100% CPU usage", Some("app.rs"), trapfall_proto::Level::Error)
        .await
        .unwrap();

    let results = trapfall_search::search_issues(&store, "100%", Some(&pid), None, None, 50, 0).await.unwrap();
    assert_eq!(results.len(), 1);
}
