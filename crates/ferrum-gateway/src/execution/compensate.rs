use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, CompensateExecutionResponse, EventId, ExecutionState,
    ObjectRef, ObjectType, ProvenanceEvent, ProvenanceEventKind, RollbackState,
};

use crate::AuthActor;
use crate::auth_actor::enforce_object_owner_guard;
use crate::execution::{
    enrich_http_compensation_if_needed, mark_lifecycle_transition_reconciled, parse_execution_id,
    record_lifecycle_transition_outbox,
};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::state::AppState;

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
    auth_actor: Option<Extension<AuthActor>>,
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
                ApiProblem::object_not_found()
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

    // P1.4d: exact owner access guard before any state mutation or adapter call.
    if let Err(problem) = enforce_object_owner_guard(
        auth_actor.as_ref().map(|Extension(a)| a),
        execution.owner_actor_id.as_ref(),
        state.server_config.auth_mode,
        state.server_config.legacy_object_compat_allow_until,
        Utc::now(),
        "execution",
        "compensate",
    ) {
        return governance_err!(state, GovernanceRoute::ExecutionsCompensate, problem);
    }

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
        hash_chain: ferrum_proto::HashChainRef {
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
