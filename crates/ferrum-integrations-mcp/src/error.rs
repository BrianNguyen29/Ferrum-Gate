//! Crate-local error types for ferrum-integrations-mcp.

use thiserror::Error;

/// Primary error type for this crate.
/// Aggregates reqwest HTTP errors, JSON serialization errors,
/// and bridge-level validation errors.
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP transport or protocol error from reqwest.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON serialization or deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The gateway returned a non-2xx response.
    #[error("gateway error ({status}): {message}")]
    Gateway {
        status: reqwest::StatusCode,
        message: String,
    },

    /// Validation error: a required field was empty or malformed.
    #[error("validation error: {0}")]
    Validation(String),
}

impl Error {
    /// Constructs a validation error with a formatted message.
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Constructs a gateway error from a raw HTTP status and body.
    pub fn gateway(status: reqwest::StatusCode, message: impl Into<String>) -> Self {
        Self::Gateway {
            status,
            message: message.into(),
        }
    }
}
