use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, HashChainRef, ObjectRef, ObjectType, ProvenanceEvent,
    ProvenanceEventKind, RollbackClass,
};

use crate::execution::{
    lifecycle_event_metadata, mark_lifecycle_obligation_written, parse_execution_id,
    record_lifecycle_transition_outbox_with_obligations,
};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::state::AppState;

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
        event_id: ferrum_proto::EventId::new(),
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
            event_id: ferrum_proto::EventId::new(),
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
