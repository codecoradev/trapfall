//! trapfalld library — shared between binary and integration tests.

pub mod auth;
pub mod config;
pub mod digest;
pub mod metrics;
pub mod rate_limit;
pub mod retention;
pub mod server;
pub mod spa;
pub mod ws;

pub use config::Config;
pub use digest::DigestTask;
pub use server::AppState;
pub use ws::WsHub;
