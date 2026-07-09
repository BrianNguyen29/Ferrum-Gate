//! Pure execution helpers for the gateway.
//!
//! Stages 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10 of the server.rs refactor: move the helper functions used
//! by execution handlers out of `server.rs` so that the handler modules can
//! stay focused on transport concerns.
//!
//! Scope (Stage 2 — pure helpers):
//! - Argument-constraint validation
//! - Resource-binding subset-of-scope validation
//! - Rollback prepare request construction
//! - Action / adapter / target inference from tool name and resource scope
//! - Rollback class inference
//! - HTTP compensation enrichment
//! - Path-UUID parsing (`parse_execution_id`)
//!
//! Scope (Stage 3 — async store/capability helpers):
//! - `get_capability_for_authorize` — load a capability from the in-memory
//!   service with a persisted-store fallback for `authorize_execution`.
//! - `mark_capability_used_durable` — mark a capability consumed in memory and
//!   persist the updated status (with atomic store-only fallback).
//! - `validate_approval_binding_digest` — enforce I6 binding-digest invariants
//!   before authorizing an execution.
//!
//! Scope (Stage 4 — low-risk HTTP handlers):
//! - `cancel_execution` — pre-side-effect guard, audit + provenance emission
//!   (moves the execution to Canceled only before adapter side effects start).
//! - `evaluate_outcome` — PDP outcome evaluation that returns the alignment
//!   verdict (allowed/forbidden vs. actual effect).
//!
//! Scope (Stage 5 — explicit manual commit handler):
//! - `commit_execution` — terminal-state guard, rollback contract `Verified`
//!   guard, `auto_commit=false` guard, `SideEffectVerified` provenance
//!   prerequisite, transition to `Committed`, emit `SideEffectCommitted`
//!   provenance event. R3/manual commit semantics preserved verbatim.
//!
//! Scope (Stage 6 — compensate HTTP handler):
//! - `compensate_execution` — state guard (ExecutedAwaitingVerify contract +
//!   Running/AwaitingVerification execution), HTTP compensation enrichment
//!   before rollback, rollback `compensate` invocation, transition to
//!   `Compensated`, and emit `SideEffectCompensated` provenance event.
//!
//! Scope (Stage 7 — verify HTTP handler):
//! - `verify_execution` — state guard (ExecutedAwaitingVerify contract +
//!   Running/AwaitingVerification execution), rollback `verify` invocation,
//!   conditional `auto_commit` branch (Verified → Committed vs.
//!   Running/AwaitingVerification), `FileHashMatches` expected_hash
//!   injection from `result_digest`, transition to `Verified`/`Failed`,
//!   emit `SideEffectVerified` provenance, and conditional
//!   `SideEffectCommitted` provenance only when verified && auto_commit.
//!
//! Scope (Stage 8 — execute HTTP handler):
//! - `execute_execution` — argument-constraint validation, DraftOnly
//!   defense-in-depth guard, lineage prerequisite gate (Prepared contract
//!   and Prepared/Authorized/Proposed execution), rollback `execute`
//!   invocation, transition to `ExecutedAwaitingVerify` (contract) /
//!   `Running` (execution), `result_digest` propagation, and emit
//!   `ToolCallExecuted` provenance event.
//!
//! Scope (Stage 9 — prepare HTTP handler):
//! - `prepare_execution` — execution/proposal/intent lookup, D1.5
//!   state guard (only `Authorized`/`Prepared` execution states accepted),
//!   DraftOnly intent guard, rollback `prepare` call, rollback contract
//!   insert, execution state update, and emit two provenance events
//!   (`SideEffectPrepared` and `ToolCallPrepared`).
//!
//! Scope (Stage 10 — authorize HTTP handler):
//! - `authorize_execution` — capability load/fallback via
//!   `get_capability_for_authorize`, I5 resource binding subset validation,
//!   I6 approval binding digest validation, durable single-use mark via
//!   `mark_capability_used_durable`, execution insert, and
//!   `ActionProposalSubmitted` provenance emission. Mechanical move; all
//!   invariants preserved verbatim.
//!
//! Out of scope (kept in `server.rs` until later stages):
//! - Non-execution handlers (policy, approval, lineage, admin, monitoring).
//!
//! Out of scope (kept in `server.rs` until later stages):
//! - HTTP handlers for the authorize / prepare lifecycle.
//!   `authorize_execution` is intentionally last due to single-use capability
//!   risk.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_cap::CapabilityError;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, ApprovalMode, AuthorizeExecutionRequest,
    AuthorizeExecutionResponse, CommitExecutionResponse, CompensateExecutionResponse, Decision,
    EventId, ExecutionId, ExecutionRecord, ExecutionState, HashChainRef, LifecycleOutboxRecord,
    ObjectRef, ObjectType, PrepareExecutionResponse, ProvenanceEvent, ProvenanceEventKind,
    ProvenanceQueryRequest, RollbackClass, RollbackState,
};
use std::sync::Arc;

