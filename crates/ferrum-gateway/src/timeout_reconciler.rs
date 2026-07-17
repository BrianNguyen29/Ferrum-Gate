use std::sync::Arc;
use std::sync::atomic::Ordering;

use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, EventId, HashChainRef, ObjectRef, ObjectType, ProvenanceEvent,
    ProvenanceEventKind,
};
use tokio::time::{MissedTickBehavior, interval};

use crate::state::AppState;

const APPROVAL_TIMEOUT_BATCH_SIZE: u32 = 100;
const QUARANTINE_TIMEOUT_BATCH_SIZE: u32 = 100;

/// Emit a provenance event recording that an approval timed out.
pub(crate) async fn emit_approval_timed_out_provenance(
    state: &AppState,
    approval: &ferrum_proto::ApprovalRequest,
) {
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "approval_id".to_string(),
        serde_json::json!(approval.approval_id.to_string()),
    );
    metadata.insert(
        "previous_state".to_string(),
        serde_json::json!(format!("{:?}", approval.state)),
    );

    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ApprovalTimedOut,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Approval,
            object_id: approval.approval_id.to_string(),
            summary: Some("Approval timed out and was transitioned to Expired".to_string()),
        },
        intent_id: Some(approval.intent_id),
        proposal_id: Some(approval.proposal_id),
        execution_id: approval.execution_id,
        capability_id: None,
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
        metadata,
        source_runtime_id: None,
    };

    if let Err(e) = state.runtime.store.provenance().append_event(&event).await {
        tracing::warn!(
            error = %e,
            approval_id = %approval.approval_id,
            "failed to append ApprovalTimedOut provenance event"
        );
    }
}

/// Reconcile one batch of stale pending approvals.
///
/// Returns the approvals that were transitioned to `Expired`. This is the
/// production tick implementation used by the background loop.
pub(crate) async fn reconcile_approval_timeouts_once(
    state: &AppState,
    now: chrono::DateTime<Utc>,
) -> Vec<ferrum_proto::ApprovalRequest> {
    let timeout_seconds = state.server_config.approval_timeout_seconds;
    match state
        .runtime
        .store
        .approvals()
        .expire_stale_pending(now, timeout_seconds, APPROVAL_TIMEOUT_BATCH_SIZE)
        .await
    {
        Ok(expired) => {
            for approval in &expired {
                state
                    .metrics
                    .approval_timeouts_total
                    .fetch_add(1, Ordering::Relaxed);
                emit_approval_timed_out_provenance(state, approval).await;
            }
            if !expired.is_empty() {
                tracing::info!(
                    count = expired.len(),
                    "approval timeout reconciliation expired stale pending approvals"
                );
            }
            expired
        }
        Err(error) => {
            tracing::error!(%error, "approval timeout reconciliation failed");
            Vec::new()
        }
    }
}

/// Background task that periodically reconciles stale pending approvals.
pub(crate) async fn approval_timeout_reconciler(
    state: Arc<AppState>,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let interval_secs = state.server_config.approval_reconciliation_interval_secs;
    let mut interval = interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Utc::now();
                reconcile_approval_timeouts_once(&state, now).await;
            }
            _ = shutdown.notified() => {
                tracing::info!("approval timeout reconciler shutting down");
                break;
            }
        }
    }
}

/// Emit a provenance event recording that a quarantine hold timed out.
pub(crate) async fn emit_quarantine_timed_out_provenance(
    state: &AppState,
    hold: &ferrum_proto::QuarantineHold,
) {
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "hold_id".to_string(),
        serde_json::json!(hold.hold_id.to_string()),
    );
    metadata.insert(
        "previous_state".to_string(),
        serde_json::json!(format!("{:?}", hold.state)),
    );

    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::QuarantineTimedOut,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::QuarantineHold,
            object_id: hold.hold_id.to_string(),
            summary: Some("Quarantine hold timed out and was transitioned to Expired".to_string()),
        },
        intent_id: Some(hold.intent_id),
        proposal_id: Some(hold.proposal_id),
        execution_id: None,
        capability_id: None,
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
        metadata,
        source_runtime_id: None,
    };

    if let Err(e) = crate::provenance::append_governance_event(&state.runtime.store, event).await {
        tracing::warn!(
            error = %e,
            hold_id = %hold.hold_id,
            "failed to append QuarantineTimedOut provenance event"
        );
    }
}

