use axum::http::{header, StatusCode, Uri};
use axum::response::Response;

/// SPA fallback handler for the standalone Axum API.
///
/// In revue-office the production frontend is served by Tauri from `../dist`.
/// The embedded server only owns `/api` and `/outputs`, so non-API fallbacks
/// return a small diagnostic response instead of embedding frontend assets.
pub async fn fallback_handler(_uri: Uri) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "name": "revueOffice",
                "version": "0.1.0",
                "message": "revueOffice API is running. The desktop frontend is served by Tauri.",
                "docs": "/api/health",
            })
            .to_string(),
        ))
        .unwrap()
}
