//! SPA handler — serve embedded static files.
//!
//! Uses rust-embed to embed the SvelteKit build output.
//! Falls back to index.html for client-side routing.

use axum::{
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../web/build/"]
#[prefix = ""]
struct Assets;

/// Serve SPA static files or fallback to index.html.
pub async fn spa_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try exact file match first
    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (StatusCode::OK, [(header::CONTENT_TYPE, mime.as_ref())], file.data.to_vec()).into_response();
    }

    // Fallback to index.html for client-side routing
    match Assets::get("index.html") {
        Some(file) => Html(String::from_utf8_lossy(&file.data).to_string()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
