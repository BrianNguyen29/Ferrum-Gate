use chrono::{Duration, Utc};
use ferrum_ledger::{InMemoryLedger, LedgerEntry};
use ferrum_proto::{
    ActionProposal, ActorRef, ActorType, ApprovalRequest, ApprovalState, CapabilityLease,
    CapabilityStatus, Decision, EffectType, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, OutcomeClause, PolicyBundleId, PrincipalId, ProposalId, ResourceMode,
    RollbackClass, RollbackContract, RollbackState, RollbackTarget, TaintBudget, TimeBudget,
    ToolBinding, TrustContextSummary,
};
use tempfile::TempDir;

use crate::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, ProposalRepo,
    ProvenanceRepo, RollbackRepo, SqliteStore,
};

async fn create_test_store() -> (TempDir, SqliteStore) {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = SqliteStore::connect(&database_url)
        .await
        .expect("failed to connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");
    (temp_dir, store)
}

fn sample_intent() -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new(),
        principal_id: PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "primary".to_string(),
            description: "test outcome".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: RollbackClass::R0NativeReversible,
        time_budget: TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: TrustContextSummary {
            input_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: vec!["test".to_string()],
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::minutes(15),
    }
}

fn sample_proposal(intent_id: IntentId) -> ActionProposal {
    ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Inspect state".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read a file".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: Utc::now(),
    }
}

