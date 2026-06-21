//! Integration tests — ingest → digest → SQLite pipeline.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use std::path::PathBuf;

use trapfall_core::Store;
use trapfalld::rate_limit::RateLimiter;
use trapfalld::server::router;
use trapfalld::{AppState, Config, WsHub};

async fn test_store() -> Store {
    let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
    let pool = backend.sqlite_pool().unwrap();
    trapfall_db::run_sqlite_migrations(pool).await.unwrap();
    Store::new(backend)
}

async fn seed_project(store: &Store) -> String {
    let slug = "test-project";
    let project = store.create_project_with_host(slug, "Test Project", "localhost:9090").await.unwrap();
    // Override DSN key to match test auth header
    let pool = store.backend().sqlite_pool().unwrap();
    sqlx::query("UPDATE projects SET dsn_key = 'abc123' WHERE slug = ?").bind(slug).execute(pool).await.unwrap();
    project.id
}

fn make_envelope_body(exception_type: &str, message: &str) -> Vec<u8> {
    let envelope_header = r#"{"event_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let item_header = r#"{"type":"event","length":0}"#;
    let event_json = format!(
        r#"{{"event_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","message":"{message}","exception":{{"values":[{{"type":"{exception_type}","value":"{message}","stacktrace":{{"frames":[{{"filename":"app.rs","lineno":42,"function":"main","in_app":true}}]}}}}]}},\"level\":\"error\"}}\"#
    );
    format!("{envelope_header}\n{item_header}\n{event_json}").into_bytes()
}

fn make_state(store: Store, rate_limiter: RateLimiter) -> AppState {
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    // Keep rx alive by spawning a dummy consumer
    tokio::spawn(async move {
        let mut rx = rx;
        while rx.recv().await.is_some() {}
    });
    let config = Config {
        db_path: PathBuf::from(":memory:"),
        listen_addr: "0.0.0.0:9090".into(),
        cors_origins: Vec::new(),
        secure_cookie: false,
        public_url: None,
    };
    AppState { store, config, ingest_tx: tx, rate_limiter, ws_hub: WsHub::new(16) }
}

