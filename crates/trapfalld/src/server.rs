//! HTTP server — Axum router, ingest handler, health check, API routes.

use axum::extract::DefaultBodyLimit;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    middleware,
    response::{IntoResponse, Json},
    routing::{get, post},
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth::AuthenticatedUser;
use crate::config::Config;
use trapfall_core::Store;
use trapfall_ingest::parse_envelope;
use trapfall_proto::{IngestEvent, IssueStatus, ListResponse};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub config: Config,
    pub ingest_tx: mpsc::Sender<IngestEvent>,
    pub rate_limiter: crate::rate_limit::RateLimiter,
    pub ws_hub: crate::ws::WsHub,
}

/// Build the Axum router.
pub fn router(state: AppState) -> Router {
    // All API routes flat — no .nest() to avoid Axum 0.8 routing quirks.
    // require_auth middleware whitelists public routes (setup, login, logout).
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(crate::metrics::metrics))
        // Public ingest API (DSN key auth)
        .route("/api/{project_id}/envelope/", post(ingest_envelope))
        // Auth + dashboard routes
        .route("/api/0/setup", get(crate::auth::setup_status).post(crate::auth::setup))
        .route("/api/0/auth/login", post(crate::auth::login))
        .route("/api/0/auth/logout", post(crate::auth::logout))
        .route("/api/0/projects", get(list_projects).post(create_project))
        .route("/api/0/projects/{slug}", get(get_project).delete(delete_project).patch(update_project))
        .route("/api/0/projects/{slug}/archive", post(archive_project).delete(unarchive_project))
        .route("/api/0/projects/{slug}/rotate-dsn", post(rotate_dsn))
        .route("/api/0/projects/{slug}/issues", get(list_issues))
        .route("/api/0/issues/{issue_id}", get(get_issue))
        .route("/api/0/issues/{issue_id}/status", post(set_issue_status))
        .route("/api/0/issues/{issue_id}/events", get(list_events))
        .route("/api/0/projects/{slug}/rules", get(list_alert_rules).post(create_alert_rule))
        .route("/api/0/rules/{rule_id}", get(get_alert_rule).delete(delete_alert_rule))
        .route("/api/0/rules/{rule_id}/toggle", post(toggle_alert_rule))
        .route("/api/0/auth/me", get(crate::auth::me))
        .route("/api/0/auth/change-password", post(crate::auth::change_password))
        .route("/api/0/projects/{slug}/search", get(search_issues))
        .route("/api/0/ws", get(crate::ws::ws_handler))
        .route_layer(middleware::from_fn_with_state(state.clone(), crate::auth::require_auth))
        .fallback(crate::spa::spa_handler)
        .layer(build_cors_layer(&state.config))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10 MB max body size (DoS protection)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
        // Swagger UI — stateless, merged after with_state
        .merge(crate::swagger::swagger_routes())
}

// ── Handlers ─────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok"
}

async fn list_projects(State(state): State<AppState>) -> Result<Json<Vec<trapfall_proto::Project>>, StatusCode> {
    let store = state.store.clone();
    let projects = store.list_projects().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Mask DSN secret keys in list responses. Dashboard clients can still see
    // host + project id; full DSN is available via the per-project rotate
    // endpoint (admin-only) and via the project detail view.
    let masked = projects.into_iter().map(|p| p.masked_dsn()).collect();
    Ok(Json(masked))
}

#[derive(serde::Deserialize)]
struct CreateProjectRequest {
    name: String,
    slug: Option<String>,
}

async fn create_project(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<trapfall_proto::Project>), StatusCode> {
    let store = state.store.clone();
    let slug = req.slug.unwrap_or_else(|| req.name.to_lowercase().replace(' ', "-"));
    // Prefer configured `public_url` (TRAPFALL_PUBLIC_URL) for DSN generation.
    // Fall back to the request Host header so local dev keeps working without
    // extra config (e.g. user accesses via http://localhost:3000).
    let host = state
        .config
        .dsn_host()
        .unwrap_or_else(|| headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("localhost:3000").to_string());
    let project = store.create_project_with_host(&slug, &req.name, &host).await.map_err(|e| {
        tracing::warn!("Create project failed: {e}");
        StatusCode::CONFLICT
    })?;
    Ok((StatusCode::CREATED, Json(project)))
}

async fn get_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<trapfall_proto::Project>, StatusCode> {
    let store = state.store.clone();
    let project = store.get_project_by_slug(&slug).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match project {
        Some(p) => Ok(Json(p)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(serde::Deserialize)]
struct UpdateProjectRequest {
    name: Option<String>,
}

async fn update_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<UpdateProjectRequest>,
) -> Result<Json<trapfall_proto::Project>, StatusCode> {
    let store = state.store.clone();
    let project = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if let Some(name) = req.name {
        let updated = store.update_project(&project.id, &name).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(updated))
    } else {
        Ok(Json(project))
    }
}