fn sample_capability(intent_id: IntentId, proposal_id: ProposalId) -> CapabilityLease {
    let now = Utc::now();
    CapabilityLease {
        capability_id: ferrum_proto::CapabilityId::new(),
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ferrum_proto::ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Read,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: TaintBudget {
            max_taint_score: 20,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        issued_by: "test".to_string(),
        policy_bundle_id: PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status: CapabilityStatus::Active,
        issued_at: now,
        expires_at: now + Duration::minutes(5),
        revoked_at: None,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn sample_execution(
    intent_id: IntentId,
    proposal_id: ProposalId,
    capability_id: ferrum_proto::CapabilityId,
) -> ExecutionRecord {
    ExecutionRecord {
        execution_id: ferrum_proto::ExecutionId::new(),
        proposal_id,
        intent_id,
        capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Authorized,
        started_at: Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn sample_rollback(
    intent_id: IntentId,
    proposal_id: ProposalId,
    execution_id: ferrum_proto::ExecutionId,
) -> RollbackContract {
    RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id,
        proposal_id,
        execution_id,
        action_type: ferrum_proto::ActionType::McpToolMutation,
        rollback_class: RollbackClass::R0NativeReversible,
        adapter_key: "noop".to_string(),
        target: RollbackTarget::Generic {
            namespace: "mcp".to_string(),
            identifier: "tool-call".to_string(),
        },
        prepare_checks: Vec::new(),
        verify_checks: Vec::new(),
        compensation_plan: Vec::new(),
        auto_commit: true,
        state: RollbackState::Prepared,
        created_at: Utc::now(),
        expires_at: None,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn sample_approval(
    intent_id: IntentId,
    proposal_id: ProposalId,
    execution_id: ferrum_proto::ExecutionId,
) -> ApprovalRequest {
    ApprovalRequest {
        approval_id: ferrum_proto::ApprovalId::new(),
        intent_id,
        proposal_id,
        execution_id: Some(execution_id),
        requested_by: ActorRef {
            actor_type: ActorType::Agent,
            actor_id: "test-agent".to_string(),
            display_name: Some("Test Agent".to_string()),
        },
        reason: "needs approval for elevated action".to_string(),
        action_digest: "digest-123".to_string(),
        expires_at: Utc::now() + Duration::hours(1),
        state: ApprovalState::Pending,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn intent_crud_and_status_transition() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    let intent_id = intent.intent_id;

    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let fetched = store
        .intents()
        .get(intent_id)
        .await
        .expect("load intent")
        .expect("intent present");
    assert_eq!(fetched.title, "Test Intent");
    assert_eq!(fetched.tags, vec!["test".to_string()]);

    store
        .intents()
        .update_status(intent_id, IntentStatus::Closed)
        .await
        .expect("close intent");

    let closed = store
        .intents()
        .get(intent_id)
        .await
        .expect("reload intent")
        .expect("intent present");
    assert!(matches!(closed.status, IntentStatus::Closed));
}

#[tokio::test]
async fn capability_crud_and_relation_query() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability_id = capability.capability_id;
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    store
        .capabilities()
        .update_status(capability_id, CapabilityStatus::Used)
        .await
        .expect("mark capability used");

    let fetched = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(matches!(fetched.status, CapabilityStatus::Used));

    let listed = store
        .capabilities()
        .list_by_intent(intent.intent_id)
        .await
        .expect("list capabilities");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].capability_id, capability_id);
}

#[tokio::test]
async fn capability_mark_used_if_active_success() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability_id = capability.capability_id;
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    // First mark_used_if_active should succeed and return true
    let result = store
        .capabilities()
        .mark_used_if_active(capability_id)
        .await
        .expect("mark_used_if_active should succeed");
    assert!(result, "First mark_used_if_active should return true");

    // Verify the capability is now marked as Used
    let fetched = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(matches!(fetched.status, CapabilityStatus::Used));
}

#[tokio::test]
async fn capability_mark_used_if_active_second_is_noop() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability_id = capability.capability_id;
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    // First mark_used_if_active succeeds
    let first_result = store
        .capabilities()
        .mark_used_if_active(capability_id)
        .await
        .expect("first mark_used_if_active should succeed");
    assert!(first_result, "First call should return true");

    // Second mark_used_if_active should be a no-op and return false
    let second_result = store
        .capabilities()
        .mark_used_if_active(capability_id)
        .await
        .expect("second mark_used_if_active should succeed");
    assert!(!second_result, "Second call should return false (no-op)");

    // Verify the capability is still Used (not changed)
    let fetched = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(matches!(fetched.status, CapabilityStatus::Used));
}

#[tokio::test]
async fn capability_revoke_persists_revoked_at() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability_id = capability.capability_id;
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    // Revoke the capability
    store
        .capabilities()
        .revoke(capability_id)
        .await
        .expect("revoke should succeed");

    // Verify the capability is now Revoked
    let fetched = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(matches!(fetched.status, CapabilityStatus::Revoked));
    assert!(fetched.revoked_at.is_some(), "revoked_at should be set");
}

#[tokio::test]
async fn capability_list_active_filters_correctly() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability1 = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability1_id = capability1.capability_id;
    store
        .capabilities()
        .insert(&capability1)
        .await
        .expect("insert capability1");

    // Create a second capability
    let capability2 = sample_capability(intent.intent_id, proposal.proposal_id);
    store
        .capabilities()
        .insert(&capability2)
        .await
        .expect("insert capability2");

    let mut expired_capability = sample_capability(intent.intent_id, proposal.proposal_id);
    expired_capability.expires_at = Utc::now() - Duration::minutes(1);
    store
        .capabilities()
        .insert(&expired_capability)
        .await
        .expect("insert expired capability");

    let active_list = store
        .capabilities()
        .list_active()
        .await
        .expect("list_active should succeed");
    assert_eq!(
        active_list.len(),
        2,
        "Should have 2 non-expired active capabilities"
    );

    // Mark one as Used
    store
        .capabilities()
        .mark_used_if_active(capability1_id)
        .await
        .expect("mark_used_if_active should succeed");

    let active_list = store
        .capabilities()
        .list_active()
        .await
        .expect("list_active should succeed");
    assert_eq!(
        active_list.len(),
        1,
        "Should have 1 active capability after marking one as used"
    );
    assert_eq!(active_list[0].capability_id, capability2.capability_id);

    store
        .capabilities()
        .revoke(capability2.capability_id)
        .await
        .expect("revoke should succeed");

    let active_list = store
        .capabilities()
        .list_active()
        .await
        .expect("list_active should succeed");
    assert!(
        active_list.is_empty(),
        "Should have no active capabilities after revoking all"
    );
}

#[tokio::test]
async fn execution_and_rollback_state_transitions() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    let execution = sample_execution(
        intent.intent_id,
        proposal.proposal_id,
        capability.capability_id,
    );
    let execution_id = execution.execution_id;
    store
        .executions()
        .insert(&execution)
        .await
        .expect("insert execution");

    store
        .executions()
        .update_state(execution_id, ExecutionState::Prepared)
        .await
        .expect("update execution state");

    let execution = store
        .executions()
        .get(execution_id)
        .await
        .expect("load execution")
        .expect("execution present");
    assert!(matches!(execution.state, ExecutionState::Prepared));

    let contract = sample_rollback(intent.intent_id, proposal.proposal_id, execution_id);
    let contract_id = contract.contract_id;
    store
        .rollback_contracts()
        .insert(&contract)
        .await
        .expect("insert rollback");

    store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::Verified)
        .await
        .expect("update rollback state");

    let rollback = store
        .rollback_contracts()
        .get(contract_id)
        .await
        .expect("load rollback")
        .expect("rollback present");
    assert!(matches!(rollback.state, RollbackState::Verified));
}

