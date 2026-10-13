//! Auth HTTP handlers — setup wizard, login/logout, middleware.
//!
//! Covers:
//! - #21: Setup wizard (first-admin bootstrap)
//! - #22: Login/logout API + auth middleware
//! - #23: Brute-force lockout (enforced via Store::authenticate)

use axum::Router;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
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
        .route("/api/0/setup", get(setup_status).post(setup))
        .route("/api/0/auth/login", post(login))
        .route("/api/0/auth/logout", post(logout))
}

/// Build auth-protected routes (require session cookie).
pub fn protected_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/auth/me", get(me))
        .route("/api/auth/change-password", post(change_password))
        .layer(middleware::from_fn_with_state(state, require_auth))
}

// ── Handlers ───────────────────────────────────────────────────────────

/// GET /api/setup — Check if setup is needed.
pub async fn setup_status(State(state): State<AppState>) -> Json<SetupStatus> {
    let store = state.store.clone();
    let has = store.has_users().await.unwrap_or(false);
    Json(SetupStatus { needs_setup: !has })
}

/// POST /api/setup — Create first admin + default project + auto-login.
pub async fn setup(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<SetupRequest>,
) -> Result<(StatusCode, [(String, String); 1], Json<SetupResponse>), (StatusCode, Json<AuthErrorJson>)> {
    let store = state.store.clone();

    // Atomic first-admin creation: the check and insert are a single statement
    // (plus an advisory lock on Postgres), so concurrent setups cannot both win.
    let user = match store.create_first_user(&req.email, &req.name, &req.password).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Err((StatusCode::FORBIDDEN, Json(AuthErrorJson { error: "Setup already completed".into() })));
        }
        Err(e) => return Err((StatusCode::BAD_REQUEST, Json(AuthErrorJson { error: e.to_string() }))),
    };

    // Create default project. Prefer configured public_url for DSN;
    // fall back to request Host header so local dev keeps working.
    let host = state
        .config
        .dsn_host()
        .unwrap_or_else(|| headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("localhost:9090").to_string());
    let project = store
        .create_project_with_host("default", "Default Project", &host)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: e.to_string() })))?;

    // Create session so user is logged in after setup
    let session = store.create_session(&user.id).await.map_err(|e| {
        tracing::warn!("Failed to create setup session: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: "Internal error".into() }))
    })?;

    let cookie = format!(
        "{}={}; HttpOnly; {}; SameSite=Strict; Path=/; Max-Age={}",
        SESSION_COOKIE,
        session.token,
        state.config.cookie_secure_flag(),
        COOKIE_MAX_AGE
    );

    Ok((
        StatusCode::CREATED,
        [("set-cookie".to_string(), cookie)],
        Json(SetupResponse { user: user.into(), project_slug: project.slug, dsn: project.dsn }),
    ))
}

/// POST /api/auth/login — Authenticate and set session cookie.
pub async fn login(
    State(state): State<AppState>,
    peer: Option<axum::extract::Extension<ConnectInfo<std::net::SocketAddr>>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<(StatusCode, [(String, String); 1], Json<LoginResponse>), (StatusCode, Json<AuthErrorJson>)> {
    let store = state.store.clone();

    let peer_ip = peer.map(|c| (c.0).0.ip());
    warn_if_unconfigured_proxy(&headers, peer_ip, state.config.trust_proxy);
    let ip = client_ip(&headers, peer_ip, state.config.trust_proxy);
    let ip = ip.as_str();

    match store.authenticate(&req.email, &req.password, ip).await {
        Ok((session, user_info)) => {
            let cookie = format!(
                "{}={}; HttpOnly; {}; SameSite=Strict; Path=/; Max-Age={}",
                SESSION_COOKIE,
                session.token,
                state.config.cookie_secure_flag(),
                COOKIE_MAX_AGE
            );
            Ok((StatusCode::OK, [("set-cookie".to_string(), cookie)], Json(LoginResponse { user: user_info })))
        }
        Err(AuthError::LockedOut | AuthError::InvalidCredentials) => {
            Err((StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: "Invalid credentials".into() })))
        }
        Err(AuthError::Internal) => {
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: "Internal error".into() })))
        }
    }
}

