//! The dashboard, embedded at build time from `packages/dashboard/dist` (FR-21).

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Serves a dashboard file; unknown paths get `index.html` (client-side routing).
pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let found = ASSETS
        .iter()
        .find(|(name, _, _)| *name == path)
        .or_else(|| ASSETS.iter().find(|(name, _, _)| *name == "index.html"));
    match found {
        Some((name, mime, body)) => {
            // Hashed asset files never change; index.html must be revalidated.
            let cache = if name.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [
                    (header::CONTENT_TYPE, *mime),
                    (header::CACHE_CONTROL, cache),
                ],
                *body,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
