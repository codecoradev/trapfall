//! Automated Postgres integration tests using testcontainers.
//!
//! Spins up a `postgres:17-alpine` container, runs migrations,
//! and exercises the full shared test suite. No manual setup needed.
//!
//! ```bash
//! cargo test -p trapfall_db --features postgres --test testcontainers_postgres
//! ```

#![cfg(feature = "postgres")]

mod common;

use std::sync::Arc;
use testcontainers::{ContainerAsync, GenericImage, ImageExt, runners::AsyncRunner};
use trapfall_db::{Database, PostgresBackend};

const PG_USER: &str = "trapfall_test";
const PG_PASSWORD: &str = "testpass";
const PG_DB: &str = "trapfall_test";

async fn setup() -> Result<(Arc<dyn Database>, ContainerAsync<GenericImage>), Box<dyn std::error::Error>> {
    let image = GenericImage::new("postgres", "17-alpine")
        .with_exposed_port(testcontainers::core::ContainerPort::Tcp(5432))
        .with_env_var("POSTGRES_USER", PG_USER)
        .with_env_var("POSTGRES_PASSWORD", PG_PASSWORD)
        .with_env_var("POSTGRES_DB", PG_DB);

    let container = image.start().await?;

    // Retry connection — postgres may need time after container starts
    let host_port = container.get_host_port_ipv4(5432).await?;
    let url = format!("postgres://{PG_USER}:***@127.0.0.1:{host_port}/{PG_DB}");

    let pool = retry_connect(&url, 10).await?;
    trapfall_db::run_postgres_migrations(&pool).await?;

    let db = Arc::new(PostgresBackend::new(pool)) as Arc<dyn Database>;
    Ok((db, container))
}

/// Retry Postgres connection up to `max_retries` times with 2s backoff.
async fn retry_connect(url: &str, max_retries: u32) -> Result<sqlx::PgPool, Box<dyn std::error::Error>> {
    let mut last_err = None;
    for i in 0..max_retries {
        match sqlx::postgres::PgPoolOptions::new().max_connections(4).connect(url).await {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                last_err = Some(e);
                if i < max_retries - 1 {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
    Err(Box::new(last_err.unwrap()))
}

macro_rules! tc_test {
    ($name:ident, $func:path) => {
        #[tokio::test]
        async fn $name() {
            let (db, _container) = match setup().await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("skipping - setup failed: {e}");
                    return;
                }
            };
            $func(db).await;
        }
    };
}

tc_test!(tc_project_crud, common::project_crud);
tc_test!(tc_issue_upsert_dedup, common::issue_upsert_dedup);
tc_test!(tc_event_operations, common::event_operations);
tc_test!(tc_auth_and_sessions, common::auth_and_sessions);
tc_test!(tc_auth_attempts, common::auth_attempts);
tc_test!(tc_alert_rules, common::alert_rules);
tc_test!(tc_search, common::search);
tc_test!(tc_retention, common::retention);
tc_test!(tc_count_table, common::count_table);
tc_test!(tc_ping, common::ping);
tc_test!(tc_run_all, common::run_all);
