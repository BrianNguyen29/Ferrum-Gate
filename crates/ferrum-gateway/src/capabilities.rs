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

use async_trait::async_trait;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::{Duration, Utc};
use ferrum_cap::{CapabilityError, CapabilityService};
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, CapabilityId, CapabilityLease, CapabilityMintRequest,
    CapabilityMintResponse, CapabilityStatus, Decision, EventId, HashChainRef, ObjectRef,
    ObjectType, PolicyBundleId, ProvenanceEvent, ProvenanceEventKind, ProvenanceQueryRequest,
    QuarantineHoldState,
};
use ferrum_store::StoreFacade;
use std::sync::Arc;

use crate::AuthActor;
use crate::auth_actor::enforce_object_owner_guard;
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

/// Ensure there is an explicit `Allowed` quarantine hold for the proposal.
/// Only an `Allowed` hold permits minting; a missing hold, or a `Pending`,
/// `Denied`, or `Expired` hold blocks.
async fn validate_quarantine_hold_for_proposal(
    state: &AppState,
    proposal_id: ferrum_proto::ProposalId,
) -> Result<(), ApiProblem> {
    let hold = state
        .runtime
        .store
        .quarantine_holds()
        .get_by_proposal(proposal_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    let Some(hold) = hold else {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "quarantine hold is missing for quarantined proposal",
        ));
    };

    match hold.state {
        QuarantineHoldState::Allowed => Ok(()),
        QuarantineHoldState::Pending => Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "proposal is under quarantine and awaiting operator resolution",
        )),
        QuarantineHoldState::Denied => Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "proposal was denied by quarantine resolution",
        )),
        QuarantineHoldState::Expired => Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "quarantine hold expired; proposal cannot be minted",
        )),
    }
}

/// Trusted gateway-only mint path: constructs a `CapabilityLease` with the
/// server-derived owner and persists it through the store capability repository.
///
/// The public `CapabilityService::mint` remains ownerless; this helper is the
/// only place an authenticated owner is bound before the initial insert. Owned
/// leases are not cached in the in-memory capability service; durable
/// authorization falls back to the store when the in-memory service misses.
async fn mint_capability_with_owner(
    store: &Arc<dyn StoreFacade>,
    request: CapabilityMintRequest,
    owner_actor_id: Option<String>,
) -> Result<CapabilityMintResponse, CapabilityError> {
    if request.requested_ttl_secs > 300 {
        return Err(CapabilityError::TtlTooLong);
    }

    let now = Utc::now();
    let lease = CapabilityLease {
        capability_id: CapabilityId::new(),
        intent_id: request.intent_id,
        proposal_id: request.proposal_id,
        tool_binding: request.tool_binding,
        resource_bindings: request.resource_bindings,
        argument_constraints: request.argument_constraints,
        taint_budget: request.taint_budget,
        approval_binding: request.approval_binding,
        issued_by: "ferrum-cap".to_string(),
        policy_bundle_id: PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status: CapabilityStatus::Active,
        issued_at: now,
        expires_at: now + Duration::seconds(request.requested_ttl_secs as i64),
        revoked_at: None,
        metadata: request.metadata,
        owner_actor_id,
    };

    store
        .capabilities()
        .insert(&lease)
        .await
        .map_err(map_store_error)?;

    Ok(CapabilityMintResponse {
        lease,
        warnings: Vec::new(),
    })
}

