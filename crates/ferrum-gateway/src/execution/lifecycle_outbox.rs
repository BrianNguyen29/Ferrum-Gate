use std::sync::Arc;

use ferrum_proto::{
    EventId, ExecutionRecord, ExecutionState, LifecycleOutboxRecord, ProvenanceEventKind,
};
use ferrum_store::StoreFacade;

pub(crate) async fn record_lifecycle_transition_outbox(
    store: &Arc<dyn StoreFacade>,
    transition_name: &str,
    previous_execution: &ExecutionRecord,
    updated_execution: &ExecutionRecord,
    previous_contract: Option<&ferrum_proto::RollbackContract>,
    updated_contract: Option<&ferrum_proto::RollbackContract>,
    intended_provenance_kind: ProvenanceEventKind,
) -> ferrum_store::Result<LifecycleOutboxRecord> {
    record_lifecycle_transition_outbox_with_obligations(
        store,
        transition_name,
        previous_execution,
        updated_execution,
        previous_contract,
        updated_contract,
        vec![intended_provenance_kind],
    )
    .await
}

pub(crate) async fn record_lifecycle_transition_outbox_with_obligations(
    store: &Arc<dyn StoreFacade>,
    transition_name: &str,
    previous_execution: &ExecutionRecord,
    updated_execution: &ExecutionRecord,
    previous_contract: Option<&ferrum_proto::RollbackContract>,
    updated_contract: Option<&ferrum_proto::RollbackContract>,
    intended_provenance_kinds: Vec<ProvenanceEventKind>,
) -> ferrum_store::Result<LifecycleOutboxRecord> {
    let mut outbox = LifecycleOutboxRecord::pending_with_obligations(
        updated_execution.execution_id,
        updated_contract
            .map(|contract| contract.contract_id)
            .or(updated_execution.rollback_contract_id),
        Some(previous_execution.state.clone()),
        updated_execution.state.clone(),
        previous_contract.map(|contract| contract.state.clone()),
        updated_contract.map(|contract| contract.state.clone()),
        intended_provenance_kinds,
        format!(
            "{}:{}:{:?}:{}",
            transition_name,
            updated_execution.execution_id,
            updated_execution.state,
            updated_contract
                .map(|contract| format!("{:?}", contract.state))
                .unwrap_or_else(|| "none".to_string())
        ),
    );
    outbox
        .metadata
        .insert("transition".to_string(), serde_json::json!(transition_name));
    store
        .lifecycle_outbox()
        .record_lifecycle_transition(updated_execution, updated_contract, &outbox)
        .await?;
    Ok(outbox)
}

pub(crate) fn lifecycle_event_metadata(
    outbox: &LifecycleOutboxRecord,
    mut metadata: ferrum_proto::JsonMap,
) -> ferrum_proto::JsonMap {
    metadata.insert(
        "lifecycle_outbox_id".to_string(),
        serde_json::json!(outbox.outbox_id.to_string()),
    );
    metadata.insert(
        "idempotency_key".to_string(),
        serde_json::json!(outbox.idempotency_key.clone()),
    );
    metadata
}

pub(crate) fn execution_is_cancelable_pre_side_effect(state: &ExecutionState) -> bool {
    matches!(
        state,
        ExecutionState::Proposed
            | ExecutionState::Authorized
            | ExecutionState::Prepared
            | ExecutionState::AwaitingApproval
    )
}

pub(crate) fn execution_is_terminal_for_commit(state: &ExecutionState) -> bool {
    matches!(
        state,
        ExecutionState::Committed
            | ExecutionState::Compensated
            | ExecutionState::RolledBack
            | ExecutionState::Denied
            | ExecutionState::Quarantined
            | ExecutionState::Failed
            | ExecutionState::Canceled
    )
}

pub(crate) async fn mark_lifecycle_obligation_written(
    store: &Arc<dyn StoreFacade>,
    outbox: &LifecycleOutboxRecord,
    event_kind: ProvenanceEventKind,
    event_id: EventId,
) -> ferrum_store::Result<()> {
    let updated = store
        .lifecycle_outbox()
        .mark_provenance_obligation_written(outbox.outbox_id, event_kind, event_id)
        .await?;
    if updated {
        Ok(())
    } else {
        Err(ferrum_store::StoreError::Other(
            "lifecycle outbox obligation update did not affect any row".to_string(),
        ))
    }
}

pub(crate) async fn mark_lifecycle_transition_reconciled(
    store: &Arc<dyn StoreFacade>,
    outbox: &LifecycleOutboxRecord,
    event_id: EventId,
) -> ferrum_store::Result<()> {
    let outbox_repo = store.lifecycle_outbox();
    mark_lifecycle_obligation_written(
        store,
        outbox,
        outbox.intended_provenance_kind.clone(),
        event_id,
    )
    .await?;
    let mut result = ferrum_proto::JsonMap::new();
    result.insert("normal_path".to_string(), serde_json::json!(true));
    outbox_repo.mark_reconciled(outbox.outbox_id, result).await
}
