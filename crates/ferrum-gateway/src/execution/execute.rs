use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActionType, ActorRef, ActorType, ApiErrorCode, ApprovalMode, EventId, ExecutionState,
    ObjectRef, ObjectType, ProvenanceEvent, ProvenanceEventKind, RollbackClass,
};

use crate::AuthActor;
use crate::auth_actor::enforce_object_owner_guard;
use crate::execution::{
    lifecycle_event_metadata, mark_lifecycle_transition_reconciled, parse_execution_id,
    record_lifecycle_transition_outbox, record_lifecycle_transition_outbox_with_obligations,
    validate_argument_constraints, validate_capability_proposal_binding,
    validate_minimum_lineage_chain,
};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::state::AppState;

pub(crate) async fn execute_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    auth_actor: Option<Extension<AuthActor>>,
    Json(request): Json<ferrum_proto::ExecuteExecutionRequest>,
) -> Result<(StatusCode, Json<ferrum_proto::ExecuteExecutionResponse>), ApiProblem> {
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
                ApiProblem::object_not_found()
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

    // P1.4d: exact owner access guard before any state mutation or adapter call.
    if let Err(problem) = enforce_object_owner_guard(
        auth_actor.as_ref().map(|Extension(a)| a),
        execution.owner_actor_id.as_ref(),
        state.server_config.auth_mode,
        state.server_config.legacy_object_compat_allow_until,
        Utc::now(),
        "execution",
        "execute",
    ) {
        return governance_err!(state, GovernanceRoute::ExecutionsExecute, problem);
    }

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

    // Slice 1 safety gate: legacy persisted R2Compensatable contracts for
    // HttpMutation or SqlMutation are unsafe to execute (no safe generic
    // compensation exists). Reject before capability/lineage checks and before
    // the state transition to Running would hand off to the adapter.
    if matches!(contract.rollback_class, RollbackClass::R2Compensatable)
        && matches!(
            contract.action_type,
            ActionType::HttpMutation | ActionType::SqlMutation
        )
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "R2Compensatable executions for HTTP/SQLite mutations are not supported",
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

    // Call execute on the adapter via the rollback service.
    let adapter_result = state
        .runtime
        .rollback
        .execute(&contract, &adapter_payload)
        .await;

    if let Err(_adapter_error) = adapter_result {
        // Adapter error after Running: the effect is ambiguous. Atomically persist
        // paired RecoveryRequired states and an ErrorRaised obligation, then return
        // a bounded 202 recovery-required response. No raw adapter error is leaked.
        let previous_contract = contract.clone();
        let mut updated_contract = contract.clone();
        updated_contract.state = ferrum_proto::RollbackState::RecoveryRequired;
        updated_contract
            .metadata
            .insert("recovery_required".to_string(), serde_json::json!(true));
        updated_contract
            .metadata
            .insert("execute_payload".to_string(), adapter_payload.clone());

        let previous_execution = execution.clone();
        let mut updated_execution = execution;
        updated_execution.state = ferrum_proto::ExecutionState::RecoveryRequired;

        let mut previous_execution = previous_execution;
        previous_execution.state = ferrum_proto::ExecutionState::Running;

        let outbox = match record_lifecycle_transition_outbox_with_obligations(
            &state.runtime.store,
            "execute-recovery",
            &previous_execution,
            &updated_execution,
            Some(&previous_contract),
            Some(&updated_contract),
            vec![ProvenanceEventKind::ErrorRaised],
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

        let recovery_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::ErrorRaised,
            occurred_at: Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("FerrumGate Gateway".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::SideEffect,
                object_id: execution_id.to_string(),
                summary: Some("Adapter error after Running; recovery required".to_string()),
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
            hash_chain: ferrum_proto::HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: {
                let mut m = ferrum_proto::JsonMap::new();
                m.insert("recovery_required".to_string(), serde_json::json!(true));
                m.insert(
                    "bounded_reason".to_string(),
                    serde_json::json!(
                        "adapter reported a recoverable error; execution requires manual review"
                    ),
                );
                lifecycle_event_metadata(&outbox, m)
            },
            source_runtime_id: None,
        };
        let recovery_event_id = recovery_event.event_id;
        if let Err(e) = append_governance_event(&state.runtime.store, recovery_event).await {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
        if let Err(e) =
            mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, recovery_event_id)
                .await
        {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }

        return governance_ok!(
            state,
            GovernanceRoute::ExecutionsExecute,
            Ok((
                StatusCode::ACCEPTED,
                Json(ferrum_proto::ExecuteExecutionResponse {
                    execution_id,
                    executed: false,
                    recovery_required: true,
                    result_digest: None,
                    rollback_contract: Some(updated_contract),
                    warnings: vec![
                        "adapter reported a recoverable error; execution requires manual review"
                            .to_string(),
                    ],
                }),
            ))
        );
    }

    let receipt = adapter_result.unwrap();

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
    // On success, transition to AwaitingVerification so the verify step can
    // later commit, fail, or enter recovery as a distinct lifecycle phase.
    let mut previous_execution = execution.clone();
    previous_execution.state = ferrum_proto::ExecutionState::Running;
    let mut updated_execution = execution;
    updated_execution.state = ferrum_proto::ExecutionState::AwaitingVerification;
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
        hash_chain: ferrum_proto::HashChainRef {
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
        Ok((
            StatusCode::OK,
            Json(ferrum_proto::ExecuteExecutionResponse {
                execution_id,
                executed: true,
                recovery_required: false,
                result_digest: receipt.result_digest,
                rollback_contract: Some(updated_contract),
                warnings: Vec::new(),
            }),
        ))
    )
}
