//! Approval governance handlers and the pagination helpers that only they use.
//!
//! Routes:
//! - `GET    /v1/approvals`                      -> [`list_approvals`]
//! - `GET    /v1/approvals/{approval_id}`        -> [`get_approval`]
//! - `POST   /v1/approvals/{approval_id}/resolve`-> [`resolve_approval`]
//!
//! Pagination helpers ([`PaginationParams`], [`encode_cursor`], [`decode_cursor`])
//! and the local `parse_approval_id` / `parse_proposal_id` UUID parsers are
//! also defined here because they are only consumed by these three handlers.
//!
//! All success paths increment the `GovernanceRoute` counter and apply the
//! output sanitizer to the response payload.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiError, ApiErrorCode, ApprovalId, ApprovalListEnvelope,
    ApprovalResolveRequest, ApprovalState, AuditAction, AuditResourceType, EventId, HashChainRef,
    MfaFactorStatus, ObjectRef, ObjectType, ProvenanceEvent, ProvenanceEventKind,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::state::AppState;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
pub(crate) struct PaginationParams {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    proposal_id: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

impl PaginationParams {
    fn limit(&self) -> Result<u32, ApiProblem> {
        match self.limit {
            Some(l) if l > MAX_LIMIT => Err(ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("limit exceeds maximum of {}", MAX_LIMIT),
            )),
            Some(l) => Ok(l),
            None => Ok(DEFAULT_LIMIT),
        }
    }

    fn offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

/// Cursor encoding for stable DESC ordering.
/// The cursor encodes (created_at_rfc3339, approval_id) to allow keyset pagination.
fn encode_cursor(created_at: chrono::DateTime<chrono::Utc>, approval_id: ApprovalId) -> String {
    let cursor_data = format!("{}:{}", created_at.to_rfc3339(), approval_id);
    URL_SAFE_NO_PAD.encode(cursor_data.as_bytes())
}

/// Cursor decoding for keyset pagination.
/// Returns (created_at, approval_id) on success.
fn decode_cursor(cursor: &str) -> Result<(chrono::DateTime<chrono::Utc>, ApprovalId), ApiProblem> {
    let decoded = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor format",
        )
    })?;
    let decoded_str = String::from_utf8(decoded).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor encoding",
        )
    })?;
    let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor structure",
        ));
    }
    let created_at = chrono::DateTime::parse_from_rfc3339(parts[0])
        .map_err(|_| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "invalid cursor timestamp",
            )
        })?
        .with_timezone(&chrono::Utc);
    let approval_id: uuid::Uuid = parts[1].parse().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor approval_id",
        )
    })?;
    Ok((created_at, ApprovalId(approval_id)))
}

fn parse_approval_id(value: &str) -> Result<ApprovalId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid approval uuid",
        )
    })?;
    Ok(ApprovalId(parsed))
}

fn parse_proposal_id(value: &str) -> Result<ferrum_proto::ProposalId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "proposal_id is not a valid uuid",
        )
    })?;
    Ok(ferrum_proto::ProposalId(parsed))
}

pub(crate) async fn list_approvals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ApprovalListEnvelope>, ApiProblem> {
    let limit = params.limit().map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::Approvals, e)
    })?;

    // Determine whether to use cursor-based or offset-based pagination
    let (items, next_cursor) = if let Some(ref cursor) = params.cursor {
        // Cursor-based pagination path
        let (created_at, approval_id) = decode_cursor(cursor).map_err(|e| {
            state
                .metrics
                .record_governance_error(GovernanceRoute::Approvals, e)
        })?;
        let limit_plus_one = limit + 1; // Fetch one extra to determine if there are more

        let approvals = if let Some(ref proposal_id) = params.proposal_id {
            // Validate proposal_id format - fail closed on invalid UUID
            let parsed_proposal_id = parse_proposal_id(proposal_id).map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::Approvals, e)
            })?;
            state
                .runtime
                .store
                .approvals()
                .list_pending_by_proposal_cursor(
                    parsed_proposal_id,
                    created_at,
                    approval_id,
                    limit_plus_one,
                )
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        } else {
            state
                .runtime
                .store
                .approvals()
                .list_pending_cursor(created_at, approval_id, limit_plus_one)
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        };

        // Determine if there are more results
        let has_more = approvals.len() > limit as usize;
        let items: Vec<_> = approvals.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            items
                .last()
                .map(|a| encode_cursor(a.created_at, a.approval_id))
        } else {
            None
        };
        (items, next_cursor)
    } else {
        // Offset-based pagination path (for backwards compatibility)
        let offset = params.offset();
        let approvals = if let Some(ref proposal_id) = params.proposal_id {
            // Validate proposal_id format - fail closed on invalid UUID
            let parsed_proposal_id = parse_proposal_id(proposal_id).map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::Approvals, e)
            })?;
            state
                .runtime
                .store
                .approvals()
                .list_pending_by_proposal_paginated(parsed_proposal_id, limit, offset)
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        } else {
            state
                .runtime
                .store
                .approvals()
                .list_pending_paginated(limit, offset)
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        };
        // Offset pagination cannot determine next_cursor reliably, so we return None
        (approvals, None)
    };

    governance_ok!(
        state,
        GovernanceRoute::Approvals,
        Ok(Json(ApprovalListEnvelope { items, next_cursor }))
    )
}

