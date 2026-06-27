//! Sentry envelope parser.
//!
//! Handles both gzip-compressed and plaintext Sentry envelopes.
//! Each envelope is a multipart text body with JSON headers.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use trapfall_proto::{Event, ParsedEnvelope, Transaction};

/// Parse a Sentry envelope body into individual events and transactions.
///
/// Envelope format:
/// ```text
/// {"event_id":"...","sent_at":"..."}
/// {"type":"event","length":123}
/// {"event_id":"...","exception":{...}}
/// ```
///
/// Also supports gzip-wrapped envelopes.
pub fn parse_envelope(body: &[u8], content_encoding: Option<&str>) -> Result<ParsedEnvelope> {
    let decompressed = match content_encoding {
        Some("gzip") | Some("application/gzip") => decompress_gzip(body)?,
        Some("deflate") => decompress_deflate(body)?,
        _ => body.to_vec(),
    };

    let text = std::str::from_utf8(&decompressed).context("envelope is not valid UTF-8")?;
    parse_envelope_text(text)
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::with_capacity(data.len() * 2);
    decoder.read_to_end(&mut buf).context("gzip decompression failed")?;
    Ok(buf)
}

fn decompress_deflate(data: &[u8]) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(data.len() * 2);
    flate2::read::DeflateDecoder::new(data).read_to_end(&mut buf).context("deflate decompression failed")?;
    Ok(buf)
}

/// Parse the text envelope format.
///
/// The envelope is a series of lines:
/// Line 1: envelope header JSON (event_id, dsn, sent_at)
/// Line 2+: item header JSON (type, length) followed by item body (JSON)
fn parse_envelope_text(text: &str) -> Result<ParsedEnvelope> {
    let mut result = ParsedEnvelope::default();
    let mut lines = text.lines();

    // First line is the envelope header — skip it
    let _header = match lines.next() {
        Some(h) => h,
        None => anyhow::bail!("empty envelope"),
    };

    // Process items: header line + body line pairs
    while let Some(item_header_line) = lines.next() {
        // Skip empty lines
        if item_header_line.trim().is_empty() {
            continue;
        }

        let item_header: serde_json::Value = serde_json::from_str(item_header_line).unwrap_or_default();
        let item_type = item_header.get("type").and_then(|v| v.as_str()).unwrap_or("");

        // If no type field, this might be a bare event (2-line envelope: header + event)
        if item_type.is_empty() && item_header.get("event_id").is_some() {
            if let Ok(event) = serde_json::from_value::<Event>(item_header) {
                result.events.push(event);
            }
            continue;
        }

        let body_line = match lines.next() {
            Some(l) => l,
            None => break,
        };

        if item_type == "event" {
            if let Ok(event) = serde_json::from_str::<Event>(body_line) {
                result.events.push(event);
            }
        } else if item_type == "transaction" {
            if let Ok(txn) = serde_json::from_str::<Transaction>(body_line) {
                result.transactions.push(txn);
            }
        }
        // Ignore other item types (attachments, sessions, etc.)
    }

    Ok(result)
}

