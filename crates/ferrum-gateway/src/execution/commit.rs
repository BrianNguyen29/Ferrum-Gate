use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, CommitExecutionResponse, EventId, ExecutionState, ObjectRef,
    ObjectType, ProvenanceEvent, ProvenanceEventKind, ProvenanceQueryRequest, RollbackState,
};

use crate::AuthActor;
use crate::auth_actor::enforce_object_owner_guard;
use crate::execution::{
    execution_is_terminal_for_commit, lifecycle_event_metadata,
    mark_lifecycle_transition_reconciled, parse_execution_id, record_lifecycle_transition_outbox,
};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::state::AppState;

/// `POST /v1/executions/{execution_id}/commit`
///
/// Explicit manual commit handler. Transitions a verified execution into the
/// `Committed` terminal state for rollback contracts that were prepared with
/// `auto_commit=false`. This is the R3 manual commit boundary.
///
/// Guards (preserved verbatim from the original `server.rs` implementation):
/// 1. Execution must not already be in a terminal state (`Committed`,
///    `Compensated`, `RolledBack`, `Failed`) → `409 Conflict`.
/// 2. Execution must have a `rollback_contract_id` → `404 Not Found`.
/// 3. The rollback contract must exist → `404 Not Found`.
/// 4. The rollback contract state must be `Verified` → `409 Conflict`.
/// 5. The rollback contract must not have been prepared with `auto_commit=true`
///    (those are auto-committed by verify) → `409 Conflict`.
/// 6. A `SideEffectVerified` provenance event must exist for the execution
///    → `409 Conflict`.
///
/// On success: transitions both the rollback contract and the execution to
/// `Committed`, emits a `SideEffectCommitted` provenance event, and returns
/// the `CommitExecutionResponse`.
pub(crate) async fn commit_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    auth_actor: Option<Extension<AuthActor>>,
) -> Result<Json<CommitExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsCommit, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::object_not_found()
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // P1.4d: exact owner access guard before any state mutation.
    if let Err(problem) = enforce_object_owner_guard(
        auth_actor.as_ref().map(|Extension(a)| a),
        execution.owner_actor_id.as_ref(),
        state.server_config.auth_mode,
        state.server_config.legacy_object_compat_allow_until,
        Utc::now(),
        "execution",
        "commit",
    ) {
        return governance_err!(state, GovernanceRoute::ExecutionsCommit, problem);
    }

    // Reject if execution is already in a terminal state.
    if execution_is_terminal_for_commit(&execution.state) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "commit not allowed: execution is already in terminal state {:?}",
                    execution.state
                ),
            )
        );
    }

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Reject if contract is not Verified.
    if !matches!(contract.state, RollbackState::Verified) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "commit not allowed: rollback contract is in state {:?}, expected Verified",
                    contract.state
                ),
            )
        );
    }

    // Reject if contract was prepared with auto_commit=true (verify already committed).
    if contract.auto_commit {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "commit not allowed: contract was prepared with auto_commit=true \
                 (execution was already auto-committed by verify)",
            )
        );
    }

    // Verify that a SideEffectVerified provenance event exists for this execution.
    let verified_events = match state
        .runtime
        .store
        .provenance()
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectVerified),
            since: None,
            until: None,
            edge_types: vec![],
        })
        .await
    {
        Ok(events) => events,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    if verified_events.is_empty() {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "commit not allowed: no SideEffectVerified provenance event found",
            )
        );
    }

    // Transition both contract and execution to Committed.
    let previous_contract = contract.clone();
    let mut updated_contract = contract;
    updated_contract.state = RollbackState::Committed;

    let previous_execution = execution.clone();
    let mut updated_execution = execution;
    updated_execution.state = ExecutionState::Committed;
    let outbox = match record_lifecycle_transition_outbox(
        &state.runtime.store,
        "commit",
        &previous_execution,
        &updated_execution,
        Some(&previous_contract),
        Some(&updated_contract),
        ProvenanceEventKind::SideEffectCommitted,
    )
    .await
    {
        Ok(outbox) => outbox,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCommit,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Emit SideEffectCommitted provenance event.
    let committed_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::SideEffectCommitted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Side effect committed".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: Some(updated_contract.contract_id),
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
        metadata: lifecycle_event_metadata(&outbox, ferrum_proto::JsonMap::new()),
        source_runtime_id: None,
    };
    let committed_event_id = committed_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, committed_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) =
        mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, committed_event_id)
            .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCommit,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCommit,
        Ok(Json(CommitExecutionResponse {
            execution_id,
            committed: true,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}