use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::{append_governance_event, validate_minimum_lineage_chain};
use crate::state::AppState;

mod approval_binding;
mod cancel;
mod durable_capability;
mod evaluate_outcome;
mod inference;
mod lifecycle_outbox;
mod resource_scope;
mod validation;

pub(crate) use approval_binding::validate_approval_binding_digest;
pub(crate) use cancel::cancel_execution;
pub(crate) use durable_capability::get_capability_for_authorize;
#[allow(unused_imports)]
pub(crate) use durable_capability::mark_capability_used_durable;
pub(crate) use evaluate_outcome::evaluate_outcome;
pub(crate) use inference::{
    build_prepare_request_for_proposal, enrich_http_compensation_if_needed, infer_rollback_class,
    parse_execution_id,
};
pub(crate) use lifecycle_outbox::{
    execution_is_cancelable_pre_side_effect, execution_is_terminal_for_commit,
    lifecycle_event_metadata, mark_lifecycle_obligation_written,
    mark_lifecycle_transition_reconciled, record_lifecycle_transition_outbox,
    record_lifecycle_transition_outbox_with_obligations,
};
pub(crate) use resource_scope::validate_resource_bindings_subset_of_scope;
pub(crate) use validation::{validate_argument_constraints, validate_capability_proposal_binding};

#[cfg(test)]
pub(crate) use inference::infer_action_type_and_adapter;

#[cfg(test)]
pub(crate) use validation::effective_arguments;

