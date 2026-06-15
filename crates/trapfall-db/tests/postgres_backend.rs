//! Postgres backend tests — runs the shared test suite against a live Postgres.
//!
//! Requires a running Postgres instance. Set `TEST_POSTGRES_URL` env var:
//!
//! ```bash
//! # Start a test Postgres via Docker:
//! docker run -d --name trapfall-test-pg \
//!   -e POSTGRES_DB=trapfall_test \
//!   -e POSTGRES_USER=trapfall \
//!   -e POSTGRES_PASSWORD=test \
//!   -p 5433:5432 \
//!   postgres:17-alpine
//!
//! TEST_POSTGRES_URL=postgres://trapfall:test@localhost:5433/trapfall_test \
//!   cargo test -p trapfall_db --features postgres --test postgres_backend
//! ```
//!
//! If `TEST_POSTGRES_URL` is not set, all tests are skipped (not failed).

#![cfg(feature = "postgres")]

mod common;

use std::sync::Arc;
use trapfall_db::{Database, PostgresBackend};

fn pg_url() -> Option<String> {
    std::env::var("TEST_POSTGRES_URL").ok().filter(|s| !s.is_empty())
}

async fn setup() -> Arc<dyn Database> {
    let url = pg_url().expect("TEST_POSTGRES_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    trapfall_db::run_postgres_migrations(&pool).await.unwrap();

    // Clean all tables for test isolation
    for table in ["alert_history", "alert_rules", "events", "issues", "sessions", "auth_attempts", "users", "projects"]
    {
        sqlx::query(&format!("DELETE FROM {table}")).execute(&pool).await.unwrap();
    }

    Arc::new(PostgresBackend::new(pool))
}

macro_rules! pg_test {
    ($name:ident, $func:path) => {
        #[tokio::test]
        async fn $name() {
            if pg_url().is_none() {
                eprintln!("⚠️ Skipping — TEST_POSTGRES_URL not set");
                return;
            }
            $func(setup().await).await;
        }
    };
}

pg_test!(pg_project_crud, common::project_crud);
pg_test!(pg_issue_upsert_dedup, common::issue_upsert_dedup);
pg_test!(pg_event_operations, common::event_operations);
pg_test!(pg_auth_and_sessions, common::auth_and_sessions);
pg_test!(pg_auth_attempts, common::auth_attempts);
pg_test!(pg_alert_rules, common::alert_rules);
pg_test!(pg_search, common::search);
pg_test!(pg_retention, common::retention);
pg_test!(pg_count_table, common::count_table);
pg_test!(pg_ping, common::ping);
pg_test!(pg_run_all, common::run_all);
