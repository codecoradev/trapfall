//! # trapfall-core
//!
//! Core logic — storage facade, auth, fingerprinting, and helpers.
//!
//! All database SQL lives in [`trapfall_db`] backends. This crate provides
//! the [`Store`] facade plus authentication and fingerprinting utilities.

pub mod auth;
pub mod fingerprint;
pub mod store;

pub use auth::{UserInfo, hash_password, verify_password};
pub use fingerprint::derive_fingerprint;
pub use store::Store;

use uuid::Uuid;

/// Generate a new UUID v4 string.
pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a DSN with the given public base URL and project ID.
/// Format: `https://{key}@{host}/{project_id}`
pub fn generate_dsn_with(host: &str, project_id: &str) -> String {
    let key = Uuid::new_v4();
    format!("https://{key}@{host}/{project_id}")
}

/// Generate a DSN with placeholder host.
/// When creating projects via CLI (no request context), we use a generic DSN.
/// Note: project_id is set to "1" as placeholder — should be regenerated with real project ID.
pub fn generate_dsn() -> String {
    generate_dsn_with("localhost:9090", "1")
}