#[tokio::test]
async fn health_check_returns_ok() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let req = Request::builder().uri("/health").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_accepts_valid_envelope_with_dsn_key() {
    let store = test_store().await;
    let project_id = seed_project(&store).await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let body = make_envelope_body("TypeError", "Cannot read property 'x' of undefined");
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{project_id}/envelope/"))
        .header("content-type", "application/octet-stream")
        .header("authorization", "Bearer abc123")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_rejects_without_auth() {
    let store = test_store().await;
    let project_id = seed_project(&store).await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let body = make_envelope_body("Error", "test");
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{project_id}/envelope/"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingest_404_for_unknown_project() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let body = make_envelope_body("Error", "test");
    let req = Request::builder()
        .method("POST")
        .uri("/api/nonexistent/envelope/")
        .header("content-type", "application/octet-stream")
        .header("authorization", "Bearer abc123")
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rate_limit_returns_429() {
    let store = test_store().await;
    let project_id = seed_project(&store).await;

    // Very restrictive: 2 burst, no refill
    let state = make_state(store, RateLimiter::new(2.0, 0.0));
    let app = router(state);

    let body = make_envelope_body("Error", "test");

    // First two should succeed
    for _ in 0..2 {
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/{project_id}/envelope/"))
            .header("content-type", "application/octet-stream")
            .header("authorization", "Bearer abc123")
            .body(Body::from(body.clone()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Third should be rate limited
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{project_id}/envelope/"))
        .header("content-type", "application/octet-stream")
        .header("authorization", "Bearer abc123")
        .body(Body::from(body.clone()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

// ── Auth Integration Tests ─────────────────────────────────────────────

#[tokio::test]
async fn setup_status_needs_setup() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let req = Request::builder().uri("/api/0/setup").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["needs_setup"], true);
}

#[tokio::test]
async fn setup_creates_admin_and_project() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
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
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // First setup
    let req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Second setup should be forbidden
    let req2 = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"second@test.com","name":"Second","password":"password456"}"#))
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn login_returns_session_cookie() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // Setup first
    let setup_req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    app.clone().oneshot(setup_req).await.unwrap();

    // Login
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/0/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","password":"password123"}"#))
        .unwrap();
    let resp = app.oneshot(login_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check set-cookie header
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("trapfall_session="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("SameSite=Strict"));
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // Setup
    let setup_req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    app.clone().oneshot(setup_req).await.unwrap();

    // Login with wrong password
    let login_req = Request::builder()
        .method("POST")
        .uri("/api/0/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","password":"wrong"}"#))
        .unwrap();
    let resp = app.oneshot(login_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_rejects_without_cookie() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // /api/0/auth/me is now under the protected nest
    let req = Request::builder().uri("/api/0/auth/me").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── Pagination regression test (#211) ──────────────────────────────────
//
// `?page=0` previously caused a `u32` underflow in the offset calculation
// that panicked the worker (`attempt to subtract with overflow`). This test
// pins the fix: clamping page to a minimum of 1, returning a normal 200
// response instead of a 500 / connection drop.
#[tokio::test]
async fn list_issues_page_zero_does_not_underflow() {
    let store = test_store().await;
    seed_project(&store).await; // gives us a "test-project" with known DSN key
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // The auth middleware rejects unauthenticated requests, so we bypass the
    // handler entirely by calling the router directly with a crafted request.
    // Since this test is about the *handler logic* (not auth), and the handler
    // is private, we exercise it via the public route by setting up a session.
    //
    // Setup wizard creates an admin + default project + sets a session cookie.
    let setup_body = Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password12345"}"#);
    let setup_req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(setup_body)
        .unwrap();
    let setup_resp = app.clone().oneshot(setup_req).await.unwrap();
    assert_eq!(setup_resp.status(), StatusCode::CREATED, "setup must succeed");

    // Extract the session cookie from the Set-Cookie header.
    let cookie = setup_resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        .expect("setup must set a session cookie")
        .to_string();

    // Hit list_issues with page=0 — previously panicked the worker.
    let req = Request::builder()
        .uri("/api/0/projects/default/issues?page=0&per_page=5")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "page=0 must return 200, not panic/500");

    // The clamped page value should be reflected in the JSON response.
    let body_bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["page"], 1, "page=0 must be clamped to 1 in the response");
    assert_eq!(body["per_page"], 5);
    assert_eq!(body["total"], 0);
}

// ── Auth Middleware Tests (#221) ───────────────────────────────────────

/// Helper: run setup wizard and return the session cookie string.
async fn setup_and_get_cookie(app: &Router) -> String {
    let setup_req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","name":"Admin","password":"password123"}"#))
        .unwrap();
    let resp = app.clone().oneshot(setup_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "setup must succeed");
    resp.headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        .expect("setup must set a session cookie")
        .to_string()
}

#[tokio::test]
async fn auth_rejects_garbage_session_token() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // A completely invalid cookie should be rejected with 401.
    let req = Request::builder()
        .uri("/api/0/auth/me")
        .header("cookie", "trapfall_session=this-is-not-a-real-token")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Body must have a JSON error shape.
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().is_some(), "error field must be a string");
}

#[tokio::test]
async fn auth_rejects_tampered_cookie_name() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // Cookie with wrong name should be treated as unauthenticated.
    let req =
        Request::builder().uri("/api/0/auth/me").header("cookie", "wrong_cookie=abc123").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_rejects_expired_session() {
    let store = test_store().await;

    // Create a user + expired session directly in the DB.
    let _user = store.create_user("expired@test.com", "Expired", "password123").await.unwrap();
    let user_id = store.get_user_by_email("expired@test.com").await.unwrap().unwrap().id;

    // Insert an expired session (expired 1 day ago).
    let expired_token = "expired-token-12345";
    let pool = store.backend().sqlite_pool().unwrap();
    let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    sqlx::query("INSERT INTO sessions (id, user_id, token, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind("sess-expired")
        .bind(&user_id)
        .bind(expired_token)
        .bind(&past)
        .bind(&past)
        .execute(pool)
        .await
        .unwrap();

    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let req = Request::builder()
        .uri("/api/0/auth/me")
        .header("cookie", format!("trapfall_session={expired_token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_accepts_valid_session() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    // Accessing /auth/me with the cookie should succeed.
    let req = Request::builder().uri("/api/0/auth/me").header("cookie", &cookie).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["email"], "admin@test.com");
}

// ── Handler Response Shape Tests (#221) ────────────────────────────────

#[tokio::test]
async fn get_nonexistent_project_returns_404() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    let req =
        Request::builder().uri("/api/0/projects/does-not-exist").header("cookie", &cookie).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "nonexistent project should be 404 not 500");
}

#[tokio::test]
async fn get_nonexistent_issue_returns_404() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    let req = Request::builder()
        .uri("/api/0/issues/nonexistent-issue-id")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND, "nonexistent issue should be 404 not 500");
}

#[tokio::test]
async fn list_issues_for_nonexistent_project_returns_404() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    let req = Request::builder()
        .uri("/api/0/projects/nonexistent/issues")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn login_error_body_has_consistent_shape() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // Setup so a user exists.
    setup_and_get_cookie(&app).await;

    // Login with wrong password.
    let req = Request::builder()
        .method("POST")
        .uri("/api/0/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"admin@test.com","password":"totally-wrong"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Body must be `{"error":"..."}`.
    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].is_string(), "error response must have string 'error' field");
    assert!(json["error"].as_str().unwrap().contains("Invalid"));
}

