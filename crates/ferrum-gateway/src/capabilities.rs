//! Capability governance handlers.
//!
//! Routes:
//! - `POST /v1/capabilities/mint`                    -> `mint_capability`
//! - `POST /v1/capabilities/{capability_id}/revoke`  -> `revoke_capability`
//!
//! `mint_capability` mints a `CapabilityLease` through the in-memory capability
//! service, persists it to satisfy foreign-key constraints in the `executions`
//! and related tables, and emits a `CapabilityMinted` provenance event.
//!
//! `revoke_capability` first attempts an in-memory revocation. On a
//! `NotFound` it falls back to the durable store, validates the lease
//! status/expiry, marks the lease as revoked, and emits a
//! `CapabilityRevoked` provenance event. Persistence is synchronous so
//! callers observe the same state the ledger has recorded.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_cap::CapabilityError;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, CapabilityId, CapabilityMintRequest, CapabilityMintResponse,
    CapabilityStatus, Decision, EventId, HashChainRef, ObjectRef, ObjectType, ProvenanceEvent,
    ProvenanceEventKind, ProvenanceQueryRequest,
};
use std::sync::Arc;

use crate::execution::{validate_approval_binding_digest, validate_argument_constraints};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::response::sanitize_json;
use crate::state::AppState;

/// Local copy of the path-UUID parser. Mirrors the helper that previously
/// lived in `server.rs` (and is now extracted to `execution.rs`); inlined
/// here to avoid forcing the surgical repair to declare `execution.rs`,
/// which depends on `regex` and `CommitExecutionResponse` that are out of
/// scope for this change.
fn parse_capability_id(value: &str) -> Result<CapabilityId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid capability uuid",
        )
    })?;
    Ok(CapabilityId(parsed))
}

fn decision_from_policy_event(event: &ProvenanceEvent) -> Option<Decision> {
    match event.metadata.get("decision")?.as_str()? {
        "Allow" => Some(Decision::Allow),
        "Deny" => Some(Decision::Deny),
        "Quarantine" => Some(Decision::Quarantine),
        "RequireApproval" => Some(Decision::RequireApproval),
        "AllowDraftOnly" => Some(Decision::AllowDraftOnly),
        _ => None,
    }
}

async fn latest_policy_decision_for_proposal(
    state: &AppState,
    proposal_id: ferrum_proto::ProposalId,
    intent_id: ferrum_proto::IntentId,
) -> Result<Decision, ApiProblem> {
    let events = state
        .runtime
        .store
        .provenance()
        .query(&ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            since: None,
            until: None,
            edge_types: Vec::new(),
        })
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    let event = events
        .iter()
        .filter(|event| event.proposal_id == Some(proposal_id))
        .max_by_key(|event| event.occurred_at)
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "capability mint requires prior PolicyEvaluated event for proposal",
            )
        })?;

    decision_from_policy_event(event).ok_or_else(|| {
        ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::Conflict,
            "PolicyEvaluated event is missing a valid decision",
        )
    })
}

pub(crate) async fn mint_capability(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CapabilityMintRequest>,
) -> Result<Json<CapabilityMintResponse>, ApiProblem> {
    let proposal = match state
        .runtime
        .store
        .proposals()
        .get(request.proposal_id)
        .await
    {
        Ok(Some(proposal)) => proposal,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "proposal not found for capability mint",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if proposal.intent_id != request.intent_id
        || proposal.server_name != request.tool_binding.server_name
        || proposal.tool_name != request.tool_binding.tool_name
    {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesMint,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "capability mint binding does not match proposal",
            )
        );
    }

    if let Err(reason) =
        validate_argument_constraints(&proposal.raw_arguments, &request.argument_constraints)
    {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesMint,
            ApiProblem::new(StatusCode::FORBIDDEN, ApiErrorCode::PolicyDenied, reason,)
        );
    }

    match state.runtime.store.intents().get(request.intent_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for capability mint",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    }

    match latest_policy_decision_for_proposal(&state, request.proposal_id, request.intent_id).await
    {
        Ok(Decision::Allow) => {}
        Ok(Decision::RequireApproval) => {
            let Some(binding) = request.approval_binding.as_ref() else {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesMint,
                    ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::PolicyDenied,
                        "policy requires approval before capability mint",
                    )
                );
            };
            if let Err(problem) =
                validate_approval_binding_digest(&state.runtime.store, binding, request.proposal_id)
                    .await
            {
                return governance_err!(state, GovernanceRoute::CapabilitiesMint, problem);
            }
        }
        Ok(decision) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::new(
                    StatusCode::FORBIDDEN,
                    ApiErrorCode::PolicyDenied,
                    format!(
                        "policy decision {:?} does not allow capability mint",
                        decision
                    ),
                )
            );
        }
        Err(problem) => return governance_err!(state, GovernanceRoute::CapabilitiesMint, problem),
    }

    let response = match state.runtime.cap.mint(request).await {
        Ok(response) => response,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::from_capability(e)
            );
        }
    };

    // Persist the capability to the store so foreign-key constraints in
    // executions and other tables are satisfied.
    // Write-queue ensures serialized writes - no more SQLite lock contention.
    if let Err(e) = state
        .runtime
        .store
        .capabilities()
        .insert(&response.lease)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesMint,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit CapabilityMinted provenance event.
    let cap_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::CapabilityMinted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Capability,
            object_id: response.lease.capability_id.to_string(),
            summary: Some("Capability minted".to_string()),
        },
        intent_id: Some(response.lease.intent_id),
        proposal_id: Some(response.lease.proposal_id),
        execution_id: None,
        capability_id: Some(response.lease.capability_id),
        rollback_contract_id: None,
        policy_bundle_id: Some(response.lease.policy_bundle_id),
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = append_governance_event(&state.runtime.store, cap_event).await {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesMint,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(state, GovernanceRoute::CapabilitiesMint, Ok(Json(response)))
}

