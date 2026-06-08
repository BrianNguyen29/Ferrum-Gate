//! I11 Output Sanitization helpers.
//!
//! These helpers centralize the response/sanitization logic shared by all
//! governance handlers (`server`, `audit`, `admin/tokens`, `admin/agents`,
//! `bridge`). Extracting them from `server.rs` reduces cross-module coupling
//! before the policy handlers are split out.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use ferrum_firewall::{SemanticFirewall, TaintScoringFirewall};
use serde::Serialize;

/// Sanitizes a serde_json::Value by stripping control characters from all string values.
/// Preserves JSON structure (keys, numeric values, bools, nulls unchanged).
pub(crate) fn sanitize_json(
    fw: &TaintScoringFirewall,
    value: serde_json::Value,
) -> serde_json::Value {
    fw.sanitize_output(value)
}

/// Build a sanitized JSON response for an API success payload.
///
/// Serializes `value` to JSON, runs the firewall output sanitizer to strip
/// control characters, then returns an `(StatusCode, Json(sanitized))` axum
/// response. This is the canonical success-path helper for governance
/// handlers; use [`sanitized_api_error_response`] for error envelopes.
pub(crate) fn sanitized_response<T: Serialize>(
    fw: &TaintScoringFirewall,
    status: StatusCode,
    value: &T,
) -> Response {
    let json_val = serde_json::to_value(value).unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to serialize response for sanitized_response");
        serde_json::json!({})
    });
    let sanitized = sanitize_json(fw, json_val);
    (status, Json(sanitized)).into_response()
}

/// Build a sanitized JSON response for an `ApiError` envelope.
///
/// Wraps [`sanitized_response`] for the error case. The sanitizer strips
/// control characters from `message` and `details` to prevent log
/// injection and downstream display corruption.
pub(crate) fn sanitized_api_error_response(
    fw: &TaintScoringFirewall,
    status: StatusCode,
    error: &ferrum_proto::ApiError,
) -> Response {
    sanitized_response(fw, status, error)
}

#[cfg(test)]
mod tests {
    // Tests for the sanitization helpers. These exercise the wire output of
    // `sanitized_response` and `sanitized_api_error_response` to confirm that
    // the firewall output sanitizer is applied uniformly.

    use super::*;
    use ferrum_proto::{ApiError, ApiErrorCode};
    use std::sync::Arc;

    /// Build a TaintScoringFirewall for test usage.
    fn test_firewall() -> Arc<TaintScoringFirewall> {
        Arc::new(TaintScoringFirewall::new())
    }

    #[tokio::test]
    async fn test_sanitized_response_strips_control_chars_from_string_value() {
        let fw = test_firewall();
        // Use a serde_json::Value with a control character in a string field.
        let value = serde_json::json!({"name": "evil\u{0000}\u{0007}\u{001f}name"});
        let response = sanitized_response(&fw, StatusCode::OK, &value);
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Control characters should be stripped.
        let name = parsed.get("name").and_then(|v| v.as_str()).unwrap();
        assert!(!name.contains('\u{0000}'));
        assert!(!name.contains('\u{0007}'));
        assert!(!name.contains('\u{001f}'));
    }

    #[tokio::test]
    async fn test_sanitized_response_preserves_numeric_and_bool_values() {
        let fw = test_firewall();
        let value = serde_json::json!({
            "count": 42,
            "ratio": 2.5,
            "active": true,
            "items": [1, 2, 3],
        });
        let response = sanitized_response(&fw, StatusCode::OK, &value);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed.get("count").and_then(|v| v.as_i64()), Some(42));
        assert_eq!(parsed.get("active").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            parsed
                .get("items")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(3)
        );
    }

    #[tokio::test]
    async fn test_sanitized_api_error_response_preserves_code_and_correlation_id() {
        let fw = test_firewall();
        let error = ApiError {
            code: ApiErrorCode::NotFound,
            message: "missing resource\u{0000}with control".to_string(),
            correlation_id: "corr-123".to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        let response = sanitized_api_error_response(&fw, StatusCode::NOT_FOUND, &error);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let parsed: ApiError = serde_json::from_slice(&body).unwrap();
        // Code is preserved as a JSON object; compare via Debug/serialization
        // since ApiErrorCode does not implement PartialEq.
        let code_serialized = serde_json::to_value(parsed.code).unwrap();
        let expected_serialized = serde_json::to_value(ApiErrorCode::NotFound).unwrap();
        assert_eq!(code_serialized, expected_serialized);
        assert_eq!(parsed.correlation_id, "corr-123");
        // Message should be sanitized (control char stripped).
        assert!(!parsed.message.contains('\u{0000}'));
    }
}
