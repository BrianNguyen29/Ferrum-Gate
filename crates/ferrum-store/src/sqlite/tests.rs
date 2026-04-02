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
            selectors: None,
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
        policy_bundle_fingerprint: None,
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
async fn ledger_append_hash_mismatch_rejected() {
    // Verify that append rejects an entry whose prev_hash does not match the chain tip.
    let (_temp_dir, store) = create_test_store().await;

    // Build and persist a genesis entry using append_event (handles FK requirement)
    let genesis = store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append genesis");

    // Craft a rogue entry that claims to follow the genesis but has a wrong prev_hash
    let rogue_prev = "wrong_hash___________________________________".into();
    let rogue_entry = LedgerEntry {
        sequence: 1,
        prev_hash: Some(rogue_prev),
        entry_hash: "rogue_hash_________________________________".into(),
        event: make_test_provenance_event(1),
    };

    // The rogue entry must NOT be accepted (FK requires event in provenance_events first)
    store
        .provenance()
        .append_event(&rogue_entry.event)
        .await
        .expect("persist rogue event");

    // Append must fail due to hash verification
    let result = store.ledger().append(&rogue_entry).await;
    assert!(
        result.is_err(),
        "append with wrong prev_hash should be rejected"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("append hash verification failed") || err_msg.contains("chain broken"),
        "error should mention hash verification failure: {}",
        err_msg
    );

    // Verify genesis is still intact and no rogue entry was persisted
    let loaded = store
        .ledger()
        .get_by_event(genesis.event.event_id)
        .await
        .expect("load genesis")
        .expect("genesis should still exist");
    assert_eq!(loaded.entry_hash.as_str(), genesis.entry_hash.as_str());
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

// ---------------------------------------------------------------------------
// Atomic ledger append_event tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ledger_append_event_genesis_has_null_prev_hash() {
    let (_temp_dir, store) = create_test_store().await;

    let event = make_test_provenance_event(0);
    let entry = store
        .ledger()
        .append_event(&event)
        .await
        .expect("append_event genesis should succeed");

    // Genesis entry must have sequence 0 and null prev_hash
    assert_eq!(entry.sequence, 0, "genesis sequence should be 0");
    assert!(
        entry.prev_hash.is_none(),
        "genesis prev_hash should be None"
    );

    // Verify the entry can be loaded back
    let loaded = store
        .ledger()
        .get_by_event(event.event_id)
        .await
        .expect("load by event_id")
        .expect("entry should exist");
    assert_eq!(loaded.sequence, 0);
    assert!(loaded.prev_hash.is_none());
    assert_eq!(loaded.entry_hash.as_str(), entry.entry_hash.as_str());
}

#[tokio::test]
async fn ledger_append_event_chain_linkage_correct() {
    let (_temp_dir, store) = create_test_store().await;

    // Append genesis
    let e0 = store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append genesis");
    let e0_hash = e0.entry_hash.clone();

    // Append second entry
    let e1 = store
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect("append second");
    let e1_hash = e1.entry_hash.clone();

    // Append third entry
    let e2 = store
        .ledger()
        .append_event(&make_test_provenance_event(2))
        .await
        .expect("append third");

    // Verify chain linkage
    assert_eq!(e0.sequence, 0);
    assert!(e0.prev_hash.is_none());

    assert_eq!(e1.sequence, 1);
    assert_eq!(
        e1.prev_hash.as_ref(),
        Some(&e0_hash),
        "e1 prev_hash should link to e0"
    );

    assert_eq!(e2.sequence, 2);
    assert_eq!(
        e2.prev_hash.as_ref(),
        Some(&e1_hash),
        "e2 prev_hash should link to e1"
    );
}

