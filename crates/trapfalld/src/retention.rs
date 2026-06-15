//! Retention task — hourly purge of old events.
//!
//! Deletes events older than the configured retention period.
//! Default: 90 days. Runs every hour.

use std::time::Duration;
use trapfall_core::Store;

/// Default retention period in days.
const DEFAULT_RETENTION_DAYS: i64 = 90;

/// Run the retention task loop.
pub async fn run_retention(store: &Store, retention_days: Option<i64>) {
    let days = retention_days.unwrap_or(DEFAULT_RETENTION_DAYS);
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;
        if let Err(e) = purge_old_events(store, days).await {
            tracing::warn!("Retention purge failed: {e}");
        }
    }
}

/// Delete events older than `days` days, then clean up orphan issues and stale auth attempts.
async fn purge_old_events(store: &Store, days: i64) -> anyhow::Result<()> {
    let db = store.backend();
    let deleted = db.purge_old_events(days).await?;
    db.purge_orphan_issues().await?;
    db.purge_stale_auth_attempts().await?;

    if deleted > 0 {
        tracing::info!("Retention purge: deleted {deleted} events older than {days} days");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_retention_is_90_days() {
        assert_eq!(DEFAULT_RETENTION_DAYS, 90);
    }
}
