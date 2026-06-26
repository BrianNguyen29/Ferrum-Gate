use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{Timelike, Utc};
use ed25519_dalek::Verifier;
use ferrum_proto::{
    ApiError, ApiErrorCode, AuditAction, AuditLogEntry, AuditLogListResponse,
    AuditMerkleRootListResponse, AuditMerkleVerifyResponse, AuditResourceType,
};
use ferrum_store::StoreError;
use ferrum_store::StoreFacade;
use serde::Deserialize;
use std::sync::Arc;

use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::response::{sanitized_api_error_response, sanitized_response};
use crate::state::AppState;

// ── Audit Log Handler ──

#[derive(Debug, Deserialize)]
pub(crate) struct ListAuditLogsQuery {
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    cursor: Option<String>,
    limit: Option<u32>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
}

pub(crate) async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListAuditLogsQuery>,
) -> Response {
    let action = params.action.and_then(|s| s.parse::<AuditAction>().ok());
    let resource_type = params
        .resource_type
        .and_then(|s| s.parse::<AuditResourceType>().ok());
    let limit = params.limit.unwrap_or(50).min(200);

    match state
        .runtime
        .store
        .audit_log()
        .list(
            action,
            resource_type,
            params.resource_id.as_deref(),
            params.cursor.as_deref(),
            limit,
            params.since,
            params.until,
        )
        .await
    {
        Ok((items, next_cursor)) => {
            let response = AuditLogListResponse {
                items,
                next_cursor,
                total: 0, // Not computed for performance
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "audit log list failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list audit logs".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            )
        }
    }
}

