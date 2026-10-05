//! Unified HTTP error type. Handlers return `Result<_, ApiError>`;
//! `ApiError` knows how to shape itself into a JSON response.
//!
//! Keep the external shape consistent — the SvelteKit side only
//! needs to decode `{ "error": <code>, "message": <string> }` and
//! surface it to the user.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use domain::DomainError;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Keep the error body small; details go to tracing, not the
        // wire. Leaking "column foo does not exist" to the client is
        // just gift-wrapping the schema for an attacker.
        let (status, code, message) = match &self {
            ApiError::Domain(DomainError::NotFound) => {
                (StatusCode::NOT_FOUND, "not_found", "not found".to_string())
            }
            ApiError::Domain(DomainError::Forbidden) => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "forbidden".to_string(),
            ),
            ApiError::Domain(DomainError::Validation(m)) => {
                (StatusCode::BAD_REQUEST, "validation", m.clone())
            }
            ApiError::Domain(DomainError::Conflict(m)) => {
                (StatusCode::CONFLICT, "conflict", m.clone())
            }
            ApiError::Domain(DomainError::Internal(_))
            | ApiError::Db(_)
            | ApiError::Internal(_) => {
                tracing::error!(error = ?self, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}