pub(crate) async fn mint_capability(
    State(state): State<Arc<AppState>>,
    auth_actor: Option<Extension<AuthActor>>,
    Json(request): Json<CapabilityMintRequest>,
) -> Result<Json<CapabilityMintResponse>, ApiProblem> {
    // Server-derived ownership: the authenticated actor is injected only via the
    // trusted internal mint call. The public HTTP DTO cannot carry an owner.
    let owner_actor_id = auth_actor.map(|Extension(actor)| actor.actor_id);
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
        Ok(Decision::Quarantine) => {
            if let Err(problem) =
                validate_quarantine_hold_for_proposal(&state, request.proposal_id).await
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

    let response =
        match mint_capability_with_owner(&state.runtime.store, request, owner_actor_id).await {
            Ok(response) => response,
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesMint,
                    ApiProblem::from_capability(e)
                );
            }
        };

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
    auth_actor: Option<Extension<AuthActor>>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    let id = parse_capability_id(&capability_id).inspect_err(|_| {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::CapabilitiesRevoke)
    })?;

    // Load the durable capability for the ownership guard before any mutation.
    let lease = match state.runtime.store.capabilities().get(id).await {
        Ok(Some(lease)) => lease,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesRevoke,
                ApiProblem::object_not_found()
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

    if let Err(problem) = enforce_object_owner_guard(
        auth_actor.as_ref().map(|Extension(a)| a),
        lease.owner_actor_id.as_ref(),
        state.server_config.auth_mode,
        state.server_config.legacy_object_compat_allow_until,
        Utc::now(),
        "capability",
        "revoke",
    ) {
        return governance_err!(state, GovernanceRoute::CapabilitiesRevoke, problem);
    }

    // Revoke the capability in the capability service (in-memory)
    // If NotFound, fall back to store and revoke there synchronously
    let (lease, persist_revoke_in_handler) = match state.runtime.cap.revoke(id).await {
        Ok(lease) => (lease, !state.runtime.cap.is_store_backed()),
        Err(CapabilityError::NotFound) => {
            // In-memory miss: load from store, validate, revoke, persist synchronously
            let lease = match state.runtime.store.capabilities().get(id).await {
                Ok(Some(lease)) => lease,
                Ok(None) => {
                    return governance_err!(
                        state,
                        GovernanceRoute::CapabilitiesRevoke,
                        ApiProblem::object_not_found()
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

            (lease, false)
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

    // In-memory capability services need the handler to persist the revocation.
    // Store-backed services and the store fallback path already persisted it.
    if persist_revoke_in_handler {
        if let Err(e) = state.runtime.store.capabilities().update(&lease).await {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesRevoke,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
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

/// Store-backed implementation of [`CapabilityService`] that uses
/// [`CapabilityRepo`] as the single source of truth for mint, get, mark_used
/// and revoke.  Atomic single-use is enforced via
/// [`CapabilityRepo::update_status_if_active`].
///
/// This service eliminates the dual-write hazard that exists when an
/// in-memory service is paired with a separate store insert.
pub struct StoreCapabilityService {
    store: Arc<dyn StoreFacade>,
}

impl StoreCapabilityService {
    pub fn new(store: Arc<dyn StoreFacade>) -> Self {
        Self { store }
    }
}

fn map_store_error(err: ferrum_store::StoreError) -> CapabilityError {
    tracing::error!(error = %err, "store capability operation failed");
    match err {
        ferrum_store::StoreError::NotFound { .. } => CapabilityError::NotFound,
        _ => CapabilityError::Internal,
    }
}

#[async_trait]
impl CapabilityService for StoreCapabilityService {
    fn is_store_backed(&self) -> bool {
        true
    }

    async fn mint(
        &self,
        request: CapabilityMintRequest,
    ) -> Result<CapabilityMintResponse, CapabilityError> {
        if request.requested_ttl_secs > 300 {
            return Err(CapabilityError::TtlTooLong);
        }

        let now = Utc::now();
        let lease = CapabilityLease {
            capability_id: CapabilityId::new(),
            intent_id: request.intent_id,
            proposal_id: request.proposal_id,
            tool_binding: request.tool_binding,
            resource_bindings: request.resource_bindings,
            argument_constraints: request.argument_constraints,
            taint_budget: request.taint_budget,
            approval_binding: request.approval_binding,
            issued_by: "ferrum-cap".to_string(),
            policy_bundle_id: PolicyBundleId::new(),
            tool_manifest_id: None,
            manifest_hash: None,
            status: CapabilityStatus::Active,
            issued_at: now,
            expires_at: now + Duration::seconds(request.requested_ttl_secs as i64),
            revoked_at: None,
            metadata: request.metadata,
            owner_actor_id: None,
        };

        self.store
            .capabilities()
            .insert(&lease)
            .await
            .map_err(map_store_error)?;

        Ok(CapabilityMintResponse {
            lease,
            warnings: Vec::new(),
        })
    }

    async fn get(&self, capability_id: CapabilityId) -> Result<CapabilityLease, CapabilityError> {
        let lease = self
            .store
            .capabilities()
            .get(capability_id)
            .await
            .map_err(map_store_error)?
            .ok_or(CapabilityError::NotFound)?;

        if matches!(lease.status, CapabilityStatus::Revoked) {
            return Err(CapabilityError::Revoked);
        }
        if lease.expires_at < Utc::now() {
            return Err(CapabilityError::Expired);
        }

        Ok(lease)
    }

    async fn mark_used(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityLease, CapabilityError> {
        let lease = self
            .store
            .capabilities()
            .get(capability_id)
            .await
            .map_err(map_store_error)?
            .ok_or(CapabilityError::NotFound)?;

        if matches!(lease.status, CapabilityStatus::Used) {
            return Err(CapabilityError::AlreadyUsed);
        }
        if matches!(lease.status, CapabilityStatus::Revoked) {
            return Err(CapabilityError::Revoked);
        }
        if lease.expires_at < Utc::now() {
            return Err(CapabilityError::Expired);
        }

        let updated = self
            .store
            .capabilities()
            .update_status_if_active(capability_id, CapabilityStatus::Used)
            .await
            .map_err(map_store_error)?;

        if !updated {
            let lease = self
                .store
                .capabilities()
                .get(capability_id)
                .await
                .map_err(map_store_error)?
                .ok_or(CapabilityError::NotFound)?;

            if matches!(lease.status, CapabilityStatus::Used) {
                return Err(CapabilityError::AlreadyUsed);
            }
            if matches!(lease.status, CapabilityStatus::Revoked) {
                return Err(CapabilityError::Revoked);
            }
            if lease.expires_at < Utc::now() {
                return Err(CapabilityError::Expired);
            }
            return Err(CapabilityError::NotFound);
        }

        let lease = self
            .store
            .capabilities()
            .get(capability_id)
            .await
            .map_err(map_store_error)?
            .ok_or(CapabilityError::NotFound)?;

        Ok(lease)
    }

    async fn revoke(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityLease, CapabilityError> {
        let lease = self
            .store
            .capabilities()
            .get(capability_id)
            .await
            .map_err(map_store_error)?
            .ok_or(CapabilityError::NotFound)?;

        if matches!(lease.status, CapabilityStatus::Revoked) {
            return Err(CapabilityError::Revoked);
        }
        if matches!(lease.status, CapabilityStatus::Used) {
            return Err(CapabilityError::AlreadyUsed);
        }
        if lease.expires_at < Utc::now() {
            return Err(CapabilityError::Expired);
        }

        let revoked_at = Utc::now();
        let updated = self
            .store
            .capabilities()
            .revoke_if_active(capability_id, revoked_at)
            .await
            .map_err(map_store_error)?;

        if !updated {
            let lease = self
                .store
                .capabilities()
                .get(capability_id)
                .await
                .map_err(map_store_error)?
                .ok_or(CapabilityError::NotFound)?;

            if matches!(lease.status, CapabilityStatus::Used) {
                return Err(CapabilityError::AlreadyUsed);
            }
            if matches!(lease.status, CapabilityStatus::Revoked) {
                return Err(CapabilityError::Revoked);
            }
            if lease.expires_at < Utc::now() {
                return Err(CapabilityError::Expired);
            }
            return Err(CapabilityError::NotFound);
        }

        let lease = self
            .store
            .capabilities()
            .get(capability_id)
            .await
            .map_err(map_store_error)?
            .ok_or(CapabilityError::NotFound)?;

        Ok(lease)
    }
}

#[cfg(test)]
mod tests {
    use super::StoreCapabilityService;
    use ferrum_cap::{CapabilityError, CapabilityService};
    use ferrum_proto::{
        CapabilityMintRequest, CapabilityStatus, JsonMap, TaintBudget, ToolBinding,
    };
    use ferrum_store::{CapabilityRepo, IntentRepo, ProposalRepo, SqliteStore};
    use std::sync::Arc;

    fn make_intent() -> ferrum_proto::IntentEnvelope {
        let now = chrono::Utc::now();
        ferrum_proto::IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: ferrum_proto::TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(15),
            owner_actor_id: None,
        }
    }

    fn make_proposal(intent_id: ferrum_proto::IntentId) -> ferrum_proto::ActionProposal {
        ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id,
            step_index: 0,
            title: "test".to_string(),
            tool_name: "test-tool".to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn make_mint_request(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
        ttl_secs: u64,
    ) -> CapabilityMintRequest {
        CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![],
            argument_constraints: vec![],
            taint_budget: TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: ttl_secs,
            metadata: JsonMap::new(),
        }
    }

    async fn setup() -> (Arc<SqliteStore>, StoreCapabilityService) {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let service = StoreCapabilityService::new(Arc::new(store.clone()));
        (Arc::new(store), service)
    }

    #[tokio::test]
    async fn test_ttl_301_rejected() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 301);
        let result = service.mint(request).await;
        assert!(matches!(result, Err(CapabilityError::TtlTooLong)));
    }

    #[tokio::test]
    async fn test_ttl_300_accepted() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let result = service.mint(request).await;
        assert!(
            result.is_ok(),
            "TTL=300 should be accepted, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_mint_and_get() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let fetched = service.get(minted.lease.capability_id).await.unwrap();
        assert_eq!(fetched.capability_id, minted.lease.capability_id);
        assert!(matches!(fetched.status, CapabilityStatus::Active));
    }

    #[tokio::test]
    async fn test_mark_used_success() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let used = service.mark_used(minted.lease.capability_id).await.unwrap();
        assert!(matches!(used.status, CapabilityStatus::Used));
    }

    #[tokio::test]
    async fn test_mark_used_already_used() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let _ = service.mark_used(minted.lease.capability_id).await.unwrap();
        let result = service.mark_used(minted.lease.capability_id).await;
        assert!(
            matches!(result, Err(CapabilityError::AlreadyUsed)),
            "second mark_used should fail with AlreadyUsed, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_mark_used_expired() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 0);
        let minted = service.mint(request).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let result = service.mark_used(minted.lease.capability_id).await;
        assert!(
            matches!(result, Err(CapabilityError::Expired)),
            "mark_used on expired capability should fail with Expired, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_mark_used_revoked() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let _ = service.revoke(minted.lease.capability_id).await.unwrap();
        let result = service.mark_used(minted.lease.capability_id).await;
        assert!(
            matches!(result, Err(CapabilityError::Revoked)),
            "mark_used on revoked capability should fail with Revoked, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_mark_used_concurrent_single_use() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let cap_id = minted.lease.capability_id;

        let service = Arc::new(service);
        let service1 = service.clone();
        let service2 = service.clone();

        let handle1 = tokio::spawn(async move { service1.mark_used(cap_id).await });
        let handle2 = tokio::spawn(async move { service2.mark_used(cap_id).await });

        let (r1, r2) = tokio::join!(handle1, handle2);
        let results = [r1.unwrap(), r2.unwrap()];
        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results
            .iter()
            .filter(|r| matches!(r, Err(CapabilityError::AlreadyUsed)))
            .count();

        assert_eq!(
            successes, 1,
            "exactly one concurrent mark_used should succeed"
        );
        assert_eq!(
            failures, 1,
            "exactly one concurrent mark_used should fail with AlreadyUsed"
        );

        let lease = store.capabilities().get(cap_id).await.unwrap().unwrap();
        assert!(matches!(lease.status, CapabilityStatus::Used));
    }

    #[tokio::test]
    async fn test_revoke_success() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let revoked = service.revoke(minted.lease.capability_id).await.unwrap();
        assert!(matches!(revoked.status, CapabilityStatus::Revoked));
        assert!(revoked.revoked_at.is_some());

        let lease = store
            .capabilities()
            .get(minted.lease.capability_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(lease.status, CapabilityStatus::Revoked));
    }

    #[tokio::test]
    async fn test_revoke_already_revoked() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let _ = service.revoke(minted.lease.capability_id).await.unwrap();
        let result = service.revoke(minted.lease.capability_id).await;
        assert!(matches!(result, Err(CapabilityError::Revoked)));
    }

    #[tokio::test]
    async fn test_revoke_already_used() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let _ = service.mark_used(minted.lease.capability_id).await.unwrap();
        let result = service.revoke(minted.lease.capability_id).await;
        assert!(matches!(result, Err(CapabilityError::AlreadyUsed)));
    }

    #[tokio::test]
    async fn test_revoke_expired() {
        let (store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 0);
        let minted = service.mint(request).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let result = service.revoke(minted.lease.capability_id).await;
        assert!(matches!(result, Err(CapabilityError::Expired)));
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let (_store, service) = setup().await;
        let result = service.get(ferrum_proto::CapabilityId::new()).await;
        assert!(matches!(result, Err(CapabilityError::NotFound)));
    }

    #[tokio::test]
    async fn test_get_revoked() {
        let (_store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        _store.intents().insert(&intent).await.unwrap();
        _store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 300);
        let minted = service.mint(request).await.unwrap();
        let _ = service.revoke(minted.lease.capability_id).await.unwrap();
        let result = service.get(minted.lease.capability_id).await;
        assert!(matches!(result, Err(CapabilityError::Revoked)));
    }

    #[tokio::test]
    async fn test_get_expired() {
        let (_store, service) = setup().await;
        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        _store.intents().insert(&intent).await.unwrap();
        _store.proposals().insert(&proposal).await.unwrap();
        let request = make_mint_request(intent.intent_id, proposal.proposal_id, 0);
        let minted = service.mint(request).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let result = service.get(minted.lease.capability_id).await;
        assert!(matches!(result, Err(CapabilityError::Expired)));
    }
}

#[cfg(test)]
mod quarantine_hold_tests {
    use super::*;
    use crate::execution::get_capability_for_authorize;
    use crate::state::{AppState, GatewayRuntime, ServerConfig};
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::{SqliteStore, StoreFacade};
    use std::sync::Arc;

    async fn test_runtime() -> GatewayRuntime {
        let pdp = Arc::new(ferrum_pdp::StaticPdpEngine);
        let cap = Arc::new(ferrum_cap::InMemoryCapabilityService::default());
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![])
    }

    fn make_intent(intent_id: ferrum_proto::IntentId) -> ferrum_proto::IntentEnvelope {
        use ferrum_proto::{
            ApprovalMode, IntentStatus, JsonMap, ResourceMode, ResourceSelector, RiskTier,
            RollbackClass, TimeBudget, TrustContextSummary,
        };
        ferrum_proto::IntentEnvelope {
            intent_id,
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![ResourceSelector::FilesystemPath {
                path: "/tmp".to_string(),
                mode: ResourceMode::Write,
                content_hash: None,
            }],
            risk_tier: RiskTier::Low,
            approval_mode: ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: TimeBudget {
                max_duration_ms: 60000,
                max_steps: 100,
                max_retries_per_step: 3,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: JsonMap::new(),
            status: IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn make_proposal(
        proposal_id: ferrum_proto::ProposalId,
        intent_id: ferrum_proto::IntentId,
    ) -> ferrum_proto::ActionProposal {
        use ferrum_proto::{JsonMap, RiskTier, RollbackClass};
        ferrum_proto::ActionProposal {
            proposal_id,
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "git.commit".to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test effect".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R1SnapshotRecoverable,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    async fn insert_hold(
        runtime: &GatewayRuntime,
        state: ferrum_proto::QuarantineHoldState,
    ) -> (ferrum_proto::ProposalId, ferrum_proto::IntentId) {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        runtime
            .store
            .intents()
            .insert(&make_intent(intent_id))
            .await
            .unwrap();
        runtime
            .store
            .proposals()
            .insert(&make_proposal(proposal_id, intent_id))
            .await
            .unwrap();

        let hold = ferrum_proto::QuarantineHold {
            hold_id: ferrum_proto::QuarantineHoldId::new(),
            intent_id,
            proposal_id,
            reason: "test".to_string(),
            matched_rule_ids: vec!["rule".to_string()],
            policy_bundle_id: None,
            state,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(300),
            created_at: chrono::Utc::now(),
            resolved_at: None,
            resolved_by: None,
            resolution_reason: None,
            metadata: ferrum_proto::JsonMap::new(),
            owner_actor_id: None,
        };
        runtime
            .store
            .quarantine_holds()
            .insert(&hold)
            .await
            .unwrap();
        (proposal_id, intent_id)
    }

    #[tokio::test]
    async fn test_no_hold_blocks_mint() {
        let runtime = test_runtime().await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let proposal_id = ferrum_proto::ProposalId::new();
        let result = validate_quarantine_hold_for_proposal(&state, proposal_id).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().1, axum::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_pending_hold_blocks_mint() {
        let runtime = test_runtime().await;
        let (proposal_id, _) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Pending).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let result = validate_quarantine_hold_for_proposal(&state, proposal_id).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().1, axum::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_allowed_hold_permits_mint() {
        let runtime = test_runtime().await;
        let (proposal_id, _) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Allowed).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let result = validate_quarantine_hold_for_proposal(&state, proposal_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_denied_hold_blocks_mint() {
        let runtime = test_runtime().await;
        let (proposal_id, _) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Denied).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let result = validate_quarantine_hold_for_proposal(&state, proposal_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_expired_hold_blocks_mint() {
        let runtime = test_runtime().await;
        let (proposal_id, _) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Expired).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let result = validate_quarantine_hold_for_proposal(&state, proposal_id).await;
        assert!(result.is_err());
    }

    async fn insert_quarantine_policy_event(
        runtime: &GatewayRuntime,
        proposal_id: ferrum_proto::ProposalId,
        intent_id: ferrum_proto::IntentId,
    ) {
        let mut metadata = ferrum_proto::JsonMap::new();
        metadata.insert("decision".to_string(), serde_json::json!("Quarantine"));
        let event = ferrum_proto::ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::PolicyEvaluated,
            occurred_at: chrono::Utc::now(),
            actor: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("FerrumGate Gateway".to_string()),
            },
            object: ferrum_proto::ObjectRef {
                object_type: ferrum_proto::ObjectType::PolicyBundle,
                object_id: proposal_id.to_string(),
                summary: Some("Policy evaluated for proposal".to_string()),
            },
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: ferrum_proto::HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata,
            source_runtime_id: None,
        };
        runtime
            .store
            .provenance()
            .append_event(&event)
            .await
            .unwrap();
    }

    fn make_mint_request(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
    ) -> ferrum_proto::CapabilityMintRequest {
        use ferrum_proto::{JsonMap, TaintBudget, ToolBinding};
        ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "git.commit".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![],
            argument_constraints: vec![],
            taint_budget: TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 60,
            metadata: JsonMap::new(),
        }
    }

    #[tokio::test]
    async fn test_mint_quarantine_decision_allowed_hold_succeeds() {
        let runtime = test_runtime().await;
        let (proposal_id, intent_id) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Allowed).await;
        insert_quarantine_policy_event(&runtime, proposal_id, intent_id).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let result = mint_capability(State(Arc::clone(&state)), None, axum::Json(request)).await;
        assert!(
            result.is_ok(),
            "expected mint to succeed for Allowed hold, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_mint_quarantine_decision_pending_hold_blocked() {
        let runtime = test_runtime().await;
        let (proposal_id, intent_id) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Pending).await;
        insert_quarantine_policy_event(&runtime, proposal_id, intent_id).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let err = mint_capability(State(Arc::clone(&state)), None, axum::Json(request))
            .await
            .unwrap_err();
        assert_eq!(err.1, axum::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_mint_quarantine_decision_denied_hold_blocked() {
        let runtime = test_runtime().await;
        let (proposal_id, intent_id) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Denied).await;
        insert_quarantine_policy_event(&runtime, proposal_id, intent_id).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let err = mint_capability(State(Arc::clone(&state)), None, axum::Json(request))
            .await
            .unwrap_err();
        assert_eq!(err.1, axum::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_mint_quarantine_decision_expired_hold_blocked() {
        let runtime = test_runtime().await;
        let (proposal_id, intent_id) =
            insert_hold(&runtime, ferrum_proto::QuarantineHoldState::Expired).await;
        insert_quarantine_policy_event(&runtime, proposal_id, intent_id).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let err = mint_capability(State(Arc::clone(&state)), None, axum::Json(request))
            .await
            .unwrap_err();
        assert_eq!(err.1, axum::http::StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_mint_quarantine_decision_no_hold_blocked() {
        let runtime = test_runtime().await;
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        runtime
            .store
            .intents()
            .insert(&make_intent(intent_id))
            .await
            .unwrap();
        runtime
            .store
            .proposals()
            .insert(&make_proposal(proposal_id, intent_id))
            .await
            .unwrap();
        insert_quarantine_policy_event(&runtime, proposal_id, intent_id).await;
        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let err = mint_capability(State(Arc::clone(&state)), None, axum::Json(request))
            .await
            .unwrap_err();
        assert_eq!(err.1, axum::http::StatusCode::FORBIDDEN);
    }

    async fn insert_allow_policy_event(
        runtime: &GatewayRuntime,
        proposal_id: ferrum_proto::ProposalId,
        intent_id: ferrum_proto::IntentId,
    ) {
        let mut metadata = ferrum_proto::JsonMap::new();
        metadata.insert("decision".to_string(), serde_json::json!("Allow"));
        let event = ferrum_proto::ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::PolicyEvaluated,
            occurred_at: chrono::Utc::now(),
            actor: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("FerrumGate Gateway".to_string()),
            },
            object: ferrum_proto::ObjectRef {
                object_type: ferrum_proto::ObjectType::PolicyBundle,
                object_id: proposal_id.to_string(),
                summary: Some("Policy evaluated for proposal".to_string()),
            },
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: ferrum_proto::HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata,
            source_runtime_id: None,
        };
        runtime
            .store
            .provenance()
            .append_event(&event)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_mint_binds_owner_from_auth_actor() {
        let runtime = test_runtime().await;
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        runtime
            .store
            .intents()
            .insert(&make_intent(intent_id))
            .await
            .unwrap();
        runtime
            .store
            .proposals()
            .insert(&make_proposal(proposal_id, intent_id))
            .await
            .unwrap();
        insert_allow_policy_event(&runtime, proposal_id, intent_id).await;

        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let auth_actor = AuthActor {
            actor_id: "gateway-user-42".to_string(),
            source: "test",
            scopes: vec!["*".to_string()],
            role: None,
        };
        let response = mint_capability(
            State(Arc::clone(&state)),
            Some(axum::Extension(auth_actor)),
            axum::Json(request),
        )
        .await
        .expect("mint should succeed")
        .0;
        assert_eq!(
            response.lease.owner_actor_id,
            Some("gateway-user-42".to_string()),
            "gateway must bind the authenticated actor as the capability owner"
        );
    }

    #[tokio::test]
    async fn test_mint_persists_owner_to_store_not_memory() {
        let runtime = test_runtime().await;
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        runtime
            .store
            .intents()
            .insert(&make_intent(intent_id))
            .await
            .unwrap();
        runtime
            .store
            .proposals()
            .insert(&make_proposal(proposal_id, intent_id))
            .await
            .unwrap();
        insert_allow_policy_event(&runtime, proposal_id, intent_id).await;

        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let auth_actor = AuthActor {
            actor_id: "gateway-user-42".to_string(),
            source: "test",
            scopes: vec!["*".to_string()],
            role: None,
        };
        let response = mint_capability(
            State(Arc::clone(&state)),
            Some(axum::Extension(auth_actor)),
            axum::Json(request),
        )
        .await
        .expect("mint should succeed")
        .0;

        let cap_id = response.lease.capability_id;
        assert_eq!(
            response.lease.owner_actor_id,
            Some("gateway-user-42".to_string())
        );

        let stored = state
            .runtime
            .store
            .capabilities()
            .get(cap_id)
            .await
            .expect("store read should succeed")
            .expect("store should contain the lease");
        assert_eq!(
            stored.owner_actor_id,
            Some("gateway-user-42".to_string()),
            "store must persist the authenticated owner"
        );

        let in_mem = state.runtime.cap.get(cap_id).await;
        assert!(
            matches!(in_mem, Err(CapabilityError::NotFound)),
            "in-memory service must not cache the owned lease: {:?}",
            in_mem
        );
    }

    #[tokio::test]
    async fn test_durable_fallback_authorization_returns_owned_lease() {
        let runtime = test_runtime().await;
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        runtime
            .store
            .intents()
            .insert(&make_intent(intent_id))
            .await
            .unwrap();
        runtime
            .store
            .proposals()
            .insert(&make_proposal(proposal_id, intent_id))
            .await
            .unwrap();
        insert_allow_policy_event(&runtime, proposal_id, intent_id).await;

        let state = AppState::test_new(runtime, ServerConfig::default());
        let request = make_mint_request(intent_id, proposal_id);
        let auth_actor = AuthActor {
            actor_id: "gateway-user-42".to_string(),
            source: "test",
            scopes: vec!["*".to_string()],
            role: None,
        };
        let response = mint_capability(
            State(Arc::clone(&state)),
            Some(axum::Extension(auth_actor)),
            axum::Json(request),
        )
        .await
        .expect("mint should succeed")
        .0;

        let cap_id = response.lease.capability_id;
        let fresh_cap = Arc::new(ferrum_cap::InMemoryCapabilityService::default())
            as Arc<dyn CapabilityService>;
        let lease = get_capability_for_authorize(&fresh_cap, &state.runtime.store, cap_id)
            .await
            .expect("durable fallback should find the persisted lease");
        assert_eq!(
            lease.owner_actor_id,
            Some("gateway-user-42".to_string()),
            "fallback authorization must return the store-owned lease"
        );
    }
}