/// Verify the audit log hash chain integrity.
pub(crate) async fn verify_audit_chain(State(state): State<Arc<AppState>>) -> Response {
    match state.runtime.store.audit_log().verify_chain().await {
        Ok(()) => {
            let response = ferrum_proto::AuditLogVerifyResponse {
                valid: true,
                total_entries: 0,
                hashed_entries: 0,
                error: None,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "audit chain verification failed");
            let response = ferrum_proto::AuditLogVerifyResponse {
                valid: false,
                total_entries: 0,
                hashed_entries: 0,
                error: Some(e.to_string()),
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct VerifyMerkleRootQuery {
    window_start: chrono::DateTime<chrono::Utc>,
}

/// Verify (compute or retrieve) the Merkle root for an hourly audit window.
pub(crate) async fn verify_audit_merkle_root(
    State(state): State<Arc<AppState>>,
    Query(params): Query<VerifyMerkleRootQuery>,
) -> Response {
    let window_start = params.window_start;
    // Require UTC-aligned hour (fail closed).
    if window_start.minute() != 0
        || window_start.second() != 0
        || window_start.timestamp_subsec_nanos() != 0
    {
        let error = ApiError {
            code: ApiErrorCode::BadRequest,
            message: "window_start must be aligned to the hour".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(
            &state.runtime.firewall,
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    match state
        .runtime
        .store
        .audit_merkle_roots()
        .compute_and_cache_root(window_start)
        .await
    {
        Ok(root) => {
            let response = AuditMerkleVerifyResponse {
                valid: true,
                window_start: root.window_start,
                root: root.root,
                entry_count: root.entry_count,
                error: None,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "audit merkle root verification failed");
            let response = AuditMerkleVerifyResponse {
                valid: false,
                window_start,
                root: String::new(),
                entry_count: 0,
                error: Some(e.to_string()),
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListMerkleRootsQuery {
    cursor: Option<String>,
    #[serde(default = "default_merkle_limit")]
    limit: u32,
}

fn default_merkle_limit() -> u32 {
    50
}

/// List cached Merkle roots with cursor-based pagination.
pub(crate) async fn list_audit_merkle_roots(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListMerkleRootsQuery>,
) -> Response {
    let limit = params.limit.min(200);
    match state
        .runtime
        .store
        .audit_merkle_roots()
        .list_roots(params.cursor.as_deref(), limit)
        .await
    {
        Ok((items, next_cursor)) => {
            let response = AuditMerkleRootListResponse {
                items,
                next_cursor,
                total: 0,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "audit merkle root list failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list merkle roots".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            )
        }
    }
}

/// Create a signed checkpoint for an audit window.
pub(crate) async fn create_checkpoint(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ferrum_proto::CreateCheckpointRequest>,
) -> Response {
    // Validate window_start is hour-aligned.
    if req.window_start.minute() != 0
        || req.window_start.second() != 0
        || req.window_start.timestamp_subsec_nanos() != 0
    {
        let error = ApiError {
            code: ApiErrorCode::BadRequest,
            message: "window_start must be aligned to the hour".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(
            &state.runtime.firewall,
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    // Verify the Merkle root matches the current computed root for the window.
    let computed = match state
        .runtime
        .store
        .audit_merkle_roots()
        .compute_and_cache_root(req.window_start)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "checkpoint creation failed: merkle root computation error");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to compute merkle root for checkpoint".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            );
        }
    };

    if computed.root != req.merkle_root || computed.entry_count != req.entry_count {
        let error = ApiError {
            code: ApiErrorCode::BadRequest,
            message: format!(
                "merkle root mismatch: expected root={} count={}, got root={} count={}",
                computed.root, computed.entry_count, req.merkle_root, req.entry_count
            ),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(
            &state.runtime.firewall,
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    // Verify Ed25519 signature.
    let payload_hash = ferrum_proto::canonical_checkpoint_hash(
        &req.window_start,
        &req.merkle_root,
        req.entry_count,
        &req.signed_at,
    );
    let sig_bytes =
        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.signature) {
            Ok(b) => b,
            Err(_) => {
                let error = ApiError {
                    code: ApiErrorCode::BadRequest,
                    message: "invalid signature encoding".to_string(),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                    retriable: false,
                    details: serde_json::json!({}),
                };
                return sanitized_api_error_response(
                    &state.runtime.firewall,
                    StatusCode::BAD_REQUEST,
                    &error,
                );
            }
        };
    let sig_array: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => {
            let error = ApiError {
                code: ApiErrorCode::BadRequest,
                message: "invalid signature length".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::BAD_REQUEST,
                &error,
            );
        }
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    let pk_bytes =
        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.public_key) {
            Ok(b) => b,
            Err(_) => {
                let error = ApiError {
                    code: ApiErrorCode::BadRequest,
                    message: "invalid public key encoding".to_string(),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                    retriable: false,
                    details: serde_json::json!({}),
                };
                return sanitized_api_error_response(
                    &state.runtime.firewall,
                    StatusCode::BAD_REQUEST,
                    &error,
                );
            }
        };
    let pk_array: [u8; 32] = match pk_bytes.try_into() {
        Ok(a) => a,
        Err(_) => {
            let error = ApiError {
                code: ApiErrorCode::BadRequest,
                message: "invalid public key length".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::BAD_REQUEST,
                &error,
            );
        }
    };
    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&pk_array) {
        Ok(k) => k,
        Err(_) => {
            let error = ApiError {
                code: ApiErrorCode::BadRequest,
                message: "invalid public key".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::BAD_REQUEST,
                &error,
            );
        }
    };
    if verifying_key.verify(&payload_hash, &signature).is_err() {
        let error = ApiError {
            code: ApiErrorCode::BadRequest,
            message: "signature verification failed".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(
            &state.runtime.firewall,
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    let checkpoint = ferrum_proto::AuditCheckpoint {
        window_start: req.window_start,
        merkle_root: req.merkle_root,
        entry_count: req.entry_count,
        signer_id: req.signer_id,
        signer_key_fingerprint: req.signer_key_fingerprint,
        signed_at: req.signed_at,
        signature: req.signature,
        public_key: req.public_key,
    };

    match state
        .runtime
        .store
        .audit_checkpoints()
        .insert(&checkpoint)
        .await
    {
        Ok(()) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: true,
                window_start: checkpoint.window_start,
                error: None,
                checkpoint: Some(checkpoint),
                current_root: Some(computed.root),
                current_entry_count: Some(computed.entry_count),
            };
            sanitized_response(&state.runtime.firewall, StatusCode::CREATED, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "checkpoint insertion failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to store checkpoint".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            )
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListCheckpointsQuery {
    cursor: Option<String>,
    #[serde(default = "default_checkpoint_limit")]
    limit: u32,
}

fn default_checkpoint_limit() -> u32 {
    50
}

/// List signed checkpoints with cursor-based pagination.
pub(crate) async fn list_checkpoints(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListCheckpointsQuery>,
) -> Response {
    let limit = params.limit.min(200);
    match state
        .runtime
        .store
        .audit_checkpoints()
        .list(params.cursor.as_deref(), limit)
        .await
    {
        Ok((items, next_cursor)) => {
            let response = ferrum_proto::AuditCheckpointListResponse {
                items,
                next_cursor,
                total: 0,
            };
            sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
        }
        Err(e) => {
            tracing::error!(error = %e, "checkpoint list failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to list checkpoints".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            )
        }
    }
}

/// Verify a stored checkpoint for an audit window.
pub(crate) async fn verify_checkpoint(
    State(state): State<Arc<AppState>>,
    Path(window_start): Path<chrono::DateTime<chrono::Utc>>,
) -> Response {
    if window_start.minute() != 0
        || window_start.second() != 0
        || window_start.timestamp_subsec_nanos() != 0
    {
        let error = ApiError {
            code: ApiErrorCode::BadRequest,
            message: "window_start must be aligned to the hour".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({}),
        };
        return sanitized_api_error_response(
            &state.runtime.firewall,
            StatusCode::BAD_REQUEST,
            &error,
        );
    }

    let checkpoint = match state
        .runtime
        .store
        .audit_checkpoints()
        .get(window_start)
        .await
    {
        Ok(Some(cp)) => cp,
        Ok(None) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("no checkpoint found for window".to_string()),
                checkpoint: None,
                current_root: None,
                current_entry_count: None,
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
        Err(e) => {
            tracing::error!(error = %e, "checkpoint get failed");
            let error = ApiError {
                code: ApiErrorCode::Internal,
                message: "failed to retrieve checkpoint".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            return sanitized_api_error_response(
                &state.runtime.firewall,
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
            );
        }
    };

    let computed = match state
        .runtime
        .store
        .audit_merkle_roots()
        .compute_and_cache_root(window_start)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "checkpoint verification failed: merkle root computation error");
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("failed to compute merkle root".to_string()),
                checkpoint: Some(checkpoint.clone()),
                current_root: None,
                current_entry_count: None,
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
    };

    if computed.root != checkpoint.merkle_root || computed.entry_count != checkpoint.entry_count {
        let response = ferrum_proto::AuditCheckpointVerifyResponse {
            valid: false,
            window_start,
            error: Some(format!(
                "merkle root mismatch: checkpoint root={} count={}, current root={} count={}",
                checkpoint.merkle_root, checkpoint.entry_count, computed.root, computed.entry_count
            )),
            checkpoint: Some(checkpoint.clone()),
            current_root: Some(computed.root),
            current_entry_count: Some(computed.entry_count),
        };
        return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
    }

    // Re-verify signature.
    let payload_hash = ferrum_proto::canonical_checkpoint_hash(
        &checkpoint.window_start,
        &checkpoint.merkle_root,
        checkpoint.entry_count,
        &checkpoint.signed_at,
    );
    let sig_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &checkpoint.signature,
    ) {
        Ok(b) => b,
        Err(_) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("invalid signature encoding in stored checkpoint".to_string()),
                checkpoint: Some(checkpoint.clone()),
                current_root: Some(computed.root),
                current_entry_count: Some(computed.entry_count),
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
    };
    let sig_array: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("invalid signature length in stored checkpoint".to_string()),
                checkpoint: Some(checkpoint.clone()),
                current_root: Some(computed.root),
                current_entry_count: Some(computed.entry_count),
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
    };
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    let pk_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &checkpoint.public_key,
    ) {
        Ok(b) => b,
        Err(_) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("invalid public key encoding in stored checkpoint".to_string()),
                checkpoint: Some(checkpoint.clone()),
                current_root: Some(computed.root),
                current_entry_count: Some(computed.entry_count),
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
    };
    let pk_array: [u8; 32] = match pk_bytes.try_into() {
        Ok(a) => a,
        Err(_) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("invalid public key length in stored checkpoint".to_string()),
                checkpoint: Some(checkpoint.clone()),
                current_root: Some(computed.root),
                current_entry_count: Some(computed.entry_count),
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
    };
    let verifying_key = match ed25519_dalek::VerifyingKey::from_bytes(&pk_array) {
        Ok(k) => k,
        Err(_) => {
            let response = ferrum_proto::AuditCheckpointVerifyResponse {
                valid: false,
                window_start,
                error: Some("invalid public key in stored checkpoint".to_string()),
                checkpoint: Some(checkpoint.clone()),
                current_root: Some(computed.root),
                current_entry_count: Some(computed.entry_count),
            };
            return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
        }
    };
    if verifying_key.verify(&payload_hash, &signature).is_err() {
        let response = ferrum_proto::AuditCheckpointVerifyResponse {
            valid: false,
            window_start,
            error: Some("signature verification failed".to_string()),
            checkpoint: Some(checkpoint.clone()),
            current_root: Some(computed.root),
            current_entry_count: Some(computed.entry_count),
        };
        return sanitized_response(&state.runtime.firewall, StatusCode::OK, &response);
    }

    let response = ferrum_proto::AuditCheckpointVerifyResponse {
        valid: true,
        window_start,
        error: None,
        checkpoint: Some(checkpoint),
        current_root: Some(computed.root),
        current_entry_count: Some(computed.entry_count),
    };
    sanitized_response(&state.runtime.firewall, StatusCode::OK, &response)
}

/// Maximum rows allowed in a single export request.
const EXPORT_MAX_ROWS: usize = 10_000;
/// Page size for export pagination loops.
const EXPORT_PAGE_SIZE: u32 = 500;

#[derive(Debug, Deserialize)]
pub(crate) struct ExportAuditLogsQuery {
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "default_export_format")]
    format: String,
}

fn default_export_format() -> String {
    "ndjson".to_string()
}

/// Escape a CSV field per RFC 4180 basic rules.
fn csv_escape_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{}\"", escaped)
    } else {
        s.to_string()
    }
}

/// Export audit logs in NDJSON, JSON, or CSV format.
///
/// Uses bounded pagination to avoid unbounded memory use. Returns 413
/// if the result set exceeds `EXPORT_MAX_ROWS`.
pub(crate) async fn export_audit_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ExportAuditLogsQuery>,
) -> Response {
    let action = params.action.and_then(|s| s.parse::<AuditAction>().ok());
    let resource_type = params
        .resource_type
        .and_then(|s| s.parse::<AuditResourceType>().ok());
    let format = params.format.to_lowercase();

    let repo = state.runtime.store.audit_log();
    let mut all_entries = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        match repo
            .list(
                action,
                resource_type,
                params.resource_id.as_deref(),
                cursor.as_deref(),
                EXPORT_PAGE_SIZE,
                params.since,
                params.until,
            )
            .await
        {
            Ok((items, next_cursor)) => {
                all_entries.extend(items);
                if all_entries.len() > EXPORT_MAX_ROWS {
                    let error = ApiError {
                        code: ApiErrorCode::PayloadTooLarge,
                        message: format!(
                            "export exceeds maximum of {} rows; narrow filters or use pagination",
                            EXPORT_MAX_ROWS
                        ),
                        correlation_id: uuid::Uuid::new_v4().to_string(),
                        retriable: false,
                        details: serde_json::json!({}),
                    };
                    return sanitized_api_error_response(
                        &state.runtime.firewall,
                        StatusCode::PAYLOAD_TOO_LARGE,
                        &error,
                    );
                }
                if next_cursor.is_none() {
                    break;
                }
                cursor = next_cursor;
            }
            Err(e) => {
                tracing::error!(error = %e, "audit log export failed");
                let error = ApiError {
                    code: ApiErrorCode::Internal,
                    message: "failed to export audit logs".to_string(),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                    retriable: false,
                    details: serde_json::json!({}),
                };
                return sanitized_api_error_response(
                    &state.runtime.firewall,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &error,
                );
            }
        }
    }

    match format.as_str() {
        "ndjson" => {
            // Success paths are intentionally raw (no output sanitization) for
            // export fidelity: callers need the exact on-disk audit record
            // (including any control characters in metadata) for verification
            // and downstream tooling.
            let mut body = String::new();
            for entry in &all_entries {
                match serde_json::to_string(entry) {
                    Ok(line) => {
                        body.push_str(&line);
                        body.push('\n');
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "audit log export serialization failed");
                        let error = ApiError {
                            code: ApiErrorCode::Internal,
                            message: "failed to serialize audit log export".to_string(),
                            correlation_id: uuid::Uuid::new_v4().to_string(),
                            retriable: false,
                            details: serde_json::json!({}),
                        };
                        return sanitized_api_error_response(
                            &state.runtime.firewall,
                            StatusCode::INTERNAL_SERVER_ERROR,
                            &error,
                        );
                    }
                }
            }
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
                body,
            )
                .into_response()
        }
        "json" => match serde_json::to_string(&all_entries) {
            // Success path is intentionally raw (no output sanitization) for
            // export fidelity: callers need the exact on-disk audit record
            // (including any control characters in metadata) for verification
            // and downstream tooling.
            Ok(body) => (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response(),
            Err(e) => {
                tracing::error!(error = %e, "audit log export serialization failed");
                let error = ApiError {
                    code: ApiErrorCode::Internal,
                    message: "failed to serialize audit log export".to_string(),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                    retriable: false,
                    details: serde_json::json!({}),
                };
                sanitized_api_error_response(
                    &state.runtime.firewall,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &error,
                )
            }
        },
        "csv" => {
            // Success path is intentionally raw (no output sanitization) for
            // export fidelity: callers need the exact on-disk audit record
            // (including any control characters in metadata) for verification
            // and downstream tooling.
            let mut body = String::from(
                "id,actor_id,action,resource_type,resource_id,result,metadata,created_at,content_hash,previous_hash\n",
            );
            for entry in &all_entries {
                let metadata = entry
                    .metadata
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_default();
                let line = format!(
                    "{},{},{},{},{},{},{},{},{},{}\n",
                    entry.id,
                    csv_escape_field(&entry.actor_id),
                    csv_escape_field(&entry.action.to_string()),
                    csv_escape_field(&entry.resource_type.to_string()),
                    csv_escape_field(&entry.resource_id),
                    csv_escape_field(&entry.result),
                    csv_escape_field(&metadata),
                    csv_escape_field(&entry.created_at.to_rfc3339()),
                    csv_escape_field(entry.content_hash.as_deref().unwrap_or("")),
                    csv_escape_field(entry.previous_hash.as_deref().unwrap_or("")),
                );
                body.push_str(&line);
            }
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/csv")],
                body,
            )
                .into_response()
        }
        _ => {
            let error = ApiError {
                code: ApiErrorCode::BadRequest,
                message: format!(
                    "invalid export format '{}': expected ndjson, json, or csv",
                    format
                ),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            };
            sanitized_api_error_response(&state.runtime.firewall, StatusCode::BAD_REQUEST, &error)
        }
    }
}

/// Append an audit log entry. Returns the store error so callers can decide
/// whether to fail closed based on configuration.
pub(crate) async fn append_audit(
    store: &Arc<dyn StoreFacade>,
    actor_id: &str,
    action: AuditAction,
    resource_type: AuditResourceType,
    resource_id: &str,
    result: &str,
    metadata: Option<serde_json::Value>,
) -> Result<(), StoreError> {
    let entry = AuditLogEntry {
        id: 0,
        actor_id: actor_id.to_string(),
        action,
        resource_type,
        resource_id: resource_id.to_string(),
        result: result.to_string(),
        metadata,
        created_at: Utc::now(),
        content_hash: None,
        previous_hash: None,
    };
    store.audit_log().append(&entry).await
}

/// Append an audit log entry respecting `audit_fail_closed`.
///
/// - When `audit_fail_closed` is false (default): logs the error and returns Ok.
/// - When `audit_fail_closed` is true: returns a 503 `ApiProblem` on store error,
///   increments the `audit_fail_closed_rejections` metric, and blocks the action.
pub(crate) async fn append_audit_checked(
    state: &Arc<AppState>,
    actor_id: &str,
    action: AuditAction,
    resource_type: AuditResourceType,
    resource_id: &str,
    result: &str,
    metadata: Option<serde_json::Value>,
    route: Option<GovernanceRoute>,
) -> Result<(), ApiProblem> {
    match append_audit(
        &state.runtime.store,
        actor_id,
        action,
        resource_type,
        resource_id,
        result,
        metadata,
    )
    .await
    {
        Ok(()) => Ok(()),
        Err(e) => {
            if state.server_config.audit_fail_closed {
                tracing::error!(error = %e, route = ?route, "audit append failed in fail-closed mode; rejecting request");
                state
                    .metrics
                    .audit_fail_closed_rejections
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ApiProblem::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    ApiErrorCode::Internal,
                    "audit append failed: action blocked by fail-closed policy",
                ))
            } else {
                tracing::warn!(error = %e, route = ?route, "failed to append audit log entry");
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitoring::GovernanceRoute;
    use crate::state::{AppState, GatewayRuntime, ServerConfig};
    use ferrum_proto::{AuditAction, AuditLogEntry, AuditResourceType};
    use ferrum_store::{SqliteStore, StoreError, StoreFacade, repos::AuditLogRepo};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    /// A test-only StoreFacade that wraps a real store but makes audit_log().append() fail.
    struct FailingAuditStoreFacade {
        inner: Arc<dyn StoreFacade>,
    }

    impl FailingAuditStoreFacade {
        fn new(inner: Arc<dyn StoreFacade>) -> Self {
            Self { inner }
        }
    }

    #[async_trait::async_trait]
    impl StoreFacade for FailingAuditStoreFacade {
        fn capabilities(&self) -> Arc<dyn ferrum_store::repos::CapabilityRepo> {
            self.inner.capabilities()
        }
        fn executions(&self) -> Arc<dyn ferrum_store::repos::ExecutionRepo> {
            self.inner.executions()
        }
        fn rollback_contracts(&self) -> Arc<dyn ferrum_store::repos::RollbackRepo> {
            self.inner.rollback_contracts()
        }
        fn lifecycle_outbox(&self) -> Arc<dyn ferrum_store::repos::LifecycleOutboxRepo> {
            self.inner.lifecycle_outbox()
        }
        fn approvals(&self) -> Arc<dyn ferrum_store::repos::ApprovalRepo> {
            self.inner.approvals()
        }
        fn provenance(&self) -> Arc<dyn ferrum_store::repos::ProvenanceRepo> {
            self.inner.provenance()
        }
        fn ledger(&self) -> Arc<dyn ferrum_store::repos::LedgerRepo> {
            self.inner.ledger()
        }
        fn intents(&self) -> Arc<dyn ferrum_store::repos::IntentRepo> {
            self.inner.intents()
        }
        fn proposals(&self) -> Arc<dyn ferrum_store::repos::ProposalRepo> {
            self.inner.proposals()
        }
        fn policy_bundles(&self) -> Arc<dyn ferrum_store::repos::PolicyBundleRepo> {
            self.inner.policy_bundles()
        }
        fn tokens(&self) -> Arc<dyn ferrum_store::repos::TokenRepo> {
            self.inner.tokens()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            Arc::new(FailingAuditLogRepo)
        }
        fn audit_merkle_roots(&self) -> Arc<dyn ferrum_store::repos::AuditMerkleRootRepo> {
            self.inner.audit_merkle_roots()
        }
        fn audit_checkpoints(&self) -> Arc<dyn ferrum_store::repos::AuditCheckpointRepo> {
            self.inner.audit_checkpoints()
        }
        fn agents(&self) -> Arc<dyn ferrum_store::repos::AgentRepo> {
            self.inner.agents()
        }
        fn write_queue_depth(&self) -> usize {
            self.inner.write_queue_depth()
        }
        async fn health_check(&self) -> ferrum_store::Result<()> {
            self.inner.health_check().await
        }
    }

    struct FailingAuditLogRepo;

    #[async_trait::async_trait]
    impl AuditLogRepo for FailingAuditLogRepo {
        async fn append(&self, _entry: &AuditLogEntry) -> ferrum_store::Result<()> {
            Err(StoreError::Other(
                "audit append failure for testing".to_string(),
            ))
        }
        async fn list(
            &self,
            _action: Option<AuditAction>,
            _resource_type: Option<AuditResourceType>,
            _resource_id: Option<&str>,
            _cursor: Option<&str>,
            _limit: u32,
            _since: Option<chrono::DateTime<chrono::Utc>>,
            _until: Option<chrono::DateTime<chrono::Utc>>,
        ) -> ferrum_store::Result<(Vec<AuditLogEntry>, Option<String>)> {
            Ok((Vec::new(), None))
        }
        async fn verify_chain(&self) -> ferrum_store::Result<()> {
            Ok(())
        }
    }

    async fn test_runtime() -> GatewayRuntime {
        use ferrum_cap::InMemoryCapabilityService;
        use ferrum_pdp::StaticPdpEngine;
        use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};

        let pdp = Arc::new(StaticPdpEngine);
        let cap = Arc::new(InMemoryCapabilityService::default());
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![])
    }

    #[tokio::test]
    async fn test_append_audit_returns_ok_on_success() {
        let runtime = test_runtime().await;
        let store = runtime.store.clone();
        let result = append_audit(
            &store,
            "test-actor",
            AuditAction::TokenCreate,
            AuditResourceType::Token,
            "token-1",
            "success",
            None,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_append_audit_returns_err_on_failure() {
        let runtime = test_runtime().await;
        let failing_store =
            Arc::new(FailingAuditStoreFacade::new(runtime.store.clone())) as Arc<dyn StoreFacade>;
        let result = append_audit(
            &failing_store,
            "test-actor",
            AuditAction::TokenCreate,
            AuditResourceType::Token,
            "token-1",
            "success",
            None,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_append_audit_checked_best_effort_ignores_failure() {
        let runtime = test_runtime().await;
        let failing_store =
            Arc::new(FailingAuditStoreFacade::new(runtime.store.clone())) as Arc<dyn StoreFacade>;
        let mut runtime = runtime;
        runtime.store = failing_store;
        let state = AppState::test_new(runtime, ServerConfig::default());

        let result = append_audit_checked(
            &state,
            "test-actor",
            AuditAction::TokenCreate,
            AuditResourceType::Token,
            "token-1",
            "success",
            None,
            Some(GovernanceRoute::AgentsCreate),
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_append_audit_checked_fail_closed_returns_503() {
        let runtime = test_runtime().await;
        let failing_store =
            Arc::new(FailingAuditStoreFacade::new(runtime.store.clone())) as Arc<dyn StoreFacade>;
        let mut runtime = runtime;
        runtime.store = failing_store;
        let config = ServerConfig {
            audit_fail_closed: true,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        let result = append_audit_checked(
            &state,
            "test-actor",
            AuditAction::TokenCreate,
            AuditResourceType::Token,
            "token-1",
            "success",
            None,
            Some(GovernanceRoute::AgentsCreate),
        )
        .await;
        assert!(result.is_err());
        let problem = result.unwrap_err();
        assert_eq!(problem.1, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_append_audit_checked_fail_closed_increments_metric() {
        let runtime = test_runtime().await;
        let failing_store =
            Arc::new(FailingAuditStoreFacade::new(runtime.store.clone())) as Arc<dyn StoreFacade>;
        let mut runtime = runtime;
        runtime.store = failing_store;
        let config = ServerConfig {
            audit_fail_closed: true,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        let before = state
            .metrics
            .audit_fail_closed_rejections
            .load(Ordering::Relaxed);
        let _ = append_audit_checked(
            &state,
            "test-actor",
            AuditAction::TokenCreate,
            AuditResourceType::Token,
            "token-1",
            "success",
            None,
            Some(GovernanceRoute::AgentsCreate),
        )
        .await;
        let after = state
            .metrics
            .audit_fail_closed_rejections
            .load(Ordering::Relaxed);
        assert_eq!(after, before + 1);
    }
}