#[tokio::test]
async fn ledger_append_event_roundtrip_verify_chain() {
    let (_temp_dir, store) = create_test_store().await;

    // Build a chain of 3 entries using append_event
    let e0 = store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append e0");
    let e1 = store
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect("append e1");
    let e2 = store
        .ledger()
        .append_event(&make_test_provenance_event(2))
        .await
        .expect("append e2");

    // Load all entries
    let loaded: Vec<LedgerEntry> = store.ledger().list_recent(10).await.expect("load entries");
    assert_eq!(loaded.len(), 3);

    // Rebuild in-memory ledger from loaded entries (newest-first → reverse to chronological)
    let by_seq: Vec<LedgerEntry> = {
        let mut v = loaded;
        v.reverse();
        v
    };
    let rebuilt = InMemoryLedger::load_entries(by_seq);

    // Verify the chain is valid
    rebuilt
        .verify_chain()
        .expect("chain should be valid after atomic append_event roundtrip");

    // Verify specific entries match what we got from append_event
    assert_eq!(
        rebuilt.entries()[0].entry_hash.as_str(),
        e0.entry_hash.as_str()
    );
    assert_eq!(
        rebuilt.entries()[1].entry_hash.as_str(),
        e1.entry_hash.as_str()
    );
    assert_eq!(
        rebuilt.entries()[2].entry_hash.as_str(),
        e2.entry_hash.as_str()
    );
}

#[tokio::test]
async fn ledger_append_event_duplicate_event_rejected() {
    use ferrum_proto::{ObjectRef, ObjectType};

    let (_temp_dir, store) = create_test_store().await;

    let event = make_test_provenance_event(0);
    let event_id = event.event_id;

    // First append should succeed
    store
        .ledger()
        .append_event(&event)
        .await
        .expect("first append_event should succeed");

    // Creating another event with the same event_id should fail at persistence level
    // (UNIQUE constraint on event_id in ledger_entries)
    let dup_event = ferrum_proto::ProvenanceEvent {
        event_id, // same id
        kind: ferrum_proto::ProvenanceEventKind::IntentCompiled,
        occurred_at: ferrum_proto::Timestamp::default(),
        actor: ActorRef {
            actor_type: ActorType::User,
            actor_id: "user-x".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::Intent,
            object_id: "intent-x".to_string(),
            summary: None,
        },
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: vec![],
        sensitivity_labels: vec![],
        parent_edges: vec![],
        hash_chain: ferrum_proto::HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
    };

    let result = store.ledger().append_event(&dup_event).await;
    assert!(
        result.is_err(),
        "duplicate event_id should be rejected by UNIQUE constraint"
    );
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

// ---------------------------------------------------------------------------
// Ledger chain verification on reload tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verify_ledger_chain_empty_ledger_succeeds() {
    // An empty ledger is valid (no entries to verify)
    let (_temp_dir, store) = create_test_store().await;

    store
        .verify_ledger_chain()
        .await
        .expect("empty ledger should pass verification");
}

#[tokio::test]
async fn verify_ledger_chain_valid_chain_succeeds() {
    // Build a valid chain of 3 entries using append_event
    let (_temp_dir, store) = create_test_store().await;

    store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append e0");
    store
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect("append e1");
    store
        .ledger()
        .append_event(&make_test_provenance_event(2))
        .await
        .expect("append e2");

    // Verification should succeed
    store
        .verify_ledger_chain()
        .await
        .expect("valid chain should pass verification");
}

#[tokio::test]
async fn verify_ledger_chain_tampered_entry_fails() {
    // Build a valid chain, then tamper with the raw_json directly in the DB
    let (_temp_dir, store) = create_test_store().await;

    store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append e0");
    store
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect("append e1");

    // Load the first entry to get its raw_json
    let entries = store.ledger().list_all().await.expect("load entries");
    let mut first_entry: ferrum_ledger::LedgerEntry = entries.into_iter().next().unwrap();

    // Tamper: change the entry_hash to a wrong value (this is what verify_chain checks)
    first_entry.entry_hash = "wronghash________________________________".into();

    // Write the tampered entry back to raw_json
    let tampered_json = serde_json::to_string(&first_entry).expect("serialize tampered entry");
    let pool = store.pool();
    sqlx::query("UPDATE ledger_entries SET raw_json = ?1 WHERE entry_id = 1")
        .bind(&tampered_json)
        .execute(pool)
        .await
        .expect("tamper update should succeed");

    let err = store
        .verify_ledger_chain()
        .await
        .expect_err("tampered chain should fail verification");

    assert!(
        format!("{}", err).contains("ledger chain verification failed"),
        "error should mention ledger chain verification: {}",
        err
    );
}

