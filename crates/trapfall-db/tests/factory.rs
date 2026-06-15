//! Tests for the connection factory (`open_database` + `normalise_url`).
//!
//! Phase 2 (#167): verify URL scheme detection, feature-flag behaviour,
//! and error messages.

use trapfall_db::Database;

// ── Helper: extract error message without requiring Debug ─────────────

fn err_msg<T>(result: Result<T, anyhow::Error>) -> String {
    result.err().map(|e| e.to_string()).unwrap_or_default()
}

// ── normalise_url ────────────────────────────────────────────────────

#[test]
fn normalise_bare_path_defaults_to_sqlite() {
    assert_eq!(trapfall_db::normalise_url("trapfall.db"), "sqlite:trapfall.db");
    assert_eq!(trapfall_db::normalise_url("./data/app.db"), "sqlite:./data/app.db");
    assert_eq!(trapfall_db::normalise_url("/var/lib/trapfall.db"), "sqlite:/var/lib/trapfall.db");
}

#[test]
fn normalise_passes_through_prefixed_urls() {
    assert_eq!(trapfall_db::normalise_url("sqlite:trapfall.db"), "sqlite:trapfall.db");
    assert_eq!(trapfall_db::normalise_url("postgres://user:pass@host:5432/db"), "postgres://user:pass@host:5432/db");
    assert_eq!(trapfall_db::normalise_url("postgresql://localhost/mydb"), "postgresql://localhost/mydb");
}

// ── open_database: SQLite ────────────────────────────────────────────

#[tokio::test]
async fn open_sqlite_memory_returns_backend() {
    let db = trapfall_db::open_database("sqlite::memory:").await.unwrap();
    // Verify it implements Database by calling ping.
    let ok = db.ping().await.unwrap();
    assert!(ok);
}

#[tokio::test]
async fn open_sqlite_bare_path_works() {
    let url = trapfall_db::normalise_url("test_factory.db");
    assert!(url.starts_with("sqlite:"));
    let db = trapfall_db::open_database(&url).await.unwrap();
    assert!(db.ping().await.unwrap());
    // Clean up.
    let _ = std::fs::remove_file("test_factory.db");
}

// ── open_database: Postgres (feature-gated) ──────────────────────────

#[tokio::test]
async fn open_postgres_url_without_feature_returns_clear_error() {
    let result = trapfall_db::open_database("postgres://localhost/db").await;
    assert!(result.is_err());

    #[cfg(not(feature = "postgres"))]
    {
        let msg = err_msg(result);
        assert!(
            msg.contains("postgres") && msg.contains("feature"),
            "error should mention postgres + feature, got: {msg}"
        );
    }
}

#[tokio::test]
async fn open_postgresql_scheme_also_detected() {
    let result = trapfall_db::open_database("postgresql://localhost/db").await;
    assert!(result.is_err());

    #[cfg(not(feature = "postgres"))]
    {
        let msg = err_msg(result);
        assert!(msg.contains("postgres"), "error should mention postgres, got: {msg}");
    }
}

// ── open_database: unknown scheme ────────────────────────────────────

#[tokio::test]
async fn open_unknown_scheme_returns_error() {
    let result = trapfall_db::open_database("mysql://localhost/db").await;
    assert!(result.is_err());
    let msg = err_msg(result);
    assert!(msg.contains("mysql") || msg.contains("unrecognised"), "error should mention the scheme, got: {msg}");
}

#[tokio::test]
async fn open_empty_url_returns_error() {
    let result = trapfall_db::open_database("").await;
    assert!(result.is_err());
}

// ── Trait object sanity ──────────────────────────────────────────────

#[tokio::test]
async fn factory_returns_trait_object() {
    let db: std::sync::Arc<dyn Database> = trapfall_db::open_database("sqlite::memory:").await.unwrap();
    // ping works without migrations.
    assert!(db.ping().await.unwrap());
    // has_users on empty DB returns false (table may not exist yet, but
    // the SQLite impl handles this gracefully by returning false).
}
