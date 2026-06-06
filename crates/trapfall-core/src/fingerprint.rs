//! Deterministic issue fingerprinting using blake3.

use blake3::Hasher;
use trapfall_proto::{Event, Exception, Fingerprint, StackFrame};

/// Derive a deterministic fingerprint from an event.
///
/// Strategy:
/// 1. If exception exists: hash (exception_type + top in-app frame function + filename)
/// 2. If message only: hash (message)
/// 3. Fallback: hash (event_id — should never happen)
pub fn derive_fingerprint(event: &Event) -> Fingerprint {
    if let Some(exceptions) = &event.exception {
        if let Some(exc) = exceptions.values.first() {
            return fingerprint_exception(exc);
        }
    }

    if let Some(msg) = &event.message {
        let mut components = vec![msg.clone()];
        if let Some(env) = &event.environment {
            components.push(env.clone());
        }
        return hash_components(&components);
    }

    // Fallback: event_id (unique → new issue every time)
    Fingerprint::new(blake3_hash(&[&event.event_id]), vec![format!("event_id:{}", event.event_id)])
}

fn fingerprint_exception(exc: &Exception) -> Fingerprint {
    let mut components = vec![exc.exc_type.clone()];

    if let Some(st) = &exc.stacktrace {
        if let Some(frame) = top_in_app_frame(&st.frames) {
            if let Some(func) = &frame.function {
                components.push(func.clone());
            }
            if let Some(file) = &frame.filename {
                components.push(file.clone());
            }
        }
    }

    hash_components(&components)
}

/// Find the topmost in-app frame (closest to the throw point).
fn top_in_app_frame(frames: &[StackFrame]) -> Option<&StackFrame> {
    // Stack frames are typically ordered bottom-up (oldest first).
    // We want the last in-app frame (closest to the error).
    frames.iter().rev().find(|f| f.in_app)
}

fn hash_components(components: &[String]) -> Fingerprint {
    let refs: Vec<&str> = components.iter().map(|s| s.as_str()).collect();
    let hash = blake3_hash(&refs);
    Fingerprint::new(hash, components.to_vec())
}

fn blake3_hash(parts: &[&str]) -> String {
    let mut hasher = Hasher::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0"); // separator
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use trapfall_proto::*;

    fn make_exception_event(exc_type: &str, function: Option<&str>, filename: Option<&str>) -> Event {
        Event {
            event_id: "test-id".into(),
            level: Level::Error,
            platform: Some("rust".into()),
            release: None,
            environment: None,
            server_name: None,
            breadcrumbs: Breadcrumbs::default(),
            exception: Some(ExceptionValues {
                values: vec![Exception {
                    exc_type: exc_type.into(),
                    value: Some("test error".into()),
                    stacktrace: Some(Stacktrace {
                        frames: vec![StackFrame {
                            filename: filename.map(|s| s.into()),
                            function: function.map(|s| s.into()),
                            module: None,
                            lineno: Some(42),
                            colno: None,
                            abs_path: None,
                            in_app: true,
                            pre_context: None,
                            context_line: None,
                            post_context: None,
                        }],
                    }),
                }],
            }),
            message: None,
            tags: serde_json::Value::Null,
            extra: serde_json::Value::Null,
            contexts: serde_json::Value::Null,
            timestamp: None,
        }
    }

    #[test]
    fn fingerprint_deterministic() {
        let event = make_exception_event("TypeError", Some("main"), Some("app.rs"));
        let fp1 = derive_fingerprint(&event);
        let fp2 = derive_fingerprint(&event);
        assert_eq!(fp1.hash, fp2.hash);
        assert_eq!(fp1.components, fp2.components);
    }

    #[test]
    fn fingerprint_different_exceptions() {
        let e1 = make_exception_event("TypeError", Some("main"), Some("app.rs"));
        let e2 = make_exception_event("ValueError", Some("main"), Some("app.rs"));
        assert_ne!(derive_fingerprint(&e1).hash, derive_fingerprint(&e2).hash);
    }

    #[test]
    fn fingerprint_same_exception_different_location() {
        let e1 = make_exception_event("TypeError", Some("func_a"), Some("a.rs"));
        let e2 = make_exception_event("TypeError", Some("func_b"), Some("b.rs"));
        assert_ne!(derive_fingerprint(&e1).hash, derive_fingerprint(&e2).hash);
    }

    #[test]
    fn fingerprint_message_fallback() {
        let event = Event {
            event_id: "id1".into(),
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
        };
        let fp = derive_fingerprint(&event);
        assert!(fp.hash.len() == 64); // blake3 hex
        assert_eq!(fp.components[0], "Database connection failed");
    }

    #[test]
    fn fingerprint_empty_stacktrace_uses_type() {
        let event = Event {
            event_id: "id2".into(),
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
                    stacktrace: Some(Stacktrace { frames: vec![] }),
                }],
            }),
            message: None,
            tags: serde_json::Value::Null,
            extra: serde_json::Value::Null,
            contexts: serde_json::Value::Null,
            timestamp: None,
        };
        let fp = derive_fingerprint(&event);
        assert!(fp.hash.len() == 64);
        assert_eq!(fp.components, vec!["Panic"]);
    }

    #[test]
    fn fingerprint_fallback_event_id() {
        let event = Event {
            event_id: "unique-id".into(),
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
        };
        let fp = derive_fingerprint(&event);
        assert!(fp.hash.len() == 64);
    }
}
