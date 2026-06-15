//! Search module — LIKE + trigram substring matching for issues.
//!
//! Thin wrapper over [`Database::search_issues`] and
//! [`Database::count_search_issues`]. All SQL lives in the backend.
//!
//! Kept as a separate crate for organisational clarity and future
//! search-backend extensions (e.g. FTS5, Postgres trigram).

use anyhow::Result;
use trapfall_core::Store;
use trapfall_proto::Issue;

/// Search issues by substring query with optional filters.
///
/// LIKE wildcards (`%`, `_`) in the query are escaped by the backend
/// to prevent unexpected matches.
pub async fn search_issues(
    store: &Store,
    query: &str,
    project_id: Option<&str>,
    status: Option<&str>,
    level: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Issue>> {
    store.backend().search_issues(query, project_id, status, level, limit, offset).await
}

/// Count issues matching a search query with optional filters.
///
/// Uses the same WHERE clause as [`search_issues`] for accurate totals.
pub async fn count_search_issues(
    store: &Store,
    query: &str,
    project_id: Option<&str>,
    status: Option<&str>,
    level: Option<&str>,
) -> Result<i64> {
    store.backend().count_search_issues(query, project_id, status, level).await
}
