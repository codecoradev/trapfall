//! SQLite backend tests — runs the shared test suite against in-memory SQLite.
//!
//! Each test creates a fresh in-memory database so they are fully isolated.

#![cfg(feature = "sqlite")]

mod common;

use std::sync::Arc;
use trapfall_db::{Database, SqliteBackend};

async fn setup() -> Arc<dyn Database> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(4).connect("sqlite::memory:").await.unwrap();
    trapfall_db::run_sqlite_migrations(&pool).await.unwrap();
    Arc::new(SqliteBackend::new(pool))
}

#[tokio::test]
async fn sqlite_project_crud() {
    common::project_crud(setup().await).await;
}

#[tokio::test]
async fn sqlite_issue_upsert_dedup() {
    common::issue_upsert_dedup(setup().await).await;
}

#[tokio::test]
async fn sqlite_event_operations() {
    common::event_operations(setup().await).await;
}

#[tokio::test]
async fn sqlite_auth_and_sessions() {
    common::auth_and_sessions(setup().await).await;
}

#[tokio::test]
async fn sqlite_auth_attempts() {
    common::auth_attempts(setup().await).await;
}

#[tokio::test]
async fn sqlite_alert_rules() {
    common::alert_rules(setup().await).await;
}

#[tokio::test]
async fn sqlite_search() {
    common::search(setup().await).await;
}

#[tokio::test]
async fn sqlite_retention() {
    common::retention(setup().await).await;
}

#[tokio::test]
async fn sqlite_count_table() {
    common::count_table(setup().await).await;
}

#[tokio::test]
async fn sqlite_ping() {
    common::ping(setup().await).await;
}

#[tokio::test]
async fn sqlite_run_all() {
    common::run_all(setup().await).await;
}
