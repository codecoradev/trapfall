//! # trapfall-ingest
//!
//! HTTP ingest pipeline — Sentry envelope parser, digest loop, handlers.

pub mod envelope;

pub use envelope::{EnvelopeLimits, extract_sentry_key, parse_envelope, parse_envelope_with_limits};
pub use trapfall_proto::ParsedEnvelope;
