//! Problem Details type used by governance HTTP handlers.
//!
//! `ApiProblem` is the canonical error type returned by every governance
//! route. It wraps a `ferrum_proto::ApiError` envelope and a `StatusCode`,
//! and implements `IntoResponse` so handlers can return `Result<_, ApiProblem>`
//! directly. The struct/impl/IntoResponse combination was previously defined
//! inline in `server.rs`; this module moves it next to the rest of the
//! error-handling surface so it can be reused by the extracted handler
//! modules (`policy_eval`, `approval`, `lineage`, ...) without taking a
//! dependency on `server`.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ferrum_cap::CapabilityError;
use ferrum_proto::{ApiError, ApiErrorCode};

/// Wrapped error envelope used by all governance HTTP handlers.
#[derive(Debug)]
pub(crate) struct ApiProblem(pub(crate) ApiError, pub(crate) StatusCode);

impl ApiProblem {
    /// Build a problem with an explicit status, error code, and message.
    pub(crate) fn new(status: StatusCode, code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self(
            ApiError {
                code,
                message: message.into(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            },
            status,
        )
    }

    /// Build an internal-server-error problem from an `anyhow::Error`.
    pub(crate) fn internal(err: anyhow::Error) -> Self {
        Self(
            ApiError {
                code: ApiErrorCode::Internal,
                message: err.to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            },
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    }

    /// Map a `CapabilityError` to a status + code pair.
    pub(crate) fn from_capability(err: CapabilityError) -> Self {
        let (status, code) = match err {
            CapabilityError::NotFound => (StatusCode::NOT_FOUND, ApiErrorCode::NotFound),
            CapabilityError::AlreadyUsed => (StatusCode::CONFLICT, ApiErrorCode::Conflict),
            CapabilityError::Revoked => (StatusCode::BAD_REQUEST, ApiErrorCode::CapabilityRevoked),
            CapabilityError::Expired => (StatusCode::BAD_REQUEST, ApiErrorCode::CapabilityExpired),
            CapabilityError::TtlTooLong => (StatusCode::BAD_REQUEST, ApiErrorCode::ValidationError),
        };
        Self::new(status, code, err.to_string())
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        (self.1, Json(self.0)).into_response()
    }
}
