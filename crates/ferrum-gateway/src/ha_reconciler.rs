//! HA reconciler for stale in-flight executions.
//!
//! On crash/restart recovery, this reconciler scans executions that are still
//! in-flight (`finished_at IS NULL`) and whose `started_at` is older than the
//! configured staleness threshold. Pre-side-effect states transition to
//! `Canceled`; post-side-effect states transition to `Failed`. All transitions
//! use the store's `compare_and_set_state` so a second pass cannot double-
//! transition an execution that has already moved to a terminal state.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use ferrum_proto::{
    ActorRef, ActorType, EventId, ExecutionState, HashChainRef, JsonMap, ObjectRef, ObjectType,
    ProvenanceEvent, ProvenanceEventKind, RollbackState, Timestamp,
};

use crate::execution::{
    lifecycle_event_metadata, mark_lifecycle_transition_reconciled,
    record_lifecycle_transition_outbox_with_obligations,
};
use crate::provenance::append_governance_event;
use crate::state::AppState;

/// In-flight states that have not yet produced a side effect. The HA reconciler
/// transitions these to `Canceled`.
const PRE_SIDE_EFFECT_STATES: &[ExecutionState] = &[
    ExecutionState::Proposed,
    ExecutionState::Authorized,
    ExecutionState::Prepared,
    ExecutionState::AwaitingApproval,
];

fn target_state_for(state: &ExecutionState) -> Option<ExecutionState> {
    if PRE_SIDE_EFFECT_STATES.contains(state) {
        Some(ExecutionState::Canceled)
    } else {
        None
    }
}

/// Perform a single HA reconciler pass and return `(canceled, recovery_required, errors)`.
pub(crate) async fn run_ha_reconciler_pass(state: &AppState) -> (u64, u64, u64) {
    let now = chrono::Utc::now();
    let threshold = state.server_config.ha_reconciler_stale_threshold_secs;
    let stale_before = now - chrono::Duration::seconds(threshold as i64);

    let mut states = Vec::with_capacity(PRE_SIDE_EFFECT_STATES.len() + 1);
    states.extend_from_slice(PRE_SIDE_EFFECT_STATES);
    states.push(ExecutionState::Running);

    let executions = match state
        .runtime
        .store
        .executions()
        .list_stale_in_flight(
            stale_before,
            &states,
            state.server_config.ha_reconciler_batch_size,
        )
        .await
    {
        Ok(execs) => execs,
        Err(error) => {
            state
                .metrics
                .ha_reconciler_errors_total
                .fetch_add(1, Ordering::Relaxed);
            tracing::error!(%error, "ha reconciler failed to list stale in-flight executions");
            return (0, 0, 1);
        }
    };

    let mut canceled = 0u64;
    let mut recovery_required = 0u64;
    let mut errors = 0u64;

    for execution in executions {
        if execution.state == ExecutionState::Running {
            match reconcile_running_prepared_pair(state, &execution, now).await {
                Ok(true) => {
                    recovery_required += 1;
                }
                Ok(false) => {
                    // Not a paired Running+Prepared candidate; skip without mutating.
                    tracing::debug!(
                        execution_id = %execution.execution_id,
                        "ha reconciler skipped Running execution without Prepared contract"
                    );
                }
                Err(error) => {
                    errors += 1;
                    tracing::error!(
                        %error,
                        execution_id = %execution.execution_id,
                        "ha reconciler failed to reconcile stale Running execution"
                    );
                }
            }
            continue;
        }

        let Some(target_state) = target_state_for(&execution.state) else {
            continue;
        };
        let previous_state = execution.state.clone();

        let cas_result = state
            .runtime
            .store
            .executions()
            .compare_and_set_state(
                execution.execution_id,
                &[previous_state],
                target_state.clone(),
            )
            .await;
        match cas_result {
            Ok(true) => {
                if target_state == ExecutionState::Canceled {
                    canceled += 1;
                }
                emit_ha_reconciler_provenance(state, &execution, target_state, now).await;
            }
            Ok(false) => {
                // Another writer already moved the execution; skip silently.
                tracing::debug!(
                    execution_id = %execution.execution_id,
                    "ha reconciler CAS skipped execution (already transitioned)"
                );
            }
            Err(error) => {
                errors += 1;
                tracing::error!(
                    %error,
                    execution_id = %execution.execution_id,
                    "ha reconciler failed to transition stale execution"
                );
            }
        }
    }

    if canceled > 0 || recovery_required > 0 {
        tracing::info!(
            canceled,
            recovery_required,
            errors,
            "ha reconciler completed pass over stale in-flight executions"
        );
    }

    // Update metrics counters.
    if canceled > 0 {
        state
            .metrics
            .ha_reconciler_canceled_total
            .fetch_add(canceled, Ordering::Relaxed);
    }
    if recovery_required > 0 {
        state
            .metrics
            .ha_reconciler_recovery_required_total
            .fetch_add(recovery_required, Ordering::Relaxed);
    }
    if errors > 0 {
        state
            .metrics
            .ha_reconciler_errors_total
            .fetch_add(errors, Ordering::Relaxed);
    }

    (canceled, recovery_required, errors)
}

