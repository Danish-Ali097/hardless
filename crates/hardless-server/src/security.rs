//! Bearer token (NFR-02) and CORS for the REST API.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::AppState;

/// Requires the token when one is configured: `Authorization: Bearer <token>`, or
/// `?token=<token>` for WebSockets (browsers cannot set headers on them).
pub async fn require_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(token) = state.token.as_deref() else {
        return next.run(request).await;
    };
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    let from_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let from_query = request
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|pair| pair.strip_prefix("token=")));
    if from_header == Some(token) || from_query == Some(token) {
        next.run(request).await
    } else {
        ApiError {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "a valid token is required (Authorization: Bearer <token>)".into(),
        }
        .into_response()
    }
}

/// Whether an `Origin` header names this machine (any port).
fn is_loopback_origin(origin: &str) -> bool {
    let host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
    let host = host.rsplit_once(':').map_or(host, |(h, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            h
        } else {
            host
        }
    });
    matches!(host, "localhost" | "127.0.0.1" | "[::1]") || host.ends_with(".localhost")
}

/// CORS for the REST API, so the shim in an app on another port can call it. Allowed
/// origins: this machine's, or any origin when a token protects the API.
pub async fn cors(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let allowed = origin
        .as_deref()
        .filter(|o| is_loopback_origin(o) || state.token.is_some())
        .and_then(|o| HeaderValue::from_str(o).ok());

    let mut response = if request.method() == Method::OPTIONS && origin.is_some() {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap_or_default()
    } else {
        next.run(request).await
    };
    if let Some(origin) = allowed {
        let headers = response.headers_mut();
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Content-Type, Authorization"),
        );
        headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::is_loopback_origin;

    #[test]
    fn loopback_origins() {
        for origin in [
            "http://localhost:5173",
            "http://127.0.0.1",
            "https://[::1]:8443",
            "http://app.localhost:3000",
        ] {
            assert!(is_loopback_origin(origin), "{origin}");
        }
        for origin in [
            "https://example.com",
            "http://localhost.example.com",
            "http://192.168.1.5:3000",
        ] {
            assert!(!is_loopback_origin(origin), "{origin}");
        }
    }
}