pub(crate) async fn revoke_capability(
    State(state): State<Arc<AppState>>,
    Path(capability_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    let id = parse_capability_id(&capability_id).inspect_err(|_| {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::CapabilitiesRevoke)
    })?;

    // Revoke the capability in the capability service (in-memory)
    // If NotFound, fall back to store and revoke there synchronously
    let lease = match state.runtime.cap.revoke(id).await {
        Ok(lease) => lease,
        Err(CapabilityError::NotFound) => {
            // In-memory miss: load from store, validate, revoke, persist synchronously
            let lease = match state.runtime.store.capabilities().get(id).await {
                Ok(Some(lease)) => lease,
                Ok(None) => {
                    return governance_err!(
                        state,
                        GovernanceRoute::CapabilitiesRevoke,
                        ApiProblem::from_capability(CapabilityError::NotFound)
                    );
                }
                Err(e) => {
                    return governance_err!(
                        state,
                        GovernanceRoute::CapabilitiesRevoke,
                        ApiProblem::internal(anyhow::Error::from(e))
                    );
                }
            };

            // Validate status
            if matches!(lease.status, CapabilityStatus::Revoked) {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::from_capability(CapabilityError::Revoked)
                );
            }
            if matches!(lease.status, CapabilityStatus::Used) {
                // Already used capabilities cannot be revoked (they're consumed, not active)
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::from_capability(CapabilityError::AlreadyUsed)
                );
            }
            if lease.expires_at < Utc::now() {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::from_capability(CapabilityError::Expired)
                );
            }

            // Set revoked status
            let mut lease = lease;
            lease.status = CapabilityStatus::Revoked;
            lease.revoked_at = Some(Utc::now());

            // Persist synchronously before returning
            if let Err(e) = state.runtime.store.capabilities().update(&lease).await {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::internal(anyhow::Error::from(e))
                );
            }

            lease
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesRevoke,
                ApiProblem::from_capability(e)
            );
        }
    };

    // Build provenance event
    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::CapabilityRevoked,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "gateway".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::Capability,
            object_id: lease.capability_id.to_string(),
            summary: None,
        },
        intent_id: Some(lease.intent_id),
        proposal_id: Some(lease.proposal_id),
        execution_id: None,
        capability_id: Some(lease.capability_id),
        rollback_contract_id: None,
        policy_bundle_id: Some(lease.policy_bundle_id),
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };

    // Persist capability revocation and append provenance event synchronously.
    // Return error if persistence fails rather than fire-and-forget.
    if let Err(e) = state.runtime.store.capabilities().update(&lease).await {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesRevoke,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    if let Err(e) = append_governance_event(&state.runtime.store, event).await {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesRevoke,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    let response = serde_json::json!({
        "ok": true,
        "capability_id": lease.capability_id.to_string()
    });
    let sanitized = sanitize_json(&state.runtime.firewall, response);
    governance_ok!(
        state,
        GovernanceRoute::CapabilitiesRevoke,
        Ok(Json(sanitized))
    )
}
