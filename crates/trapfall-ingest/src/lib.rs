//! # trapfall-ingest
//!
//! HTTP ingest pipeline — Sentry envelope parser, digest loop, handlers.

pub mod envelope;

pub use envelope::{extract_sentry_key, parse_envelope};
pub use trapfall_proto::ParsedEnvelope;
