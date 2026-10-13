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
        if let Some(exc) = exceptions.values.last() {
            return fingerprint_exception(exc);
        }
    }

    if let Some(msg) = &event.message {
        let mut components = vec![normalize_message(msg)];
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
        if let Some(frame) = top_in_app_frame(&st.frames).or_else(|| st.frames.last()) {
            // Tagged so (function=None, file=Some) and (function=Some, file=None)
            // cannot hash identically.
            if let Some(func) = &frame.function {
                components.push(format!("function:{func}"));
            }
            if let Some(file) = &frame.filename {
                components.push(format!("file:{file}"));
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

/// Replace volatile tokens (UUIDs, hex ids, numbers) so messages that differ
/// only by identifiers group together, e.g. `user 42 not found`.
fn normalize_message(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.is_empty() {
            return;
        }
        let has_digit = token.chars().any(|c| c.is_ascii_digit());
        let hexish = token.len() >= 8 && token.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
        if has_digit && (hexish || token.chars().all(|c| c.is_ascii_digit() || c == '.')) {
            out.push_str("<n>");
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in msg.chars() {
        if c.is_alphanumeric() || c == '-' || c == '.' {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
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

    #[test]
    fn fingerprint_uses_last_exception_in_chain() {
        let mut e = make_exception_event("Inner", Some("f"), Some("a.rs"));
        let outer = make_exception_event("Outer", Some("f"), Some("a.rs"));
        e.exception.as_mut().unwrap().values.extend(outer.exception.unwrap().values);
        assert_eq!(derive_fingerprint(&e).components[0], "Outer");
    }

    #[test]
    fn fingerprint_falls_back_to_last_frame_when_none_in_app() {
        let mut a = make_exception_event("E", Some("f1"), Some("a.rs"));
        let mut b = make_exception_event("E", Some("f2"), Some("b.rs"));
        for ev in [&mut a, &mut b] {
            ev.exception.as_mut().unwrap().values[0].stacktrace.as_mut().unwrap().frames[0].in_app = false;
        }
        assert_ne!(derive_fingerprint(&a).hash, derive_fingerprint(&b).hash);
    }

    #[test]
    fn fingerprint_function_vs_file_not_ambiguous() {
        let a = make_exception_event("E", Some("x"), None);
        let b = make_exception_event("E", None, Some("x"));
        assert_ne!(derive_fingerprint(&a).hash, derive_fingerprint(&b).hash);
    }

    #[test]
    fn fingerprint_message_normalizes_ids_and_numbers() {
        let mk = |m: &str| {
            let mut e = make_exception_event("x", None, None);
            e.exception = None;
            e.message = Some(m.into());
            derive_fingerprint(&e)
        };
        assert_eq!(mk("user 42 not found").hash, mk("user 9001 not found").hash);
        assert_eq!(
            mk("job 3f2b8c1e-1111-2222-3333-444455556666 failed").hash,
            mk("job aaaaaaaa-1111-2222-3333-444455556666 failed").hash
        );
        assert_ne!(mk("user 42 not found").hash, mk("user 42 not allowed").hash);
    }
}
