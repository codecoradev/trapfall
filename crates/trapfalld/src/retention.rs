//! Retention task — hourly purge of old events.
//!
//! Deletes events older than the configured retention period.
//! Default: 90 days. Runs every hour.

use crate::alert::AlertSignal;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use trapfall_core::Store;

/// Default retention period in days.
const DEFAULT_RETENTION_DAYS: i64 = 90;

/// Run the retention task loop.
///
/// Each tick also evaluates cron monitors for missed check-ins, signalling
/// `alert_tx` when one goes stale.
pub async fn run_retention(store: &Store, retention_days: Option<i64>, alert_tx: Option<UnboundedSender<AlertSignal>>) {
    let days = retention_days.unwrap_or(DEFAULT_RETENTION_DAYS);
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;
        if let Err(e) = purge_old_events(store, days).await {
            tracing::warn!("Retention purge failed: {e}");
        }
        if let Err(e) = crate::monitor::check_monitors(store, chrono::Utc::now(), alert_tx.as_ref()).await {
            tracing::warn!("Monitor staleness check failed: {e}");
        }
    }
}

/// Delete events older than `days` days, then clean up orphan issues and stale auth attempts.
async fn purge_old_events(store: &Store, days: i64) -> anyhow::Result<()> {
    let db = store.backend();
    let deleted = db.purge_old_events(days).await?;
    db.purge_orphan_issues(days).await?;
    // Check-in history only; monitor state rows are kept.
    db.purge_old_check_ins(days).await?;
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

    #[tokio::test]
    async fn purge_deletes_old_check_ins_but_keeps_monitors() {
        use trapfall_proto::{CheckIn, CheckInStatus};
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        let store = Store::new(backend);
        let project = store.create_project("cron", "Cron").await.unwrap();
        let ci = |id: &str, slug: &str| CheckIn {
            check_in_id: id.into(),
            monitor_slug: slug.into(),
            status: CheckInStatus::Ok,
            duration: None,
            timestamp: None,
            monitor_config: None,
        };
        let db = store.backend();
        let now = chrono::Utc::now().to_rfc3339();
        db.record_check_in(&project.id, &ci("old", "mixed"), "2020-01-01T00:00:00+00:00").await.unwrap();
        db.record_check_in(&project.id, &ci("new", "mixed"), &now).await.unwrap();
        db.record_check_in(&project.id, &ci("old-only", "stale"), "2020-01-01T00:00:00+00:00").await.unwrap();

        purge_old_events(&store, 90).await.unwrap();

        let monitors = db.list_monitors(&project.id).await.unwrap();
        assert_eq!(monitors.len(), 2, "monitor state rows must survive purge");
        let id = |slug: &str| monitors.iter().find(|m| m.slug == slug).unwrap().id.clone();
        let (rows, total) = db.list_check_ins(&project.id, &id("mixed"), 10, 0).await.unwrap().unwrap();
        assert_eq!((total, rows[0].check_in_id.as_str()), (1, "new"));
        assert_eq!(db.list_check_ins(&project.id, &id("stale"), 10, 0).await.unwrap().unwrap().1, 0);
    }
}