#[tokio::test]
async fn approval_resolution_round_trip() {
    let (_temp_dir, store) = create_test_store().await;
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    let execution = sample_execution(
        intent.intent_id,
        proposal.proposal_id,
        capability.capability_id,
    );
    store
        .executions()
        .insert(&execution)
        .await
        .expect("insert execution");

    let approval = sample_approval(
        intent.intent_id,
        proposal.proposal_id,
        execution.execution_id,
    );
    let approval_id = approval.approval_id;
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("insert approval");

    let (pending, _) = store
        .approvals()
        .list_pending_cursor(100, None)
        .await
        .expect("list pending");
    assert_eq!(pending.len(), 1);

    store
        .approvals()
        .resolve(approval_id, ApprovalState::Granted)
        .await
        .expect("resolve approval");

    let resolved = store
        .approvals()
        .get(approval_id)
        .await
        .expect("load approval")
        .expect("approval present");
    assert!(matches!(resolved.state, ApprovalState::Granted));
}

// ---------------------------------------------------------------------------
// Ledger tests
// ---------------------------------------------------------------------------

fn make_test_provenance_event(sequence: u64) -> ferrum_proto::ProvenanceEvent {
    use ferrum_proto::{
        EventId, HashChainRef, ObjectRef, ObjectType, SensitivityLabel, Timestamp, TrustLabel,
    };

    ferrum_proto::ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::UserGoalReceived,
        occurred_at: Timestamp::default(),
        actor: ActorRef {
            actor_type: ActorType::User,
            actor_id: format!("user-{}", sequence),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::Intent,
            object_id: format!("intent-{}", sequence),
            summary: None,
        },
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: vec![TrustLabel::Trusted],
        sensitivity_labels: vec![SensitivityLabel::Public],
        parent_edges: vec![],
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
    }
}

#[tokio::test]
async fn ledger_append_and_load_single_entry() {
    let (_temp_dir, store) = create_test_store().await;

    let mut in_mem = InMemoryLedger::new();
    let event = make_test_provenance_event(0);
    let entry = in_mem
        .append(event.clone())
        .expect("build genesis entry")
        .clone();

    // Foreign key requires event to exist in provenance_events first
    store
        .provenance()
        .append_event(&event)
        .await
        .expect("persist event");
    store
        .ledger()
        .append(&entry)
        .await
        .expect("persist genesis");

    let loaded = store
        .ledger()
        .get_by_event(entry.event.event_id)
        .await
        .expect("load from db")
        .expect("entry should be present");

    assert_eq!(loaded.entry_hash.as_str(), entry.entry_hash.as_str());
    assert_eq!(loaded.prev_hash, entry.prev_hash);
    assert_eq!(loaded.sequence, entry.sequence);
}

#[tokio::test]
async fn ledger_append_load_and_verify_chain() {
    let (_temp_dir, store) = create_test_store().await;

    // Build a chain of 3 entries in memory
    let mut in_mem = InMemoryLedger::new();
    let e0 = in_mem
        .append(make_test_provenance_event(0))
        .unwrap()
        .clone();
    let e1 = in_mem
        .append(make_test_provenance_event(1))
        .unwrap()
        .clone();
    let e2 = in_mem
        .append(make_test_provenance_event(2))
        .unwrap()
        .clone();

    // Persist all three (events must be in provenance_events first due to FK)
    let events = [e0.event.clone(), e1.event.clone(), e2.event.clone()];
    for ev in &events {
        store
            .provenance()
            .append_event(ev)
            .await
            .expect("persist event");
    }
    store.ledger().append(&e0).await.expect("persist e0");
    store.ledger().append(&e1).await.expect("persist e1");
    store.ledger().append(&e2).await.expect("persist e2");

    // Load them back (list_recent returns newest-first, so reverse for ordered reconstruction)
    let loaded: Vec<LedgerEntry> = store.ledger().list_recent(10).await.expect("load entries");
    assert_eq!(loaded.len(), 3);

    // Rebuild in-memory ledger from loaded entries (newest-first → reverse to chronological)
    let by_seq: Vec<LedgerEntry> = {
        let mut v = loaded;
        v.reverse();
        v
    };
    let rebuilt = InMemoryLedger::load_entries(by_seq);
    rebuilt
        .verify_chain()
        .expect("chain should be valid after roundtrip");
}

