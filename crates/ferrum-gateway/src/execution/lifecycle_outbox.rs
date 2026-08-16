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

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActionType, CapabilityId, CapabilityLease, CapabilityStatus, Decision, EventId,
        ExecutionId, ExecutionRecord, ExecutionState, IntentId, JsonMap, PrincipalId, ProposalId,
        ProvenanceEventKind, RiskTier, RollbackClass, RollbackContract, RollbackContractId,
        RollbackState, RollbackTarget, ToolBinding,
    };
    use ferrum_store::{
        AgentRepo, ApprovalRepo, AuditCheckpointRepo, AuditLogRepo, AuditMerkleRootRepo,
        CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, LifecycleOutboxClaim,
        LifecycleOutboxLease, LifecycleOutboxLeaseStats, LifecycleOutboxRepo, MfaCredentialRepo,
        PolicyBundleRepo, ProposalRepo, ProvenanceRepo, QuarantineHoldRepo,
        ReconciliationFailureDisposition, RollbackRepo, SqliteStore, StoreError, StoreFacade,
        TokenRepo,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    fn test_execution() -> ExecutionRecord {
        ExecutionRecord {
            execution_id: ExecutionId::new(),
            proposal_id: ProposalId::new(),
            intent_id: IntentId::new(),
            capability_id: CapabilityId::new(),
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Authorized,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: JsonMap::new(),
            owner_actor_id: None,
        }
    }

    fn test_contract() -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            execution_id: ExecutionId::new(),
            action_type: ActionType::FileWrite,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "noop".to_string(),
            target: RollbackTarget::Generic {
                namespace: "test".to_string(),
                identifier: "target".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::PendingPrepare,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: JsonMap::new(),
        }
    }

    fn test_outbox() -> LifecycleOutboxRecord {
        LifecycleOutboxRecord::pending(
            ExecutionId::new(),
            None,
            Some(ExecutionState::Authorized),
            ExecutionState::Running,
            Some(RollbackState::PendingPrepare),
            Some(RollbackState::Prepared),
            ProvenanceEventKind::SideEffectPrepared,
            "test-idempotency".to_string(),
        )
    }

    /// Test-only LifecycleOutboxRepo that always fails `record_lifecycle_transition`.
    struct FailingRecordLifecycleTransitionRepo;

    #[async_trait::async_trait]
    impl LifecycleOutboxRepo for FailingRecordLifecycleTransitionRepo {
        async fn enqueue_lifecycle_transition(
            &self,
            _record: &LifecycleOutboxRecord,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn record_lifecycle_transition(
            &self,
            _execution: &ExecutionRecord,
            _rollback_contract: Option<&RollbackContract>,
            _outbox: &LifecycleOutboxRecord,
        ) -> ferrum_store::Result<()> {
            Err(StoreError::Other(
                "record_lifecycle_transition failure for testing".to_string(),
            ))
        }
        async fn record_authorization(
            &self,
            _capability: &CapabilityLease,
            _execution: &ExecutionRecord,
            _outbox: &LifecycleOutboxRecord,
        ) -> ferrum_store::Result<bool> {
            Ok(false)
        }
        async fn mark_provenance_written(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _event_id: EventId,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn mark_provenance_obligation_written(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _event_kind: ProvenanceEventKind,
            _event_id: EventId,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_reconciled(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _result: JsonMap,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn mark_needs_operator_review(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _reason: String,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn reset_for_retry(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _actor_id: String,
            _reason: Option<String>,
        ) -> ferrum_store::Result<Option<LifecycleOutboxRecord>> {
            Ok(None)
        }
        async fn mark_operator_resolved(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _actor_id: String,
            _reason: String,
        ) -> ferrum_store::Result<Option<LifecycleOutboxRecord>> {
            Ok(None)
        }
        async fn get(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
        ) -> ferrum_store::Result<Option<LifecycleOutboxRecord>> {
            Ok(None)
        }
        async fn list_by_status(
            &self,
            _status: ferrum_proto::LifecycleOutboxStatus,
            _limit: u32,
        ) -> ferrum_store::Result<Vec<LifecycleOutboxRecord>> {
            Ok(Vec::new())
        }
        async fn claim_pending_reconciliation(
            &self,
            _limit: u32,
            _lease_owner: &str,
            _lease_ttl: chrono::Duration,
        ) -> ferrum_store::Result<Vec<LifecycleOutboxClaim>> {
            Ok(Vec::new())
        }
        async fn renew_reconciliation_lease(
            &self,
            _lease: &LifecycleOutboxLease,
            _lease_ttl: chrono::Duration,
        ) -> ferrum_store::Result<bool> {
            Ok(false)
        }
        async fn mark_provenance_written_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _event_id: EventId,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_provenance_obligation_written_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _event_kind: ProvenanceEventKind,
            _event_id: EventId,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_reconciled_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _result: JsonMap,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_needs_operator_review_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _reason: String,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn record_reconciliation_failure(
            &self,
            _lease: &LifecycleOutboxLease,
            _error: String,
            _max_attempts: u32,
        ) -> ferrum_store::Result<ReconciliationFailureDisposition> {
            Ok(ReconciliationFailureDisposition::Retryable)
        }
        async fn reconciliation_lease_stats(
            &self,
        ) -> ferrum_store::Result<LifecycleOutboxLeaseStats> {
            Ok(LifecycleOutboxLeaseStats::default())
        }
        async fn list_pending_reconciliation(
            &self,
            _limit: u32,
        ) -> ferrum_store::Result<Vec<LifecycleOutboxRecord>> {
            Ok(Vec::new())
        }
    }

    struct FailingRecordLifecycleTransitionStoreFacade;

    #[async_trait::async_trait]
    impl StoreFacade for FailingRecordLifecycleTransitionStoreFacade {
        fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
            unimplemented!()
        }
        fn executions(&self) -> Arc<dyn ExecutionRepo> {
            unimplemented!()
        }
        fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
            unimplemented!()
        }
        fn lifecycle_outbox(&self) -> Arc<dyn LifecycleOutboxRepo> {
            Arc::new(FailingRecordLifecycleTransitionRepo)
        }
        fn approvals(&self) -> Arc<dyn ApprovalRepo> {
            unimplemented!()
        }
        fn quarantine_holds(&self) -> Arc<dyn QuarantineHoldRepo> {
            unimplemented!()
        }
        fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
            unimplemented!()
        }
        fn ledger(&self) -> Arc<dyn LedgerRepo> {
            unimplemented!()
        }
        fn intents(&self) -> Arc<dyn IntentRepo> {
            unimplemented!()
        }
        fn proposals(&self) -> Arc<dyn ProposalRepo> {
            unimplemented!()
        }
        fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
            unimplemented!()
        }
        fn tokens(&self) -> Arc<dyn TokenRepo> {
            unimplemented!()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            unimplemented!()
        }
        fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
            unimplemented!()
        }
        fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
            unimplemented!()
        }
        fn agents(&self) -> Arc<dyn AgentRepo> {
            unimplemented!()
        }
        fn mfa_credentials(&self) -> Arc<dyn MfaCredentialRepo> {
            unimplemented!()
        }
        fn write_queue_depth(&self) -> usize {
            0
        }
        async fn health_check(&self) -> ferrum_store::Result<()> {
            Ok(())
        }
    }

    /// Test-only LifecycleOutboxRepo that returns false for obligation CAS and
    /// records whether `mark_reconciled` was called.
    struct ZeroRowObligationRepo {
        mark_reconciled_called: AtomicBool,
    }

    impl ZeroRowObligationRepo {
        fn new() -> Self {
            Self {
                mark_reconciled_called: AtomicBool::new(false),
            }
        }
        fn reconciled(&self) -> bool {
            self.mark_reconciled_called.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl LifecycleOutboxRepo for ZeroRowObligationRepo {
        async fn enqueue_lifecycle_transition(
            &self,
            _record: &LifecycleOutboxRecord,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn record_lifecycle_transition(
            &self,
            _execution: &ExecutionRecord,
            _rollback_contract: Option<&RollbackContract>,
            _outbox: &LifecycleOutboxRecord,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn record_authorization(
            &self,
            _capability: &CapabilityLease,
            _execution: &ExecutionRecord,
            _outbox: &LifecycleOutboxRecord,
        ) -> ferrum_store::Result<bool> {
            Ok(false)
        }
        async fn mark_provenance_written(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _event_id: EventId,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn mark_provenance_obligation_written(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _event_kind: ProvenanceEventKind,
            _event_id: EventId,
        ) -> ferrum_store::Result<bool> {
            Ok(false)
        }
        async fn mark_reconciled(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _result: JsonMap,
        ) -> ferrum_store::Result<()> {
            self.mark_reconciled_called.store(true, Ordering::Relaxed);
            Ok(())
        }
        async fn mark_needs_operator_review(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _reason: String,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn reset_for_retry(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _actor_id: String,
            _reason: Option<String>,
        ) -> ferrum_store::Result<Option<LifecycleOutboxRecord>> {
            Ok(None)
        }
        async fn mark_operator_resolved(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
            _actor_id: String,
            _reason: String,
        ) -> ferrum_store::Result<Option<LifecycleOutboxRecord>> {
            Ok(None)
        }
        async fn get(
            &self,
            _outbox_id: ferrum_proto::LifecycleOutboxId,
        ) -> ferrum_store::Result<Option<LifecycleOutboxRecord>> {
            Ok(None)
        }
        async fn list_by_status(
            &self,
            _status: ferrum_proto::LifecycleOutboxStatus,
            _limit: u32,
        ) -> ferrum_store::Result<Vec<LifecycleOutboxRecord>> {
            Ok(Vec::new())
        }
        async fn claim_pending_reconciliation(
            &self,
            _limit: u32,
            _lease_owner: &str,
            _lease_ttl: chrono::Duration,
        ) -> ferrum_store::Result<Vec<LifecycleOutboxClaim>> {
            Ok(Vec::new())
        }
        async fn renew_reconciliation_lease(
            &self,
            _lease: &LifecycleOutboxLease,
            _lease_ttl: chrono::Duration,
        ) -> ferrum_store::Result<bool> {
            Ok(false)
        }
        async fn mark_provenance_written_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _event_id: EventId,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_provenance_obligation_written_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _event_kind: ProvenanceEventKind,
            _event_id: EventId,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_reconciled_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _result: JsonMap,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn mark_needs_operator_review_claimed(
            &self,
            _lease: &LifecycleOutboxLease,
            _reason: String,
        ) -> ferrum_store::Result<bool> {
            Ok(true)
        }
        async fn record_reconciliation_failure(
            &self,
            _lease: &LifecycleOutboxLease,
            _error: String,
            _max_attempts: u32,
        ) -> ferrum_store::Result<ReconciliationFailureDisposition> {
            Ok(ReconciliationFailureDisposition::Retryable)
        }
        async fn reconciliation_lease_stats(
            &self,
        ) -> ferrum_store::Result<LifecycleOutboxLeaseStats> {
            Ok(LifecycleOutboxLeaseStats::default())
        }
        async fn list_pending_reconciliation(
            &self,
            _limit: u32,
        ) -> ferrum_store::Result<Vec<LifecycleOutboxRecord>> {
            Ok(Vec::new())
        }
    }

    struct ZeroRowObligationStoreFacade {
        outbox_repo: Arc<ZeroRowObligationRepo>,
    }

    impl ZeroRowObligationStoreFacade {
        fn new(outbox_repo: Arc<ZeroRowObligationRepo>) -> Self {
            Self { outbox_repo }
        }
    }

    #[async_trait::async_trait]
    impl StoreFacade for ZeroRowObligationStoreFacade {
        fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
            unimplemented!()
        }
        fn executions(&self) -> Arc<dyn ExecutionRepo> {
            unimplemented!()
        }
        fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
            unimplemented!()
        }
        fn lifecycle_outbox(&self) -> Arc<dyn LifecycleOutboxRepo> {
            self.outbox_repo.clone()
        }
        fn approvals(&self) -> Arc<dyn ApprovalRepo> {
            unimplemented!()
        }
        fn quarantine_holds(&self) -> Arc<dyn QuarantineHoldRepo> {
            unimplemented!()
        }
        fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
            unimplemented!()
        }
        fn ledger(&self) -> Arc<dyn LedgerRepo> {
            unimplemented!()
        }
        fn intents(&self) -> Arc<dyn IntentRepo> {
            unimplemented!()
        }
        fn proposals(&self) -> Arc<dyn ProposalRepo> {
            unimplemented!()
        }
        fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
            unimplemented!()
        }
        fn tokens(&self) -> Arc<dyn TokenRepo> {
            unimplemented!()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            unimplemented!()
        }
        fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
            unimplemented!()
        }
        fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
            unimplemented!()
        }
        fn agents(&self) -> Arc<dyn AgentRepo> {
            unimplemented!()
        }
        fn mfa_credentials(&self) -> Arc<dyn MfaCredentialRepo> {
            unimplemented!()
        }
        fn write_queue_depth(&self) -> usize {
            0
        }
        async fn health_check(&self) -> ferrum_store::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn record_lifecycle_transition_outbox_propagates_store_error() {
        let store: Arc<dyn StoreFacade> = Arc::new(FailingRecordLifecycleTransitionStoreFacade);
        let previous_execution = test_execution();
        let mut updated_execution = previous_execution.clone();
        updated_execution.state = ExecutionState::Running;

        let result = record_lifecycle_transition_outbox(
            &store,
            "prepare",
            &previous_execution,
            &updated_execution,
            None,
            None,
            ProvenanceEventKind::SideEffectPrepared,
        )
        .await;

        assert!(result.is_err(), "store error must be propagated");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("record_lifecycle_transition failure"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn record_lifecycle_transition_outbox_with_obligations_sets_multiple_obligations() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;

        let previous_execution = test_execution();
        let mut updated_execution = previous_execution.clone();
        updated_execution.state = ExecutionState::Running;

        // Seed required foreign rows so the transaction can succeed.
        let intent = ferrum_proto::IntentEnvelope {
            intent_id: previous_execution.intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
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
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            owner_actor_id: None,
        };
        let proposal = ferrum_proto::ActionProposal {
            proposal_id: previous_execution.proposal_id,
            intent_id: previous_execution.intent_id,
            step_index: 0,
            title: "test".to_string(),
            tool_name: "test".to_string(),
            server_name: "test".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        };
        let capability = CapabilityLease {
            capability_id: previous_execution.capability_id,
            intent_id: previous_execution.intent_id,
            proposal_id: previous_execution.proposal_id,
            tool_binding: ToolBinding {
                server_name: "test".to_string(),
                tool_name: "test".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![],
            argument_constraints: vec![],
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            issued_by: "test".to_string(),
            policy_bundle_id: ferrum_proto::PolicyBundleId::new(),
            tool_manifest_id: None,
            manifest_hash: None,
            status: CapabilityStatus::Active,
            issued_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            revoked_at: None,
            metadata: JsonMap::new(),
            owner_actor_id: None,
        };
        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        store.capabilities().insert(&capability).await.unwrap();
        store
            .executions()
            .insert(&previous_execution)
            .await
            .unwrap();
        let mut contract = test_contract();
        contract.intent_id = previous_execution.intent_id;
        contract.proposal_id = previous_execution.proposal_id;
        contract.execution_id = previous_execution.execution_id;
        store.rollback_contracts().insert(&contract).await.unwrap();

        let mut updated_execution = previous_execution.clone();
        updated_execution.rollback_contract_id = Some(contract.contract_id);
        updated_execution.state = ExecutionState::Running;
        let mut updated_contract = contract.clone();
        updated_contract.state = RollbackState::Prepared;

        let outbox = record_lifecycle_transition_outbox_with_obligations(
            &store,
            "prepare",
            &previous_execution,
            &updated_execution,
            Some(&contract),
            Some(&updated_contract),
            vec![
                ProvenanceEventKind::SideEffectPrepared,
                ProvenanceEventKind::ToolCallPrepared,
            ],
        )
        .await
        .unwrap();

        assert_eq!(outbox.provenance_obligations.len(), 2);
        assert_eq!(
            outbox.provenance_obligations[0].event_kind,
            ProvenanceEventKind::SideEffectPrepared
        );
        assert_eq!(
            outbox.provenance_obligations[1].event_kind,
            ProvenanceEventKind::ToolCallPrepared
        );
        assert_eq!(outbox.metadata.get("transition").unwrap(), "prepare");

        let stored = store
            .lifecycle_outbox()
            .get(outbox.outbox_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.provenance_obligations.len(), 2);
    }

    #[tokio::test]
    async fn mark_lifecycle_obligation_written_rejects_zero_row_update() {
        let outbox_repo = Arc::new(ZeroRowObligationRepo::new());
        let store: Arc<dyn StoreFacade> = Arc::new(ZeroRowObligationStoreFacade::new(outbox_repo));
        let outbox = test_outbox();
        let event_id = EventId::new();

        let result = mark_lifecycle_obligation_written(
            &store,
            &outbox,
            ProvenanceEventKind::SideEffectPrepared,
            event_id,
        )
        .await;

        assert!(result.is_err(), "zero-row CAS update must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("did not affect any row"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn mark_lifecycle_transition_reconciled_does_not_reconcile_on_obligation_failure() {
        let outbox_repo = Arc::new(ZeroRowObligationRepo::new());
        let store: Arc<dyn StoreFacade> =
            Arc::new(ZeroRowObligationStoreFacade::new(outbox_repo.clone()));
        let outbox = test_outbox();
        let event_id = EventId::new();

        let result = mark_lifecycle_transition_reconciled(&store, &outbox, event_id).await;

        assert!(result.is_err(), "obligation failure must be propagated");
        assert!(
            !outbox_repo.reconciled(),
            "mark_reconciled must not be called when obligation write fails"
        );
    }

    #[test]
    fn lifecycle_event_metadata_includes_outbox_and_idempotency() {
        let outbox = test_outbox();
        let metadata = lifecycle_event_metadata(&outbox, JsonMap::new());
        assert_eq!(
            metadata.get("lifecycle_outbox_id").unwrap(),
            &serde_json::json!(outbox.outbox_id.to_string())
        );
        assert_eq!(
            metadata.get("idempotency_key").unwrap(),
            &serde_json::json!(outbox.idempotency_key.clone())
        );
    }

    #[test]
    fn execution_is_cancelable_pre_side_effect_recognizes_pre_side_effect_states() {
        assert!(execution_is_cancelable_pre_side_effect(
            &ExecutionState::Proposed
        ));
        assert!(execution_is_cancelable_pre_side_effect(
            &ExecutionState::Authorized
        ));
        assert!(execution_is_cancelable_pre_side_effect(
            &ExecutionState::Prepared
        ));
        assert!(execution_is_cancelable_pre_side_effect(
            &ExecutionState::AwaitingApproval
        ));
        assert!(!execution_is_cancelable_pre_side_effect(
            &ExecutionState::Running
        ));
        assert!(!execution_is_cancelable_pre_side_effect(
            &ExecutionState::Committed
        ));
    }

    #[test]
    fn execution_is_terminal_for_commit_recognizes_terminal_states() {
        assert!(execution_is_terminal_for_commit(&ExecutionState::Committed));
        assert!(execution_is_terminal_for_commit(
            &ExecutionState::Compensated
        ));
        assert!(execution_is_terminal_for_commit(
            &ExecutionState::RolledBack
        ));
        assert!(execution_is_terminal_for_commit(&ExecutionState::Denied));
        assert!(execution_is_terminal_for_commit(
            &ExecutionState::Quarantined
        ));
        assert!(execution_is_terminal_for_commit(&ExecutionState::Failed));
        assert!(execution_is_terminal_for_commit(&ExecutionState::Canceled));
        assert!(!execution_is_terminal_for_commit(&ExecutionState::Proposed));
        assert!(!execution_is_terminal_for_commit(&ExecutionState::Running));
    }
}
