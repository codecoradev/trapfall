//! Retention task — hourly purge of old events.
//!
//! Deletes events older than the configured retention period.
//! Default: 90 days. Runs every hour.

use sqlx::SqlitePool;
use std::time::Duration;

/// Default retention period in days.
const DEFAULT_RETENTION_DAYS: i64 = 90;

/// Run the retention task loop.
pub async fn run_retention(pool: SqlitePool, retention_days: Option<i64>) {
    let days = retention_days.unwrap_or(DEFAULT_RETENTION_DAYS);
    let mut interval = tokio::time::interval(Duration::from_secs(3600));

    loop {
        interval.tick().await;
        if let Err(e) = purge_old_events(&pool, days).await {
            tracing::warn!("Retention purge failed: {e}");
        }
    }
}

/// Delete events older than `days` days, then clean up orphan issues.
async fn purge_old_events(pool: &SqlitePool, days: i64) -> Result<(), sqlx::Error> {
    // Delete old events
    let result = sqlx::query("DELETE FROM events WHERE received_at < datetime('now', ?)")
        .bind(format!("-{days} days"))
        .execute(pool)
        .await?;

    let deleted = result.rows_affected();

    // Single query: delete issues with zero remaining events (orphans after event purge)
    sqlx::query("DELETE FROM issues WHERE id NOT IN (SELECT DISTINCT issue_id FROM events)").execute(pool).await.ok(); // best-effort

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
