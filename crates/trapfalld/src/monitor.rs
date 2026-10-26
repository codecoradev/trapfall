//! Sentry Crons staleness evaluation — run from the hourly retention tick.
//!
//! A monitor is *missed* when it has a crontab schedule, its last status is not
//! already `missed`, and `now` is past the next expected check-in (after
//! `last_seen`) plus the check-in margin (default 60 minutes).

use crate::alert::AlertSignal;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use tokio::sync::mpsc::UnboundedSender;
use trapfall_core::Store;
use trapfall_db::common::MonitorRow;

/// Margin applied when a monitor does not configure `checkin_margin`.
const DEFAULT_MARGIN_MINUTES: i64 = 60;

/// Search horizon for the next matching minute.
const MAX_LOOKAHEAD_MINUTES: i64 = 366 * 24 * 60;

/// One cron field: does `value` match? Supports `*`, `a`, `a-b`, `a/n`, `a-b/n`, `*/n` and `,` lists.
/// `None` if the field is malformed or out of `min..=max`.
fn field_matches(field: &str, value: u32, min: u32, max: u32) -> Option<bool> {
    let mut hit = false;
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().ok().filter(|s| *s > 0)?),
            None => (part, 1),
        };
        let (lo, hi) = match range {
            "*" => (min, max),
            r => match r.split_once('-') {
                Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
                None => {
                    let a: u32 = r.parse().ok()?;
                    // `a/n` runs from a to max; plain `a` is just a.
                    (a, if part.contains('/') { max } else { a })
                }
            },
        };
        if lo < min || hi > max || lo > hi {
            return None;
        }
        hit |= value >= lo && value <= hi && (value - lo) % step == 0;
    }
    Some(hit)
}

/// First minute strictly after `after` matching the 5-field UTC crontab `cron`.
/// `None` for an invalid expression or no match within a year.
// ponytail: UTC only (monitor timezone ignored), minute-by-minute scan, no
// names (JAN/MON) or `L`/`#`; swap in a cron crate if those matter.
pub fn next_run(cron: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let f: Vec<&str> = cron.split_whitespace().collect();
    let [min, hour, dom, mon, dow] = f[..] else { return None };
    // Validate every field once so a bad expression is `None`, not "never".
    for (fld, lo, hi) in [(min, 0, 59), (hour, 0, 23), (dom, 1, 31), (mon, 1, 12), (dow, 0, 7)] {
        field_matches(fld, lo, lo, hi)?;
    }
    let mut t = after.with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
    for _ in 0..MAX_LOOKAHEAD_MINUTES {
        let wd = t.weekday().num_days_from_sunday();
        // Day-of-week accepts 0 or 7 for Sunday.
        let dow_ok = field_matches(dow, wd, 0, 7)? || (wd == 0 && field_matches(dow, 7, 0, 7)?);
        let dom_ok = field_matches(dom, t.day(), 1, 31)?;
        // Vixie cron: when both day fields are restricted, either may match.
        let day_ok = if dom != "*" && dow != "*" { dom_ok || dow_ok } else { dom_ok && dow_ok };
        if day_ok
            && field_matches(min, t.minute(), 0, 59)?
            && field_matches(hour, t.hour(), 0, 23)?
            && field_matches(mon, t.month(), 1, 12)?
        {
            return Some(t);
        }
        t += Duration::minutes(1);
    }
    None
}

/// Has `m` missed its expected check-in as of `now`?
pub fn is_missed(m: &MonitorRow, now: DateTime<Utc>) -> bool {
    if m.last_status == "missed" {
        return false;
    }
    let Some(cron) = m.schedule.as_deref() else { return false };
    let Ok(last_seen) = DateTime::parse_from_rfc3339(&m.last_seen) else { return false };
    let Some(next) = next_run(cron, last_seen.with_timezone(&Utc)) else { return false };
    now > next + Duration::minutes(m.checkin_margin.unwrap_or(DEFAULT_MARGIN_MINUTES))
}