#[tokio::test]
async fn reconcile_capabilities_with_executions_transitions_active_with_executions_to_used() {
    // Test that a capability which is Active but has execution history
    // gets reconciled to Used (split-brain repair).
    let (_temp_dir, store) = create_test_store().await;

    // Insert prerequisite intent and proposal
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    // Create an Active capability
    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability_id = capability.capability_id;
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    // Verify it is Active before reconciliation
    let loaded = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(matches!(loaded.status, CapabilityStatus::Active));

    // Create an execution record referencing this capability (simulating legacy split-brain)
    let execution = sample_execution(intent.intent_id, proposal.proposal_id, capability_id);
    store
        .executions()
        .insert(&execution)
        .await
        .expect("insert execution");

    // Run reconciliation
    let reconciled = store
        .reconcile_capabilities_with_executions()
        .await
        .expect("reconciliation should succeed");

    // One capability should have been reconciled
    assert_eq!(reconciled, 1, "expected 1 capability to be reconciled");

    // Verify the capability is now Used
    let loaded = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(
        matches!(loaded.status, CapabilityStatus::Used),
        "capability should be Used after reconciliation"
    );
}

#[tokio::test]
async fn reconcile_capabilities_with_executions_leaves_orphan_active_capabilities_alone() {
    // Test that a capability which is Active and has NO execution history
    // is NOT modified by reconciliation.
    let (_temp_dir, store) = create_test_store().await;

    // Insert prerequisite intent and proposal
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    // Create an Active capability with NO execution record
    let capability = sample_capability(intent.intent_id, proposal.proposal_id);
    let capability_id = capability.capability_id;
    store
        .capabilities()
        .insert(&capability)
        .await
        .expect("insert capability");

    // Run reconciliation
    let reconciled = store
        .reconcile_capabilities_with_executions()
        .await
        .expect("reconciliation should succeed");

    // No capabilities should be reconciled (no execution history)
    assert_eq!(reconciled, 0, "expected 0 capabilities to be reconciled");

    // Verify the capability is still Active
    let loaded = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("load capability")
        .expect("capability present");
    assert!(
        matches!(loaded.status, CapabilityStatus::Active),
        "capability should still be Active"
    );
}

#[tokio::test]
async fn reconcile_capabilities_with_executions_handles_mixed_state() {
    // Test reconciliation with a mix of:
    // - Active capability with execution history (should become Used)
    // - Active capability without execution history (should stay Active)
    let (_temp_dir, store) = create_test_store().await;

    // Insert prerequisite intent and proposal
    let intent = sample_intent();
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent");

    let proposal = sample_proposal(intent.intent_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal");

    // Create first Active capability WITH execution history
    let cap1 = sample_capability(intent.intent_id, proposal.proposal_id);
    let cap1_id = cap1.capability_id;
    store
        .capabilities()
        .insert(&cap1)
        .await
        .expect("insert cap1");

    let exec1 = sample_execution(intent.intent_id, proposal.proposal_id, cap1_id);
    store
        .executions()
        .insert(&exec1)
        .await
        .expect("insert exec1");

    // Create second Active capability WITHOUT execution history
    let cap2 = sample_capability(intent.intent_id, proposal.proposal_id);
    let cap2_id = cap2.capability_id;
    store
        .capabilities()
        .insert(&cap2)
        .await
        .expect("insert cap2");

    // Run reconciliation
    let reconciled = store
        .reconcile_capabilities_with_executions()
        .await
        .expect("reconciliation should succeed");

    // Only one capability should be reconciled (cap1)
    assert_eq!(reconciled, 1, "expected 1 capability to be reconciled");

    // Verify cap1 is now Used
    let loaded1 = store
        .capabilities()
        .get(cap1_id)
        .await
        .expect("load cap1")
        .expect("cap1 present");
    assert!(
        matches!(loaded1.status, CapabilityStatus::Used),
        "cap1 should be Used"
    );

    // Verify cap2 is still Active
    let loaded2 = store
        .capabilities()
        .get(cap2_id)
        .await
        .expect("load cap2")
        .expect("cap2 present");
    assert!(
        matches!(loaded2.status, CapabilityStatus::Active),
        "cap2 should still be Active"
    );
}
