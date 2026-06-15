//! Error type for database operations.

use thiserror::Error;

/// Database error. Currently a thin wrapper; will grow as Postgres lands.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("backend unavailable: {0}")]
    Backend(String),
    #[error("other: {0}")]
    Other(String),
}

impl From<anyhow::Error> for DbError {
    fn from(e: anyhow::Error) -> Self {
        DbError::Other(e.to_string())
    }
}
