use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// API-level error type convertible to HTTP responses.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    message: String,
    timestamp: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_type) = match &self {
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "Bad Request"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "Not Found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "Conflict"),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        };

        let body = ErrorBody {
            error: error_type.to_string(),
            message: self.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}
