//! Digest task — single-writer that batches events into SQLite.
//!
//! Receives IngestEvents via mpsc channel, groups them by fingerprint,
//! and commits to the database periodically or when the buffer is full.

use sqlx::SqlitePool;
use tokio::sync::mpsc;
use trapfall_core::Store;
use trapfall_proto::IngestEvent;

/// Maximum events to buffer before forcing a flush.
const FLUSH_THRESHOLD: usize = 50;

/// Maximum time (ms) between flushes.
const FLUSH_INTERVAL_MS: u64 = 2000;

pub struct DigestTask {
    pool: SqlitePool,
    rx: mpsc::Receiver<IngestEvent>,
}

impl DigestTask {
    pub fn new(pool: SqlitePool, rx: mpsc::Receiver<IngestEvent>) -> Self {
        Self { pool, rx }
    }

    pub async fn run(mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let store = Store::new(self.pool.clone());
        let mut buffer: Vec<IngestEvent> = Vec::with_capacity(FLUSH_THRESHOLD);
        let mut interval =
            tokio::time::interval(std::time::Duration::from_millis(FLUSH_INTERVAL_MS));

        loop {
            tokio::select! {
                Some(event) = self.rx.recv() => {
                    buffer.push(event);
                    if buffer.len() >= FLUSH_THRESHOLD {
                        Self::flush(&store, &mut buffer).await;
                    }
                }
                _ = interval.tick() => {
                    if !buffer.is_empty() {
                        Self::flush(&store, &mut buffer).await;
                    }
                }
                else => {
                    // Channel closed — drain remaining
                    if !buffer.is_empty() {
                        Self::flush(&store, &mut buffer).await;
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    async fn flush(store: &Store, buffer: &mut Vec<IngestEvent>) {
        let events = std::mem::take(buffer);
        let count = events.len();

        for ev in events {
            if let Err(e) = Self::process_event(store, ev).await {
                tracing::warn!("Failed to process event: {e}");
            }
        }

        tracing::trace!("Flushed {count} events");
    }

    async fn process_event(store: &Store, ev: IngestEvent) -> anyhow::Result<()> {
        // Derive title from event
        let title = derive_title(&ev);

        // Derive culprit from exception stacktrace
        let culprit = ev.event.exception.as_ref().and_then(|ex| {
            ex.values.first().and_then(|exc| {
                exc.stacktrace.as_ref().and_then(|st| {
                    st.frames.last().map(|f| {
                        format!(
                            "{}:{}:{}",
                            f.filename.as_deref().unwrap_or("<unknown>"),
                            f.lineno.unwrap_or(0),
                            f.function.as_deref().unwrap_or("<anonymous>")
                        )
                    })
                })
            })
        });

        let level = ev.event.level;

        // Upsert issue (creates or increments count)
        let issue = store
            .upsert_issue(&ev.project_id, &ev.fingerprint.hash, &title, culprit.as_deref(), level)
            .await?;

        // Store raw event JSON
        let event_json = serde_json::to_string(&ev.event).unwrap_or_default();
        store.insert_event(&issue.id, &ev.project_id, &event_json).await?;

        Ok(())
    }
}

fn derive_title(ev: &IngestEvent) -> String {
    // Exception type is the best title
    if let Some(exceptions) = &ev.event.exception {
        if let Some(exc) = exceptions.values.first() {
            if let Some(value) = &exc.value {
                return format!("{}: {}", exc.exc_type, value);
            }
            return exc.exc_type.clone();
        }
    }

    // Fallback to message
    ev.event.message.clone().unwrap_or_else(|| "Unknown Error".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_title_from_exception() {
        use trapfall_proto::*;
        let ev = IngestEvent {
            project_id: "p1".into(),
            fingerprint: Fingerprint::new("hash".into(), vec![]),
            event: Event {
                event_id: "e1".into(),
                level: Level::Error,
                platform: None,
                release: None,
                environment: None,
                server_name: None,
                breadcrumbs: Breadcrumbs::default(),
                exception: Some(ExceptionValues {
                    values: vec![Exception {
                        exc_type: "TypeError".into(),
                        value: Some("cannot read property 'x'".into()),
                        stacktrace: None,
                    }],
                }),
                message: None,
                tags: serde_json::Value::Null,
                extra: serde_json::Value::Null,
                contexts: serde_json::Value::Null,
                timestamp: None,
            },
            received_at: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(derive_title(&ev), "TypeError: cannot read property 'x'");
    }

    #[test]
    fn derive_title_from_exception_no_value() {
        use trapfall_proto::*;
        let ev = IngestEvent {
            project_id: "p1".into(),
            fingerprint: Fingerprint::new("hash".into(), vec![]),
            event: Event {
                event_id: "e1".into(),
                level: Level::Error,
                platform: None,
                release: None,
                environment: None,
                server_name: None,
                breadcrumbs: Breadcrumbs::default(),
                exception: Some(ExceptionValues {
                    values: vec![Exception {
                        exc_type: "Panic".into(),
                        value: None,
                        stacktrace: None,
                    }],
                }),
                message: Some("fallback msg".into()),
                tags: serde_json::Value::Null,
                extra: serde_json::Value::Null,
                contexts: serde_json::Value::Null,
                timestamp: None,
            },
            received_at: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(derive_title(&ev), "Panic");
    }

    #[test]
    fn derive_title_fallback_message() {
        use trapfall_proto::*;
        let ev = IngestEvent {
            project_id: "p1".into(),
            fingerprint: Fingerprint::new("hash".into(), vec![]),
            event: Event {
                event_id: "e1".into(),
                level: Level::Error,
                platform: None,
                release: None,
                environment: None,
                server_name: None,
                breadcrumbs: Breadcrumbs::default(),
                exception: None,
                message: Some("Database connection failed".into()),
                tags: serde_json::Value::Null,
                extra: serde_json::Value::Null,
                contexts: serde_json::Value::Null,
                timestamp: None,
            },
            received_at: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(derive_title(&ev), "Database connection failed");
    }

    #[test]
    fn derive_title_fallback_unknown() {
        use trapfall_proto::*;
        let ev = IngestEvent {
            project_id: "p1".into(),
            fingerprint: Fingerprint::new("hash".into(), vec![]),
            event: Event {
                event_id: "e1".into(),
                level: Level::Error,
                platform: None,
                release: None,
                environment: None,
                server_name: None,
                breadcrumbs: Breadcrumbs::default(),
                exception: None,
                message: None,
                tags: serde_json::Value::Null,
                extra: serde_json::Value::Null,
                contexts: serde_json::Value::Null,
                timestamp: None,
            },
            received_at: "2026-01-01T00:00:00Z".into(),
        };
        assert_eq!(derive_title(&ev), "Unknown Error");
    }
}
