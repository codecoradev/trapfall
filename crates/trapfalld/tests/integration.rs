//! Integration tests — ingest → digest → SQLite pipeline.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceExt;

use std::path::PathBuf;

use trapfalld::rate_limit::RateLimiter;
use trapfalld::server::router;
use trapfalld::{AppState, Config};
async fn test_pool() -> SqlitePool {
    let options = SqliteConnectOptions::new().filename(":memory:").create_if_missing(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();

    let migration_sql = include_str!("../migrations/20260606000001_initial.sql");
    sqlx::raw_sql(migration_sql).execute(&pool).await.unwrap();

    pool
}

async fn seed_project(pool: &SqlitePool) -> String {
    let slug = "test-project";
    let id = uuid::Uuid::new_v4().to_string();
    let dsn = format!("https://abc123@localhost:9090/{slug}");

    sqlx::query("INSERT INTO projects (id, slug, name, dsn_key, dsn) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(slug)
        .bind("Test Project")
        .bind("abc123")
        .bind(&dsn)
        .execute(pool)
        .await
        .unwrap();

    slug.to_string()
}

fn make_envelope_body(exception_type: &str, message: &str) -> Vec<u8> {
    let envelope_header = r#"{"event_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let item_header = r#"{"type":"event","length":0}"#;
    let event_json = format!(
        r#"{{"event_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","message":"{message}","exception":{{"values":[{{"type":"{exception_type}","value":"{message}","stacktrace":{{"frames":[{{"filename":"app.rs","lineno":42,"function":"main","in_app":true}}]}}}}]}},"level":"error"}}"#
    );
    format!("{envelope_header}\n{item_header}\n{event_json}").into_bytes()
}

fn make_state(pool: SqlitePool, rate_limiter: RateLimiter) -> AppState {
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    // Keep rx alive by spawning a dummy consumer
    tokio::spawn(async move {
        let mut rx = rx;
        while rx.recv().await.is_some() {}
    });
    let config = Config { db_path: PathBuf::from(":memory:"), listen_addr: "0.0.0.0:9090".into() };
    AppState { pool, config, ingest_tx: tx, rate_limiter }
}

#[tokio::test]
async fn health_check_returns_ok() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    let req = Request::builder().uri("/health").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_accepts_valid_envelope() {
    let pool = test_pool().await;
    let slug = seed_project(&pool).await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    let body = make_envelope_body("TypeError", "Cannot read property 'x' of undefined");
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{slug}/envelope/"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_404_for_unknown_project() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    let body = make_envelope_body("Error", "test");
    let req = Request::builder()
        .method("POST")
        .uri("/api/nonexistent/envelope/")
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rate_limit_returns_429() {
    let pool = test_pool().await;
    let slug = seed_project(&pool).await;

    // Very restrictive: 2 burst, no refill
    let state = make_state(pool, RateLimiter::new(2.0, 0.0));
    let app = router(state);

    let body = make_envelope_body("Error", "test");

    // First two should succeed
    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/{slug}/envelope/"))
            .header("content-type", "application/octet-stream")
            .body(Body::from(body.clone()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Third should be rate limited
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{slug}/envelope/"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(body.clone()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ── Auth Integration Tests ─────────────────────────────────────────────

#[tokio::test]
async fn setup_status_needs_setup() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    let req = Request::builder().uri("/api/setup").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["needs_setup"], true);
}

#[tokio::test]
async fn setup_creates_admin_and_project() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user"]["email"], "admin@test.com");
    assert_eq!(json["project_slug"], "default");
    assert!(json["dsn"].as_str().unwrap().contains("https://"));
}

#[tokio::test]
async fn setup_forbidden_after_first_user() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    // First setup
    let req = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Second setup should be forbidden
    let req2 = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"second@test.com","name":"Second","password":"password456"}"#))
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_returns_session_cookie() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    // Setup first
    let setup_req = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    app.clone().oneshot(setup_req).await.unwrap();

    // Login
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","password":"password123"}"#))
        .unwrap();
    let resp = app.oneshot(login_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check set-cookie header
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("trapfall_session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("SameSite=Strict"));
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    // Setup
    let setup_req = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    app.clone().oneshot(setup_req).await.unwrap();

    // Login with wrong password
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","password":"wrong"}"#))
        .unwrap();
    let resp = app.oneshot(login_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_rejects_without_cookie() {
    let pool = test_pool().await;
    let state = make_state(pool, RateLimiter::default());
    let app = router(state);

    let req = Request::builder().uri("/api/auth/me").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