#[tokio::test]
async fn verify_ledger_chain_reload_after_close_and_reconnect() {
    // Test that a chain persists correctly and verifies after store reconnection
    use ferrum_ledger::InMemoryLedger;

    let (temp_dir, store1) = create_test_store().await;

    // Build a chain using store1
    let e0 = store1
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append e0");
    let e1 = store1
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect("append e1");

    // Drop store1 to release the DB lock
    drop(store1);

    // Reconnect to the same DB
    let db_path = temp_dir.path().join("store.sqlite");
    let database_url = format!("sqlite://{}", db_path.display());
    let store2 = SqliteStore::connect(&database_url)
        .await
        .expect("reconnect to sqlite");

    // Verify the chain on the reconnected store
    store2
        .verify_ledger_chain()
        .await
        .expect("chain should verify after reconnect");

    // Also verify the entries match what we originally appended
    let loaded: Vec<ferrum_ledger::LedgerEntry> =
        store2.ledger().list_all().await.expect("list all entries");
    assert_eq!(loaded.len(), 2);

    let rebuilt = InMemoryLedger::load_entries(loaded.clone());
    rebuilt
        .verify_chain()
        .expect("chain should be valid after roundtrip");

    assert_eq!(loaded[0].entry_hash.as_str(), e0.entry_hash.as_str());
    assert_eq!(loaded[1].entry_hash.as_str(), e1.entry_hash.as_str());
}

#[tokio::test]
async fn verify_ledger_chain_detects_tampered_content_hash_column() {
    // Test that tampering the content_hash column (without modifying raw_json)
    // is detected by the column cross-check.
    let (_temp_dir, store) = create_test_store().await;

    store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append genesis");

    // Tamper the content_hash column directly (simulates DB-level tamper)
    let pool = store.pool();
    sqlx::query("UPDATE ledger_entries SET content_hash = 'tampered_content_hash_______' WHERE entry_id = 1")
        .execute(pool)
        .await
        .expect("tamper content_hash should succeed");

    let err = store
        .verify_ledger_chain()
        .await
        .expect_err("tampered content_hash should fail verification");

    assert!(
        format!("{}", err).contains("content_hash column"),
        "error should mention content_hash column tamper: {}",
        err
    );
}

#[tokio::test]
async fn verify_ledger_chain_detects_tampered_previous_ledger_hash_column() {
    // Test that tampering the previous_ledger_hash column (without modifying raw_json)
    // is detected by the column cross-check.
    let (_temp_dir, store) = create_test_store().await;

    let _e0 = store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append genesis");
    store
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect("append second");

    // Tamper the previous_ledger_hash column of entry 2 to point to wrong hash
    let pool = store.pool();
    sqlx::query("UPDATE ledger_entries SET previous_ledger_hash = 'wrong_prev_hash_______________' WHERE entry_id = 2")
        .execute(pool)
        .await
        .expect("tamper previous_ledger_hash should succeed");

    // The tamper should be caught by the column cross-check
    let err = store
        .verify_ledger_chain()
        .await
        .expect_err("tampered previous_ledger_hash should fail verification");

    assert!(
        format!("{}", err).contains("previous_ledger_hash column"),
        "error should mention previous_ledger_hash column tamper: {}",
        err
    );
}

#[tokio::test]
async fn append_event_rejects_tampered_tip_content_hash() {
    // Test that append_event verifies tip integrity before using tip hash as prev_hash.
    // If the tip's content_hash column is tampered after verify_ledger_chain passes,
    // the next append must reject the tampered tip rather than building a wrong chain.
    let (_temp_dir, store) = create_test_store().await;

    // Append genesis entry
    store
        .ledger()
        .append_event(&make_test_provenance_event(0))
        .await
        .expect("append genesis");

    // Tamper the tip's content_hash column directly (simulates live DB tamper)
    let pool = store.pool();
    sqlx::query("UPDATE ledger_entries SET content_hash = 'tampered_tip_content_hash____' WHERE entry_id = 1")
        .execute(pool)
        .await
        .expect("tamper content_hash should succeed");

    // Attempt to append another event - this must fail because the tip is invalid
    let err = store
        .ledger()
        .append_event(&make_test_provenance_event(1))
        .await
        .expect_err("append with tampered tip should fail");

    assert!(
        format!("{}", err).contains("append rejected: tip content_hash"),
        "error should mention append rejected and tip content_hash: {}",
        err
    );
}
