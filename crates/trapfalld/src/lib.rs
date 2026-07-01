//! trapfalld library — shared between binary and integration tests.

pub mod alert;
pub mod attachment_storage;
pub mod auth;
pub mod config;
pub mod digest;
pub mod log_time;
pub mod metrics;
pub mod migrate;
pub mod rate_limit;
pub mod retention;
pub mod server;
pub mod spa;
pub mod swagger;
pub mod ws;

pub use alert::spawn_alert_engine;
pub use config::Config;
pub use digest::DigestTask;
pub use server::AppState;
pub use ws::WsHub;