#[tokio::test]
async fn setup_second_time_returns_403_with_error_body() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // First setup succeeds.
    setup_and_get_cookie(&app).await;

    // Second setup → 403 Forbidden.
    let req = Request::builder()
        .method("POST")
        .uri("/api/0/setup")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"email":"second@test.com","name":"Second","password":"password123"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].is_string());
    assert!(json["error"].as_str().unwrap().contains("already"));
}

#[tokio::test]
async fn ingest_invalid_utf8_body_returns_400() {
    let store = test_store().await;
    let project_id = seed_project(&store).await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // Send a body with invalid UTF-8 bytes — the envelope parser should reject it.
    let invalid_utf8: Vec<u8> = vec![0xFF, 0xFE, 0xFD, 0x00, 0x80];
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{project_id}/envelope/"))
        .header("content-type", "application/octet-stream")
        .header("authorization", "Bearer abc123")
        .body(Body::from(invalid_utf8))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ingest_dsn_key_mismatch_returns_401() {
    let store = test_store().await;
    let project_id = seed_project(&store).await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    // Valid envelope body but wrong DSN key.
    let body = make_envelope_body("Error", "test mismatch");
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/{project_id}/envelope/"))
        .header("content-type", "application/octet-stream")
        .header("authorization", "Bearer wrong-dsn-key")
        .body(Body::from(body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_projects_returns_masked_dsn() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    let req = Request::builder().uri("/api/0/projects").header("cookie", &cookie).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let projects = json.as_array().expect("projects should be an array");
    assert!(!projects.is_empty(), "setup should have created a default project");

    // The DSN in the list response must be masked (contains "...").
    let dsn = projects[0]["dsn"].as_str().expect("project must have a dsn");
    assert!(dsn.contains("..."), "list_projects must mask DSN, got: {dsn}");
}

#[tokio::test]
async fn logout_clears_session_cookie() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    // Logout.
    let req = Request::builder()
        .method("POST")
        .uri("/api/0/auth/logout")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Set-Cookie should clear the session (Max-Age=0).
    let set_cookie = resp.headers().get("set-cookie").and_then(|v| v.to_str().ok()).expect("logout must set a cookie");
    assert!(set_cookie.contains("trapfall_session=;") || set_cookie.contains("trapfall_session="));
    assert!(set_cookie.contains("Max-Age=0"), "logout cookie must clear with Max-Age=0");

    // After logout, the old session should no longer work.
    let req = Request::builder().uri("/api/0/auth/me").header("cookie", &cookie).body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "session must be invalid after logout");
}

#[tokio::test]
async fn delete_non_archived_project_returns_409() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    // Deleting a non-archived project should return 409 Conflict.
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/0/projects/default")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT, "delete on non-archived project must be 409");
}

#[tokio::test]
async fn search_nonexistent_project_returns_404() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    let req = Request::builder()
        .uri("/api/0/projects/nonexistent/search?q=error")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn change_password_wrong_current_returns_401() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/0/auth/change-password")
        .header("cookie", &cookie)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"current_password":"wrong-old","new_password":"newpass123"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("incorrect"));
}

#[tokio::test]
async fn change_password_weak_new_returns_400() {
    let store = test_store().await;
    let state = make_state(store, RateLimiter::default());
    let app = router(state);

    let cookie = setup_and_get_cookie(&app).await;

    // New password is too short (no digit) → 400.
    let req = Request::builder()
        .method("POST")
        .uri("/api/0/auth/change-password")
        .header("cookie", &cookie)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"current_password":"password123","new_password":"weak"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