/// POST /api/auth/logout — Clear session cookie.
pub async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> (StatusCode, [(String, String); 1]) {
    // Extract session token from cookie
    if let Some(token) = extract_session_token(&headers) {
        let store = state.store.clone();
        let _ = store.delete_session(&token).await;
    }

    let clear_cookie = format!(
        "{}=; HttpOnly; {}; SameSite=Strict; Path=/; Max-Age=0",
        SESSION_COOKIE,
        state.config.cookie_secure_flag()
    );
    (StatusCode::OK, [("set-cookie".to_string(), clear_cookie)])
}

/// GET /auth/me — Get current user info (protected).
pub async fn me(user: AuthenticatedUser) -> Json<UserInfo> {
    Json(user.0)
}

/// POST /api/auth/change-password — Change password for authenticated user.
pub async fn change_password(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    user: AuthenticatedUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, (StatusCode, Json<AuthErrorJson>)> {
    let store = state.store.clone();

    // Verify current password
    let db_user = store
        .get_user_by_id(&user.0.id)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: "Internal error".into() })))?
        .ok_or((StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: "User not found".into() })))?;

    if !trapfall_core::auth::verify_password_async(&req.current_password, &db_user.password_hash).await {
        return Err((StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: "Current password is incorrect".into() })));
    }

    // Validate and update
    trapfall_core::auth::validate_password(&req.new_password)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(AuthErrorJson { error: e })))?;

    store
        .update_password(&db_user.id, &req.new_password)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: e.to_string() })))?;

    // A password change must end any other (possibly stolen) sessions; keep the caller's.
    let current = extract_session_token(&headers);
    if let Err(e) = store.invalidate_other_sessions(&db_user.id, current.as_deref()).await {
        tracing::warn!("Failed to invalidate sessions after password change: {e}");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(AuthErrorJson { error: "Internal error".into() })));
    }

    Ok(StatusCode::OK)
}

// ── Middleware ──────────────────────────────────────────────────────────

/// Exact public routes (no authentication required).
const PUBLIC_PATHS: &[&str] =
    &["/health", "/metrics", "/api/0/config", "/api/0/setup", "/api/0/auth/login", "/api/0/auth/logout"];

/// Whether `path` is public. Uses exact matching (never suffix/substring), so
/// project slugs like `setup` or `envelope` cannot bypass authentication.
/// The ingest route `/api/{project_id}/envelope/` is matched by path segments.
pub(crate) fn is_public_path(path: &str) -> bool {
    if PUBLIC_PATHS.contains(&path) {
        return true;
    }
    // Ingest: exactly api / {id} / envelope, with an optional trailing slash.
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    let mut segs = trimmed.split('/');
    matches!(
        (segs.next(), segs.next(), segs.next(), segs.next(), segs.next(), segs.next()),
        (Some(""), Some("api"), Some(id), Some("envelope"), None, None) if !id.is_empty()
    )
}

/// Auth middleware — extracts session cookie, validates, injects user.
pub async fn require_auth(State(state): State<AppState>, mut req: Request<axum::body::Body>, next: Next) -> Response {
    // Public routes that don't require authentication
    let path = req.uri().path();
    let is_public = is_public_path(path);
    if is_public {
        return next.run(req).await;
    }

    let reject = |msg: &str| -> Response {
        (StatusCode::UNAUTHORIZED, Json(AuthErrorJson { error: msg.to_string() })).into_response()
    };

    let token = match extract_session_token(req.headers()) {
        Some(t) => t,
        None => return reject("Not authenticated"),
    };

    let store = state.store.clone();
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

/// Determine the client IP used for login rate limiting.
///
/// By default only the TCP peer address is used. When `trust_proxy` is set
/// (TrapFall is behind a proxy that overwrites the headers), the *rightmost*
/// valid `X-Forwarded-For` entry (the address our proxy observed) or `X-Real-IP`
/// is used instead; a client-supplied leftmost entry is never trusted.
pub(crate) fn client_ip(headers: &axum::http::HeaderMap, peer: Option<std::net::IpAddr>, trust_proxy: bool) -> String {
    if trust_proxy {
        let from_xff = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next())
            .map(str::trim)
            .filter(|s| s.parse::<std::net::IpAddr>().is_ok());
        let from_real = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| s.parse::<std::net::IpAddr>().is_ok());
        if let Some(ip) = from_xff.or(from_real) {
            return ip.to_string();
        }
    }
    peer.map(|p| p.to_string()).unwrap_or_else(|| "unknown".to_string())
}

fn is_local_or_private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