/// If the execution is `Running` and its linked rollback contract is `Prepared`,
/// atomically transition both to `RecoveryRequired` with an `ErrorRaised` provenance
/// obligation. Returns `Ok(true)` when a transition was performed, `Ok(false)` when
/// the pair is not a candidate, and `Err` on unexpected store failure.
async fn reconcile_running_prepared_pair(
    state: &AppState,
    execution: &ferrum_proto::ExecutionRecord,
    occurred_at: Timestamp,
) -> Result<bool, ferrum_store::StoreError> {
    let contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => return Ok(false),
    };

    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await?
    {
        Some(c) => c,
        None => return Ok(false),
    };

    if contract.state != RollbackState::Prepared {
        return Ok(false);
    }

    let previous_execution = execution.clone();
    let mut updated_execution = execution.clone();
    updated_execution.state = ExecutionState::RecoveryRequired;

    let previous_contract = contract.clone();
    let mut updated_contract = contract.clone();
    updated_contract.state = RollbackState::RecoveryRequired;
    updated_contract
        .metadata
        .insert("recovery_required".to_string(), serde_json::json!(true));
    updated_contract
        .metadata
        .insert("ha_reconciler".to_string(), serde_json::json!(true));

    let outbox = record_lifecycle_transition_outbox_with_obligations(
        &state.runtime.store,
        "ha-recovery",
        &previous_execution,
        &updated_execution,
        Some(&previous_contract),
        Some(&updated_contract),
        vec![ProvenanceEventKind::ErrorRaised],
    )
    .await?;

    let recovery_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ErrorRaised,
        occurred_at,
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway HA reconciler".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution.execution_id.to_string(),
            summary: Some(
                "HA reconciler detected stale Running+Prepared pair; recovery required".to_string(),
            ),
        },
        intent_id: Some(execution.intent_id),
        proposal_id: Some(execution.proposal_id),
        execution_id: Some(execution.execution_id),
        capability_id: Some(execution.capability_id),
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
        metadata: {
            let mut m = JsonMap::new();
            m.insert("recovery_required".to_string(), serde_json::json!(true));
            m.insert("ha_reconciler".to_string(), serde_json::json!(true));
            m.insert(
                "bounded_reason".to_string(),
                serde_json::json!(
                    "stale Running+Prepared pair detected after restart; recovery required"
                ),
            );
            lifecycle_event_metadata(&outbox, m)
        },
        source_runtime_id: None,
    };
    let recovery_event_id = recovery_event.event_id;
    append_governance_event(&state.runtime.store, recovery_event).await?;
    mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, recovery_event_id).await?;
    Ok(true)
}

/// Background task that runs a startup scan and then periodic HA reconciler
/// passes until the shutdown notify fires.
pub(crate) async fn ha_reconciler(state: Arc<AppState>, shutdown: Arc<tokio::sync::Notify>) {
    let interval_secs = state.server_config.ha_reconciler_interval_secs;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                run_ha_reconciler_pass(&state).await;
            }
            _ = shutdown.notified() => {
                tracing::info!("ha reconciler shutting down");
                break;
            }
        }
    }
}