// ---------------------------------------------------------------------------
// Stage 4 — Low-risk HTTP handlers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Stage 4 — Low-risk HTTP handlers
// ---------------------------------------------------------------------------

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
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
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
        hash_chain: HashChainRef {
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

/// Compensate a running or awaiting-verification execution by invoking the
/// rollback service's `compensate` action and transitioning the contract and
/// execution to `Compensated`. Emits a `SideEffectCompensated` provenance
/// event for lineage.
///
/// State guard (WS-Compensate):
/// - Contract must be in `ExecutedAwaitingVerify`.
/// - Execution must be in `Running` or `AwaitingVerification`.
///
/// Other states yield a 409 Conflict.
pub(crate) async fn compensate_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<CompensateExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsCompensate, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
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
                GovernanceRoute::ExecutionsCompensate,
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
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Compensate state guard
    match (&contract.state, &execution.state) {
        (RollbackState::ExecutedAwaitingVerify, ExecutionState::Running)
        | (RollbackState::ExecutedAwaitingVerify, ExecutionState::AwaitingVerification) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "compensate not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Enrich HTTP placeholder compensation plans before compensate so that
    // parse_replay_contract can validate method/payload/expected_statuses.
    let contract = enrich_http_compensation_if_needed(contract);

    // Call compensate on the contract. The adapter may report that the call
    // completed but recovery did not; that must not be promoted to a recovered
    // terminal state.
    let recovery_receipt = match state.runtime.rollback.compensate(&contract).await {
        Ok(receipt) => receipt,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(e)
            );
        }
    };

    let recovery_incomplete = !recovery_receipt.recovered;

    // Update contract state to Compensated only after recovered=true.
    let previous_contract = contract.clone();
    let mut updated_contract = contract.clone();
    updated_contract.state = if recovery_incomplete {
        RollbackState::Failed
    } else {
        RollbackState::Compensated
    };
    updated_contract.metadata.insert(
        "recovered".to_string(),
        serde_json::json!(recovery_receipt.recovered),
    );
    updated_contract.metadata.insert(
        "recovery_action".to_string(),
        serde_json::json!("compensate"),
    );
    if recovery_incomplete {
        updated_contract
            .metadata
            .insert("recovery_incomplete".to_string(), serde_json::json!(true));
    }
    if !recovery_receipt.adapter_metadata.is_empty() {
        updated_contract.metadata.insert(
            "recovery_adapter_metadata".to_string(),
            serde_json::json!(recovery_receipt.adapter_metadata.clone()),
        );
    }
    // Update execution state to Compensated only after recovered=true.
    let previous_execution = execution.clone();
    let mut updated_execution = execution;
    updated_execution.state = if recovery_incomplete {
        ExecutionState::Failed
    } else {
        ExecutionState::Compensated
    };
    updated_execution.finished_at = Some(Utc::now());
    let terminal_kind = if recovery_incomplete {
        ProvenanceEventKind::ErrorRaised
    } else {
        ProvenanceEventKind::SideEffectCompensated
    };
    let outbox = match record_lifecycle_transition_outbox(
        &state.runtime.store,
        "compensate",
        &previous_execution,
        &updated_execution,
        Some(&previous_contract),
        Some(&updated_contract),
        terminal_kind.clone(),
    )
    .await
    {
        Ok(outbox) => outbox,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Emit provenance event. Incomplete recovery is terminal but not recovered,
    // so emit ErrorRaised with explicit recovery metadata instead of
    // SideEffectCompensated.
    let terminal_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: terminal_kind,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some(if recovery_incomplete {
                "Recovery incomplete after compensation".to_string()
            } else {
                "Execution compensated".to_string()
            }),
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
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: {
            let mut metadata = ferrum_proto::JsonMap::new();
            metadata.insert(
                "recovered".to_string(),
                serde_json::json!(recovery_receipt.recovered),
            );
            metadata.insert(
                "recovery_action".to_string(),
                serde_json::json!("compensate"),
            );
            if recovery_incomplete {
                metadata.insert("recovery_incomplete".to_string(), serde_json::json!(true));
                metadata.insert(
                    "recovery_state".to_string(),
                    serde_json::json!("incomplete"),
                );
            }
            if !recovery_receipt.adapter_metadata.is_empty() {
                metadata.insert(
                    "recovery_adapter_metadata".to_string(),
                    serde_json::json!(recovery_receipt.adapter_metadata),
                );
            }
            metadata
        },
        source_runtime_id: None,
    };
    let terminal_event_id = terminal_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, terminal_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) =
        mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, terminal_event_id).await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCompensate,
        Ok(Json(CompensateExecutionResponse {
            execution_id,
            compensated: !recovery_incomplete,
            rollback_contract: Some(updated_contract),
            warnings: if recovery_incomplete {
                vec![
                    "recovery-incomplete: compensation adapter did not report recovered=true"
                        .to_string(),
                ]
            } else {
                Vec::new()
            },
        }))
    )
}

