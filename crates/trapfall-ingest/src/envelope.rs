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
fn parse_envelope_binary(data: &[u8]) -> Result<ParsedEnvelope> {
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

            // Check we have enough data.
            if pos + length > data.len() {
                anyhow::bail!(
                    "truncated attachment: declared {} bytes but only {} remaining",
                    length,
                    data.len() - pos
                );
            }

            let attachment_data = data[pos..pos + length].to_vec();
            pos += length;

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
            // Find the next \n to delimit the body.
            let body_end = match find_newline(data, pos) {
                Some(end) => end,
                None => {
                    // Last item with no trailing newline — use remaining bytes.
                    data.len()
                }
            };
            let body_bytes = &data[pos..body_end];
            pos = if body_end < data.len() { body_end + 1 } else { body_end };

            // If length is declared, honour it and advance past any extra
            // bytes the newline scan would have missed.
            if let Some(length) = hdr.length {
                let declared_end = (body_end + 1).saturating_sub(body_bytes.len()) + length;
                if declared_end <= data.len() {
                    pos = declared_end;
                }
            }

            let body_text = match std::str::from_utf8(body_bytes) {
                Ok(t) => t,
                Err(_) => continue, // Skip non-UTF-8 text items.
            };

            match hdr.item_type.as_str() {
                "event" => {
                    if let Ok(event) = serde_json::from_str::<Event>(body_text) {
                        result.events.push(event);
                    }
                }
                "transaction" => {
                    if let Ok(txn) = serde_json::from_str::<Transaction>(body_text) {
                        result.transactions.push(txn);
                    }
                }
                "session" => {
                    if let Ok(session) = serde_json::from_str::<SessionUpdate>(body_text) {
                        result.session_updates.push(session);
                    }
                }
                "sessions" => {
                    if let Ok(aggregates) = serde_json::from_str::<SessionAggregates>(body_text) {
                        result.session_aggregates.push(aggregates);
                    }
                }
                _ => {} // Ignore unknown item types.
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
    let decompressed = match content_encoding {
        Some("gzip") | Some("application/gzip") => decompress_gzip(body)?,
        Some("deflate") => decompress_deflate(body)?,
        _ => body.to_vec(),
    };

    // Use binary parser for all envelopes (handles both text and binary items).
    parse_envelope_binary(&decompressed)
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
        trimmed.strip_prefix("sentry_key=").map(str::to_string)
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

        let result = parse_envelope_binary(&envelope).unwrap();
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

        let result = parse_envelope_binary(&envelope).unwrap();
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

        let result = parse_envelope_binary(&envelope).unwrap();
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
        let result_binary = parse_envelope_binary(envelope).unwrap();

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

        let result = parse_envelope_binary(&envelope);
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

        let result = parse_envelope_binary(&envelope).unwrap();
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
}
