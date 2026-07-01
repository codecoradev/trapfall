//! Log timestamp formatting in the configured display timezone.
//!
//! Storage timestamps always remain UTC RFC3339 (see `Config::timezone`).
//! This module only customizes the timestamps that `tracing` emits to stdout
//! so operators reading `docker logs` see wall-clock local time.

use crate::config::parse_timezone;
use chrono::{DateTime, Utc};
use tracing_subscriber::fmt::time::FormatTime;

/// `tracing` timer that renders timestamps in the configured IANA timezone.
///
/// Falls back to UTC if the timezone cannot be determined at init time.
pub struct LocalTzTimer(chrono_tz::Tz);

impl LocalTzTimer {
    /// Build from the configured timezone. Parses `TRAPFALL_TIMEZONE` (or
    /// defaults to UTC) — used early in startup before `Config` is fully built.
    pub fn from_env() -> Self {
        let name = parse_timezone();
        Self(name.parse().unwrap_or(chrono_tz::UTC))
    }
}

impl FormatTime for LocalTzTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now: DateTime<Utc> = Utc::now();
        let local = now.with_timezone(&self.0);
        // Compact, sortable: 2026-07-01 10:39:29.163 +07:00
        w.write_str(&local.format("%Y-%m-%d %H:%M:%S%.3f %z").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iana_zone_validates() {
        // Direct unit test of the parser's parse step (env mutation is unsafe
        // in edition 2024 and racy). UTC on empty/invalid, IANA name passed
        // through on valid input.
        assert!("Asia/Jakarta".parse::<chrono_tz::Tz>().is_ok());
        assert!("Not/A_Zone".parse::<chrono_tz::Tz>().is_err());
        assert!("UTC".parse::<chrono_tz::Tz>().is_ok());
    }
    #[test]
    fn format_time_emits_offset() {
        let timer = LocalTzTimer(chrono_tz::Asia::Jakarta);
        let mut buf = String::new();
        let mut writer = tracing_subscriber::fmt::format::Writer::new(&mut buf);
        timer.format_time(&mut writer).unwrap();
        // Jakarta is UTC+7
        assert!(buf.contains("+0700"), "expected +0700 offset in: {buf}");
    }
}