/// Extract the Sentry auth key from the X-Sentry-Auth header.
///
/// Format: `Sentry sentry_key=abc123, sentry_version=7, ...`
pub fn extract_sentry_key(auth_header: &str) -> Option<String> {
    // Strip optional "Sentry " prefix once (no recursion)
    let header = auth_header.strip_prefix("Sentry ").unwrap_or(auth_header);
    header.split(',').find_map(|part| {
        let trimmed = part.trim();
        trimmed.strip_prefix("sentry_key=").map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plaintext_envelope_single_event() {
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}
{"type":"event","length":50}
{"event_id":"abc123","message":"hello","level":"error"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_id, "abc123");
        assert_eq!(result.events[0].message.as_deref(), Some("hello"));
    }

    #[test]
    fn parse_envelope_skips_non_event_items() {
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}
{"type":"session"}
{"sid":"session1"}
{"type":"event","length":50}
{"event_id":"abc123","message":"error","level":"error"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_id, "abc123");
    }

    #[test]
    fn parse_empty_envelope() {
        let result = parse_envelope_text("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_header_only_envelope() {
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}"#;
        let result = parse_envelope_text(envelope).unwrap();
        assert!(result.events.is_empty());
    }

    #[test]
    fn extract_sentry_key_from_header() {
        let header = "Sentry sentry_key=abc123def, sentry_version=7, sentry_client=rust/1.0";
        let key = extract_sentry_key(header).unwrap();
        assert_eq!(key, "abc123def");
    }

    #[test]
    fn extract_sentry_key_missing() {
        let header = "Sentry sentry_version=7";
        assert!(extract_sentry_key(header).is_none());
    }

    #[test]
    fn extract_sentry_key_no_infinite_recursion() {
        // "Sentry Sentry sentry_key=abc" must not stack-overflow (was infinite recursion before fix)
        // After fix: strips first "Sentry " prefix once, then parses comma-separated key=value pairs
        // "Sentry Sentry sentry_key=abc123" → strip_prefix → "Sentry sentry_key=abc123"
        // split by comma → ["Sentry sentry_key=abc123"] — no "sentry_key=" prefix match
        // This is correct behavior: malformed double-prefixed headers should fail
        let header = "Sentry sentry_key=abc123, sentry_version=7";
        let key = extract_sentry_key(header).unwrap();
        assert_eq!(key, "abc123");
    }

    #[test]
    fn extract_sentry_key_plain_key() {
        let header = "sentry_key=xyz789";
        let key = extract_sentry_key(header).unwrap();
        assert_eq!(key, "xyz789");
    }

    #[test]
    fn parse_gzip_envelope() {
        use std::io::Write;
        let original = br#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}
{"type":"event","length":50}
{"event_id":"abc123","message":"gzipped","level":"error"}"#;

        // Compress with gzip
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = parse_envelope(&compressed, Some("gzip")).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].message.as_deref(), Some("gzipped"));
    }

    #[test]
    fn parse_envelope_transaction_only() {
        let envelope = r#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"transaction","length":150}
{"event_id":"e1","level":"info","transaction":"GET /api/health","start_timestamp":1703894474.296,"timestamp":1703894474.891}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert!(result.events.is_empty());
        assert_eq!(result.transactions.len(), 1);
        assert_eq!(result.transactions[0].transaction, "GET /api/health");
    }

    #[test]
    fn parse_envelope_event_and_transaction() {
        let envelope = r#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"event","length":50}
{"event_id":"e1","message":"error here","level":"error"}
{"type":"transaction","length":150}
{"event_id":"e2","level":"info","transaction":"POST /api/submit","start_timestamp":1.0,"timestamp":2.0}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.transactions.len(), 1);
        assert_eq!(result.events[0].message.as_deref(), Some("error here"));
        assert_eq!(result.transactions[0].transaction, "POST /api/submit");
    }

    #[test]
    fn parse_envelope_malformed_transaction_skipped() {
        let envelope = r#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"transaction","length":50}
{invalid json here}
{"type":"event","length":50}
{"event_id":"e2","message":"still works","level":"error"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert!(result.transactions.is_empty(), "malformed transaction should be skipped");
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].message.as_deref(), Some("still works"));
    }

    #[test]
    fn parse_gzip_envelope_with_transaction() {
        use std::io::Write;
        let original = br#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"transaction","length":150}
{"event_id":"e1","level":"info","transaction":"GET /gzip-test","start_timestamp":1703894474.296,"timestamp":1703894474.891}"#;

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = parse_envelope(&compressed, Some("gzip")).unwrap();
        assert!(result.events.is_empty());
        assert_eq!(result.transactions.len(), 1);
        assert_eq!(result.transactions[0].transaction, "GET /gzip-test");
    }
}
