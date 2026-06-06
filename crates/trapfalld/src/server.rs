//! HTTP server — Axum router, ingest handler, health check, API routes.

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use trapfall_core::Store;
use trapfall_ingest::parse_envelope;
use trapfall_proto::{IngestEvent, IssueStatus, ListResponse};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    #[allow(dead_code)]
    pub config: Config,
    pub ingest_tx: mpsc::Sender<IngestEvent>,
    pub rate_limiter: crate::rate_limit::RateLimiter,
    pub ws_hub: crate::ws::WsHub,
}

/// Build the Axum router.
pub fn router(state: AppState) -> Router {
    let auth_routes = crate::auth::auth_routes();
    let protected_routes = crate::auth::protected_routes(state.clone());

    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(crate::metrics::metrics))
        // ── Public API (ingest) ──────────────────────────────────────
        .route("/api/{project_id}/envelope/", post(ingest_envelope))
        // ── Dashboard API (auth-protected) ───────────────────────────
        .route("/api/0/projects", get(list_projects))
        .route("/api/0/projects/{slug}", get(get_project))
        .route("/api/0/projects/{slug}/issues", get(list_issues))
        .route("/api/0/issues/{issue_id}", get(get_issue))
        .route("/api/0/issues/{issue_id}/status", post(set_issue_status))
        .route("/api/0/issues/{issue_id}/events", get(list_events))
        .route("/api/0/ws", get(crate::ws::ws_handler))
        .merge(auth_routes)
        .merge(protected_routes)
        .fallback(crate::spa::spa_handler)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ── Handlers ─────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok"
}

async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<trapfall_proto::Project>>, StatusCode> {
    let store = Store::new(state.pool);
    let projects = store.list_projects().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(projects))
}

async fn get_project(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<trapfall_proto::Project>, StatusCode> {
    let store = Store::new(state.pool);
    let project =
        store.get_project_by_slug(&slug).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match project {
        Some(p) => Ok(Json(p)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn ingest_envelope(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> StatusCode {
    // Rate limit check
    if !state.rate_limiter.try_consume(&project_id, 1.0) {
        return StatusCode::TOO_MANY_REQUESTS;
    }
    // Extract content encoding
    let encoding = headers.get("content-encoding").and_then(|v| v.to_str().ok());

    // Parse envelope
    let events = match parse_envelope(&body, encoding) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to parse envelope: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    if events.is_empty() {
        return StatusCode::OK;
    }

    // Process each event
    let store = Store::new(state.pool.clone());

    // Validate project exists
    if let Ok(Some(_project)) = store.get_project_by_slug(&project_id).await {
        // Valid project — process events
    } else {
        tracing::debug!("Unknown project: {project_id}");
        return StatusCode::NOT_FOUND;
    }

    let mut accepted = 0;
    for event in events {
        let fingerprint = trapfall_core::derive_fingerprint(&event);
        let ingest_event = IngestEvent {
            project_id: project_id.clone(),
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

    tracing::trace!("Accepted {accepted} events for project {project_id}");
    StatusCode::OK
}

// ── Issue / Event Handlers ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ListIssuesQuery {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
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
    let store = Store::new(state.pool);

    // Resolve project slug to ID
    let project = store
        .get_project_by_slug(&slug)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let offset = ((query.page - 1) * query.per_page) as i64;
    let limit = query.per_page as i64;

    let issues = store
        .list_issues(&project.id, limit, offset)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Total count approximation — use issues.len() for now
    let total = issues.len() as i64;

    Ok(Json(ListResponse { data: issues, total, page: query.page, per_page: query.per_page }))
}

async fn get_issue(
    State(state): State<AppState>,
    Path(issue_id): Path<String>,
) -> Result<Json<trapfall_proto::Issue>, StatusCode> {
    let store = Store::new(state.pool);
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
    let store = Store::new(state.pool);
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
    let store = Store::new(state.pool);
    let offset = ((query.page - 1) * query.per_page) as i64;
    let limit = query.per_page as i64;

    let events = store
        .list_events(&issue_id, limit, offset)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total = events.len() as i64;

    Ok(Json(ListResponse { data: events, total, page: query.page, per_page: query.per_page }))
}
