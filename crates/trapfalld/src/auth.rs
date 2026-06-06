//! Auth HTTP handlers — setup wizard, login/logout, middleware.
//!
//! Covers:
//! - #21: Setup wizard (first-admin bootstrap)
//! - #22: Login/logout API + auth middleware
//! - #23: Brute-force lockout (enforced via Store::authenticate)

use axum::Router;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use trapfall_core::Store;
use trapfall_core::auth::{AuthError, UserInfo};

use crate::AppState;

// ── Request/Response Types ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SetupRequest {
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SetupResponse {
    pub user: UserInfo,
    pub project_slug: String,
    pub dsn: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub user: UserInfo,
}

#[derive(Serialize)]
pub struct SetupStatus {
    pub needs_setup: bool,
}

#[derive(Serialize)]
pub struct AuthErrorJson {
    pub error: String,
}

// ── Cookie Constants ───────────────────────────────────────────────────

const SESSION_COOKIE: &str = "trapfall_session";
const COOKIE_MAX_AGE: &str = "604800"; // 7 days in seconds

// ── Router ─────────────────────────────────────────────────────────────

/// Build auth routes (no auth middleware — these are public).
pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/api/setup", get(setup_status).post(setup))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
}

/// Build auth-protected routes (require session cookie).
pub fn protected_routes(state: AppState) -> Router<AppState> {
    Router::new().route("/api/auth/me", get(me)).layer(middleware::from_fn_with_state(state, require_auth))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// GET /api/setup — Check if setup is needed.
async fn setup_status(State(state): State<AppState>) -> Json<SetupStatus> {
    let store = Store::new(state.pool);
    let has = store.has_users().await.unwrap_or(false);
    Json(SetupStatus { needs_setup: !has })
}

/// POST /api/setup — Create first admin + default project.
async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<(StatusCode, Json<SetupResponse>), (StatusCode, Json<AuthErrorJson>)> {
    let store = Store::new(state.pool);

    // Only allow when no users exist
    if store.has_users().await.unwrap_or(false) {
        return Err((StatusCode::FORBIDDEN, Json(AuthErrorJson { error: "Setup already completed".into() })));
    }

    // Create admin user
    let user = store
        .create_user(&req.email, &req.name, &req.password)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(AuthErrorJson { error: e.to_string() })))?;

    // Create default project
    let project = store
        .create_project("default", "Default Project")
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: e.to_string() })))?;

    Ok((StatusCode::CREATED, Json(SetupResponse { user: user.into(), project_slug: project.slug, dsn: project.dsn })))
}

/// POST /api/auth/login — Authenticate and set session cookie.
async fn login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<(StatusCode, [(String, String); 1], Json<LoginResponse>), (StatusCode, Json<AuthErrorJson>)> {
    let store = Store::new(state.pool);

    // Extract client IP (best-effort)
    let ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    match store.authenticate(&req.email, &req.password, ip).await {
        Ok((session, user_info)) => {
            let cookie = format!(
                "{}={}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age={}",
                SESSION_COOKIE, session.token, COOKIE_MAX_AGE
            );
            Ok((StatusCode::OK, [("set-cookie".to_string(), cookie)], Json(LoginResponse { user: user_info })))
        }
        Err(AuthError::LockedOut) => {
            Err((StatusCode::TOO_MANY_REQUESTS, Json(AuthErrorJson { error: "Invalid credentials".into() })))
        }
        Err(AuthError::InvalidCredentials) => {
            Err((StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: "Invalid credentials".into() })))
        }
        Err(AuthError::Internal) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: "Internal error".into() })))
        }
    }
}

/// POST /api/auth/logout — Clear session cookie.
async fn logout(State(state): State<AppState>, headers: axum::http::HeaderMap) -> (StatusCode, [(String, String); 1]) {
    // Extract session token from cookie
    if let Some(token) = extract_session_token(&headers) {
        let store = Store::new(state.pool);
        let _ = store.delete_session(&token).await;
    }

    let clear_cookie = format!("{}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0", SESSION_COOKIE);
    (StatusCode::OK, [("set-cookie".to_string(), clear_cookie)])
}

/// GET /auth/me — Get current user info (protected).
pub async fn me(user: AuthenticatedUser) -> Json<UserInfo> {
    Json(user.0)
}

// ── Middleware ──────────────────────────────────────────────────────────

/// Auth middleware — extracts session cookie, validates, injects user.
pub async fn require_auth(State(state): State<AppState>, mut req: Request<axum::body::Body>, next: Next) -> Response {
    let reject = |msg: &str| -> Response {
        (StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: msg.to_string() })).into_response()
    };

    let token = match extract_session_token(req.headers()) {
        Some(t) => t,
        None => return reject("Not authenticated"),
    };

    let store = Store::new(state.pool);
    let session = match store.get_session(&token).await {
        Ok(Some(s)) => s,
        Ok(None) => return reject("Session expired"),
        Err(_) => return reject("Internal error"),
    };

    let user = match store.get_user_by_id(&session.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return reject("User not found"),
        Err(_) => return reject("Internal error"),
    };

    // Inject user info into request extensions
    req.extensions_mut().insert(AuthenticatedUser(user.into()));
    next.run(req).await
}

// ── Extractor ──────────────────────────────────────────────────────────

/// Authenticated user extracted from request extensions.
#[derive(Clone)]
pub struct AuthenticatedUser(pub UserInfo);

impl<S: Send + Sync> FromRequestParts<S> for AuthenticatedUser {
    type Rejection = (StatusCode, Json<AuthErrorJson>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: "Not authenticated".into() })))
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Extract session token from Cookie header.
fn extract_session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(token) = cookie.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Some(token.to_string());
        }
    }
    None
}