/// Verify a previously executed side effect by invoking the rollback
/// service's `verify` action and transitioning the contract and execution
/// based on the outcome. Emits a `SideEffectVerified` provenance event, and
/// conditionally a `SideEffectCommitted` event when verification succeeds
/// and the contract was prepared with `auto_commit=true`.
///
/// State guard (WS-Verify):
/// - Contract must be in `ExecutedAwaitingVerify`.
/// - Execution must be in `Running` or `AwaitingVerification`.
///
/// Other states yield a 409 Conflict.
///
/// D1.6 / R3 branching: when `verified=true && auto_commit=true`, the
/// execution transitions to `Committed` and a `SideEffectCommitted`
/// provenance event is emitted. When `auto_commit=false` (e.g. R3
/// irreversible-high-consequence), the execution remains in its current
/// state awaiting an explicit `/commit` call.
pub(crate) async fn verify_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::VerifyExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsVerify, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
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
                GovernanceRoute::ExecutionsVerify,
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
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Verify guard: contract must be ExecutedAwaitingVerify and execution must be
    // Running or AwaitingVerification. Return 409 Conflict for invalid state transitions.
    match (&contract.state, &execution.state) {
        (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::Running,
        )
        | (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::AwaitingVerification,
        ) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "verify not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Call verify on the adapter via the rollback service.
    // Before calling verify, update FileHashMatches checks with the result_digest
    // so that they can verify post-execute content hash.
    let mut verify_contract = contract.clone();
    if matches!(
        verify_contract.rollback_class,
        RollbackClass::R3IrreversibleHighConsequence
    ) {
        verify_contract.auto_commit = false;
    }
    if let Some(ref result_digest) = execution.result_digest {
        for check in &mut verify_contract.verify_checks {
            if matches!(check.check_type, ferrum_proto::CheckType::FileHashMatches) {
                check.config.insert(
                    "expected_hash".to_string(),
                    serde_json::json!(result_digest),
                );
            }
        }
        // Also update after_hash on the persisted contract for future reference
        if let ferrum_proto::RollbackTarget::FilePath {
            ref mut after_hash, ..
        } = verify_contract.target
        {
            *after_hash = Some(result_digest.clone());
        }
    }

    let verified = match state.runtime.rollback.verify(&verify_contract).await {
        Ok(verified) => verified,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(e)
            );
        }
    };

    // Update contract state based on verification result.
    // Persist verify_contract (not the original contract) so that verify-time
    // mutations (expected_hash on FileHashMatches checks, after_hash on target)
    // are stored for future inspection.
    let previous_contract = contract.clone();
    let mut updated_contract = verify_contract;
    updated_contract.state = if verified {
        ferrum_proto::RollbackState::Verified
    } else {
        ferrum_proto::RollbackState::Failed
    };
    // D1.6 / R3 enforcement: Only set execution to Committed (and emit SideEffectCommitted)
    // when verified=true AND contract.auto_commit=true. When auto_commit=false, the execution
    // remains in Running/AwaitingVerification state to await explicit commit.
    // This preserves the verified result in contract state while respecting rollback semantics.
    // R3 (irreversible-high-consequence) is normalized to auto_commit=false before verify;
    // explicit commit is required even if a malformed contract was inserted directly.
    let previous_execution = execution.clone();
    let mut updated_execution = execution;
    if verified {
        if updated_contract.auto_commit {
            // auto_commit=true: normal path - execution becomes Committed
            updated_execution.state = ferrum_proto::ExecutionState::Committed;
        } else {
            // auto_commit=false: verified but not committed - keep execution in current state
            // Contract is Verified but execution stays Running/AwaitingVerification
            // This allows explicit commit via separate flow when auto_commit=false
        }
    } else {
        updated_execution.state = ferrum_proto::ExecutionState::Failed;
    }
    let transition_provenance_kinds = if verified && updated_contract.auto_commit {
        vec![
            ProvenanceEventKind::SideEffectVerified,
            ProvenanceEventKind::SideEffectCommitted,
        ]
    } else {
        vec![ProvenanceEventKind::SideEffectVerified]
    };
    let outbox = match record_lifecycle_transition_outbox_with_obligations(
        &state.runtime.store,
        "verify",
        &previous_execution,
        &updated_execution,
        Some(&previous_contract),
        Some(&updated_contract),
        transition_provenance_kinds,
    )
    .await
    {
        Ok(outbox) => outbox,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Emit SideEffectVerified provenance event (regardless of verification result).
    let verified_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectVerified,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Side effect verified".to_string()),
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
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("verified".to_string(), serde_json::json!(verified));
            lifecycle_event_metadata(&outbox, m)
        },
        source_runtime_id: None,
    };
    let verified_event_id = verified_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, verified_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) = mark_lifecycle_obligation_written(
        &state.runtime.store,
        &outbox,
        ProvenanceEventKind::SideEffectVerified,
        verified_event_id,
    )
    .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if !(verified && updated_contract.auto_commit)
        && let Err(e) = {
            let mut result = ferrum_proto::JsonMap::new();
            result.insert("normal_path".to_string(), serde_json::json!(true));
            state
                .runtime
                .store
                .lifecycle_outbox()
                .mark_reconciled(outbox.outbox_id, result)
                .await
        }
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit SideEffectCommitted provenance event only when verification succeeded AND auto_commit=true.
    // When auto_commit=false, SideEffectCommitted is suppressed to preserve rollback semantics.
    if verified && updated_contract.auto_commit {
        let committed_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::SideEffectCommitted,
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
            hash_chain: HashChainRef {
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
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
        if let Err(e) = mark_lifecycle_obligation_written(
            &state.runtime.store,
            &outbox,
            ProvenanceEventKind::SideEffectCommitted,
            committed_event_id,
        )
        .await
        {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
        let mut result = ferrum_proto::JsonMap::new();
        result.insert("normal_path".to_string(), serde_json::json!(true));
        if let Err(e) = state
            .runtime
            .store
            .lifecycle_outbox()
            .mark_reconciled(outbox.outbox_id, result)
            .await
        {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsVerify,
        Ok(Json(ferrum_proto::VerifyExecutionResponse {
            execution_id,
            verified,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

pub(crate) async fn execute_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    Json(request): Json<ferrum_proto::ExecuteExecutionRequest>,
) -> Result<Json<ferrum_proto::ExecuteExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsExecute, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS3: Defense-in-depth — enforce draft-only guard at execute checkpoint.
    // Look up the intent and reject execution if the intent enforces draft-only mode.
    // This is defense-in-depth; prepare already blocks DraftOnly, but execute also
    // guards against any path that might bypass prepare.
    let intent = match state.runtime.store.intents().get(execution.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if matches!(intent.approval_mode, ApprovalMode::DraftOnly) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "draft-only intent cannot proceed to execute",
            )
        );
    }

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
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
                GovernanceRoute::ExecutionsExecute,
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
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Execute guard: contract must be Prepared and execution must be Prepared or Authorized.
    // Return 409 Conflict for invalid state transitions.
    match (&contract.state, &execution.state) {
        (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Prepared)
        | (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Authorized)
        | (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Proposed) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "execute not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    if contract.execution_id != execution.execution_id
        || contract.intent_id != execution.intent_id
        || contract.proposal_id != execution.proposal_id
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "rollback contract binding does not match execution",
            )
        );
    }

    let capability = match state
        .runtime
        .store
        .capabilities()
        .get(execution.capability_id)
        .await
    {
        Ok(Some(capability)) => capability,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "capability not found for execution",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    let proposal = match state
        .runtime
        .store
        .proposals()
        .get(execution.proposal_id)
        .await
    {
        Ok(Some(proposal)) => proposal,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "proposal not found for execution",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if let Err(reason) = validate_capability_proposal_binding(&capability, &proposal) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                reason,
            )
        );
    }

    let adapter_payload = if request.payload.is_null() {
        proposal.raw_arguments.clone()
    } else if request.payload == proposal.raw_arguments {
        request.payload.clone()
    } else {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "execute payload must exactly match approved proposal arguments",
            )
        );
    };
    if let Err(reason) =
        validate_argument_constraints(&adapter_payload, &capability.argument_constraints)
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(StatusCode::FORBIDDEN, ApiErrorCode::PolicyDenied, reason,)
        );
    }

    if let Err(reason) = validate_minimum_lineage_chain(&state.runtime.store, &execution).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(StatusCode::CONFLICT, ApiErrorCode::Conflict, reason,)
        );
    }

    match state
        .runtime
        .store
        .executions()
        .compare_and_set_state(
            execution_id,
            &[
                ExecutionState::Prepared,
                ExecutionState::Authorized,
                ExecutionState::Proposed,
            ],
            ExecutionState::Running,
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    "execute not allowed: execution was already claimed or state changed",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    }

    // Call execute on the adapter via the rollback service
    let receipt = match state
        .runtime
        .rollback
        .execute(&contract, &adapter_payload)
        .await
    {
        Ok(receipt) => receipt,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(e)
            );
        }
    };

    // Update contract state to ExecutedAwaitingVerify and capture after_hash from
    // the execute receipt so after_hash is available for inspection immediately
    // after execute (before verify has run).
    let previous_contract = contract.clone();
    let mut updated_contract = contract.clone();
    updated_contract.state = ferrum_proto::RollbackState::ExecutedAwaitingVerify;
    if let ferrum_proto::RollbackTarget::FilePath {
        ref mut after_hash, ..
    } = updated_contract.target
    {
        *after_hash = receipt.result_digest.clone();
    }
    // For HTTP targets, propagate request_digest from execute receipt into target
    // so that compensation replay can validate digest matching.
    if let ferrum_proto::RollbackTarget::HttpRequest {
        ref mut request_digest,
        ..
    } = updated_contract.target
    {
        if let Some(digest) = receipt
            .adapter_metadata
            .get("request_digest")
            .and_then(|v| v.as_str())
        {
            *request_digest = digest.to_string();
        }
    }
    // Propagate adapter_metadata from execute receipt into contract metadata so that
    // rollback/compensate can access critical fields (e.g., branch_name for GitBranchCreate).
    for (key, value) in &receipt.adapter_metadata {
        updated_contract.metadata.insert(key.clone(), value.clone());
    }
    // Store execute payload for later compensation enrichment (HTTP replay).
    updated_contract
        .metadata
        .insert("execute_payload".to_string(), adapter_payload.clone());
    // The execution was atomically claimed as Running before the adapter call.
    // Finalize result metadata with a CAS from that claimed state so a stale
    // completion cannot overwrite a concurrent lifecycle transition.
    let mut previous_execution = execution.clone();
    previous_execution.state = ferrum_proto::ExecutionState::Running;
    let mut updated_execution = execution;
    updated_execution.state = ferrum_proto::ExecutionState::Running;
    updated_execution.result_digest = receipt.result_digest.clone();
    let outbox = match record_lifecycle_transition_outbox(
        &state.runtime.store,
        "execute",
        &previous_execution,
        &updated_execution,
        Some(&previous_contract),
        Some(&updated_contract),
        ProvenanceEventKind::ToolCallExecuted,
    )
    .await
    {
        Ok(outbox) => outbox,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Emit ToolCallExecuted provenance event.
    let tool_executed_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ToolCallExecuted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Tool call executed".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: updated_execution.rollback_contract_id,
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
    let tool_executed_event_id = tool_executed_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, tool_executed_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) =
        mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, tool_executed_event_id)
            .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsExecute,
        Ok(Json(ferrum_proto::ExecuteExecutionResponse {
            execution_id,
            executed: true,
            result_digest: receipt.result_digest,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

// ---------------------------------------------------------------------------
// Stage 9 — Prepare HTTP handler
// ---------------------------------------------------------------------------

/// `POST /v1/executions/{execution_id}/prepare`
///
/// Prepares an execution by invoking the rollback service's `prepare` action
/// to mint a rollback contract. The handler enforces:
///
/// 1. D1.5 state guard — only `Authorized` or `Prepared` execution states
///    may transition to `Prepared`; all other states return 409 Conflict.
/// 2. DraftOnly intent guard — if the intent enforces `ApprovalMode::DraftOnly`,
///    prepare is rejected with 403 PolicyDenied (defense-in-depth in addition
///    to `evaluate` short-circuiting at this mode).
/// 3. Rollback contract insert — the contract from `rollback.prepare` is
///    persisted and the execution's `rollback_contract_id` is updated.
/// 4. Two provenance events — `SideEffectPrepared` and `ToolCallPrepared` —
///    are emitted through the governance provenance helper, which links each
///    event to its causal parent edge when the parent exists.
pub(crate) async fn prepare_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<PrepareExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsPrepare, e);
        }
    };

    // Look up the existing execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // D1.5 mandatory: Reject prepare for non-preparable execution states.
    // Only Authorized or Prepared executions can transition to Prepared.
    // All other states (Proposed, Running, Committed, Compensated, etc.) return 409 Conflict.
    match execution.state {
        ExecutionState::Authorized | ExecutionState::Prepared => {
            // Valid state - proceed with prepare
        }
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "execution in state '{:?}' cannot be prepared; only '{:?}' or '{:?}' are preparable",
                        execution.state,
                        ExecutionState::Authorized,
                        ExecutionState::Prepared
                    ),
                )
            );
        }
    }

    // Look up the proposal to retrieve the real rollback_class.
    // The proposal is the most reliable existing linked record for this execution.
    let proposal = match state
        .runtime
        .store
        .proposals()
        .get(execution.proposal_id)
        .await
    {
        Ok(Some(proposal)) => proposal,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "proposal not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    let rollback_class = proposal.requested_rollback_class.clone();

    // WS3: Enforce draft-only guard at prepare checkpoint.
    // Look up the intent and reject preparation if the intent enforces draft-only mode.
    // This prevents a draft-only intent from bypassing evaluate and reaching prepare.
    let intent = match state.runtime.store.intents().get(execution.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if matches!(intent.approval_mode, ApprovalMode::DraftOnly) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "draft-only intent cannot proceed to prepare",
            )
        );
    }

    let request = match build_prepare_request_for_proposal(
        &state.runtime.rollback,
        execution.intent_id,
        execution_id,
        &rollback_class,
        &proposal,
        &intent.resource_scope,
    ) {
        Ok(request) => request,
        Err(reason) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::BAD_REQUEST,
                    ApiErrorCode::ValidationError,
                    reason,
                )
            );
        }
    };

    let response = match state.runtime.rollback.prepare(request).await {
        Ok(response) => response,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(e)
            );
        }
    };

    // Capture execution IDs for provenance before moving into updated_execution
    let execution_intent_id = execution.intent_id;
    let execution_proposal_id = execution.proposal_id;
    let execution_capability_id = execution.capability_id;

    // Link the contract to the execution by updating rollback_contract_id
    let previous_execution = execution.clone();
    let mut updated_execution = execution;
    updated_execution.rollback_contract_id = Some(response.contract.contract_id);
    let updated_contract = response.contract.clone();
    let outbox = match record_lifecycle_transition_outbox_with_obligations(
        &state.runtime.store,
        "prepare",
        &previous_execution,
        &updated_execution,
        None,
        Some(&updated_contract),
        vec![
            ProvenanceEventKind::SideEffectPrepared,
            ProvenanceEventKind::ToolCallPrepared,
        ],
    )
    .await
    {
        Ok(outbox) => outbox,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Emit provenance event for preparation.
    let prepare_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectPrepared,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: response.contract.contract_id.to_string(),
            summary: Some("Execution prepared with rollback contract".to_string()),
        },
        intent_id: Some(execution_intent_id),
        proposal_id: Some(execution_proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(execution_capability_id),
        rollback_contract_id: Some(response.contract.contract_id),
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
    let prepare_event_id = prepare_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, prepare_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) = mark_lifecycle_obligation_written(
        &state.runtime.store,
        &outbox,
        ProvenanceEventKind::SideEffectPrepared,
        prepare_event_id,
    )
    .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit ToolCallPrepared provenance event.
    let tool_prepared_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ToolCallPrepared,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Tool call prepared for execution".to_string()),
        },
        intent_id: Some(execution_intent_id),
        proposal_id: Some(execution_proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(execution_capability_id),
        rollback_contract_id: Some(response.contract.contract_id),
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
    let tool_prepared_event_id = tool_prepared_event.event_id;
    if let Err(e) = append_governance_event(&state.runtime.store, tool_prepared_event).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    if let Err(e) = mark_lifecycle_obligation_written(
        &state.runtime.store,
        &outbox,
        ProvenanceEventKind::ToolCallPrepared,
        tool_prepared_event_id,
    )
    .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }
    let mut reconciliation_result = ferrum_proto::JsonMap::new();
    reconciliation_result.insert("normal_path".to_string(), serde_json::json!(true));
    if let Err(e) = state
        .runtime
        .store
        .lifecycle_outbox()
        .mark_reconciled(outbox.outbox_id, reconciliation_result)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsPrepare,
        Ok(Json(PrepareExecutionResponse {
            execution_id,
            prepared: response.accepted,
            rollback_contract: Some(response.contract),
            warnings: response.warnings,
        }))
    )
}

// ---------------------------------------------------------------------------
// Stage 10 — Authorize HTTP handler (single-use capability gate)
// ---------------------------------------------------------------------------

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
/// 5. Mark the capability as used in memory and persist the updated status
///    via `mark_capability_used_durable`. Returns `AlreadyUsed` if the
///    capability has already been consumed (single-use enforcement).
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
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(CapabilityError::AlreadyUsed)
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

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