async fn emit_ha_reconciler_provenance(
    state: &AppState,
    execution: &ferrum_proto::ExecutionRecord,
    new_state: ExecutionState,
    occurred_at: Timestamp,
) {
    let stale_for_secs = (occurred_at - execution.started_at).num_seconds();

    let mut metadata = JsonMap::new();
    metadata.insert("reconciler".to_string(), serde_json::json!("ha"));
    metadata.insert(
        "previous_state".to_string(),
        serde_json::json!(format!("{:?}", execution.state)),
    );
    metadata.insert(
        "new_state".to_string(),
        serde_json::json!(format!("{:?}", new_state)),
    );
    metadata.insert(
        "stale_for_secs".to_string(),
        serde_json::json!(stale_for_secs),
    );

    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ErrorRaised,
        occurred_at,
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution.execution_id.to_string(),
            summary: Some(format!(
                "HA reconciler transitioned execution from {:?} to {:?}",
                execution.state, new_state
            )),
        },
        intent_id: Some(execution.intent_id),
        proposal_id: Some(execution.proposal_id),
        execution_id: Some(execution.execution_id),
        capability_id: Some(execution.capability_id),
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
    };

    if let Err(e) = crate::provenance::append_governance_event(&state.runtime.store, event).await {
        state
            .metrics
            .ha_reconciler_errors_total
            .fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            error = %e,
            execution_id = %execution.execution_id,
            "failed to append HA reconciler ErrorRaised provenance event"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        CapabilityId, CapabilityLease, CapabilityStatus, Decision, ExecutionId, ExecutionRecord,
        ExecutionState, IntentEnvelope, LifecycleOutboxStatus, PrincipalId, ProposalId,
        ProvenanceEventKind, RollbackClass, RollbackContract, RollbackContractId, RollbackState,
        RollbackTarget,
    };

    fn create_test_intent() -> IntentEnvelope {
        IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30000,
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
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            owner_actor_id: None,
        }
    }

    fn create_test_proposal(intent_id: ferrum_proto::IntentId) -> ferrum_proto::ActionProposal {
        ferrum_proto::ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id,
            step_index: 0,
            title: "test".to_string(),
            tool_name: "test-tool".to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn create_test_capability(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
    ) -> CapabilityLease {
        CapabilityLease {
            capability_id: CapabilityId::new(),
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
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
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            revoked_at: None,
            metadata: ferrum_proto::JsonMap::new(),
            owner_actor_id: None,
        }
    }

    fn create_test_execution(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
        capability_id: CapabilityId,
    ) -> ExecutionRecord {
        ExecutionRecord {
            execution_id: ExecutionId::new(),
            proposal_id,
            intent_id,
            capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Proposed,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
            owner_actor_id: None,
        }
    }

    fn create_test_rollback_contract(
        execution_id: ExecutionId,
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
        state: RollbackState,
    ) -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id,
            proposal_id,
            execution_id,
            action_type: ferrum_proto::ActionType::FileWrite,
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
            state,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    async fn setup_test_state() -> (
        Arc<AppState>,
        ferrum_proto::IntentId,
        ProposalId,
        CapabilityId,
    ) {
        let runtime = crate::server::test_runtime().await;
        let config = crate::ServerConfig {
            ha_reconciler_enabled: true,
            ha_reconciler_interval_secs: 60,
            ha_reconciler_stale_threshold_secs: 300,
            ha_reconciler_batch_size: 100,
            ..Default::default()
        };

        let store = runtime.store.clone();
        let intent = create_test_intent();
        let intent_id = intent.intent_id;
        store.intents().insert(&intent).await.unwrap();
        let proposal = create_test_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store.proposals().insert(&proposal).await.unwrap();
        let capability = create_test_capability(intent_id, proposal_id);
        let capability_id = capability.capability_id;
        store.capabilities().insert(&capability).await.unwrap();

        let state = AppState::test_new(runtime, config);
        (state, intent_id, proposal_id, capability_id)
    }

    #[tokio::test]
    async fn ha_reconciler_transitions_proposed_to_canceled() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Proposed;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;
        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();

        let (canceled, recovery_required, errors) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled, 1);
        assert_eq!(recovery_required, 0);
        assert_eq!(errors, 0);

        let retrieved = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, ExecutionState::Canceled);
        assert!(
            retrieved.finished_at.is_some(),
            "reconciled Canceled execution must set finished_at"
        );
    }

    #[tokio::test]
    async fn ha_reconciler_transitions_paired_running_to_recovery_required() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Running;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;

        let contract = create_test_rollback_contract(
            execution_id,
            intent_id,
            proposal_id,
            RollbackState::Prepared,
        );
        let contract_id = contract.contract_id;
        execution.rollback_contract_id = Some(contract_id);

        // rollback_contracts has a foreign key to executions, so execution must
        // be inserted first. The write queue processes ops in order and the
        // contract will be visible when the reconciler runs.
        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();
        state
            .runtime
            .store
            .rollback_contracts()
            .insert(&contract)
            .await
            .unwrap();

        let (canceled, recovery_required, errors) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled, 0);
        assert_eq!(recovery_required, 1);
        assert_eq!(errors, 0);

        let retrieved = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, ExecutionState::RecoveryRequired);
        assert!(
            retrieved.finished_at.is_none(),
            "RecoveryRequired is non-terminal and must not set finished_at"
        );

        let retrieved_contract = state
            .runtime
            .store
            .rollback_contracts()
            .get(contract_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved_contract.state, RollbackState::RecoveryRequired);
        assert_eq!(
            retrieved_contract.metadata.get("ha_reconciler"),
            Some(&serde_json::json!(true))
        );

        let reconciled = state
            .runtime
            .store
            .lifecycle_outbox()
            .list_by_status(LifecycleOutboxStatus::Reconciled, 10)
            .await
            .unwrap();
        assert_eq!(reconciled.len(), 1);
        let outbox = &reconciled[0];
        assert_eq!(outbox.status, LifecycleOutboxStatus::Reconciled);
        assert_eq!(outbox.execution_id, execution_id);
        assert_eq!(outbox.rollback_contract_id, Some(contract_id));
        let kinds: Vec<_> = outbox
            .provenance_obligations
            .iter()
            .map(|o| o.event_kind.clone())
            .collect();
        assert!(kinds.contains(&ProvenanceEventKind::ErrorRaised));
    }

    #[tokio::test]
    async fn ha_reconciler_second_pass_is_idempotent() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Proposed;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;
        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();

        let (canceled1, recovery_required1, errors1) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled1, 1);
        assert_eq!(recovery_required1, 0);
        assert_eq!(errors1, 0);

        let first = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.state, ExecutionState::Canceled);
        assert!(first.finished_at.is_some());

        let (canceled2, recovery_required2, errors2) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled2, 0);
        assert_eq!(recovery_required2, 0);
        assert_eq!(errors2, 0);

        let second = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.state, ExecutionState::Canceled);
        assert_eq!(second.finished_at, first.finished_at);
    }

    #[tokio::test]
    async fn ha_reconciler_skips_recent_executions() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Proposed;
        execution.started_at = now - chrono::Duration::seconds(10);
        let execution_id = execution.execution_id;
        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();

        let (canceled, recovery_required, errors) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled, 0);
        assert_eq!(recovery_required, 0);
        assert_eq!(errors, 0);

        let retrieved = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, ExecutionState::Proposed);
    }

    #[tokio::test]
    async fn ha_reconciler_emits_provenance_and_updates_metrics() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Running;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;

        let contract = create_test_rollback_contract(
            execution_id,
            intent_id,
            proposal_id,
            RollbackState::Prepared,
        );
        let contract_id = contract.contract_id;
        execution.rollback_contract_id = Some(contract_id);

        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();
        state
            .runtime
            .store
            .rollback_contracts()
            .insert(&contract)
            .await
            .unwrap();

        run_ha_reconciler_pass(&state).await;

        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(intent_id),
                execution_id: Some(execution_id),
                capability_id: None,
                event_kind: Some(ferrum_proto::ProvenanceEventKind::ErrorRaised),
                since: None,
                until: None,
                edge_types: vec![],
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].metadata.get("ha_reconciler"),
            Some(&serde_json::json!(true))
        );

        assert_eq!(
            state
                .metrics
                .ha_reconciler_recovery_required_total
                .load(Ordering::Relaxed),
            1
        );
    }

    #[tokio::test]
    async fn ha_reconciler_skips_running_without_prepared_contract() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Running;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;
        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();

        let (canceled, recovery_required, errors) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled, 0);
        assert_eq!(recovery_required, 0);
        assert_eq!(errors, 0);

        let retrieved = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, ExecutionState::Running);
    }

    #[tokio::test]
    async fn ha_reconciler_skips_running_with_non_prepared_contract() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Running;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;

        let contract = create_test_rollback_contract(
            execution_id,
            intent_id,
            proposal_id,
            RollbackState::ExecutedAwaitingVerify,
        );
        let contract_id = contract.contract_id;
        execution.rollback_contract_id = Some(contract_id);

        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();
        state
            .runtime
            .store
            .rollback_contracts()
            .insert(&contract)
            .await
            .unwrap();

        let (canceled, recovery_required, errors) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled, 0);
        assert_eq!(recovery_required, 0);
        assert_eq!(errors, 0);

        let retrieved = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, ExecutionState::Running);
    }

    #[tokio::test]
    async fn ha_reconciler_skips_awaiting_verification() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::AwaitingVerification;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;
        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();

        let (canceled, recovery_required, errors) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled, 0);
        assert_eq!(recovery_required, 0);
        assert_eq!(errors, 0);

        let retrieved = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, ExecutionState::AwaitingVerification);
    }

    #[tokio::test]
    async fn ha_reconciler_second_pass_is_idempotent_for_recovery() {
        let (state, intent_id, proposal_id, capability_id) = setup_test_state().await;
        let now = chrono::Utc::now();

        let mut execution = create_test_execution(intent_id, proposal_id, capability_id);
        execution.state = ExecutionState::Running;
        execution.started_at = now - chrono::Duration::minutes(10);
        let execution_id = execution.execution_id;

        let contract = create_test_rollback_contract(
            execution_id,
            intent_id,
            proposal_id,
            RollbackState::Prepared,
        );
        let contract_id = contract.contract_id;
        execution.rollback_contract_id = Some(contract_id);

        state
            .runtime
            .store
            .executions()
            .insert(&execution)
            .await
            .unwrap();
        state
            .runtime
            .store
            .rollback_contracts()
            .insert(&contract)
            .await
            .unwrap();

        let (canceled1, recovery_required1, errors1) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled1, 0);
        assert_eq!(recovery_required1, 1);
        assert_eq!(errors1, 0);

        let first = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.state, ExecutionState::RecoveryRequired);

        let (canceled2, recovery_required2, errors2) = run_ha_reconciler_pass(&state).await;
        assert_eq!(canceled2, 0);
        assert_eq!(recovery_required2, 0);
        assert_eq!(errors2, 0);

        let second = state
            .runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.state, ExecutionState::RecoveryRequired);
        assert_eq!(second.finished_at, first.finished_at);
    }

    #[test]
    fn server_config_rejects_production_without_ha_reconciler() {
        let config = crate::ServerConfig {
            bind_addr: "0.0.0.0:8080".parse().unwrap(),
            store_dsn: "sqlite:///var/lib/ferrumgate/ferrumgate.db".to_string(),
            auth_mode: crate::AuthMode::Bearer,
            bearer_token: Some("a".repeat(64)),
            fs_workdir: Some(std::path::PathBuf::from("/var/lib/ferrumgate/workdir")),
            lifecycle_reconciliation_enabled: true,
            approval_timeout_enabled: true,
            audit_fail_closed: true,
            ha_reconciler_enabled: false,
            ..Default::default()
        };

        let err = config.validate().expect_err("expected validation failure");
        assert!(
            err.contains("ha_reconciler_enabled"),
            "expected ha_reconciler_enabled error, got: {err}"
        );
    }
}
