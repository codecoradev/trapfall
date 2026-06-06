//! Sentry envelope parser.
//!
//! Handles both gzip-compressed and plaintext Sentry envelopes.
//! Each envelope is a multipart text body with JSON headers.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use trapfall_proto::Event;

/// Parse a Sentry envelope body into individual events.
///
/// Envelope format:
/// ```text
/// {"event_id":"...","sent_at":"..."}
/// {"type":"event","length":123}
/// {"event_id":"...","exception":{...}}
/// ```
///
/// Also supports gzip-wrapped envelopes.
pub fn parse_envelope(body: &[u8], content_encoding: Option<&str>) -> Result<Vec<Event>> {
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
    flate2::read::DeflateDecoder::new(data)
        .read_to_end(&mut buf)
        .context("deflate decompression failed")?;
    Ok(buf)
}

/// Parse the text envelope format.
///
/// The envelope is a series of lines:
/// Line 1: envelope header JSON (event_id, dsn, sent_at)
/// Line 2+: item header JSON (type, length) followed by item body (JSON)
fn parse_envelope_text(text: &str) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    let mut lines = text.lines();

    // First line is the envelope header — skip it
    let _header = match lines.next() {
        Some(h) => h,
        None => anyhow::bail!("empty envelope"),
    };

    // Process items: header line + body line pairs
    while let Some(item_header_line) = lines.next() {
        let item_header: serde_json::Value =
            serde_json::from_str(item_header_line).unwrap_or_default();

        let item_type = item_header.get("type").and_then(|v| v.as_str()).unwrap_or("");

        let body_line = match lines.next() {
            Some(l) => l,
            None => break,
        };

        if item_type == "event" {
            if let Ok(event) = serde_json::from_str::<Event>(body_line) {
                events.push(event);
            }
        }
        // Ignore other item types (attachments, sessions, etc.)
    }

    Ok(events)
}

/// Extract the Sentry auth key from the X-Sentry-Auth header.
///
/// Format: `Sentry sentry_key=abc123, sentry_version=7, ...`
pub fn extract_sentry_key(auth_header: &str) -> Option<String> {
    auth_header
        .split(',')
        .find_map(|part| {
            let trimmed = part.trim();
            trimmed.strip_prefix("sentry_key=").map(str::to_string)
        })
        .or_else(|| {
            // Also try without "Sentry " prefix
            auth_header.strip_prefix("Sentry ").and_then(extract_sentry_key)
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

        let events = parse_envelope_text(envelope).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "abc123");
        assert_eq!(events[0].message.as_deref(), Some("hello"));
    }

    #[test]
    fn parse_envelope_skips_non_event_items() {
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}
{"type":"session"}
{"sid":"session1"}
{"type":"event","length":50}
{"event_id":"abc123","message":"error","level":"error"}"#;

        let events = parse_envelope_text(envelope).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "abc123");
    }

    #[test]
    fn parse_empty_envelope() {
        let result = parse_envelope_text("");
        assert!(result.is_err());
    }

    #[test]
    fn parse_header_only_envelope() {
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}"#;
        let events = parse_envelope_text(envelope).unwrap();
        assert!(events.is_empty());
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
    fn parse_gzip_envelope() {
        use std::io::Write;
        let original = br#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}
{"type":"event","length":50}
{"event_id":"abc123","message":"gzipped","level":"error"}"#;

        // Compress with gzip
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        let events = parse_envelope(&compressed, Some("gzip")).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message.as_deref(), Some("gzipped"));
    }
}
