//! REST error bodies: `{ error: { code, message, details? } }` with 400/404/409/500.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hardless_core::RegistryError;
use serde_json::json;

/// An API error.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status.
    pub status: StatusCode,
    /// Machine-readable code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

impl ApiError {
    /// A 400 error.
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    /// A 404 error.
    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code,
            message: message.into(),
        }
    }

    /// A 501 error for something a later milestone adds.
    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_IMPLEMENTED,
            code: "not_implemented",
            message: message.into(),
        }
    }
}

impl From<RegistryError> for ApiError {
    fn from(err: RegistryError) -> Self {
        let status = match err {
            RegistryError::NotFound(_) => StatusCode::NOT_FOUND,
            RegistryError::DuplicateId(_) => StatusCode::CONFLICT,
            RegistryError::UnknownKind(_)
            | RegistryError::InvalidConfig(_)
            | RegistryError::UnsupportedFault { .. } => StatusCode::BAD_REQUEST,
            RegistryError::DuplicateKind(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            status,
            code: err.code(),
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({ "error": { "code": self.code, "message": self.message } });
        (self.status, Json(body)).into_response()
    }
}
