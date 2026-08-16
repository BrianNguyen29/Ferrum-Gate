//! Quarantine hold governance handlers.
//!
//! Routes:
//! - `GET    /v1/quarantines`                      -> [`list_quarantine_holds`]
//! - `GET    /v1/quarantines/{hold_id}`            -> [`get_quarantine_hold`]
//! - `POST   /v1/quarantines/{hold_id}/resolve`    -> [`resolve_quarantine_hold`]

use axum::{
    Json,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ApiError, ApiErrorCode, AuditAction, AuditResourceType, EventId, HashChainRef, MfaFactorStatus,
    ObjectRef, ObjectType, ProvenanceEvent, ProvenanceEventKind, QuarantineHoldId,
    QuarantineHoldState, QuarantineResolveRequest,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AuthActor;
use crate::macros::{governance_err, governance_ok};
use crate::mfa::{TotpVerifyResult, check_agent_mfa_lockout, reset_mfa_lockout_after_success};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::state::AppState;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
pub(crate) struct ListParams {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    proposal_id: Option<String>,
}

impl ListParams {
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

fn parse_hold_id(value: &str) -> Result<QuarantineHoldId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid quarantine hold uuid",
        )
    })?;
    Ok(QuarantineHoldId(parsed))
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

pub(crate) async fn list_quarantine_holds(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Json<ferrum_proto::QuarantineListEnvelope>, ApiProblem> {
    let limit = params.limit().map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::Quarantines, e)
    })?;
    let offset = params.offset();

    let (items, next_cursor) = if let Some(ref proposal_id) = params.proposal_id {
        let parsed = parse_proposal_id(proposal_id).map_err(|e| {
            state
                .metrics
                .record_governance_error(GovernanceRoute::Quarantines, e)
        })?;
        let hold = state
            .runtime
            .store
            .quarantine_holds()
            .get_by_proposal(parsed)
            .await
            .map_err(|e| {
                state.metrics.record_governance_error(
                    GovernanceRoute::Quarantines,
                    ApiProblem::internal(anyhow::Error::from(e)),
                )
            })?;
        (hold.into_iter().collect(), None)
    } else {
        let (items, next_cursor) = state
            .runtime
            .store
            .quarantine_holds()
            .list_pending(limit, offset)
            .await
            .map_err(|e| {
                state.metrics.record_governance_error(
                    GovernanceRoute::Quarantines,
                    ApiProblem::internal(anyhow::Error::from(e)),
                )
            })?;
        (items, next_cursor)
    };

    governance_ok!(
        state,
        GovernanceRoute::Quarantines,
        Ok(Json(ferrum_proto::QuarantineListEnvelope {
            items,
            next_cursor,
        }))
    )
}

pub(crate) async fn get_quarantine_hold(
    State(state): State<Arc<AppState>>,
    Path(hold_id): Path<String>,
) -> Result<Json<ferrum_proto::QuarantineHold>, ApiProblem> {
    let hold_id = parse_hold_id(&hold_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::QuarantinesHoldId, e)
    })?;
    let hold = state
        .runtime
        .store
        .quarantine_holds()
        .get(hold_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesHoldId,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesHoldId,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "quarantine hold not found",
                ),
            )
        })?;
    governance_ok!(state, GovernanceRoute::QuarantinesHoldId, Ok(Json(hold)))
}

fn mfa_locked_problem(retry_after_secs: u64) -> ApiProblem {
    ApiProblem(
        ApiError {
            code: ApiErrorCode::MfaLocked,
            message: "MFA is locked due to too many failed attempts".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            retriable: false,
            details: serde_json::json!({"retry_after_seconds": retry_after_secs}),
        },
        StatusCode::FORBIDDEN,
    )
}

fn totp_verify_result_to_problem(result: TotpVerifyResult) -> ApiProblem {
    match result {
        TotpVerifyResult::Locked {
            retry_after_seconds,
        } => mfa_locked_problem(retry_after_seconds),
        TotpVerifyResult::Invalid => ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::MfaInvalid,
            "MFA verification code is invalid",
        ),
        TotpVerifyResult::Internal => {
            ApiProblem::internal(anyhow::anyhow!("MFA verification failed"))
        }
        TotpVerifyResult::Success { .. } => {
            unreachable!("totp_verify_result_to_problem should not be called with Success")
        }
    }
}

