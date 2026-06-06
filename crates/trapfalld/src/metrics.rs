//! Metrics endpoint — simple JSON health/stats.
//!
//! Not Prometheus-format — just a simple JSON endpoint for MVP.
//! Can be upgraded to Prometheus exposition later.

use axum::extract::State;
use axum::response::Json;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::server::AppState;

pub async fn metrics(State(state): State<AppState>) -> Json<Value> {
    let pool = &state.pool;

    let issue_count = count_rows(pool, "issues").await;
    let event_count = count_rows(pool, "events").await;
    let project_count = count_rows(pool, "projects").await;

    Json(json!({
        "status": "ok",
        "stats": {
            "projects": project_count,
            "issues": issue_count,
            "events": event_count,
        }
    }))
}

async fn count_rows(pool: &SqlitePool, table: &str) -> i64 {
    let query = format!("SELECT COUNT(*) as count FROM {table}");
    let row: (i64,) = sqlx::query_as(&query).fetch_one(pool).await.unwrap_or((0,));
    row.0
}
