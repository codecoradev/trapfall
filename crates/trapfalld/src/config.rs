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
    /// Display timezone (`TRAPFALL_TIMEZONE`, default `UTC`).
    ///
    /// IANA timezone name (e.g. `Asia/Jakarta`, `America/New_York`). Used
    /// **for display only** — log timestamps and the `/api/0/config` payload
    /// consumed by the dashboard. All persisted timestamps remain UTC; this
    /// never affects storage. Invalid values fall back to `UTC` with a warn.
    #[serde(default = "default_timezone")]
    pub timezone: String,
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

/// Default display timezone: UTC.
fn default_timezone() -> String {
    "UTC".to_string()
}

impl Config {
    /// Parsed IANA timezone for display (UTC on parse failure).
    ///
    /// All persisted timestamps stay UTC RFC3339; this is used only by log
    /// formatting and the public config endpoint.
    pub fn tz(&self) -> chrono_tz::Tz {
        self.timezone.parse().unwrap_or(chrono_tz::UTC)
    }

    /// Returns "Secure" if `secure_cookie` is true, empty string otherwise.
    pub fn cookie_secure_flag(&self) -> &'static str {
        if self.secure_cookie { "Secure" } else { "" }
    }

    /// Explicitly-configured public host to use when minting DSNs for
    /// new projects.
    ///
    /// Backed by `TRAPFALL_PUBLIC_URL` (legacy alias `TRAPFALL_DSN_HOST`).
    /// Returns `None` when unset — callers should then fall back to the
    /// per-request `Host` header (useful for dev where the user accesses the
    /// instance via `localhost:<port>`).
    ///
    /// The returned value is normalized to a bare host[:port] (scheme and
    /// trailing slash stripped) because `generate_dsn_with` already prepends
    /// `https://` to the host when composing a Sentry-compatible DSN.
    ///
    /// Note: we intentionally do **not** fall back to `listen_addr` here.
    /// `listen_addr` defaults to `0.0.0.0:9090`, which is not a usable DSN
    /// host (most network stacks reject `0.0.0.0` as a destination).
    pub fn dsn_host(&self) -> Option<String> {
        self.public_url.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(normalize_dsn_host)
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
            timezone: parse_timezone(),
        }
    }

    /// Returns the default data directory: `~/.codecora/trapfall/`.
    ///
    /// Override with `TRAPFALL_DATA_DIR` env var.
    pub fn default_data_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("TRAPFALL_DATA_DIR") {
            return PathBuf::from(dir);
        }
        // Use $HOME on Unix, fallback to current directory
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".codecora").join("trapfall")
    }

    /// Returns the default database path: `~/.codecora/trapfall/trapfall.db`.
    ///
    /// Only used when no explicit `TRAPFALL_DATABASE_URL` or `--db` is provided.
    pub fn default_db_path() -> String {
        let dir = Self::default_data_dir();
        dir.join("trapfall.db").to_string_lossy().to_string()
    }

    /// Ensure the data directory exists, creating it if necessary.
    pub fn ensure_data_dir(dir: &PathBuf) -> std::io::Result<()> {
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
            tracing::info!("Created data directory: {}", dir.display());
        }
        Ok(())
    }
}

/// Parse `TRAPFALL_CORS_ORIGINS` (comma-separated, trimmed, empty filtered).
fn parse_cors_origins() -> Vec<String> {
    std::env::var("TRAPFALL_CORS_ORIGINS")
        .ok()
        .map(|s| s.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect())
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

/// Parse `TRAPFALL_TIMEZONE` as an IANA timezone name (e.g. `Asia/Jakarta`).
/// Invalid/unset values fall back to `UTC`. Invalid values emit a warning.
pub fn parse_timezone() -> String {
    match std::env::var("TRAPFALL_TIMEZONE") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return "UTC".to_string();
            }
            if trimmed.parse::<chrono_tz::Tz>().is_ok() {
                trimmed.to_string()
            } else {
                tracing::warn!(
                    timezone = %trimmed,
                    "Invalid TRAPFALL_TIMEZONE — falling back to UTC. Use an IANA name like 'Asia/Jakarta'."
                );
                "UTC".to_string()
            }
        }
        Err(_) => "UTC".to_string(),
    }
}

/// Normalize a user-provided public-URL value into a bare `host[:port]`.
///
/// Accepts all of: `https://trapfall.example.com`,
/// `http://trapfall.example.com:3000`, `trapfall.example.com/`,
/// `trapfall.example.com:3000`. Returns just the authority component so it
/// can be composed into a Sentry-style DSN (`https://<key>@<host>/<id>`).
fn normalize_dsn_host(raw: &str) -> String {
    let stripped =
        raw.trim().strip_prefix("https://").or_else(|| raw.trim().strip_prefix("http://")).unwrap_or(raw.trim());
    // Drop any trailing path / slash — we only want the authority.
    let authority = stripped.split('/').next().unwrap_or(stripped);
    authority.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cfg() -> Config {
        Config {
            db_path: PathBuf::from("/tmp/test-trapfall.db"),
            listen_addr: "0.0.0.0:9090".into(),
            cors_origins: vec![],
            secure_cookie: true,
            public_url: None,
            timezone: "UTC".to_string(),
        }
    }

    #[test]
    fn dsn_host_strips_scheme_and_trailing_slash() {
        let mut cfg = base_cfg();
        cfg.public_url = Some("https://trapfall.example.com/".into());
        assert_eq!(cfg.dsn_host().as_deref(), Some("trapfall.example.com"));

        cfg.public_url = Some("http://errors.app.io:3000/path".into());
        assert_eq!(cfg.dsn_host().as_deref(), Some("errors.app.io:3000"));

        // Bare host (no scheme) also accepted.
        cfg.public_url = Some("trapfall.example.com".into());
        assert_eq!(cfg.dsn_host().as_deref(), Some("trapfall.example.com"));
    }

    #[test]
    fn dsn_host_none_when_unset() {
        let cfg = base_cfg();
        assert_eq!(cfg.dsn_host(), None);
    }

    #[test]
    fn dsn_host_none_when_empty_or_whitespace() {
        let mut cfg = base_cfg();
        cfg.public_url = Some("   ".into());
        assert_eq!(cfg.dsn_host(), None);
    }

    #[test]
    fn default_data_dir_uses_home() {
        // Just verify it doesn't panic and contains codecora/trapfall
        let dir = Config::default_data_dir();
        let path = dir.to_string_lossy();
        assert!(path.contains("codecora"), "path should contain codecora: {path}");
        assert!(path.contains("trapfall"), "path should contain trapfall: {path}");
    }

    #[test]
    fn default_db_path_contains_data_dir() {
        let path = Config::default_db_path();
        assert!(path.contains("codecora"), "path should contain codecora: {path}");
        assert!(path.ends_with("trapfall.db"), "path should end with trapfall.db: {path}");
    }

    #[test]
    fn ensure_data_dir_creates_directory() {
        let dir = std::env::temp_dir().join("trapfall-test-ensure-dir");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!dir.exists());
        Config::ensure_data_dir(&dir).unwrap();
        assert!(dir.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cookie_secure_flag_toggles() {
        let mut cfg = base_cfg();
        assert_eq!(cfg.cookie_secure_flag(), "Secure");
        cfg.secure_cookie = false;
        assert_eq!(cfg.cookie_secure_flag(), "");
    }
}