async fn verify_mfa_factor(
    state: &AppState,
    request: &QuarantineResolveRequest,
    effective_actor_id: &str,
) -> Result<(), ApiProblem> {
    let mfa_factor = match request.mfa_factor {
        Some(ref f) => f,
        None => {
            return Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::MfaRequired,
                "MFA factor is required for quarantine resolve",
            ));
        }
    };

    let key_hex = match &state.server_config.mfa_secret_key {
        Some(k) => k,
        None => {
            return Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::Misconfigured,
                "mfa_secret_key is not configured",
            ));
        }
    };

    let key_bytes = match crate::mfa::decode_hex_key(key_hex) {
        Ok(b) => b,
        Err(msg) => {
            return Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::Misconfigured,
                msg,
            ));
        }
    };

    let code = match mfa_factor.code {
        Some(ref c) => c,
        None => {
            return Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::MfaInvalid,
                "MFA verification code is missing",
            ));
        }
    };

    // Check agent-level lockout before fetching the factor.
    match check_agent_mfa_lockout(state.runtime.store.mfa_credentials(), effective_actor_id).await {
        Ok(Some(retry_after_secs)) => return Err(mfa_locked_problem(retry_after_secs)),
        Ok(None) => {}
        Err(e) => return Err(ApiProblem::internal(anyhow::Error::from(e))),
    }

    let record = match state
        .runtime
        .store
        .mfa_credentials()
        .get(mfa_factor.id)
        .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::MfaInvalid,
                "MFA factor not found",
            ));
        }
        Err(e) => {
            return Err(ApiProblem::internal(anyhow::Error::from(e)));
        }
    };

    if record.agent_id != effective_actor_id {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::MfaInvalid,
            "MFA factor does not belong to the actor",
        ));
    }

    if record.status != MfaFactorStatus::Active {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::MfaInvalid,
            "MFA factor is not active",
        ));
    }

    let result = crate::mfa::verify_totp_with_lockout(
        state.runtime.store.mfa_credentials(),
        &key_bytes,
        &record.agent_id,
        &record,
        code,
        state.server_config.mfa_lockout_max_attempts,
        state.server_config.mfa_lockout_duration_secs,
    )
    .await;

    let matched_counter = match result {
        TotpVerifyResult::Success { counter } => counter,
        other => return Err(totp_verify_result_to_problem(other)),
    };

    match state
        .runtime
        .store
        .mfa_credentials()
        .record_use(mfa_factor.id, matched_counter)
        .await
    {
        Ok(true) => {
            if let Err(e) = reset_mfa_lockout_after_success(
                state.runtime.store.mfa_credentials(),
                &record.agent_id,
                record.mfa_factor_id,
            )
            .await
            {
                tracing::error!(error = %e, "mfa lockout reset failed during quarantine resolve");
                return Err(ApiProblem::internal(anyhow::Error::from(e)));
            }
            Ok(())
        }
        Ok(false) => Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::MfaInvalid,
            "MFA verification code is invalid",
        )),
        Err(e) => {
            tracing::error!(error = %e, "mfa record_use failed during quarantine resolve");
            Err(ApiProblem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Internal,
                "MFA state update failed",
            ))
        }
    }
}

