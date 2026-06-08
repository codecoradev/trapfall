//! Daemon configuration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub db_path: PathBuf,
    pub listen_addr: String,
    /// Allowed CORS origins. Empty = allow all (development only).
    /// Production should list explicit origins e.g. `["https://trapfall.example.com"]`.
    #[serde(default)]
    pub cors_origins: Vec<String>,
}