pub(crate) async fn get_approval(
    State(state): State<Arc<AppState>>,
    Path(approval_id): Path<String>,
) -> Result<Json<ferrum_proto::ApprovalRequest>, ApiProblem> {
    let approval_id = parse_approval_id(&approval_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ApprovalsApprovalId, e)
    })?;
    let approval = state
        .runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsApprovalId,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsApprovalId,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "approval not found",
                ),
            )
        })?;
    governance_ok!(
        state,
        GovernanceRoute::ApprovalsApprovalId,
        Ok(Json(approval))
    )
}

pub(crate) async fn resolve_approval(
    State(state): State<Arc<AppState>>,
    Path(approval_id): Path<String>,
    Json(request): Json<ApprovalResolveRequest>,
) -> Result<Json<ferrum_proto::ApprovalRequest>, ApiProblem> {
    let approval_id = parse_approval_id(&approval_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ApprovalsResolve, e)
    })?;

    // ADR008 Phase 2: MFA verification for approval resolve.
    // When enabled, require a valid active TOTP factor from the resolver.
    if state.server_config.approval_mfa_required {
        let mfa_factor = match request.mfa_factor {
            Some(ref f) => f,
            None => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::MfaRequired,
                        "MFA factor is required for approval resolve",
                    )
                );
            }
        };

        let key_hex = match &state.server_config.mfa_secret_key {
            Some(k) => k,
            None => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::Misconfigured,
                        "mfa_secret_key is not configured",
                    )
                );
            }
        };

        let key_bytes = match crate::mfa::decode_hex_key(key_hex) {
            Ok(b) => b,
            Err(msg) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(StatusCode::FORBIDDEN, ApiErrorCode::Misconfigured, msg,)
                );
            }
        };

        let record = match state
            .runtime
            .store
            .mfa_credentials()
            .get(mfa_factor.id)
            .await
        {
            Ok(Some(r)) => r,
            Ok(None) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::MfaInvalid,
                        "MFA factor not found",
                    )
                );
            }
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::internal(anyhow::Error::from(e))
                );
            }
        };

        if record.agent_id != request.actor.actor_id {
            return governance_err!(
                state,
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::new(
                    StatusCode::FORBIDDEN,
                    ApiErrorCode::MfaInvalid,
                    "MFA factor does not belong to the actor",
                )
            );
        }

        if record.status != MfaFactorStatus::Active {
            return governance_err!(
                state,
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::new(
                    StatusCode::FORBIDDEN,
                    ApiErrorCode::MfaInvalid,
                    "MFA factor is not active",
                )
            );
        }

        // Check lockout before attempting verification.
        if let Some(locked_until) = record.locked_until {
            let now = chrono::Utc::now();
            if locked_until > now {
                let retry_after_secs = (locked_until - now).num_seconds().max(0) as u64;
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem(
                        ApiError {
                            code: ApiErrorCode::MfaLocked,
                            message: "MFA factor is locked due to too many failed attempts"
                                .to_string(),
                            correlation_id: uuid::Uuid::new_v4().to_string(),
                            retriable: false,
                            details: serde_json::json!({"retry_after_seconds": retry_after_secs}),
                        },
                        StatusCode::FORBIDDEN,
                    )
                );
            }
        }

        let secret = match crate::mfa::decrypt_secret(
            &key_bytes,
            &record.encrypted_secret,
            &record.secret_nonce,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "mfa decrypt_secret failed during approval resolve");
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::MfaInvalid,
                        "failed to verify MFA factor",
                    )
                );
            }
        };

        let code = match mfa_factor.code {
            Some(ref c) => c,
            None => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::MfaInvalid,
                        "MFA verification code is missing",
                    )
                );
            }
        };

        let now = chrono::Utc::now().timestamp() as u64;
        let matched_counter = match crate::mfa::verify_totp_code_with_counter(&secret, code, now) {
            Ok(c) => c,
            Err(_) => {
                let repo = state.runtime.store.mfa_credentials();
                let locked = repo
                    .record_failed_attempt(
                        mfa_factor.id,
                        state.server_config.mfa_lockout_max_attempts,
                        state.server_config.mfa_lockout_duration_secs,
                    )
                    .await;
                if let Ok(true) = locked {
                    let retry_after_secs = state.server_config.mfa_lockout_duration_secs;
                    return governance_err!(
                        state,
                        GovernanceRoute::ApprovalsResolve,
                        ApiProblem(
                            ApiError {
                                code: ApiErrorCode::MfaLocked,
                                message: "MFA factor is locked due to too many failed attempts"
                                    .to_string(),
                                correlation_id: uuid::Uuid::new_v4().to_string(),
                                retriable: false,
                                details: serde_json::json!({"retry_after_seconds": retry_after_secs}),
                            },
                            StatusCode::FORBIDDEN,
                        )
                    );
                }
                if let Err(ref e) = locked {
                    tracing::warn!(error = %e, "record_failed_attempt failed during approval resolve");
                }
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::MfaInvalid,
                        "MFA verification code is invalid",
                    )
                );
            }
        };

        // Reset lockout on successful verification.
        if let Err(e) = state
            .runtime
            .store
            .mfa_credentials()
            .reset_lockout(mfa_factor.id)
            .await
        {
            tracing::warn!(error = %e, "reset_lockout failed during approval resolve");
        }

        // Record successful use with counter for replay protection (CAS in DB)
        match state
            .runtime
            .store
            .mfa_credentials()
            .record_use(mfa_factor.id, matched_counter)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::MfaInvalid,
                        "MFA verification code is invalid",
                    )
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "mfa record_use failed during approval resolve");
                return governance_err!(
                    state,
                    GovernanceRoute::ApprovalsResolve,
                    ApiProblem::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ApiErrorCode::Internal,
                        "MFA state update failed",
                    )
                );
            }
        }
    }

    // Fetch the approval from the store
    let approval = state
        .runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "approval not found",
                ),
            )
        })?;

    // Check if approval is already terminal
    if !matches!(approval.state, ApprovalState::Pending) {
        return governance_err!(
            state,
            GovernanceRoute::ApprovalsResolve,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "approval is in terminal state {:?}, cannot resolve",
                    approval.state
                ),
            )
        );
    }

    // Check if approval has expired
    if approval.expires_at < Utc::now() {
        return governance_err!(
            state,
            GovernanceRoute::ApprovalsResolve,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "approval has expired, cannot resolve"
            )
        );
    }

    // Map approve to target state
    let target_state = if request.approve {
        ApprovalState::Granted
    } else {
        ApprovalState::Denied
    };

    // Call store to resolve the approval (validates transition)
    state
        .runtime
        .store
        .approvals()
        .resolve(approval_id, target_state.clone())
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Audit log: approval resolved
    if let Err(problem) = crate::audit::append_audit_checked(
        &state,
        &request.actor.actor_id,
        AuditAction::ApprovalResolve,
        AuditResourceType::Approval,
        &approval_id.to_string(),
        "success",
        Some(serde_json::json!({
            "approved": request.approve,
            "reason": request.reason,
        })),
        Some(GovernanceRoute::ApprovalsResolve),
    )
    .await
    {
        return governance_err!(state, GovernanceRoute::ApprovalsResolve, problem);
    }

    // Fetch the updated approval
    let updated_approval = state
        .runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "approval not found after resolve",
                ),
            )
        })?;

    // Emit gateway-owned provenance event
    let event_kind = if request.approve {
        ProvenanceEventKind::ApprovalGranted
    } else {
        ProvenanceEventKind::ApprovalDenied
    };
    let event_kind_for_summary = event_kind.clone();

    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "actor_id".to_string(),
        serde_json::json!(request.actor.actor_id),
    );
    if let Some(reason) = &request.reason {
        metadata.insert("reason".to_string(), serde_json::json!(reason));
    }

    let provenance_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: event_kind,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Approval,
            object_id: approval_id.to_string(),
            summary: Some(format!(
                "Approval {:?} for proposal",
                event_kind_for_summary
            )),
        },
        intent_id: Some(approval.intent_id),
        proposal_id: Some(approval.proposal_id),
        execution_id: approval.execution_id,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata,
        source_runtime_id: None,
    };

    state
        .runtime
        .store
        .provenance()
        .append_event(&provenance_event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ApprovalsResolve,
        Ok(Json(updated_approval))
    )
}
