//! Provenance append and lifecycle-chain validation helpers.

use ferrum_proto::{
    Decision, ExecutionRecord, ProvenanceEdge, ProvenanceEdgeType, ProvenanceEvent,
    ProvenanceEventKind, ProvenanceQueryRequest, lineage_parent_spec,
};
use ferrum_store::{StoreError, StoreFacade};
use std::sync::Arc;

fn same_context(child: &ProvenanceEvent, parent: &ProvenanceEvent) -> bool {
    [
        (
            child.intent_id.map(|id| id.to_string()),
            parent.intent_id.map(|id| id.to_string()),
        ),
        (
            child.proposal_id.map(|id| id.to_string()),
            parent.proposal_id.map(|id| id.to_string()),
        ),
        (
            child.execution_id.map(|id| id.to_string()),
            parent.execution_id.map(|id| id.to_string()),
        ),
        (
            child.capability_id.map(|id| id.to_string()),
            parent.capability_id.map(|id| id.to_string()),
        ),
        (
            child.policy_bundle_id.map(|id| id.to_string()),
            parent.policy_bundle_id.map(|id| id.to_string()),
        ),
    ]
    .into_iter()
    .all(|(child_id, parent_id)| match (child_id, parent_id) {
        (Some(child_id), Some(parent_id)) => child_id == parent_id,
        _ => true,
    })
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

fn has_approval_granted_for_policy(
    events: &[ProvenanceEvent],
    execution: &ExecutionRecord,
    policy: &ProvenanceEvent,
) -> bool {
    events.iter().any(|event| {
        matches!(event.kind, ProvenanceEventKind::ApprovalGranted)
            && event.intent_id == Some(execution.intent_id)
            && event.proposal_id == Some(execution.proposal_id)
            && event.occurred_at >= policy.occurred_at
    })
}

fn same_event_kind(left: &ProvenanceEventKind, right: &ProvenanceEventKind) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

/// Append an internally-generated event and persist its inferred causal edge.
pub(crate) async fn append_governance_event(
    store: &Arc<dyn StoreFacade>,
    mut event: ProvenanceEvent,
) -> ferrum_store::Result<()> {
    let mut inferred_edges = Vec::new();
    if let Some((parent_kind, edge_type)) = lineage_parent_spec(&event.kind) {
        // Causal eligibility is determined by persisted visibility, matching kind,
        // and same-context validation; `occurred_at` is observational only and
        // must not gate parent resolution because parent/child wall-clock order can
        // invert under fast scheduling.
        let candidates = store
            .provenance()
            .query(&ProvenanceQueryRequest {
                intent_id: event.intent_id,
                execution_id: None,
                capability_id: None,
                event_kind: Some(parent_kind.clone()),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await?;

        if let Some(parent) = candidates
            .into_iter()
            .rev()
            .find(|candidate| same_context(&event, candidate))
        {
            inferred_edges.push(ProvenanceEdge {
                edge_type,
                from_event_id: parent.event_id,
                to_event_id: Some(event.event_id),
                summary: Some(format!("{:?} -> {:?}", parent.kind, event.kind)),
            });
        } else if event
            .metadata
            .get("lineage_parent_optional")
            .and_then(|value| value.as_bool())
            == Some(true)
        {
            event.metadata.insert(
                "lineage_parent_missing".to_string(),
                serde_json::json!(format!("{:?}", parent_kind)),
            );
        } else {
            return Err(StoreError::Other(format!(
                "lineage parent missing for {:?}: expected {:?}",
                event.kind, parent_kind
            )));
        }
    }

    event.parent_edges.extend(inferred_edges);
    store
        .provenance()
        .append_event_with_edges(&event, &event.parent_edges)
        .await?;
    Ok(())
}

fn find_event(
    events: &[ProvenanceEvent],
    predicate: impl Fn(&ProvenanceEvent) -> bool,
    label: &str,
) -> Result<ProvenanceEvent, String> {
    events
        .iter()
        .rev()
        .find(|event| predicate(event))
        .cloned()
        .ok_or_else(|| format!("minimum lineage prerequisite missing: {label}"))
}

async fn require_parent_event(
    store: &Arc<dyn StoreFacade>,
    child: &ProvenanceEvent,
    events: &[ProvenanceEvent],
    expected_parent_kind: ProvenanceEventKind,
    expected_edge_type: ProvenanceEdgeType,
    label: &str,
) -> Result<ProvenanceEvent, String> {
    let edges = store
        .provenance()
        .get_edges_to(child.event_id)
        .await
        .map_err(|e| format!("failed to load lineage edges: {e}"))?;

    for edge in edges
        .iter()
        .filter(|edge| edge.edge_type == expected_edge_type)
        .filter(|edge| {
            edge.to_event_id
                .is_none_or(|to_event_id| to_event_id.to_string() == child.event_id.to_string())
        })
    {
        if let Some(parent) = events
            .iter()
            .find(|event| event.event_id.to_string() == edge.from_event_id.to_string())
            .filter(|event| same_event_kind(&event.kind, &expected_parent_kind))
            .filter(|event| same_context(child, event))
        {
            return Ok(parent.clone());
        }
    }

    Err(format!(
        "minimum lineage edge missing: {label} -> {:?}",
        child.kind
    ))
}

/// Require the complete governed chain before an adapter side effect executes.
pub(crate) async fn validate_minimum_lineage_chain(
    store: &Arc<dyn StoreFacade>,
    execution: &ExecutionRecord,
) -> Result<(), String> {
    let events = store
        .provenance()
        .query(&ProvenanceQueryRequest {
            intent_id: Some(execution.intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
            edge_types: Vec::new(),
        })
        .await
        .map_err(|e| format!("failed to load minimum lineage: {e}"))?;

    let tool_prepared = find_event(
        &events,
        |event| {
            matches!(event.kind, ProvenanceEventKind::ToolCallPrepared)
                && event.execution_id == Some(execution.execution_id)
                && event.proposal_id == Some(execution.proposal_id)
        },
        "ToolCallPrepared",
    )?;

    let prepared = require_parent_event(
        store,
        &tool_prepared,
        &events,
        ProvenanceEventKind::SideEffectPrepared,
        ProvenanceEdgeType::Caused,
        "SideEffectPrepared",
    )
    .await?;
    if prepared.execution_id != Some(execution.execution_id)
        || prepared.proposal_id != Some(execution.proposal_id)
    {
        return Err("minimum lineage SideEffectPrepared context mismatch".to_string());
    }

    let submitted = require_parent_event(
        store,
        &prepared,
        &events,
        ProvenanceEventKind::ActionProposalSubmitted,
        ProvenanceEdgeType::Caused,
        "ActionProposalSubmitted",
    )
    .await?;
    if submitted.execution_id != Some(execution.execution_id)
        || submitted.proposal_id != Some(execution.proposal_id)
    {
        return Err("minimum lineage ActionProposalSubmitted context mismatch".to_string());
    }

    let capability = require_parent_event(
        store,
        &submitted,
        &events,
        ProvenanceEventKind::CapabilityMinted,
        ProvenanceEdgeType::AuthorizedBy,
        "CapabilityMinted",
    )
    .await?;
    if capability.proposal_id != Some(execution.proposal_id)
        || capability.capability_id != Some(execution.capability_id)
    {
        return Err("minimum lineage CapabilityMinted context mismatch".to_string());
    }

    let policy = require_parent_event(
        store,
        &capability,
        &events,
        ProvenanceEventKind::PolicyEvaluated,
        ProvenanceEdgeType::EvaluatedByPolicy,
        "PolicyEvaluated",
    )
    .await?;
    if policy.proposal_id != Some(execution.proposal_id) {
        return Err("minimum lineage PolicyEvaluated context mismatch".to_string());
    }

    match decision_from_policy_event(&policy) {
        Some(Decision::Allow) => {}
        Some(Decision::RequireApproval) => {
            if !has_approval_granted_for_policy(&events, execution, &policy) {
                return Err(
                    "minimum lineage requires ApprovalGranted for RequireApproval decision"
                        .to_string(),
                );
            }
        }
        Some(decision) => {
            return Err(format!(
                "minimum lineage policy decision {:?} does not allow side effect",
                decision
            ));
        }
        None => {
            return Err("minimum lineage PolicyEvaluated decision missing".to_string());
        }
    }

    let chain = [policy, capability, submitted, prepared, tool_prepared];
    for pair in chain.windows(2) {
        if pair[0].occurred_at > pair[1].occurred_at {
            return Err(format!(
                "minimum lineage order invalid: {:?} occurs after {:?}",
                pair[0].kind, pair[1].kind
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use ferrum_proto::{
        ActorRef, ActorType, CapabilityId, Decision, EventId, ExecutionId, ExecutionState,
        HashChainRef, IntentId, JsonMap, ObjectRef, ObjectType, ProposalId, ProvenanceEdge,
        ProvenanceEdgeType,
    };
    use ferrum_store::{
        AgentRepo, ApprovalRepo, AuditCheckpointRepo, AuditLogRepo, AuditMerkleRootRepo,
        CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, LifecycleOutboxRepo,
        MfaCredentialRepo, PolicyBundleRepo, ProposalRepo, ProvenanceRepo, QuarantineHoldRepo,
        RollbackRepo, SqliteStore, StoreFacade, TokenRepo,
    };
    use std::sync::Arc;

    fn test_event(
        kind: ProvenanceEventKind,
        occurred_at: chrono::DateTime<Utc>,
        execution: &ExecutionRecord,
        execution_id: Option<ExecutionId>,
        capability_id: Option<CapabilityId>,
    ) -> ProvenanceEvent {
        let mut metadata = JsonMap::new();
        if matches!(kind, ProvenanceEventKind::PolicyEvaluated) {
            metadata.insert("decision".to_string(), serde_json::json!("Allow"));
        }

        ProvenanceEvent {
            event_id: EventId::new(),
            kind,
            occurred_at,
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::SideEffect,
                object_id: "test".to_string(),
                summary: None,
            },
            intent_id: Some(execution.intent_id),
            proposal_id: Some(execution.proposal_id),
            execution_id,
            capability_id,
            rollback_contract_id: execution.rollback_contract_id,
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
            metadata,
            source_runtime_id: None,
        }
    }

    fn test_execution() -> ExecutionRecord {
        ExecutionRecord {
            execution_id: ExecutionId::new(),
            proposal_id: ProposalId::new(),
            intent_id: IntentId::new(),
            capability_id: CapabilityId::new(),
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Prepared,
            started_at: Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: JsonMap::new(),
            owner_actor_id: None,
        }
    }

    /// Test-only StoreFacade that proxies every repo except provenance, which
    /// always fails on `query`. Used to prove that lineage validation
    /// propagates store failures rather than silently succeeding.
    struct FailingProvenanceQueryStoreFacade {
        inner: Arc<dyn StoreFacade>,
    }

    impl FailingProvenanceQueryStoreFacade {
        fn new(inner: Arc<dyn StoreFacade>) -> Self {
            Self { inner }
        }
    }

    #[async_trait::async_trait]
    impl StoreFacade for FailingProvenanceQueryStoreFacade {
        fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
            self.inner.capabilities()
        }
        fn executions(&self) -> Arc<dyn ExecutionRepo> {
            self.inner.executions()
        }
        fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
            self.inner.rollback_contracts()
        }
        fn lifecycle_outbox(&self) -> Arc<dyn LifecycleOutboxRepo> {
            self.inner.lifecycle_outbox()
        }
        fn approvals(&self) -> Arc<dyn ApprovalRepo> {
            self.inner.approvals()
        }
        fn quarantine_holds(&self) -> Arc<dyn QuarantineHoldRepo> {
            self.inner.quarantine_holds()
        }
        fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
            Arc::new(FailingProvenanceQueryRepo)
        }
        fn ledger(&self) -> Arc<dyn LedgerRepo> {
            self.inner.ledger()
        }
        fn intents(&self) -> Arc<dyn IntentRepo> {
            self.inner.intents()
        }
        fn proposals(&self) -> Arc<dyn ProposalRepo> {
            self.inner.proposals()
        }
        fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
            self.inner.policy_bundles()
        }
        fn tokens(&self) -> Arc<dyn TokenRepo> {
            self.inner.tokens()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            self.inner.audit_log()
        }
        fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
            self.inner.audit_merkle_roots()
        }
        fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
            self.inner.audit_checkpoints()
        }
        fn agents(&self) -> Arc<dyn AgentRepo> {
            self.inner.agents()
        }
        fn mfa_credentials(&self) -> Arc<dyn MfaCredentialRepo> {
            self.inner.mfa_credentials()
        }
        fn write_queue_depth(&self) -> usize {
            self.inner.write_queue_depth()
        }
        async fn health_check(&self) -> ferrum_store::Result<()> {
            self.inner.health_check().await
        }
    }

    struct FailingProvenanceQueryRepo;

    #[async_trait::async_trait]
    impl ProvenanceRepo for FailingProvenanceQueryRepo {
        async fn append_event(
            &self,
            _event: &ferrum_proto::ProvenanceEvent,
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn append_event_with_edges(
            &self,
            _event: &ferrum_proto::ProvenanceEvent,
            _edges: &[ferrum_proto::ProvenanceEdge],
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn get_event(
            &self,
            _event_id: ferrum_proto::EventId,
        ) -> ferrum_store::Result<Option<ferrum_proto::ProvenanceEvent>> {
            Ok(None)
        }
        async fn append_edges(
            &self,
            _to_event_id: ferrum_proto::EventId,
            _edges: &[ferrum_proto::ProvenanceEdge],
        ) -> ferrum_store::Result<()> {
            Ok(())
        }
        async fn query(
            &self,
            _request: &ferrum_proto::ProvenanceQueryRequest,
        ) -> ferrum_store::Result<Vec<ferrum_proto::ProvenanceEvent>> {
            Err(ferrum_store::StoreError::Other(
                "provenance query failure for testing".to_string(),
            ))
        }
        async fn get_edges_to(
            &self,
            _to_event_id: ferrum_proto::EventId,
        ) -> ferrum_store::Result<Vec<ferrum_proto::ProvenanceEdge>> {
            Ok(Vec::new())
        }
        async fn get_edges_from(
            &self,
            _from_event_ids: &[ferrum_proto::EventId],
        ) -> ferrum_store::Result<Vec<ferrum_proto::ProvenanceEdge>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn minimum_lineage_rejects_empty_lineage() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();

        let error = validate_minimum_lineage_chain(&store, &execution)
            .await
            .unwrap_err();
        assert!(
            error.contains("ToolCallPrepared"),
            "error should name the missing prerequisite: {error}"
        );
    }

    #[tokio::test]
    async fn minimum_lineage_rejects_partial_chain() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        // Chain stops at SideEffectPrepared; ToolCallPrepared is missing.
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::PolicyEvaluated,
                base,
                &execution,
                None,
                None,
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::CapabilityMinted,
                base + Duration::milliseconds(1),
                &execution,
                None,
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ActionProposalSubmitted,
                base + Duration::milliseconds(2),
                &execution,
                Some(execution.execution_id),
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::SideEffectPrepared,
                base + Duration::milliseconds(3),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();

        let error = validate_minimum_lineage_chain(&store, &execution)
            .await
            .unwrap_err();
        assert!(
            error.contains("ToolCallPrepared"),
            "error should name the missing prerequisite: {error}"
        );
    }

    #[tokio::test]
    async fn minimum_lineage_rejects_missing_parent_edges() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        // Insert all required events directly, without edges. The events exist
        // but are not causally linked, so the chain should be rejected.
        for kind in [
            ProvenanceEventKind::PolicyEvaluated,
            ProvenanceEventKind::CapabilityMinted,
            ProvenanceEventKind::ActionProposalSubmitted,
            ProvenanceEventKind::SideEffectPrepared,
            ProvenanceEventKind::ToolCallPrepared,
        ] {
            let offset = match kind {
                ProvenanceEventKind::PolicyEvaluated => 0,
                ProvenanceEventKind::CapabilityMinted => 1,
                ProvenanceEventKind::ActionProposalSubmitted => 2,
                ProvenanceEventKind::SideEffectPrepared => 3,
                ProvenanceEventKind::ToolCallPrepared => 4,
                _ => 5,
            };
            let event = test_event(
                kind,
                base + Duration::milliseconds(offset),
                &execution,
                Some(execution.execution_id),
                Some(execution.capability_id),
            );
            store.provenance().append_event(&event).await.unwrap();
        }

        let error = validate_minimum_lineage_chain(&store, &execution)
            .await
            .unwrap_err();
        assert!(
            error.contains("SideEffectPrepared"),
            "error should name the missing edge: {error}"
        );
    }

    #[tokio::test]
    async fn minimum_lineage_rejects_wrong_parent_edge() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        // Build a correct chain first, then add a second PolicyEvaluated event.
        // Force ToolCallPrepared's parent edge to point to the later, unrelated
        // PolicyEvaluated instead of SideEffectPrepared.
        let policy = test_event(
            ProvenanceEventKind::PolicyEvaluated,
            base,
            &execution,
            None,
            None,
        );
        let policy_id = policy.event_id;
        append_governance_event(&store, policy).await.unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::CapabilityMinted,
                base + Duration::milliseconds(1),
                &execution,
                None,
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ActionProposalSubmitted,
                base + Duration::milliseconds(2),
                &execution,
                Some(execution.execution_id),
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::SideEffectPrepared,
                base + Duration::milliseconds(3),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();

        let wrong_parent = test_event(
            ProvenanceEventKind::PolicyEvaluated,
            base + Duration::milliseconds(4),
            &execution,
            None,
            None,
        );
        let wrong_parent_id = wrong_parent.event_id;
        store
            .provenance()
            .append_event(&wrong_parent)
            .await
            .unwrap();

        let mut tool_prepared = test_event(
            ProvenanceEventKind::ToolCallPrepared,
            base + Duration::milliseconds(5),
            &execution,
            Some(execution.execution_id),
            None,
        );
        tool_prepared.parent_edges.push(ProvenanceEdge {
            edge_type: ProvenanceEdgeType::Caused,
            from_event_id: wrong_parent_id,
            to_event_id: Some(tool_prepared.event_id),
            summary: None,
        });
        store
            .provenance()
            .append_event_with_edges(&tool_prepared, &tool_prepared.parent_edges)
            .await
            .unwrap();

        let error = validate_minimum_lineage_chain(&store, &execution)
            .await
            .unwrap_err();
        assert!(
            error.contains("SideEffectPrepared"),
            "error should identify the missing correct parent edge: {error}"
        );
        assert!(
            !error.contains(&policy_id.to_string()),
            "error should not be satisfied by the unrelated policy event"
        );
    }

    #[tokio::test]
    async fn minimum_lineage_propagates_store_query_failure() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let failing: Arc<dyn StoreFacade> = Arc::new(FailingProvenanceQueryStoreFacade::new(store));

        let error = validate_minimum_lineage_chain(&failing, &execution)
            .await
            .unwrap_err();
        assert!(
            error.contains("failed to load minimum lineage"),
            "error should propagate store failure: {error}"
        );
    }

    #[tokio::test]
    async fn append_governance_event_rejects_missing_required_parent() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        let error = append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::CapabilityMinted,
                base,
                &execution,
                None,
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("PolicyEvaluated"));
    }

    #[tokio::test]
    async fn minimum_lineage_follows_persisted_edges_not_latest_kind() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::PolicyEvaluated,
                base,
                &execution,
                None,
                None,
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::CapabilityMinted,
                base + Duration::milliseconds(1),
                &execution,
                None,
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::PolicyEvaluated,
                base + Duration::milliseconds(2),
                &execution,
                None,
                None,
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ActionProposalSubmitted,
                base + Duration::milliseconds(3),
                &execution,
                Some(execution.execution_id),
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::SideEffectPrepared,
                base + Duration::milliseconds(4),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ToolCallPrepared,
                base + Duration::milliseconds(5),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();

        validate_minimum_lineage_chain(&store, &execution)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn minimum_lineage_rejects_denied_policy_decision() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        let mut policy = test_event(
            ProvenanceEventKind::PolicyEvaluated,
            base,
            &execution,
            None,
            None,
        );
        policy
            .metadata
            .insert("decision".to_string(), serde_json::json!("Deny"));
        append_governance_event(&store, policy).await.unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::CapabilityMinted,
                base + Duration::milliseconds(1),
                &execution,
                None,
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ActionProposalSubmitted,
                base + Duration::milliseconds(2),
                &execution,
                Some(execution.execution_id),
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::SideEffectPrepared,
                base + Duration::milliseconds(3),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ToolCallPrepared,
                base + Duration::milliseconds(4),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();

        let error = validate_minimum_lineage_chain(&store, &execution)
            .await
            .unwrap_err();
        assert!(error.contains("Deny"));
    }

    #[tokio::test]
    async fn append_governance_event_uses_persisted_parent_with_later_timestamp() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::PolicyEvaluated,
                base,
                &execution,
                None,
                None,
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::CapabilityMinted,
                base + Duration::milliseconds(1),
                &execution,
                None,
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ActionProposalSubmitted,
                base + Duration::milliseconds(2),
                &execution,
                Some(execution.execution_id),
                Some(execution.capability_id),
            ),
        )
        .await
        .unwrap();
        append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::SideEffectPrepared,
                base + Duration::milliseconds(3),
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap();
        let prepared = test_event(
            ProvenanceEventKind::ToolCallPrepared,
            base + Duration::milliseconds(5),
            &execution,
            Some(execution.execution_id),
            None,
        );
        let prepared_id = prepared.event_id;
        append_governance_event(&store, prepared).await.unwrap();

        let executed = test_event(
            ProvenanceEventKind::ToolCallExecuted,
            base + Duration::milliseconds(4),
            &execution,
            Some(execution.execution_id),
            None,
        );
        let executed_id = executed.event_id;
        append_governance_event(&store, executed).await.unwrap();

        let edges = store.provenance().get_edges_to(executed_id).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_type, ProvenanceEdgeType::Caused);
        assert_eq!(edges[0].from_event_id, prepared_id);
    }

    #[tokio::test]
    async fn append_governance_event_rejects_tool_call_executed_without_parent() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let store: Arc<dyn StoreFacade> = store;
        let execution = test_execution();
        let base = Utc::now();

        let error = append_governance_event(
            &store,
            test_event(
                ProvenanceEventKind::ToolCallExecuted,
                base,
                &execution,
                Some(execution.execution_id),
                None,
            ),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("ToolCallPrepared"));
    }
}
