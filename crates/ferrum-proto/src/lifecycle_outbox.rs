use crate::{
    EventId, ExecutionId, ExecutionState, JsonMap, LifecycleOutboxId, ProvenanceEdgeType,
    ProvenanceEventKind, RollbackContractId, RollbackState, Timestamp, lineage_parent_spec,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum LifecycleOutboxStatus {
    PendingProvenance,
    ProvenanceWritten,
    Reconciled,
    NeedsOperatorReview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceObligation {
    pub event_kind: ProvenanceEventKind,
    pub parent_kind: Option<ProvenanceEventKind>,
    pub edge_type: Option<ProvenanceEdgeType>,
    pub event_id: Option<EventId>,
}

impl ProvenanceObligation {
    pub fn pending(event_kind: ProvenanceEventKind) -> Self {
        let (parent_kind, edge_type) = lineage_parent_spec(&event_kind)
            .map(|(parent, edge)| (Some(parent), Some(edge)))
            .unwrap_or((None, None));
        Self {
            event_kind,
            parent_kind,
            edge_type,
            event_id: None,
        }
    }

    pub fn is_satisfied(&self) -> bool {
        self.event_id.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleOutboxRecord {
    pub outbox_id: LifecycleOutboxId,
    pub execution_id: ExecutionId,
    pub rollback_contract_id: Option<RollbackContractId>,
    pub previous_execution_state: Option<ExecutionState>,
    pub new_execution_state: ExecutionState,
    pub previous_rollback_state: Option<RollbackState>,
    pub new_rollback_state: Option<RollbackState>,
    pub intended_provenance_kind: ProvenanceEventKind,
    pub idempotency_key: String,
    pub status: LifecycleOutboxStatus,
    pub provenance_event_id: Option<EventId>,
    #[serde(default)]
    pub provenance_obligations: Vec<ProvenanceObligation>,
    pub attempt_count: u32,
    pub last_error: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleOutboxListResponse {
    pub items: Vec<LifecycleOutboxRecord>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleOutboxRetryRequest {
    pub actor_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleOutboxRetryResponse {
    pub record: LifecycleOutboxRecord,
    pub reconciliation_report: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleOutboxResolveRequest {
    pub actor_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LifecycleOutboxResolveResponse {
    pub record: LifecycleOutboxRecord,
}

impl LifecycleOutboxRecord {
    pub fn pending(
        execution_id: ExecutionId,
        rollback_contract_id: Option<RollbackContractId>,
        previous_execution_state: Option<ExecutionState>,
        new_execution_state: ExecutionState,
        previous_rollback_state: Option<RollbackState>,
        new_rollback_state: Option<RollbackState>,
        intended_provenance_kind: ProvenanceEventKind,
        idempotency_key: String,
    ) -> Self {
        Self::pending_with_obligations(
            execution_id,
            rollback_contract_id,
            previous_execution_state,
            new_execution_state,
            previous_rollback_state,
            new_rollback_state,
            vec![intended_provenance_kind],
            idempotency_key,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn pending_with_obligations(
        execution_id: ExecutionId,
        rollback_contract_id: Option<RollbackContractId>,
        previous_execution_state: Option<ExecutionState>,
        new_execution_state: ExecutionState,
        previous_rollback_state: Option<RollbackState>,
        new_rollback_state: Option<RollbackState>,
        intended_provenance_kinds: Vec<ProvenanceEventKind>,
        idempotency_key: String,
    ) -> Self {
        let now = chrono::Utc::now();
        let intended_provenance_kind = intended_provenance_kinds
            .first()
            .cloned()
            .unwrap_or(ProvenanceEventKind::ErrorRaised);
        Self {
            outbox_id: LifecycleOutboxId::new(),
            execution_id,
            rollback_contract_id,
            previous_execution_state,
            new_execution_state,
            previous_rollback_state,
            new_rollback_state,
            intended_provenance_kind: intended_provenance_kind.clone(),
            idempotency_key,
            status: LifecycleOutboxStatus::PendingProvenance,
            provenance_event_id: None,
            provenance_obligations: intended_provenance_kinds
                .into_iter()
                .map(ProvenanceObligation::pending)
                .collect(),
            attempt_count: 0,
            last_error: None,
            created_at: now,
            updated_at: now,
            metadata: JsonMap::new(),
        }
    }
}