/// Reconcile one batch of stale pending quarantine holds.
///
/// Returns the holds that were transitioned to `Expired`. This is the
/// production tick implementation used by the background loop.
pub(crate) async fn reconcile_quarantine_timeouts_once(
    state: &AppState,
    now: chrono::DateTime<Utc>,
) -> Vec<ferrum_proto::QuarantineHold> {
    let timeout_seconds = state.server_config.quarantine_timeout_seconds;
    match state
        .runtime
        .store
        .quarantine_holds()
        .expire_stale_pending(now, timeout_seconds, QUARANTINE_TIMEOUT_BATCH_SIZE)
        .await
    {
        Ok(expired) => {
            for hold in &expired {
                state
                    .metrics
                    .quarantine_timeouts_total
                    .fetch_add(1, Ordering::Relaxed);
                emit_quarantine_timed_out_provenance(state, hold).await;
            }
            if !expired.is_empty() {
                tracing::info!(
                    count = expired.len(),
                    "quarantine timeout reconciliation expired stale pending holds"
                );
            }
            expired
        }
        Err(error) => {
            tracing::error!(%error, "quarantine timeout reconciliation failed");
            Vec::new()
        }
    }
}

/// Background task that periodically reconciles stale pending quarantine holds.
pub(crate) async fn quarantine_timeout_reconciler(
    state: Arc<AppState>,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let interval_secs = state.server_config.quarantine_reconciliation_interval_secs;
    let mut interval = interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Utc::now();
                reconcile_quarantine_timeouts_once(&state, now).await;
            }
            _ = shutdown.notified() => {
                tracing::info!("quarantine timeout reconciler shutting down");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, GatewayRuntime, ServerConfig};
    use ferrum_cap::InMemoryCapabilityService;
    use ferrum_pdp::StaticPdpEngine;
    use ferrum_proto::{
        ActionProposal, ActorRef, ActorType, ApprovalId, ApprovalRequest, ApprovalState, EventId,
        HashChainRef, IntentEnvelope, IntentId, IntentStatus, JsonMap, ObjectRef, ObjectType,
        PrincipalId, ProposalId, ProvenanceEvent, ProvenanceEventKind, QuarantineHold,
        QuarantineHoldId, QuarantineHoldState, RiskTier, RollbackClass, TimeBudget,
        TrustContextSummary,
    };
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::{SqliteStore, StoreFacade};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    async fn test_state_with_config(server_config: ServerConfig) -> Arc<AppState> {
        let pdp = Arc::new(StaticPdpEngine);
        let cap = Arc::new(InMemoryCapabilityService::default());
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let runtime =
            GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
        AppState::test_new(runtime, server_config)
    }

    async fn test_state() -> Arc<AppState> {
        test_state_with_config(ServerConfig::default()).await
    }

    fn test_intent() -> IntentEnvelope {
        let now = Utc::now();
        IntentEnvelope {
            intent_id: IntentId::new(),
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
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
            status: IntentStatus::Active,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(15),
            owner_actor_id: None,
        }
    }

    fn test_proposal(intent_id: IntentId) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: Utc::now(),
            owner_actor_id: None,
        }
    }

    async fn seed_intent_and_proposal(state: &AppState) -> (IntentId, ProposalId) {
        let intent = test_intent();
        let proposal = test_proposal(intent.intent_id);
        state.runtime.store.intents().insert(&intent).await.unwrap();
        state
            .runtime
            .store
            .proposals()
            .insert(&proposal)
            .await
            .unwrap();
        (intent.intent_id, proposal.proposal_id)
    }

    fn test_approval(
        now: chrono::DateTime<Utc>,
        intent_id: IntentId,
        proposal_id: ProposalId,
        state: ApprovalState,
    ) -> ApprovalRequest {
        ApprovalRequest {
            approval_id: ApprovalId::new(),
            intent_id,
            proposal_id,
            execution_id: None,
            requested_by: ActorRef {
                actor_type: ActorType::User,
                actor_id: "test-actor".to_string(),
                display_name: Some("Test Actor".to_string()),
            },
            reason: "test approval".to_string(),
            action_digest: "digest".to_string(),
            expires_at: now + chrono::Duration::seconds(60),
            state,
            created_at: now - chrono::Duration::seconds(7200),
            resolver_evidence_version: None,
            owner_actor_id: None,
        }
    }

    fn test_quarantine_hold(
        now: chrono::DateTime<Utc>,
        intent_id: IntentId,
        proposal_id: ProposalId,
        state: QuarantineHoldState,
    ) -> QuarantineHold {
        QuarantineHold {
            hold_id: QuarantineHoldId::new(),
            intent_id,
            proposal_id,
            reason: "test quarantine".to_string(),
            matched_rule_ids: vec!["rule-1".to_string()],
            policy_bundle_id: None,
            state,
            expires_at: now - chrono::Duration::seconds(1),
            created_at: now - chrono::Duration::seconds(7200),
            resolved_at: None,
            resolved_by: None,
            resolution_reason: None,
            metadata: JsonMap::new(),
            owner_actor_id: None,
        }
    }

    async fn seed_quarantine_lineage(
        state: &AppState,
        hold: &QuarantineHold,
    ) -> (ProvenanceEvent, ProvenanceEvent) {
        let policy_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::PolicyEvaluated,
            occurred_at: Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::Proposal,
                object_id: hold.proposal_id.to_string(),
                summary: None,
            },
            intent_id: Some(hold.intent_id),
            proposal_id: Some(hold.proposal_id),
            execution_id: None,
            capability_id: None,
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
            metadata: {
                let mut m = JsonMap::new();
                m.insert("decision".to_string(), serde_json::json!("Quarantine"));
                m
            },
            source_runtime_id: None,
        };
        state
            .runtime
            .store
            .provenance()
            .append_event(&policy_event)
            .await
            .unwrap();

        let hold_created_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::QuarantineHoldCreated,
            occurred_at: Utc::now() + chrono::Duration::milliseconds(1),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::QuarantineHold,
                object_id: hold.hold_id.to_string(),
                summary: None,
            },
            intent_id: Some(hold.intent_id),
            proposal_id: Some(hold.proposal_id),
            execution_id: None,
            capability_id: None,
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
            metadata: JsonMap::new(),
            source_runtime_id: None,
        };
        state
            .runtime
            .store
            .provenance()
            .append_event(&hold_created_event)
            .await
            .unwrap();

        (policy_event, hold_created_event)
    }

    // Repository tests: exercise the underlying approval repo directly.

    #[tokio::test]
    async fn approval_expire_stale_pending_expires_and_emits_provenance() {
        let state = test_state().await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let approval = test_approval(now, intent_id, proposal_id, ApprovalState::Pending);
        state
            .runtime
            .store
            .approvals()
            .insert(&approval)
            .await
            .unwrap();

        let expired = state
            .runtime
            .store
            .approvals()
            .expire_stale_pending(now, 3600, 100)
            .await
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].approval_id, approval.approval_id);

        emit_approval_timed_out_provenance(&state, &expired[0]).await;

        state
            .metrics
            .approval_timeouts_total
            .fetch_add(1, Ordering::Relaxed);
        assert_eq!(
            state
                .metrics
                .approval_timeouts_total
                .load(Ordering::Relaxed),
            1
        );
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(approval.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::ApprovalTimedOut),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].object.object_id, approval.approval_id.to_string());

        let stored = state
            .runtime
            .store
            .approvals()
            .get(approval.approval_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(stored.state, ApprovalState::Expired));
    }

    #[tokio::test]
    async fn approval_expire_stale_pending_respects_batch_size() {
        let state = test_state().await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        for _ in 0..5 {
            let approval = test_approval(now, intent_id, proposal_id, ApprovalState::Pending);
            state
                .runtime
                .store
                .approvals()
                .insert(&approval)
                .await
                .unwrap();
        }

        let expired = state
            .runtime
            .store
            .approvals()
            .expire_stale_pending(now, 3600, 2)
            .await
            .unwrap();
        assert_eq!(expired.len(), 2, "batch size must be honored");

        let remaining = state
            .runtime
            .store
            .approvals()
            .expire_stale_pending(now, 3600, 2)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 2);
    }

    #[tokio::test]
    async fn approval_expire_stale_pending_leaves_terminal_approvals() {
        let state = test_state().await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let approval = test_approval(now, intent_id, proposal_id, ApprovalState::Pending);
        state
            .runtime
            .store
            .approvals()
            .insert(&approval)
            .await
            .unwrap();

        // A concurrent resolver transitions the approval to terminal before the
        // reconciler's CAS write.
        let resolved = state
            .runtime
            .store
            .approvals()
            .resolve(approval.approval_id, ApprovalState::Granted, now)
            .await
            .unwrap();
        assert!(resolved, "approval should be resolvable");

        let expired = state
            .runtime
            .store
            .approvals()
            .expire_stale_pending(now, 3600, 100)
            .await
            .unwrap();
        assert!(
            expired.is_empty(),
            "terminal approvals must not be re-expired"
        );

        let stored = state
            .runtime
            .store
            .approvals()
            .get(approval.approval_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(stored.state, ApprovalState::Granted));
    }

    #[tokio::test]
    async fn quarantine_expire_stale_pending_expires_and_emits_provenance() {
        let state = test_state().await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let hold = test_quarantine_hold(now, intent_id, proposal_id, QuarantineHoldState::Pending);
        state
            .runtime
            .store
            .quarantine_holds()
            .insert(&hold)
            .await
            .unwrap();
        seed_quarantine_lineage(&state, &hold).await;

        let expired = state
            .runtime
            .store
            .quarantine_holds()
            .expire_stale_pending(now, 86400, 100)
            .await
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].hold_id, hold.hold_id);

        emit_quarantine_timed_out_provenance(&state, &expired[0]).await;

        state
            .metrics
            .quarantine_timeouts_total
            .fetch_add(1, Ordering::Relaxed);
        assert_eq!(
            state
                .metrics
                .quarantine_timeouts_total
                .load(Ordering::Relaxed),
            1
        );
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(hold.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::QuarantineTimedOut),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].object.object_id, hold.hold_id.to_string());

        let stored = state
            .runtime
            .store
            .quarantine_holds()
            .get(hold.hold_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, QuarantineHoldState::Expired);
    }

    #[tokio::test]
    async fn quarantine_expire_stale_pending_leaves_terminal_holds() {
        let state = test_state().await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let hold = test_quarantine_hold(now, intent_id, proposal_id, QuarantineHoldState::Pending);
        state
            .runtime
            .store
            .quarantine_holds()
            .insert(&hold)
            .await
            .unwrap();

        let actor = ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "operator-1".to_string(),
            display_name: None,
        };
        let resolved = state
            .runtime
            .store
            .quarantine_holds()
            .resolve(hold.hold_id, true, &actor, Some("allowed"), now)
            .await
            .unwrap();
        assert!(resolved, "quarantine hold should be resolvable");

        let expired = state
            .runtime
            .store
            .quarantine_holds()
            .expire_stale_pending(now, 86400, 100)
            .await
            .unwrap();
        assert!(expired.is_empty(), "terminal holds must not be re-expired");

        let stored = state
            .runtime
            .store
            .quarantine_holds()
            .get(hold.hold_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, QuarantineHoldState::Allowed);
    }

    // Tick tests: exercise the production tick helper through the normal StoreFacade.

    fn approval_timeout_config() -> ServerConfig {
        ServerConfig {
            approval_timeout_enabled: true,
            approval_timeout_seconds: 1,
            approval_reconciliation_interval_secs: 1,
            ..Default::default()
        }
    }

    fn quarantine_timeout_config() -> ServerConfig {
        ServerConfig {
            quarantine_timeout_enabled: true,
            quarantine_timeout_seconds: 1,
            quarantine_reconciliation_interval_secs: 1,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn reconcile_approval_timeouts_once_expires_stale_pending_and_emits_provenance() {
        let state = test_state_with_config(approval_timeout_config()).await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let approval = test_approval(now, intent_id, proposal_id, ApprovalState::Pending);
        state
            .runtime
            .store
            .approvals()
            .insert(&approval)
            .await
            .unwrap();

        let expired = reconcile_approval_timeouts_once(&state, now).await;

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].approval_id, approval.approval_id);
        assert_eq!(
            state
                .metrics
                .approval_timeouts_total
                .load(Ordering::Relaxed),
            1
        );
        let stored = state
            .runtime
            .store
            .approvals()
            .get(approval.approval_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(stored.state, ApprovalState::Expired));
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(approval.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::ApprovalTimedOut),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].object.object_id, approval.approval_id.to_string());
    }

    #[tokio::test]
    async fn reconcile_approval_timeouts_once_respects_batch_size() {
        let state = test_state_with_config(approval_timeout_config()).await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let total = APPROVAL_TIMEOUT_BATCH_SIZE + 1;
        for _ in 0..total {
            let approval = test_approval(now, intent_id, proposal_id, ApprovalState::Pending);
            state
                .runtime
                .store
                .approvals()
                .insert(&approval)
                .await
                .unwrap();
        }

        let first = reconcile_approval_timeouts_once(&state, now).await;
        assert_eq!(first.len(), APPROVAL_TIMEOUT_BATCH_SIZE as usize);
        assert_eq!(
            state
                .metrics
                .approval_timeouts_total
                .load(Ordering::Relaxed),
            APPROVAL_TIMEOUT_BATCH_SIZE as u64
        );
        let pending = state
            .runtime
            .store
            .approvals()
            .list_pending()
            .await
            .unwrap();
        assert_eq!(pending.len(), 1, "one stale approval should remain");

        let second = reconcile_approval_timeouts_once(&state, now).await;
        assert_eq!(second.len(), 1);
        assert_eq!(
            state
                .metrics
                .approval_timeouts_total
                .load(Ordering::Relaxed),
            total as u64
        );
        let pending = state
            .runtime
            .store
            .approvals()
            .list_pending()
            .await
            .unwrap();
        assert!(pending.is_empty(), "no pending approvals should remain");
    }

    #[tokio::test]
    async fn reconcile_approval_timeouts_once_leaves_terminal_approvals() {
        let state = test_state_with_config(approval_timeout_config()).await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let approval = test_approval(now, intent_id, proposal_id, ApprovalState::Pending);
        state
            .runtime
            .store
            .approvals()
            .insert(&approval)
            .await
            .unwrap();

        let resolved = state
            .runtime
            .store
            .approvals()
            .resolve(approval.approval_id, ApprovalState::Granted, now)
            .await
            .unwrap();
        assert!(resolved);

        let expired = reconcile_approval_timeouts_once(&state, now).await;

        assert!(
            expired.is_empty(),
            "terminal approvals must not be re-expired"
        );
        assert_eq!(
            state
                .metrics
                .approval_timeouts_total
                .load(Ordering::Relaxed),
            0
        );
        let stored = state
            .runtime
            .store
            .approvals()
            .get(approval.approval_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(stored.state, ApprovalState::Granted));
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(approval.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::ApprovalTimedOut),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn reconcile_quarantine_timeouts_once_expires_stale_pending_and_emits_provenance() {
        let state = test_state_with_config(quarantine_timeout_config()).await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let hold = test_quarantine_hold(now, intent_id, proposal_id, QuarantineHoldState::Pending);
        state
            .runtime
            .store
            .quarantine_holds()
            .insert(&hold)
            .await
            .unwrap();
        seed_quarantine_lineage(&state, &hold).await;

        let expired = reconcile_quarantine_timeouts_once(&state, now).await;

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].hold_id, hold.hold_id);
        assert_eq!(
            state
                .metrics
                .quarantine_timeouts_total
                .load(Ordering::Relaxed),
            1
        );
        let stored = state
            .runtime
            .store
            .quarantine_holds()
            .get(hold.hold_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, QuarantineHoldState::Expired);
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(hold.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::QuarantineTimedOut),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].object.object_id, hold.hold_id.to_string());
    }

    #[tokio::test]
    async fn reconcile_quarantine_timeouts_once_respects_batch_size() {
        let state = test_state_with_config(quarantine_timeout_config()).await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let total = QUARANTINE_TIMEOUT_BATCH_SIZE + 1;
        for _ in 0..total {
            let hold =
                test_quarantine_hold(now, intent_id, proposal_id, QuarantineHoldState::Pending);
            state
                .runtime
                .store
                .quarantine_holds()
                .insert(&hold)
                .await
                .unwrap();
            seed_quarantine_lineage(&state, &hold).await;
        }

        let first = reconcile_quarantine_timeouts_once(&state, now).await;
        assert_eq!(first.len(), QUARANTINE_TIMEOUT_BATCH_SIZE as usize);
        assert_eq!(
            state
                .metrics
                .quarantine_timeouts_total
                .load(Ordering::Relaxed),
            QUARANTINE_TIMEOUT_BATCH_SIZE as u64
        );
        let (pending, _) = state
            .runtime
            .store
            .quarantine_holds()
            .list_pending(1000, 0)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1, "one stale hold should remain");

        let second = reconcile_quarantine_timeouts_once(&state, now).await;
        assert_eq!(second.len(), 1);
        assert_eq!(
            state
                .metrics
                .quarantine_timeouts_total
                .load(Ordering::Relaxed),
            total as u64
        );
        let (pending, _) = state
            .runtime
            .store
            .quarantine_holds()
            .list_pending(1000, 0)
            .await
            .unwrap();
        assert!(pending.is_empty(), "no pending holds should remain");
    }

    #[tokio::test]
    async fn reconcile_quarantine_timeouts_once_leaves_terminal_holds() {
        let state = test_state_with_config(quarantine_timeout_config()).await;
        let (intent_id, proposal_id) = seed_intent_and_proposal(&state).await;
        let now = Utc::now();
        let hold = test_quarantine_hold(now, intent_id, proposal_id, QuarantineHoldState::Pending);
        state
            .runtime
            .store
            .quarantine_holds()
            .insert(&hold)
            .await
            .unwrap();
        seed_quarantine_lineage(&state, &hold).await;

        let actor = ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "operator-1".to_string(),
            display_name: None,
        };
        let resolved = state
            .runtime
            .store
            .quarantine_holds()
            .resolve(hold.hold_id, true, &actor, Some("allowed"), now)
            .await
            .unwrap();
        assert!(resolved);

        let expired = reconcile_quarantine_timeouts_once(&state, now).await;

        assert!(expired.is_empty(), "terminal holds must not be re-expired");
        assert_eq!(
            state
                .metrics
                .quarantine_timeouts_total
                .load(Ordering::Relaxed),
            0
        );
        let stored = state
            .runtime
            .store
            .quarantine_holds()
            .get(hold.hold_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, QuarantineHoldState::Allowed);
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(hold.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::QuarantineTimedOut),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert!(events.is_empty());
    }

    // Loop tests: verify the background loop terminates on a single shutdown signal.

    #[tokio::test]
    async fn approval_timeout_reconciler_shutdown_terminates_loop() {
        let state = test_state().await;
        tokio::time::pause();
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let handle = tokio::spawn(approval_timeout_reconciler(state.clone(), shutdown.clone()));

        // Yield so the spawned task enters the select! before the shutdown signal arrives.
        tokio::task::yield_now().await;
        shutdown.notify_one();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn quarantine_timeout_reconciler_shutdown_terminates_loop() {
        let state = test_state().await;
        tokio::time::pause();
        let shutdown = Arc::new(tokio::sync::Notify::new());
        let handle = tokio::spawn(quarantine_timeout_reconciler(
            state.clone(),
            shutdown.clone(),
        ));

        tokio::task::yield_now().await;
        shutdown.notify_one();
        handle.await.unwrap();
    }
}
