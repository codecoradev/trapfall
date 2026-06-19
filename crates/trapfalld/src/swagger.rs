//! Swagger UI + OpenAPI spec serving.

use axum::Router;
use axum::response::Html;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "src/"]
#[include = "openapi.yaml"]
struct OpenApiAssets;

const SWAGGER_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>TrapFall API Docs</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
    SwaggerUIBundle({
        url: "/api/docs/openapi.yaml",
        dom_id: '#swagger-ui',
        presets: [
            SwaggerUIBundle.presets.apis,
            SwaggerUIBundle.SwaggerUIStandalonePreset
        ],
        layout: "BaseLayout",
        tryItOutEnabled: true
    });
    </script>
</body>
</html>"#;

pub async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_HTML)
}

pub async fn openapi_spec() -> impl axum::response::IntoResponse {
    let spec = OpenApiAssets::get("openapi.yaml")
        .map(|f| String::from_utf8_lossy(&f.data).to_string())
        .unwrap_or_else(|| "openapi: 3.0.3\ninfo:\n  title: TrapFall API\n  version: \"0.0.0\"".to_string());
    // Serve as YAML, not HTML — tooling (openapi-generator, redocly, etc.) parse
    // the body and rely on the content-type.
    (
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/yaml; charset=utf-8"),
            (axum::http::header::CACHE_CONTROL, "no-cache"),
        ],
        spec,
    )
}

pub fn swagger_routes() -> Router<()> {
    Router::new()
        .route("/api/docs", axum::routing::get(swagger_ui))
        .route("/api/docs/openapi.yaml", axum::routing::get(openapi_spec))
}
