use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, AuthorizeExecutionRequest, AuthorizeExecutionResponse,
    Decision, EventId, ExecutionId, ExecutionRecord, ExecutionState, HashChainRef,
    LifecycleOutboxRecord, ObjectRef, ObjectType, ProvenanceEvent, ProvenanceEventKind,
};

use crate::execution::{
    classify_authorization_cas_failure, get_capability_for_authorize, lifecycle_event_metadata,
    mark_lifecycle_transition_reconciled, validate_approval_binding_digest,
    validate_capability_proposal_binding, validate_resource_bindings_subset_of_scope,
};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::state::AppState;

/// `POST /v1/executions/authorize`
///
/// Authorize an execution by consuming its single-use capability. The handler
/// enforces the capability gate in this exact order:
///
/// 1. Load capability from in-memory service, falling back to persisted store
///    via `get_capability_for_authorize`.
/// 2. Binding invariant — reject (403 IntegrityMismatch) if
///    `request.proposal_id != lease.proposal_id` before any durable
///    capability mutation.
/// 3. I5 invariant — validate that capability `resource_bindings` is a subset
///    of the intent's `resource_scope`.
/// 4. I6 invariant — if the capability has an `approval_binding`, validate the
///    approval binding digest (and the proposal's canonical action digest)
///    against the binding. Skipped when `approval_binding=None`.
/// 5. Persist the single-use Active -> Used transition atomically via
///    `record_authorization` (which also requires `expires_at > now`). On a
///    lost CAS the capability is reloaded and mapped to the accurate error
///    (`AlreadyUsed` / `Revoked` / `Expired` / `NotFound`); the in-memory
///    cache is synced only after the durable transition commits.
/// 6. Insert an `ExecutionRecord` (state `Authorized` for dry-run, `Prepared`
///    otherwise) and emit an `ActionProposalSubmitted` provenance event.
///
/// All guards, ordering, status codes, and the response schema are preserved
/// verbatim from the original `server.rs` implementation.
pub(crate) async fn authorize_execution(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AuthorizeExecutionRequest>,
) -> Result<Json<AuthorizeExecutionResponse>, ApiProblem> {
    // Load capability from in-memory service, falling back to persisted store.
    // This ensures capability survives in-memory state loss.
    let lease = match get_capability_for_authorize(
        &state.runtime.cap,
        &state.runtime.store,
        request.capability_id,
    )
    .await
    {
        Ok(lease) => lease,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(e)
            );
        }
    };

    // Binding invariant: the request's proposal_id MUST match the lease's
    // proposal_id. This prevents a holder of one capability from using it to
    // authorize an unrelated proposal, and ensures the durable single-use
    // mark below only fires for a matched (capability, proposal) pair.
    if request.proposal_id != lease.proposal_id {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "request proposal_id does not match capability lease proposal_id",
            )
        );
    }

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
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "proposal not found for capability",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if let Err(reason) = validate_capability_proposal_binding(&lease, &proposal) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                reason,
            )
        );
    }

    // I5: Validate that capability resource_bindings is a subset of intent resource_scope.
    // This prevents a capability from expanding beyond the intent's authorized scope.
    let intent = match state.runtime.store.intents().get(lease.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for capability",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if let Err(scope_violation) =
        validate_resource_bindings_subset_of_scope(&lease.resource_bindings, &intent.resource_scope)
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                scope_violation,
            )
        );
    }

    // I6: Validate approval binding digest if present.
    // This ensures the proposal digest matches the approved action digest.
    // Skipped when approval_binding=None (backward compatible).
    if let Some(ref binding) = lease.approval_binding {
        validate_approval_binding_digest(&state.runtime.store, binding, request.proposal_id)
            .await
            .map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::ExecutionsAuthorize, e)
            })?;
    }

    let record = ExecutionRecord {
        execution_id: ExecutionId::new(),
        proposal_id: request.proposal_id,
        intent_id: lease.intent_id,
        capability_id: lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: if request.dry_run {
            ExecutionState::Authorized
        } else {
            ExecutionState::Prepared
        },
        started_at: Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
        owner_actor_id: lease.owner_actor_id.clone(),
    };

    let mut outbox = LifecycleOutboxRecord::pending(
        record.execution_id,
        None,
        None,
        record.state.clone(),
        None,
        None,
        ProvenanceEventKind::ActionProposalSubmitted,
        format!("authorize:{}", record.execution_id),
    );
    outbox
        .metadata
        .insert("transition".to_string(), serde_json::json!("authorize"));

    match state
        .runtime
        .store
        .lifecycle_outbox()
        .record_authorization(&lease, &record, &outbox)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            // The durable single-use CAS did not win. Reload the capability and
            // map the true cause (Used / Revoked / Expired / Missing) instead
            // of assuming AlreadyUsed, so an expired-but-Active capability is
            // reported accurately.
            let err =
                classify_authorization_cas_failure(&state.runtime.store, request.capability_id)
                    .await;
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(err)
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    }
    if let Err(error) = state.runtime.cap.mark_used(request.capability_id).await {
        tracing::debug!(
            ?error,
            "capability authorization transaction committed; in-memory cache sync skipped"
        );
    }

    // Emit provenance event for authorization.
    let auth_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ActionProposalSubmitted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: record.execution_id.to_string(),
            summary: Some("Execution authorized".to_string()),
        },
        intent_id: Some(record.intent_id),
        proposal_id: Some(record.proposal_id),
        execution_id: Some(record.execution_id),
        capability_id: Some(record.capability_id),
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
        metadata: lifecycle_event_metadata(&outbox, ferrum_proto::JsonMap::new()),
        source_runtime_id: None,
    };
    let auth_event_id = auth_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, auth_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) =
        mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, auth_event_id).await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsAuthorize,
        Ok(Json(AuthorizeExecutionResponse {
            execution: record,
            warnings: Vec::new(),
        }))
    )
}
