//! Daemon configuration.
//!
//! All runtime config is loaded from environment variables (with sensible
//! defaults) via [`Config::from_env`]. No inline `std::env::var` calls should
//! live outside this module — keep config centralized so deployment stays
//! flexible.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// TrapFall daemon configuration.
///
/// Built exclusively from environment variables via [`Config::from_env`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Database URL/path as resolved from `TRAPFALL_DATABASE_URL`
    /// (or CLI `--db`). Reflects the *actual* backend in use — not a hardcoded
    /// placeholder. Stored as a path because the value can be a file path
    /// (`/data/trapfall.db`) or a URL (`postgres://...`).
    pub db_path: PathBuf,
    /// HTTP listen address (`TRAPFALL_LISTEN`, default `0.0.0.0:9090`).
    pub listen_addr: String,
    /// Allowed CORS origins (`TRAPFALL_CORS_ORIGINS`, comma-separated).
    /// Empty = allow all (development only). Production should list explicit
    /// origins e.g. `https://trapfall.example.com`.
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// Whether to set the `Secure` flag on auth cookies
    /// (`TRAPFALL_SECURE_COOKIE`, default `true`).
    /// Set to `false`/`0` for local HTTP development.
    #[serde(default = "default_secure_cookie")]
    pub secure_cookie: bool,
    /// Public base URL of the TrapFall instance
    /// (`TRAPFALL_PUBLIC_URL` / legacy `TRAPFALL_DSN_HOST`).
    ///
    /// Used to generate DSN values for new projects instead of trusting the
    /// per-request `Host` header. Falls back to `listen_addr` when unset.
    /// Example: `https://trapfall.example.com` or `http://localhost:9090`.
    #[serde(default)]
    pub public_url: Option<String>,
}

fn default_secure_cookie() -> bool {
    true
}

impl Config {
    /// Returns "Secure" if `secure_cookie` is true, empty string otherwise.
    pub fn cookie_secure_flag(&self) -> &'static str {
        if self.secure_cookie { "Secure" } else { "" }
    }

    /// Host (or full base URL) to use when minting a DSN for new projects.
    ///
    /// Order of preference:
    /// 1. Explicit `public_url` env var (`TRAPFALL_PUBLIC_URL` /
    ///    `TRAPFALL_DSN_HOST`).
    /// 2. Fallback to `listen_addr`.
    ///
    /// The returned string strips any trailing slash from a URL-style value so
    /// callers can safely format it into a DSN.
    pub fn dsn_host(&self) -> String {
        let raw = self
            .public_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.listen_addr)
            .trim_end_matches('/');
        raw.to_string()
    }

    /// Load configuration from environment variables.
    ///
    /// `db_url` is the already-resolved database URL (from CLI `--db` /
    /// `TRAPFALL_DATABASE_URL`) — passed in by the caller so the same source
    /// of truth is used for the global DB handle and the recorded config.
    /// `listen_addr` is the already-resolved listen address (from CLI
    /// `serve --listen` / `TRAPFALL_LISTEN`).
    pub fn from_env(db_url: &str, listen_addr: &str) -> Self {
        Self {
            db_path: PathBuf::from(db_url),
            listen_addr: listen_addr.to_string(),
            cors_origins: parse_cors_origins(),
            secure_cookie: parse_secure_cookie(),
            public_url: parse_public_url(),
        }
    }
}

/// Parse `TRAPFALL_CORS_ORIGINS` (comma-separated, trimmed, empty filtered).
fn parse_cors_origins() -> Vec<String> {
    std::env::var("TRAPFALL_CORS_ORIGINS")
        .ok()
        .map(|s| {
            s.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect()
        })
        .unwrap_or_default()
}

/// Parse `TRAPFALL_SECURE_COOKIE`. Default `true`. `false`/`0`/`no` disables it.
fn parse_secure_cookie() -> bool {
    match std::env::var("TRAPFALL_SECURE_COOKIE") {
        Ok(v) => {
            let lower = v.trim().to_lowercase();
            !(lower == "false" || lower == "0" || lower == "no" || lower == "off")
        }
        Err(_) => default_secure_cookie(),
    }
}

/// Parse `TRAPFALL_PUBLIC_URL`, falling back to legacy `TRAPFALL_DSN_HOST`.
fn parse_public_url() -> Option<String> {
    std::env::var("TRAPFALL_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("TRAPFALL_DSN_HOST").ok().filter(|s| !s.trim().is_empty()))
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cfg() -> Config {
        Config {
            db_path: PathBuf::from("trapfall.db"),
            listen_addr: "0.0.0.0:9090".into(),
            cors_origins: vec![],
            secure_cookie: true,
            public_url: None,
        }
    }

    #[test]
    fn dsn_host_uses_public_url_when_set() {
        let mut cfg = base_cfg();
        cfg.public_url = Some("https://trapfall.example.com/".into());
        assert_eq!(cfg.dsn_host(), "https://trapfall.example.com");
    }

    #[test]
    fn dsn_host_falls_back_to_listen_addr() {
        let cfg = base_cfg();
        assert_eq!(cfg.dsn_host(), "0.0.0.0:9090");
    }

    #[test]
    fn dsn_host_ignores_empty_public_url() {
        let mut cfg = base_cfg();
        cfg.public_url = Some("   ".into());
        assert_eq!(cfg.dsn_host(), "0.0.0.0:9090");
    }

    #[test]
    fn cookie_secure_flag_toggles() {
        let mut cfg = base_cfg();
        assert_eq!(cfg.cookie_secure_flag(), "Secure");
        cfg.secure_cookie = false;
        assert_eq!(cfg.cookie_secure_flag(), "");
    }
}
