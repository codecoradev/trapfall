//! Sentry envelope parser.
//!
//! Handles both gzip-compressed and plaintext Sentry envelopes.
//! Each envelope is a multipart body with JSON headers and either text or
//! binary payloads. Binary payloads (attachments) are length-delimited and
//! may contain arbitrary bytes including `\n`.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::io::Read;
use trapfall_proto::{Attachment, Event, ParsedEnvelope, SessionAggregates, SessionUpdate, Transaction};

// ── Limits ────────────────────────────────────────────────────────────────

/// Resource limits applied while decoding an envelope.
///
/// `max_ingest_body_bytes` only bounds the *compressed* request body, so these
/// bound what a compressed payload may expand to and what it may contain.
#[derive(Debug, Clone)]
pub struct EnvelopeLimits {
    /// Maximum size of the decompressed envelope.
    pub max_decompressed_bytes: usize,
    /// Maximum number of attachment items per envelope.
    pub max_attachments: usize,
    /// Maximum size of a single attachment.
    pub max_attachment_bytes: usize,
}

impl Default for EnvelopeLimits {
    fn default() -> Self {
        Self { max_decompressed_bytes: 32 * 1024 * 1024, max_attachments: 16, max_attachment_bytes: 20 * 1024 * 1024 }
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────

/// Parsed item header from a single envelope line.
#[derive(Debug, Default)]
struct ItemHeader {
    item_type: String,
    length: Option<usize>,
    filename: Option<String>,
    content_type: Option<String>,
    attachment_type: Option<String>,
}

/// Find the position of the next `\n` byte starting from `start`.
fn find_newline(data: &[u8], start: usize) -> Option<usize> {
    data[start..].iter().position(|&b| b == b'\n').map(|pos| start + pos)
}

/// Parse an item header JSON from raw bytes.
///
/// If the line is not valid JSON, returns a default (empty) header — callers
/// should skip items with no type.
fn parse_item_header(line: &[u8]) -> ItemHeader {
    let text = match std::str::from_utf8(line) {
        Ok(t) => t,
        Err(_) => return ItemHeader::default(),
    };
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return ItemHeader::default(),
    };
    ItemHeader {
        item_type: value.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        length: value.get("length").and_then(|v| v.as_u64()).map(|n| n as usize),
        filename: value.get("filename").and_then(|v| v.as_str()).map(str::to_string),
        content_type: value.get("content_type").and_then(|v| v.as_str()).map(str::to_string),
        attachment_type: value.get("attachment_type").and_then(|v| v.as_str()).map(str::to_string),
    }
}

// ── Binary-safe parser ────────────────────────────────────────────────────

/// Parse a Sentry envelope from raw bytes (binary-safe).
///
/// This parser works on `&[u8]` rather than `&str` so that binary attachment
/// payloads are not corrupted by newline splitting. Text items (event,
/// transaction, session, sessions) are still parsed as UTF-8 JSON after
/// newline-delimited extraction.
fn parse_envelope_binary(data: &[u8], limits: &EnvelopeLimits) -> Result<ParsedEnvelope> {
    let mut result = ParsedEnvelope::default();
    let mut pos = 0;

    // --- Envelope header line (first \n) ---
    let header_end = find_newline(data, pos).context("empty envelope: no newline found for envelope header")?;
    // Skip the envelope header (we don't need it for parsing items).
    pos = header_end + 1;

    // --- Item loop ---
    while pos < data.len() {
        // Find the item header line (text ending with \n).
        let header_end = match find_newline(data, pos) {
            Some(end) => end,
            None => break, // No more complete items.
        };
        let header_line = &data[pos..header_end];
        let hdr = parse_item_header(header_line);
        pos = header_end + 1; // Skip past the \n

        // Skip items with no type.
        if hdr.item_type.is_empty() {
            // Fallback: might be a bare event JSON (2-line envelope).
            if let Ok(text) = std::str::from_utf8(header_line) {
                if let Ok(event) = serde_json::from_str::<Event>(text) {
                    result.events.push(event);
                }
            }
            continue;
        }

        if hdr.item_type == "attachment" {
            // ── Binary attachment ──
            let length = hdr.length.context("attachment item missing required 'length' field")?;

            if length > limits.max_attachment_bytes {
                anyhow::bail!("attachment of {length} bytes exceeds limit of {} bytes", limits.max_attachment_bytes);
            }
            if result.attachments.len() >= limits.max_attachments {
                anyhow::bail!("too many attachments (limit {})", limits.max_attachments);
            }

            // Overflow-safe bounds check.
            let end = match pos.checked_add(length) {
                Some(end) if end <= data.len() => end,
                _ => anyhow::bail!(
                    "truncated attachment: declared {} bytes but only {} remaining",
                    length,
                    data.len() - pos
                ),
            };

            let attachment_data = data[pos..end].to_vec();
            pos = end;

            // Skip one trailing \n if present (envelope items are \n-terminated).
            if pos < data.len() && data[pos] == b'\n' {
                pos += 1;
            }

            result.attachments.push(Attachment {
                filename: hdr.filename.unwrap_or_default(),
                content_type: hdr.content_type,
                attachment_type: hdr.attachment_type,
                data: attachment_data,
            });
        } else {
            // ── Text item (event, transaction, session, sessions) ──
            // Honour a declared length when it fits inside the data (the body
            // may legally contain newlines); (it must end at a newline or EOF, which
            // tolerates SDKs that miscount); otherwise delimit by newline.
            let declared_end = hdr
                .length
                .and_then(|len| pos.checked_add(len))
                .filter(|&end| end == data.len() || data.get(end) == Some(&b'\n'));
            let (body_end, next_pos) = match declared_end {
                Some(end) => {
                    // Skip one trailing \n if present.
                    let next = if data.get(end) == Some(&b'\n') { end + 1 } else { end };
                    (end, next)
                }
                None => match find_newline(data, pos) {
                    Some(end) => (end, end + 1),
                    // Last item with no trailing newline — use remaining bytes.
                    None => (data.len(), data.len()),
                },
            };
            let body_bytes = &data[pos..body_end];
            pos = next_pos;

            let body_text = match std::str::from_utf8(body_bytes) {
                Ok(t) => t,
                Err(_) => continue, // Skip non-UTF-8 text items.
            };

            match hdr.item_type.as_str() {
                "event" => match serde_json::from_str::<Event>(body_text) {
                    Ok(event) => result.events.push(event),
                    Err(e) => tracing::warn!(item = "event", error = %e, "dropping event: deserialize failed"),
                },
                "transaction" => match serde_json::from_str::<Transaction>(body_text) {
                    Ok(txn) => result.transactions.push(txn),
                    Err(e) => {
                        tracing::warn!(item = "transaction", error = %e, "dropping transaction: deserialize failed")
                    }
                },
                "session" => match serde_json::from_str::<SessionUpdate>(body_text) {
                    Ok(session) => result.session_updates.push(session),
                    Err(e) => tracing::warn!(item = "session", error = %e, "dropping session: deserialize failed"),
                },
                "sessions" => match serde_json::from_str::<SessionAggregates>(body_text) {
                    Ok(aggregates) => result.session_aggregates.push(aggregates),
                    Err(e) => {
                        tracing::warn!(item = "sessions", error = %e, "dropping session aggregates: deserialize failed")
                    }
                },
                _ => tracing::debug!(item_type = %hdr.item_type, "ignoring unknown envelope item type"),
            }
        }
    }

    Ok(result)
}

// ── Public entry point ────────────────────────────────────────────────────

/// Parse a Sentry envelope body into individual events, transactions,
/// sessions, and attachments.
///
/// Also supports gzip- and deflate-wrapped envelopes.
pub fn parse_envelope(body: &[u8], content_encoding: Option<&str>) -> Result<ParsedEnvelope> {
    parse_envelope_with_limits(body, content_encoding, &EnvelopeLimits::default())
}

/// Like [`parse_envelope`] but with explicit [`EnvelopeLimits`].
pub fn parse_envelope_with_limits(
    body: &[u8],
    content_encoding: Option<&str>,
    limits: &EnvelopeLimits,
) -> Result<ParsedEnvelope> {
    let encoding = content_encoding.map(|e| e.trim().to_ascii_lowercase());
    let decompressed = match encoding.as_deref() {
        Some("gzip") | Some("x-gzip") | Some("application/gzip") => {
            read_capped(GzDecoder::new(body), limits.max_decompressed_bytes, "gzip")?
        }
        Some("deflate") => {
            read_capped(flate2::read::DeflateDecoder::new(body), limits.max_decompressed_bytes, "deflate")?
        }
        _ => {
            if body.len() > limits.max_decompressed_bytes {
                anyhow::bail!("envelope exceeds limit of {} bytes", limits.max_decompressed_bytes);
            }
            body.to_vec()
        }
    };

    // Use binary parser for all envelopes (handles both text and binary items).
    parse_envelope_binary(&decompressed, limits)
}

/// Read a decoder to the end, failing if output would exceed `max` bytes.
fn read_capped<R: Read>(decoder: R, max: usize, kind: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    // `take(max + 1)` lets us distinguish "exactly max" from "over max".
    let cap = (max as u64).saturating_add(1);
    decoder.take(cap).read_to_end(&mut buf).with_context(|| format!("{kind} decompression failed"))?;
    if buf.len() > max {
        anyhow::bail!("decompressed {kind} envelope exceeds limit of {max} bytes");
    }
    Ok(buf)
}

// ── Legacy text parser (kept for backward-compat in tests) ────────────────

/// Parse the text envelope format.
///
/// The envelope is a series of lines:
/// Line 1: envelope header JSON (event_id, dsn, sent_at)
/// Line 2+: item header JSON (type, length) followed by item body (JSON)
///
/// NOTE: This parser is NOT binary-safe — it splits on `\n` and therefore
/// corrupts attachment payloads. Kept for backward-compat in tests only.
#[allow(dead_code)]
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
        } else if item_type == "session" {
            if let Ok(session) = serde_json::from_str::<SessionUpdate>(body_line) {
                result.session_updates.push(session);
            }
        } else if item_type == "sessions" {
            if let Ok(aggregates) = serde_json::from_str::<SessionAggregates>(body_line) {
                result.session_aggregates.push(aggregates);
            }
        }
        // Ignore other item types (attachments, etc.)
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
        let (k, v) = trimmed.split_once('=')?;
        (k.trim() == "sentry_key").then(|| v.trim().to_string()).filter(|v| !v.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing text-based tests (backward compat) ──

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
    fn parse_event_message_as_object_sentry_dart_format() {
        // Modern SDKs (Dart/Flutter, JS, etc.) send `message` as the
        // Sentry Message interface object, not a plain string. Without the
        // custom deserializer this whole event is silently dropped.
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-07-01T00:00:00Z"}
{"type":"event"}
{"event_id":"abc123","message":{"formatted":"Null check operator used on a null value","message":"%s"},"level":"error","platform":"dart"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1, "message object must not drop the event");
        assert_eq!(result.events[0].event_id, "abc123");
        assert_eq!(result.events[0].message.as_deref(), Some("Null check operator used on a null value"));
    }

    #[test]
    fn parse_envelope_with_numeric_timestamps_ingests() {
        // Real JS/Python SDKs send epoch numbers for event and breadcrumb timestamps.
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-08-27T00:00:00Z"}
{"type":"event"}
{"event_id":"abc123","timestamp":1756278600.5,"message":"boom","level":"error","breadcrumbs":{"values":[{"timestamp":1756278599,"category":"console","message":"hi"}]}}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1, "numeric timestamps must not drop the event");
        let ev = &result.events[0];
        assert_eq!(ev.timestamp.as_deref(), Some("2025-08-27T07:10:00.500Z"));
        assert_eq!(ev.breadcrumbs.values[0].timestamp.as_deref(), Some("2025-08-27T07:09:59Z"));
    }

    #[test]
    fn parse_event_message_object_falls_back_to_message_key() {
        // When `formatted` is absent, fall back to the `message` template.
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-07-01T00:00:00Z"}
{"type":"event"}
{"event_id":"abc123","message":{"message":"template only"},"level":"error"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].message.as_deref(), Some("template only"));
    }

    #[test]
    fn parse_envelope_session_and_event_together() {
        let envelope = r#"{"event_id":"abc123","sent_at":"2026-01-01T00:00:00Z"}
{"type":"session"}
{"session_id":"sess-1","init":true,"started":"2026-06-27T10:00:00Z","status":"ok","errors":0,"attributes":{"release":"myapp@1.0.0"}}
{"type":"event","length":50}
{"event_id":"abc123","message":"error","level":"error"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.session_updates.len(), 1);
        assert_eq!(result.session_updates[0].session_id, "sess-1");
        assert_eq!(result.events[0].event_id, "abc123");
    }

    #[test]
    fn parse_envelope_aggregated_sessions() {
        let envelope = r#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"sessions"}
{"aggregates":[{"started":"2026-06-27T10:00:00Z","exited":90,"errored":5,"abnormal":2,"crashed":3}],"attributes":{"release":"myapp@1.0.0"}}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert!(result.events.is_empty());
        assert_eq!(result.session_aggregates.len(), 1);
        assert_eq!(result.session_aggregates[0].aggregates[0].crashed, 3);
        assert_eq!(result.session_aggregates[0].attributes.release, "myapp@1.0.0");
    }

    #[test]
    fn parse_envelope_malformed_session_skipped() {
        let envelope = r#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"session"}
{invalid json here}
{"type":"event","length":50}
{"event_id":"e2","message":"still works","level":"error"}"#;

        let result = parse_envelope_text(envelope).unwrap();
        assert!(result.session_updates.is_empty(), "malformed session should be skipped");
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_id, "e2");
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
        let header = "Sentry sentry_key=abc123, sentry_version=7";
        let key = extract_sentry_key(header).unwrap();
        assert_eq!(key, "abc123");
    }

    #[test]
    fn extract_sentry_key_tolerates_whitespace() {
        assert_eq!(extract_sentry_key("Sentry sentry_key=abc,sentry_version=7").as_deref(), Some("abc"));
        assert_eq!(extract_sentry_key("Sentry sentry_version=7,sentry_key=abc").as_deref(), Some("abc"));
        assert_eq!(extract_sentry_key("Sentry  sentry_version=7 ,  sentry_key = abc ").as_deref(), Some("abc"));
        assert!(extract_sentry_key("Sentry sentry_key=").is_none());
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

    // ── New binary-safe tests ──

    /// Helper: build a raw envelope from parts.
    fn build_envelope(parts: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for part in parts {
            out.extend_from_slice(part);
        }
        out
    }

    #[test]
    fn parse_envelope_with_attachment_and_event() {
        let event_body = br#"{"event_id":"e1","message":"test error","level":"error"}"#;
        let event_header = format!(r#"{{"type":"event","length":{}}}"#, event_body.len());

        let attachment_data: &[u8] = b"hello\nworld\r\n\xff\x00binary";
        let attachment_header = format!(
            r#"{{"type":"attachment","length":{},"filename":"ss.png","content_type":"image/png"}}"#,
            attachment_data.len()
        );

        let envelope = build_envelope(&[
            b"{\"event_id\":\"e1\",\"sent_at\":\"2026-01-01T00:00:00Z\"}\n",
            event_header.as_bytes(),
            b"\n",
            event_body,
            b"\n",
            attachment_header.as_bytes(),
            b"\n",
            attachment_data,
            b"\n",
        ]);

        let result = parse_envelope_binary(&envelope, &EnvelopeLimits::default()).unwrap();
        assert_eq!(result.events.len(), 1, "should have 1 event");
        assert_eq!(result.attachments.len(), 1, "should have 1 attachment");
        assert_eq!(result.events[0].event_id, "e1");
        assert_eq!(result.attachments[0].data, attachment_data);
        assert_eq!(result.attachments[0].filename, "ss.png");
        assert_eq!(result.attachments[0].content_type.as_deref(), Some("image/png"));
    }

    #[test]
    fn parse_envelope_binary_data_with_newlines() {
        let attachment_data: &[u8] = b"hello\nworld\r\n\xff\x00binary";
        let attachment_header =
            format!(r#"{{"type":"attachment","length":{},"filename":"debug.log"}}"#, attachment_data.len());

        let envelope = build_envelope(&[
            b"{\"event_id\":\"e1\",\"sent_at\":\"2026-01-01T00:00:00Z\"}\n",
            attachment_header.as_bytes(),
            b"\n",
            attachment_data,
            b"\n",
        ]);

        let result = parse_envelope_binary(&envelope, &EnvelopeLimits::default()).unwrap();
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].data, attachment_data);
        // The key assertion: the data contains embedded newlines and null bytes
        // and round-trips correctly (not corrupted by newline splitting).
        assert_eq!(result.attachments[0].data.len(), 21);
        assert_eq!(&result.attachments[0].data[5..13], b"\nworld\r\n");
        assert_eq!(result.attachments[0].data[13], 0xff);
        assert_eq!(result.attachments[0].data[14], 0x00);
    }

    #[test]
    fn parse_envelope_attachment_only() {
        let attachment_data: &[u8] = b"\x89PNG\r\n\x1a\nfake-png-data";
        let attachment_header = format!(
            r#"{{"type":"attachment","length":{},"filename":"screenshot.png","content_type":"image/png","attachment_type":"event.attachment"}}"#,
            attachment_data.len()
        );

        let envelope = build_envelope(&[
            b"{\"event_id\":\"e1\",\"sent_at\":\"2026-01-01T00:00:00Z\"}\n",
            attachment_header.as_bytes(),
            b"\n",
            attachment_data,
            b"\n",
        ]);

        let result = parse_envelope_binary(&envelope, &EnvelopeLimits::default()).unwrap();
        assert!(result.events.is_empty(), "should have no events");
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].data, attachment_data);
        assert_eq!(result.attachments[0].filename, "screenshot.png");
        assert_eq!(result.attachments[0].attachment_type.as_deref(), Some("event.attachment"));
    }

    #[test]
    fn parse_envelope_no_attachments_backward_compat() {
        // Standard text-only envelope (event + transaction)
        let envelope = br#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"event","length":50}
{"event_id":"e1","message":"error here","level":"error"}
{"type":"transaction","length":150}
{"event_id":"e2","level":"info","transaction":"POST /api/submit","start_timestamp":1.0,"timestamp":2.0}"#;

        let result_text = parse_envelope_text(std::str::from_utf8(envelope).unwrap()).unwrap();
        let result_binary = parse_envelope_binary(envelope, &EnvelopeLimits::default()).unwrap();

        // Events should match
        assert_eq!(result_binary.events.len(), result_text.events.len());
        assert_eq!(result_binary.events.len(), 1);
        assert_eq!(result_binary.events[0].message, result_text.events[0].message);

        // Transactions should match
        assert_eq!(result_binary.transactions.len(), result_text.transactions.len());
        assert_eq!(result_binary.transactions.len(), 1);
        assert_eq!(result_binary.transactions[0].transaction, result_text.transactions[0].transaction);

        // No attachments from either parser
        assert!(result_text.attachments.is_empty());
        assert!(result_binary.attachments.is_empty());
    }

    #[test]
    fn parse_envelope_truncated_attachment() {
        // Attachment declares length=100 but only provides 10 bytes
        let attachment_header = r#"{"type":"attachment","length":100,"filename":"big.bin"}"#;
        let short_data = b"only10byte";

        let envelope = build_envelope(&[
            b"{\"event_id\":\"e1\",\"sent_at\":\"2026-01-01T00:00:00Z\"}\n",
            attachment_header.as_bytes(),
            b"\n",
            short_data,
        ]);

        let result = parse_envelope_binary(&envelope, &EnvelopeLimits::default());
        assert!(result.is_err(), "truncated attachment should return error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("truncated attachment"), "error should mention truncated attachment, got: {err}");
    }

    #[test]
    fn parse_envelope_multiple_attachments() {
        let att1_data: &[u8] = b"first attachment\nwith newline";
        let att1_header = format!(
            r#"{{"type":"attachment","length":{},"filename":"a1.txt","content_type":"text/plain"}}"#,
            att1_data.len()
        );
        let att2_data: &[u8] = b"\x89PNG\r\n\x1a\nfake";
        let att2_header = format!(
            r#"{{"type":"attachment","length":{},"filename":"a2.png","content_type":"image/png"}}"#,
            att2_data.len()
        );

        let envelope = build_envelope(&[
            b"{\"event_id\":\"e1\",\"sent_at\":\"2026-01-01T00:00:00Z\"}\n",
            att1_header.as_bytes(),
            b"\n",
            att1_data,
            b"\n",
            att2_header.as_bytes(),
            b"\n",
            att2_data,
            b"\n",
        ]);

        let result = parse_envelope_binary(&envelope, &EnvelopeLimits::default()).unwrap();
        assert_eq!(result.attachments.len(), 2, "should have 2 attachments");
        assert_eq!(result.attachments[0].data, att1_data);
        assert_eq!(result.attachments[0].filename, "a1.txt");
        assert_eq!(result.attachments[1].data, att2_data);
        assert_eq!(result.attachments[1].filename, "a2.png");
    }

    #[test]
    fn parse_gzip_envelope_with_attachment() {
        use std::io::Write;

        let event_body = br#"{"event_id":"e1","message":"gzipped","level":"error"}"#;
        let event_header = format!(r#"{{"type":"event","length":{}}}"#, event_body.len());
        let attachment_data: &[u8] = b"\x89PNG\r\n\x1a\nFAKE";
        let attachment_header = format!(
            r#"{{"type":"attachment","length":{},"filename":"img.png","content_type":"image/png"}}"#,
            attachment_data.len()
        );

        let raw_envelope = build_envelope(&[
            b"{\"event_id\":\"e1\",\"sent_at\":\"2026-01-01T00:00:00Z\"}\n",
            event_header.as_bytes(),
            b"\n",
            event_body,
            b"\n",
            attachment_header.as_bytes(),
            b"\n",
            attachment_data,
            b"\n",
        ]);

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&raw_envelope).unwrap();
        let compressed = encoder.finish().unwrap();

        let result = parse_envelope(&compressed, Some("gzip")).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].message.as_deref(), Some("gzipped"));
        assert_eq!(result.attachments.len(), 1);
        assert_eq!(result.attachments[0].data, attachment_data);
        assert_eq!(result.attachments[0].filename, "img.png");
    }

    // ── parse_envelope() uses binary parser for text-only envelopes ──

    #[test]
    fn parse_envelope_entry_point_no_encoding() {
        let envelope = br#"{"event_id":"e1","sent_at":"2026-01-01T00:00:00Z"}
{"type":"event","length":50}
{"event_id":"e1","message":"via entry point","level":"error"}"#;

        let result = parse_envelope(envelope, None).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].message.as_deref(), Some("via entry point"));
        assert!(result.attachments.is_empty());
    }

    // ── Hardening tests (#330) ──

    fn gzip_bytes(data: &[u8]) -> Vec<u8> {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        let mut enc = GzEncoder::new(Vec::new(), Compression::best());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn gzip_bomb_is_rejected() {
        // ~16 MiB of zeros compresses to a few KiB; cap at 1 MiB.
        let bomb = gzip_bytes(&vec![0u8; 16 * 1024 * 1024]);
        assert!(bomb.len() < 64 * 1024);
        let limits = EnvelopeLimits { max_decompressed_bytes: 1024 * 1024, ..EnvelopeLimits::default() };
        let err = parse_envelope_with_limits(&bomb, Some("gzip"), &limits).unwrap_err().to_string();
        assert!(err.contains("exceeds"), "got: {err}");
    }

    #[test]
    fn deflate_bomb_is_rejected() {
        use flate2::{Compression, write::DeflateEncoder};
        use std::io::Write;
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
        enc.write_all(&vec![0u8; 8 * 1024 * 1024]).unwrap();
        let bomb = enc.finish().unwrap();
        let limits = EnvelopeLimits { max_decompressed_bytes: 1024 * 1024, ..EnvelopeLimits::default() };
        assert!(parse_envelope_with_limits(&bomb, Some("deflate"), &limits).is_err());
    }

    #[test]
    fn legit_gzip_under_limit_still_parses() {
        let raw = b"{\"event_id\":\"e1\"}\n{\"type\":\"event\"}\n{\"event_id\":\"e1\",\"message\":\"hi\"}\n";
        let gz = gzip_bytes(raw);
        let r = parse_envelope(&gz, Some("gzip")).unwrap();
        assert_eq!(r.events.len(), 1);
    }

    #[test]
    fn content_encoding_is_case_insensitive() {
        let raw = b"{\"event_id\":\"e1\"}\n{\"type\":\"event\"}\n{\"event_id\":\"e1\",\"message\":\"hi\"}\n";
        let gz = gzip_bytes(raw);
        for enc in ["GZIP", "Gzip", " gzip ", "x-gzip"] {
            let r = parse_envelope(&gz, Some(enc)).unwrap_or_else(|e| panic!("{enc}: {e}"));
            assert_eq!(r.events.len(), 1, "{enc}");
        }
    }

    #[test]
    fn huge_declared_length_does_not_overflow() {
        let env = format!("{{\"event_id\":\"e1\"}}\n{{\"type\":\"attachment\",\"length\":{}}}\nabc", u64::MAX);
        let r = parse_envelope_binary(env.as_bytes(), &EnvelopeLimits::default());
        assert!(r.is_err());
    }

    #[test]
    fn attachment_count_is_capped() {
        let mut env = b"{\"event_id\":\"e1\"}\n".to_vec();
        for _ in 0..5 {
            env.extend_from_slice(b"{\"type\":\"attachment\",\"length\":1}\nx\n");
        }
        let limits = EnvelopeLimits { max_attachments: 3, ..EnvelopeLimits::default() };
        assert!(parse_envelope_binary(&env, &limits).is_err());
    }

    #[test]
    fn attachment_size_is_capped() {
        let mut env = b"{\"event_id\":\"e1\"}\n{\"type\":\"attachment\",\"length\":10}\n".to_vec();
        env.extend_from_slice(&[b'x'; 10]);
        let limits = EnvelopeLimits { max_attachment_bytes: 5, ..EnvelopeLimits::default() };
        assert!(parse_envelope_binary(&env, &limits).is_err());
    }

    #[test]
    fn text_item_honours_declared_length() {
        // Declared length spans an embedded newline (valid JSON whitespace).
        let body = b"{\"event_id\":\"e1\",\n\"message\":\"a\"}";
        let mut env = b"{\"event_id\":\"e1\"}\n".to_vec();
        env.extend_from_slice(format!("{{\"type\":\"event\",\"length\":{}}}\n", body.len()).as_bytes());
        env.extend_from_slice(body);
        env.extend_from_slice(b"\n{\"type\":\"event\"}\n{\"event_id\":\"e2\",\"message\":\"b\"}\n");
        let r = parse_envelope_binary(&env, &EnvelopeLimits::default()).unwrap();
        assert_eq!(r.events.len(), 2);
    }
}
