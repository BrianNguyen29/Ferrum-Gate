use chrono::{Duration, Utc};
use ferrum_proto::{
    ActionProposal, ActorRef, ActorType, ApprovalRequest, ApprovalState, CapabilityLease,
    CapabilityStatus, Decision, EffectType, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, OutcomeClause, PolicyBundleId, PrincipalId, ProposalId, ResourceMode,
    RollbackClass, RollbackContract, RollbackState, RollbackTarget, TaintBudget, TimeBudget,
    ToolBinding, TrustContextSummary,
};
use tempfile::TempDir;

use crate::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, RollbackRepo,
    SqliteStore,
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

    let pending = store
        .approvals()
        .list_pending()
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
