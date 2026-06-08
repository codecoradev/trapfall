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
    /// Set to false to disable Secure cookie flag (e.g. for local HTTP development).
    /// Default: true. Production should always use true with HTTPS.
    #[serde(default = "default_secure_cookie")]
    pub secure_cookie: bool,
}

fn default_secure_cookie() -> bool {
    true
}

impl Config {
    /// Returns "Secure" if secure_cookie is true, empty string otherwise.
    pub fn cookie_secure_flag(&self) -> &'static str {
        if self.secure_cookie { "Secure" } else { "" }
    }
}
