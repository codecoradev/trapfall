//! Metrics endpoint — simple JSON health/stats.

use axum::extract::State;
use axum::response::Json;
use serde_json::{Value, json};

use crate::server::AppState;

pub async fn metrics(State(state): State<AppState>) -> Json<Value> {
    let db = state.store.backend();

    let issue_count = db.count_table("issues").await.unwrap_or(0);
    let event_count = db.count_table("events").await.unwrap_or(0);
    let project_count = db.count_table("projects").await.unwrap_or(0);

    Json(json!({
        "status": "ok",
        "stats": {
            "projects": project_count,
            "issues": issue_count,
            "events": event_count,
        }
    }))
}