async fn delete_project(State(state): State<AppState>, Path(slug): Path<String>) -> StatusCode {
    let store = state.store.clone();
    let project = match store.get_project_by_slug(&slug).await {
        Ok(Some(p)) => p,
        _ => return StatusCode::NOT_FOUND,
    };
    // Only allow deleting archived projects
    if project.archived_at.is_none() {
        return StatusCode::CONFLICT;
    }
    match store.delete_project(&project.id).await {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(e) => {
            tracing::error!("Failed to delete project: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn archive_project(State(state): State<AppState>, Path(slug): Path<String>) -> StatusCode {
    let store = state.store.clone();
    let project = match store.get_project_by_slug(&slug).await {
        Ok(Some(p)) => p,
        _ => return StatusCode::NOT_FOUND,
    };
    match store.archive_project(&project.id).await {
        Ok(()) => StatusCode::OK,
        Err(e) => {
            tracing::error!("Failed to archive project: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn unarchive_project(State(state): State<AppState>, Path(slug): Path<String>) -> StatusCode {
    let store = state.store.clone();
    let project = match store.get_project_by_slug(&slug).await {
        Ok(Some(p)) => p,
        _ => return StatusCode::NOT_FOUND,
    };
    match store.unarchive_project(&project.id).await {
        Ok(()) => StatusCode::OK,
        Err(e) => {
            tracing::error!("Failed to unarchive project: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

async fn rotate_dsn(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<trapfall_proto::Project>, StatusCode> {
    let store = state.store.clone();
    let project = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    store.rotate_dsn(&project.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Re-fetch to get updated DSN
    let updated = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(updated))
}

async fn ingest_envelope(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> StatusCode {
    tracing::info!("Ingest request: project_id={project_id} body_len={}", body.len());
    if !state.rate_limiter.try_consume(&project_id, 1.0) {
        return StatusCode::TOO_MANY_REQUESTS;
    }

    // Validate DSN key from Authorization header
    let store = state.store.clone();
    // Extract DSN key: try X-Sentry-Auth header first, then Authorization Bearer
    let dsn_key = headers
        .get("x-sentry-auth")
        .and_then(|v| v.to_str().ok())
        .and_then(trapfall_ingest::extract_sentry_key)
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
        });
    let dsn_key = match dsn_key {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    // Verify DSN key matches project
    let project = match store.get_project_by_id(&project_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::warn!("Project not found by id: {project_id}");
            return StatusCode::NOT_FOUND;
        }
        Err(e) => {
            tracing::error!("DB error looking up project: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };
    match store.get_project_by_dsn_key(&dsn_key).await {
        Ok(Some(p)) if p.id == project.id => {}
        Ok(Some(p)) => {
            tracing::warn!("DSN key mismatch: expected project {} got {}", project.id, p.id);
            return StatusCode::UNAUTHORIZED;
        }
        Ok(None) => {
            tracing::warn!("No project found for DSN key");
            return StatusCode::UNAUTHORIZED;
        }
        Err(e) => {
            tracing::error!("DB error checking DSN key: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    }

    // Extract content encoding
    let encoding = headers.get("content-encoding").and_then(|v| v.to_str().ok());
    tracing::info!("Encoding: {:?}", encoding);

    // Parse envelope
    let events = match parse_envelope(&body, encoding) {
        Ok(e) => {
            tracing::info!("Parsed {} events", e.len());
            e
        }
        Err(e) => {
            tracing::warn!("Failed to parse envelope: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    if events.is_empty() {
        return StatusCode::OK;
    }

    let mut accepted = 0;
    for event in events {
        let fingerprint = trapfall_core::derive_fingerprint(&event);
        let ingest_event = IngestEvent {
            project_id: project.id.clone(),
            fingerprint,
            event,
            received_at: chrono::Utc::now().to_rfc3339(),
        };

        match state.ingest_tx.send(ingest_event).await {
            Ok(()) => accepted += 1,
            Err(e) => {
                tracing::warn!("Ingest channel full or closed: {e}");
                return StatusCode::SERVICE_UNAVAILABLE;
            }
        }
    }

    tracing::info!("Accepted {accepted} events for project {project_id}");
    StatusCode::OK
}

// ── Issue / Event Handlers ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ListIssuesQuery {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
    status: Option<String>,
    level: Option<String>,
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    20
}

async fn list_issues(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListIssuesQuery>,
) -> Result<Json<ListResponse<trapfall_proto::Issue>>, StatusCode> {
    let store = state.store.clone();

    // Resolve project slug to ID
    let project = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let limit = query.per_page.min(100) as i64;
    let page = query.page.max(1);
    let offset = ((page - 1) * limit as u32) as i64;

    let total = store.count_issues(&project.id, query.status.as_deref(), query.level.as_deref()).await.unwrap_or(0);
    let issues = store
        .list_issues_filtered(&project.id, query.status.as_deref(), query.level.as_deref(), limit, offset)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ListResponse { data: issues, total, page, per_page: limit as u32 }))
}

async fn get_issue(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
) -> Result<Json<trapfall_proto::Issue>, StatusCode> {
    let store = state.store.clone();
    store
        .get_issue(&issue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

#[derive(Deserialize)]
struct SetStatusRequest {
    status: IssueStatus,
}

async fn set_issue_status(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
    Json(req): Json<SetStatusRequest>,
) -> StatusCode {
    let store = state.store.clone();
    match store.set_issue_status(&issue_id, req.status).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
struct ListEventsQuery {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
}

async fn list_events(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<ListResponse<trapfall_proto::StoredEvent>>, StatusCode> {
    let store = state.store.clone();
    let limit = query.per_page.min(100) as i64;
    let page = query.page.max(1);
    let offset = ((page - 1) * limit as u32) as i64;

    let total = store.count_events(&issue_id).await.unwrap_or(0);
    let events = store.list_events(&issue_id, limit, offset).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ListResponse { data: events, total, page, per_page: limit as u32 }))
}

// ── Alert Rule Handlers ────────────────────────────────────────────────

async fn list_alert_rules(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Vec<trapfall_proto::AlertRule>>, StatusCode> {
    let store = state.store.clone();
    let project = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let rules = store.list_alert_rules(&project.id).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rules))
}

async fn create_alert_rule(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(req): Json<trapfall_proto::CreateAlertRule>,
) -> Result<Json<trapfall_proto::AlertRule>, StatusCode> {
    let store = state.store.clone();
    let project = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let action_type = req.action_type.unwrap_or_else(|| "webhook".to_string());
    let action_config = req.action_config.unwrap_or(serde_json::json!({}));
    let cooldown = req.cooldown_seconds.unwrap_or(300);

    let rule = store
        .create_alert_rule(
            &project.id,
            &req.name,
            &serde_json::to_string(&req.conditions).unwrap_or_default(),
            &action_type,
            &serde_json::to_string(&action_config).unwrap_or_default(),
            cooldown,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rule))
}

async fn get_alert_rule(
    State(state): State<AppState>,
    Path(rule_id): Path<String>,
) -> Result<Json<trapfall_proto::AlertRule>, StatusCode> {
    let store = state.store.clone();
    store
        .get_alert_rule(&rule_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
        .map(Json)
}

async fn delete_alert_rule(State(state): State<AppState>, Path(rule_id): Path<String>) -> StatusCode {
    let store = state.store.clone();
    match store.delete_alert_rule(&rule_id).await {
        Ok(true) => StatusCode::OK,
        Ok(false) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
struct ToggleRequest {
    enabled: bool,
}

async fn toggle_alert_rule(
    State(state): State<AppState>,
    Path(rule_id): Path<String>,
    Json(req): Json<ToggleRequest>,
) -> StatusCode {
    let store = state.store.clone();
    match store.toggle_alert_rule(&rule_id, req.enabled).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ── Search Handler ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    status: Option<String>,
    level: Option<String>,
    per_page: Option<i64>,
    limit: Option<i64>,
    page: Option<i64>,
}

async fn search_issues(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    let store = state.store.clone();
    let project = match store.get_project_by_slug(&slug).await {
        Ok(Some(p)) => p,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    // Frontend sends per_page (preferred) or limit, and page is 1-indexed.
    let per_page = query.per_page.or(query.limit).unwrap_or(50).clamp(1, 100);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let total = trapfall_search::count_search_issues(
        &state.store,
        &query.q,
        Some(&project.id),
        query.status.as_deref(),
        query.level.as_deref(),
    )
    .await
    .unwrap_or(0);

    match trapfall_search::search_issues(
        &state.store,
        &query.q,
        Some(&project.id),
        query.status.as_deref(),
        query.level.as_deref(),
        per_page,
        offset,
    )
    .await
    {
        Ok(issues) => {
            Json(ListResponse { data: issues, total, page: page as u32, per_page: per_page as u32 }).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ── CORS Builder ──────────────────────────────────────────────────────

/// Build CORS layer from config. Empty `cors_origins` = allow all (dev mode).
/// Production should set explicit origins.
fn build_cors_layer(config: &Config) -> CorsLayer {
    let allow_headers =
        [axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION, axum::http::header::COOKIE];
    let allow_methods = [Method::GET, Method::POST, Method::DELETE, Method::OPTIONS];

    if config.cors_origins.is_empty() {
        tracing::warn!("CORS: allowing all origins — set cors_origins in config for production");
        CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
    } else {
        let origins: Vec<_> = config.cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
        CorsLayer::new().allow_origin(origins).allow_methods(allow_methods).allow_headers(allow_headers)
    }
}
