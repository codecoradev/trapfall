//! Automated Postgres integration tests using testcontainers.
//!
//! Spins up a `postgres:17-alpine` container, runs migrations,
//! and exercises the full shared test suite. No manual setup needed.

#![cfg(feature = "postgres")]

mod common;

use std::sync::Arc;
use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage, ImageExt};
use trapfall_db::{Database, PostgresBackend};

const PG_USER: &str = "trapfall_test";
const PG_PASSWORD: &str = "testpass";
const PG_DB: &str = "trapfall_test";

fn docker_available() -> bool {
    std::process::Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn setup() -> (Arc<dyn Database>, ContainerAsync<GenericImage>) {
    let image = GenericImage::new("postgres", "17-alpine")
        .with_exposed_port(testcontainers::core::ContainerPort::Tcp(5432))
        .with_env_var("POSTGRES_USER", PG_USER)
        .with_env_var("POSTGRES_PASSWORD", PG_PASSWORD)
        .with_env_var("POSTGRES_DB", PG_DB);

    let container = image.start().await.expect("failed to start postgres container");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let host_port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("no port mapping");

    let url = format!("postgres://{PG_USER}:***@127.0.0.1:{host_port}/{PG_DB}");

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("failed to connect to testcontainers postgres");
    trapfall_db::run_postgres_migrations(&pool)
        .await
        .expect("failed to run migrations");

    let db = Arc::new(PostgresBackend::new(pool)) as Arc<dyn Database>;
    (db, container)
}

macro_rules! tc_test {
    ($name:ident, $func:path) => {
        #[tokio::test]
        async fn $name() {
            if !docker_available() {
                eprintln!("skipping - Docker not available");
                return;
            }
            let (db, _container) = setup().await;
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
