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
        let cache_control = if is_immutable_asset(path) {
            "public, max-age=31536000, immutable" // 1 year for hashed assets
        } else {
            "no-cache" // HTML/other: revalidate
        };
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime.as_ref()), (header::CACHE_CONTROL, cache_control)],
            file.data.to_vec(),
        )
            .into_response();
    }

    // Fallback to index.html for client-side routing
    match Assets::get("index.html") {
        Some(file) => (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-cache")],
            Html(String::from_utf8_lossy(&file.data).to_string()),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Check if path is a hashed/immutable static asset (JS/CSS/fonts/images).
fn is_immutable_asset(path: &str) -> bool {
    path.starts_with("_app/") // SvelteKit build output
        || path.ends_with(".js")
        || path.ends_with(".css")
        || path.ends_with(".woff2")
        || path.ends_with(".woff")
        || path.ends_with(".ttf")
        || path.ends_with(".png")
        || path.ends_with(".jpg")
        || path.ends_with(".svg")
        || path.ends_with(".ico")
}
