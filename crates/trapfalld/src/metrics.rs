//! Metrics endpoint — simple JSON health/stats.

use axum::extract::State;
use axum::response::Json;
use serde_json::{Value, json};

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

/// Count rows using parameterized query — table name validated against whitelist.
async fn count_rows(pool: &sqlx::SqlitePool, table: &str) -> i64 {
    // Whitelist allowed table names to prevent SQL injection
    let allowed = ["issues", "events", "projects", "alert_rules", "alert_history"];
    if !allowed.contains(&table) {
        return 0;
    }
    let query = format!("SELECT COUNT(*) as count FROM {table}");
    let row: (i64,) = sqlx::query_as(&query).fetch_one(pool).await.unwrap_or((0,));
    row.0
}