/// Mark stale monitors `missed` and emit one alert signal each. Returns how many were missed.
pub async fn check_monitors(
    store: &Store,
    now: DateTime<Utc>,
    alert_tx: Option<&UnboundedSender<AlertSignal>>,
) -> anyhow::Result<usize> {
    let mut missed = 0;
    for m in store.backend().list_schedulable_monitors().await? {
        if !is_missed(&m, now) {
            continue;
        }
        store.backend().mark_monitor_missed(&m.id).await?;
        missed += 1;
        tracing::warn!(monitor = %m.slug, project = %m.project_id, "Monitor missed check-in");
        if let Some(tx) = alert_tx {
            let _ = tx.send(AlertSignal::MonitorMissed {
                project_id: m.project_id,
                monitor_slug: m.slug,
                last_status: m.last_status,
                last_seen: m.last_seen,
                schedule: m.schedule,
            });
        }
    }
    Ok(missed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn monitor(schedule: Option<&str>, margin: Option<i64>, status: &str) -> MonitorRow {
        MonitorRow {
            id: "m1".into(),
            project_id: "p1".into(),
            slug: "nightly".into(),
            schedule: schedule.map(Into::into),
            checkin_margin: margin,
            last_seen: "2026-01-01T03:00:05Z".into(),
            last_status: status.into(),
            created_at: "2026-01-01T03:00:05Z".into(),
        }
    }

    #[test]
    fn next_run_basic() {
        assert_eq!(next_run("0 3 * * *", at("2026-01-01T03:00:05Z")), Some(at("2026-01-02T03:00:00Z")));
        assert_eq!(next_run("*/15 * * * *", at("2026-01-01T10:07:00Z")), Some(at("2026-01-01T10:15:00Z")));
        assert_eq!(next_run("0 0 1 * *", at("2026-01-15T00:00:00Z")), Some(at("2026-02-01T00:00:00Z")));
        // Monday 09:30 — 2026-01-01 is a Thursday.
        assert_eq!(next_run("30 9 * * 1", at("2026-01-01T00:00:00Z")), Some(at("2026-01-05T09:30:00Z")));
        assert_eq!(next_run("not a cron", at("2026-01-01T00:00:00Z")), None);
        assert_eq!(next_run("61 * * * *", at("2026-01-01T00:00:00Z")), None);
    }

    #[test]
    fn staleness_with_explicit_margin() {
        let m = monitor(Some("0 3 * * *"), Some(5), "ok");
        assert!(!is_missed(&m, at("2026-01-02T03:04:00Z")));
        assert!(!is_missed(&m, at("2026-01-02T03:05:00Z")));
        assert!(is_missed(&m, at("2026-01-02T03:06:00Z")));
    }

    #[test]
    fn staleness_default_margin_is_60_minutes() {
        let m = monitor(Some("0 3 * * *"), None, "in_progress");
        assert!(!is_missed(&m, at("2026-01-02T04:00:00Z")));
        assert!(is_missed(&m, at("2026-01-02T04:01:00Z")));
    }

    #[test]
    fn staleness_skips_missed_unscheduled_and_invalid() {
        let late = at("2026-03-01T00:00:00Z");
        assert!(!is_missed(&monitor(Some("0 3 * * *"), None, "missed"), late));
        assert!(!is_missed(&monitor(None, None, "ok"), late));
        assert!(!is_missed(&monitor(Some("garbage"), None, "ok"), late));
    }

    #[tokio::test]
    async fn check_monitors_marks_missed_once_and_signals() {
        use trapfall_proto::{CheckIn, CheckInStatus, MonitorConfig, MonitorSchedule};
        let backend = trapfall_db::open_database("sqlite::memory:").await.unwrap();
        trapfall_db::run_sqlite_migrations(backend.sqlite_pool().unwrap()).await.unwrap();
        let store = Store::new(backend);
        let project = store.create_project("cron", "Cron").await.unwrap();
        let ci = CheckIn {
            check_in_id: "c1".into(),
            monitor_slug: "nightly".into(),
            status: CheckInStatus::Ok,
            duration: None,
            timestamp: None,
            monitor_config: Some(MonitorConfig {
                schedule: Some(MonitorSchedule { kind: "crontab".into(), value: "0 3 * * *".into() }),
                checkin_margin: Some(5),
                ..Default::default()
            }),
        };
        store.backend().record_check_in(&project.id, &ci, "2026-01-01T03:00:05Z").await.unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        assert_eq!(check_monitors(&store, at("2026-01-02T03:04:00Z"), Some(&tx)).await.unwrap(), 0);
        assert_eq!(check_monitors(&store, at("2026-01-02T03:10:00Z"), Some(&tx)).await.unwrap(), 1);
        // Already missed: no second alert.
        assert_eq!(check_monitors(&store, at("2026-01-02T05:00:00Z"), Some(&tx)).await.unwrap(), 0);
        match rx.try_recv().unwrap() {
            AlertSignal::MonitorMissed { monitor_slug, last_status, .. } => {
                assert_eq!(monitor_slug, "nightly");
                assert_eq!(last_status, "ok");
            }
            _ => panic!("expected MonitorMissed"),
        }
        assert!(rx.try_recv().is_err());
        let monitors = store.backend().list_monitors(&project.id).await.unwrap();
        assert_eq!(monitors[0].last_status, "missed");

        // A new check-in clears the missed state.
        let ok = CheckIn { monitor_config: None, ..ci };
        store.backend().record_check_in(&project.id, &ok, "2026-01-02T03:20:00Z").await.unwrap();
        let monitors = store.backend().list_monitors(&project.id).await.unwrap();
        assert_eq!(monitors[0].last_status, "ok");
        assert_eq!(monitors[0].schedule.as_deref(), Some("0 3 * * *"));
        let (rows, total) = store.backend().list_check_ins(&project.id, &monitors[0].id, 10, 0).await.unwrap().unwrap();
        assert_eq!((rows.len(), total), (2, 2));
    }
}