pub(crate) async fn resolve_quarantine_hold(
    State(state): State<Arc<AppState>>,
    auth_actor: Option<Extension<AuthActor>>,
    Path(hold_id): Path<String>,
    Json(request): Json<QuarantineResolveRequest>,
) -> Result<Json<ferrum_proto::QuarantineHold>, ApiProblem> {
    let hold_id = parse_hold_id(&hold_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::QuarantinesResolve, e)
    })?;

    // Authenticated actor binding: when an AuthActor is present (Scoped/OIDC/Agent),
    // it is authoritative. Reject any request-body actor asserting a different
    // identity before MFA or state mutation (fail closed). When no AuthActor is
    // present (Bearer/Disabled), retain the request-body actor for compatibility.
    //
    // The authenticated ActorRef is fully server-derived: actor_type is mapped from
    // the auth source and display_name is cleared, so a forged request body cannot
    // persist or provenance an attacker-controlled actor_type/display_name. An empty
    // body actor_id is treated as omitted for compatibility.
    let effective_actor = match &auth_actor {
        Some(Extension(actor)) => {
            if !request.actor.actor_id.is_empty() && request.actor.actor_id != actor.actor_id {
                return governance_err!(
                    state,
                    GovernanceRoute::QuarantinesResolve,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::Forbidden,
                        "request actor does not match authenticated actor",
                    )
                );
            }
            let actor_type = if actor.source == "agent" {
                ferrum_proto::ActorType::Agent
            } else {
                ferrum_proto::ActorType::Operator
            };
            ferrum_proto::ActorRef {
                actor_type,
                actor_id: actor.actor_id.clone(),
                display_name: None,
            }
        }
        None => request.actor.clone(),
    };

    if state.server_config.approval_mfa_required {
        if let Err(problem) = verify_mfa_factor(&state, &request, &effective_actor.actor_id).await {
            return governance_err!(state, GovernanceRoute::QuarantinesResolve, problem);
        }
    }

    let hold = state
        .runtime
        .store
        .quarantine_holds()
        .get(hold_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesResolve,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "quarantine hold not found",
                ),
            )
        })?;

    if !matches!(hold.state, QuarantineHoldState::Pending) {
        return governance_err!(
            state,
            GovernanceRoute::QuarantinesResolve,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "quarantine hold is in terminal state {:?}, cannot resolve",
                    hold.state
                ),
            )
        );
    }

    if hold.expires_at < Utc::now() {
        return governance_err!(
            state,
            GovernanceRoute::QuarantinesResolve,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "quarantine hold has expired, cannot resolve",
            )
        );
    }

    let resolved_at = Utc::now();
    let updated = state
        .runtime
        .store
        .quarantine_holds()
        .resolve(
            hold_id,
            request.allow,
            &effective_actor,
            request.reason.as_deref(),
            resolved_at,
        )
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    if !updated {
        return governance_err!(
            state,
            GovernanceRoute::QuarantinesResolve,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "quarantine hold was already resolved",
            )
        );
    }

    if let Err(problem) = crate::audit::append_audit_checked(
        &state,
        &effective_actor.actor_id,
        AuditAction::QuarantineResolve,
        AuditResourceType::QuarantineHold,
        &hold_id.to_string(),
        "success",
        Some(serde_json::json!({
            "allowed": request.allow,
            "reason": request.reason,
        })),
        Some(GovernanceRoute::QuarantinesResolve),
    )
    .await
    {
        return governance_err!(state, GovernanceRoute::QuarantinesResolve, problem);
    }

    let updated_hold = state
        .runtime
        .store
        .quarantine_holds()
        .get(hold_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesResolve,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "quarantine hold not found after resolve",
                ),
            )
        })?;

    let event_kind = ProvenanceEventKind::QuarantineResolved;
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "actor_id".to_string(),
        serde_json::json!(&effective_actor.actor_id),
    );
    metadata.insert("allowed".to_string(), serde_json::json!(request.allow));
    if let Some(reason) = &request.reason {
        metadata.insert("reason".to_string(), serde_json::json!(reason));
    }

    let provenance_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: event_kind,
        occurred_at: Utc::now(),
        actor: effective_actor.clone(),
        object: ObjectRef {
            object_type: ObjectType::QuarantineHold,
            object_id: hold_id.to_string(),
            summary: Some(format!(
                "Quarantine hold {} for proposal",
                if request.allow { "allowed" } else { "denied" }
            )),
        },
        intent_id: Some(hold.intent_id),
        proposal_id: Some(hold.proposal_id),
        execution_id: None,
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

    crate::provenance::append_governance_event(&state.runtime.store, provenance_event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::QuarantinesResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::QuarantinesResolve,
        Ok(Json(updated_hold))
    )
}