/// True when a loopback/private peer sends proxy headers while `trust_proxy`
/// is off: almost certainly a reverse proxy, so all clients share one IP.
pub(crate) fn looks_like_unconfigured_proxy(
    headers: &axum::http::HeaderMap,
    peer: Option<std::net::IpAddr>,
    trust_proxy: bool,
) -> bool {
    !trust_proxy
        && peer.is_some_and(is_local_or_private)
        && (headers.contains_key("x-forwarded-for") || headers.contains_key("x-real-ip"))
}

/// Log (once per process) that `TRAPFALL_TRUST_PROXY` probably needs enabling.
fn warn_if_unconfigured_proxy(headers: &axum::http::HeaderMap, peer: Option<std::net::IpAddr>, trust_proxy: bool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if looks_like_unconfigured_proxy(headers, peer, trust_proxy) && !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            "Login requests arrive from a private/loopback peer carrying X-Forwarded-For; all clients share one \
             rate-limit identity. If TrapFall is behind a reverse proxy, set TRAPFALL_TRUST_PROXY=true."
        );
    }
}

/// Extract session token from Cookie header.
pub fn extract_session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(token) = cookie.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Some(token.to_string());
        }
    }
    None
}

#[cfg(test)]
mod public_path_tests {
    use super::is_public_path;

    #[test]
    fn exact_public_routes() {
        for p in ["/health", "/metrics", "/api/0/config", "/api/0/setup", "/api/0/auth/login", "/api/0/auth/logout"] {
            assert!(is_public_path(p), "{p}");
        }
    }

    #[test]
    fn ingest_route_matched_by_segments() {
        assert!(is_public_path("/api/42/envelope/"));
        assert!(is_public_path("/api/42/envelope"));
        assert!(!is_public_path("/api/0/projects/envelope/issues"));
        assert!(!is_public_path("/api/42/envelope/extra"));
        assert!(!is_public_path("/api//envelope/"));
        assert!(!is_public_path("/x/api/42/envelope/"));
    }

    #[test]
    fn slug_lookalikes_are_not_public() {
        for p in [
            "/api/0/projects/setup",
            "/api/0/projects/x/setup",
            "/api/0/projects/auth/login",
            "/api/0/projects/x/auth/logout",
            "/api/0/projects/envelope/issues",
            "/api/0/projects/x/health",
        ] {
            assert!(!is_public_path(p), "{p}");
        }
    }
}

#[cfg(test)]
mod client_ip_tests {
    use super::client_ip;
    use axum::http::HeaderMap;
    use std::net::IpAddr;

    fn hm(k: &'static str, v: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(k, v.parse().unwrap());
        h
    }
    fn peer() -> Option<IpAddr> {
        Some("10.0.0.7".parse().unwrap())
    }

    #[test]
    fn detects_unconfigured_proxy() {
        let h = hm("x-forwarded-for", "1.2.3.4");
        assert!(super::looks_like_unconfigured_proxy(&h, peer(), false));
        assert!(super::looks_like_unconfigured_proxy(&h, Some("127.0.0.1".parse().unwrap()), false));
        assert!(!super::looks_like_unconfigured_proxy(&h, peer(), true));
        assert!(!super::looks_like_unconfigured_proxy(&h, Some("8.8.8.8".parse().unwrap()), false));
        assert!(!super::looks_like_unconfigured_proxy(&HeaderMap::new(), peer(), false));
    }

    #[test]
    fn spoofed_headers_ignored_by_default() {
        assert_eq!(client_ip(&hm("x-forwarded-for", "1.2.3.4"), peer(), false), "10.0.0.7");
        assert_eq!(client_ip(&hm("x-real-ip", "1.2.3.4"), peer(), false), "10.0.0.7");
    }

    #[test]
    fn trusted_proxy_uses_rightmost_xff() {
        // Leftmost is attacker-controlled; the proxy appended the real one last.
        assert_eq!(client_ip(&hm("x-forwarded-for", "9.9.9.9, 1.2.3.4"), peer(), true), "1.2.3.4");
    }

    #[test]
    fn trusted_proxy_falls_back_to_real_ip_then_peer() {
        assert_eq!(client_ip(&hm("x-real-ip", "5.6.7.8"), peer(), true), "5.6.7.8");
        assert_eq!(client_ip(&hm("x-forwarded-for", "garbage"), peer(), true), "10.0.0.7");
        assert_eq!(client_ip(&HeaderMap::new(), None, true), "unknown");
    }
}
