use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use ferrum_proto::{
    ApiError, ApiErrorCode, AuditAction, AuditResourceType, LifecycleOutboxId,
    LifecycleOutboxListResponse, LifecycleOutboxResolveRequest, LifecycleOutboxResolveResponse,
    LifecycleOutboxRetryRequest, LifecycleOutboxRetryResponse, LifecycleOutboxStatus,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    audit::append_audit,
    response::{sanitized_api_error_response, sanitized_response},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ListLifecycleOutboxQuery {
    status: Option<String>,
    limit: Option<u32>,
}

pub(crate) async fn list_lifecycle_outbox(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListLifecycleOutboxQuery>,
) -> Response {
    let limit = params.limit.unwrap_or(50).min(200);
    let status = match params.status.as_deref() {
        None | Some("needs_operator_review") => LifecycleOutboxStatus::NeedsOperatorReview,
        Some("pending") | Some("pending_provenance") => LifecycleOutboxStatus::PendingProvenance,
        Some("provenance_written") => LifecycleOutboxStatus::ProvenanceWritten,
        Some("reconciled") => LifecycleOutboxStatus::Reconciled,
        Some(other) => {
            let error = ApiError {
                code: ApiErrorCode::ValidationError,
                message: format!("unsupported lifecycle outbox status filter: {}", other),
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

    match state
        .runtime
        .store
        .lifecycle_outbox()
        .list_by_status(status, limit)
        .await
    {
        Ok(items) => {
            let total = items.len();
            sanitized_response(
                &state.runtime.firewall,
                StatusCode::OK,
                &LifecycleOutboxListResponse { items, total },
            )
        }
        Err(e) => internal_error(&state, "failed to list lifecycle outbox records", e),
    }
}

pub(crate) async fn get_lifecycle_outbox(
    State(state): State<Arc<AppState>>,
    Path(outbox_id): Path<LifecycleOutboxId>,
) -> Response {
    match state.runtime.store.lifecycle_outbox().get(outbox_id).await {
        Ok(Some(record)) => sanitized_response(&state.runtime.firewall, StatusCode::OK, &record),
        Ok(None) => not_found(&state, outbox_id),
        Err(e) => internal_error(&state, "failed to get lifecycle outbox record", e),
    }
}

pub(crate) async fn retry_lifecycle_outbox(
    State(state): State<Arc<AppState>>,
    Path(outbox_id): Path<LifecycleOutboxId>,
    Json(req): Json<LifecycleOutboxRetryRequest>,
) -> Response {
    let record = match state
        .runtime
        .store
        .lifecycle_outbox()
        .reset_for_retry(outbox_id, req.actor_id.clone(), req.reason.clone())
        .await
    {
        Ok(Some(record)) => record,
        Ok(None) => return not_found(&state, outbox_id),
        Err(e) => return internal_error(&state, "failed to reset lifecycle outbox record", e),
    };

    let report = match ferrum_store::reconcile_lifecycle_outbox(&state.runtime.store, 1_000).await {
        Ok(report) => serde_json::json!({
            "scanned": report.scanned,
            "already_reconciled": report.already_reconciled,
            "repaired_missing_provenance": report.repaired_missing_provenance,
            "needs_operator_review": report.needs_operator_review,
        }),
        Err(e) => {
            tracing::warn!(error = %e, %outbox_id, "operator retry reconciliation failed");
            serde_json::json!({ "error": e.to_string() })
        }
    };

    let refreshed = state
        .runtime
        .store
        .lifecycle_outbox()
        .get(outbox_id)
        .await
        .ok()
        .flatten()
        .unwrap_or(record);

    append_audit(
        &state.runtime.store,
        &req.actor_id,
        AuditAction::LifecycleOutboxRetry,
        AuditResourceType::LifecycleOutbox,
        &outbox_id.to_string(),
        "success",
        Some(serde_json::json!({
            "reason": req.reason,
            "reconciliation_report": report,
        })),
    )
    .await;

    sanitized_response(
        &state.runtime.firewall,
        StatusCode::OK,
        &LifecycleOutboxRetryResponse {
            record: refreshed,
            reconciliation_report: report,
        },
    )
}

pub(crate) async fn resolve_lifecycle_outbox(
    State(state): State<Arc<AppState>>,
    Path(outbox_id): Path<LifecycleOutboxId>,
    Json(req): Json<LifecycleOutboxResolveRequest>,
) -> Response {
    if req.reason.trim().is_empty() {
        let error = ApiError {
            code: ApiErrorCode::ValidationError,
            message: "reason is required to resolve lifecycle outbox operator review".to_string(),
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
        .lifecycle_outbox()
        .mark_operator_resolved(outbox_id, req.actor_id.clone(), req.reason.clone())
        .await
    {
        Ok(Some(record)) => {
            append_audit(
                &state.runtime.store,
                &req.actor_id,
                AuditAction::LifecycleOutboxResolve,
                AuditResourceType::LifecycleOutbox,
                &outbox_id.to_string(),
                "success",
                Some(serde_json::json!({ "reason": req.reason })),
            )
            .await;
            sanitized_response(
                &state.runtime.firewall,
                StatusCode::OK,
                &LifecycleOutboxResolveResponse { record },
            )
        }
        Ok(None) => not_found(&state, outbox_id),
        Err(e) => internal_error(&state, "failed to resolve lifecycle outbox record", e),
    }
}

fn not_found(state: &Arc<AppState>, outbox_id: LifecycleOutboxId) -> Response {
    let error = ApiError {
        code: ApiErrorCode::NotFound,
        message: "lifecycle outbox record not found".to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        retriable: false,
        details: serde_json::json!({ "outbox_id": outbox_id.to_string() }),
    };
    sanitized_api_error_response(&state.runtime.firewall, StatusCode::NOT_FOUND, &error)
}

fn internal_error(state: &Arc<AppState>, message: &str, error: impl std::fmt::Display) -> Response {
    tracing::error!(error = %error, "{message}");
    let error = ApiError {
        code: ApiErrorCode::Internal,
        message: message.to_string(),
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
