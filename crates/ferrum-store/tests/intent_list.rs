//! Tests for IntentRepo::list_intents_with_exec_state

use chrono::Utc;
use ferrum_proto::{
    CapabilityId, CapabilityLease, CapabilityStatus, Decision, ExecutionId, ExecutionRecord,
    ExecutionState, IntentEnvelope, IntentId, IntentStatus, JsonMap, PrincipalId, ProposalId,
    RiskTier, Timestamp,
};
use ferrum_store::{CapabilityRepo, ExecutionRepo, IntentRepo, SqliteStore};

/// Creates a Timestamp relative to now.
fn ts_offset(seconds: i64) -> Timestamp {
    Utc::now() + chrono::Duration::seconds(seconds)
}

/// Creates a minimal but complete IntentEnvelope for testing.
fn make_intent_envelope(intent_id: IntentId, status: IntentStatus) -> IntentEnvelope {
    IntentEnvelope {
        intent_id,
        principal_id: PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![],
        forbidden_outcomes: vec![],
        resource_scope: vec![],
        risk_tier: RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 300_000,
            max_steps: 100,
            max_retries_per_step: 3,
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
        status,
        created_at: ts_offset(0),
        expires_at: ts_offset(3600),
    }
}

/// Inserts a complete IntentEnvelope into the store.
async fn insert_intent(store: &SqliteStore, envelope: &IntentEnvelope) {
    let raw_json = serde_json::to_string(envelope).unwrap();
    sqlx::query(
        "INSERT INTO intents (intent_id, principal_id, normalized_goal, status, risk_tier, \
         approval_mode, default_rollback_class, created_at, expires_at, raw_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(envelope.intent_id.to_string())
    .bind(envelope.principal_id.to_string())
    .bind(&envelope.normalized_goal)
    .bind(format!("{:?}", envelope.status))
    .bind(format!("{:?}", envelope.risk_tier))
    .bind(format!("{:?}", envelope.approval_mode))
    .bind(format!("{:?}", envelope.default_rollback_class))
    .bind(envelope.created_at.to_rfc3339())
    .bind(envelope.expires_at.to_rfc3339())
    .bind(&raw_json)
    .execute(store.pool())
    .await
    .unwrap();
}

/// Creates and inserts a minimal Proposal for the given intent.
async fn insert_proposal(store: &SqliteStore, intent_id: IntentId) -> ProposalId {
    let proposal_id = ProposalId::new();
    let raw_json = serde_json::json!({
        "proposal_id": proposal_id.to_string(),
        "intent_id": intent_id.to_string(),
        "step_index": 0,
        "server_name": "test-server",
        "tool_name": "test-tool",
        "estimated_risk": "Low",
        "requested_rollback_class": "R0NativeReversible",
        "created_at": ts_offset(0).to_rfc3339(),
    });
    sqlx::query(
        "INSERT INTO proposals (proposal_id, intent_id, step_index, server_name, tool_name, \
         estimated_risk, requested_rollback_class, created_at, raw_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(proposal_id.to_string())
    .bind(intent_id.to_string())
    .bind(0)
    .bind("test-server")
    .bind("test-tool")
    .bind("Low")
    .bind("R0NativeReversible")
    .bind(ts_offset(0).to_rfc3339())
    .bind(raw_json.to_string())
    .execute(store.pool())
    .await
    .unwrap();
    proposal_id
}

/// Inserts a capability for the given intent/proposal.
async fn insert_capability(
    store: &SqliteStore,
    intent_id: IntentId,
    proposal_id: ProposalId,
) -> CapabilityId {
    let cap_id = CapabilityId::new();
    let cap = CapabilityLease {
        capability_id: cap_id,
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".into(),
            tool_name: "test-tool".into(),
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
        issued_by: "test".into(),
        policy_bundle_id: ferrum_proto::PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status: CapabilityStatus::Active,
        issued_at: ts_offset(0),
        expires_at: ts_offset(3600),
        revoked_at: None,
        metadata: JsonMap::new(),
    };
    store.capabilities().insert(&cap).await.unwrap();
    cap_id
}

/// Inserts an execution record for the given intent/proposal/capability.
async fn insert_execution(
    store: &SqliteStore,
    intent_id: IntentId,
    proposal_id: ProposalId,
    cap_id: CapabilityId,
    state: ExecutionState,
) -> ExecutionId {
    let exec_id = ExecutionId::new();
    let exec = ExecutionRecord {
        execution_id: exec_id,
        proposal_id,
        intent_id,
        capability_id: cap_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state,
        started_at: ts_offset(0),
        finished_at: None,
        result_digest: None,
        metadata: JsonMap::new(),
    };
    store.executions().insert(&exec).await.unwrap();
    exec_id
}

#[tokio::test]
async fn list_intents_with_exec_state_no_execution_returns_none() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    // Insert an intent with no execution
    let intent_id = IntentId::new();
    let envelope = make_intent_envelope(intent_id, IntentStatus::Active);
    insert_intent(&store, &envelope).await;

    // Query using list_intents_with_exec_state
    let repo = store.intents();
    let (items, next_cursor) = repo
        .list_intents_with_exec_state(None, &[], None, 100)
        .await
        .unwrap();

    assert_eq!(items.len(), 1, "should return 1 intent");
    let (intent, exec_state) = &items[0];
    assert_eq!(intent.intent_id, intent_id);
    assert!(
        exec_state.is_none(),
        "exec_state should be None when no execution exists"
    );
    assert!(next_cursor.is_none(), "should have no next cursor");
}

#[tokio::test]
async fn list_intents_with_exec_state_with_execution_returns_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    // Insert intent, proposal, capability, and execution
    let intent_id = IntentId::new();
    let envelope = make_intent_envelope(intent_id, IntentStatus::Active);
    insert_intent(&store, &envelope).await;

    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = insert_capability(&store, intent_id, proposal_id).await;
    insert_execution(
        &store,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Running,
    )
    .await;

    // Query using list_intents_with_exec_state
    let repo = store.intents();
    let (items, next_cursor) = repo
        .list_intents_with_exec_state(None, &[], None, 100)
        .await
        .unwrap();

    assert_eq!(items.len(), 1, "should return 1 intent");
    let (intent, exec_state) = &items[0];
    assert_eq!(intent.intent_id, intent_id);
    assert!(
        exec_state.is_some(),
        "exec_state should be Some when execution exists"
    );
    assert_eq!(exec_state.as_ref().unwrap(), "Running");
    assert!(next_cursor.is_none(), "should have no next cursor");
}

#[tokio::test]
async fn list_intents_with_exec_state_multiple_intents_mixed_execution_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    // Intent 1 has an execution
    let intent1_id = IntentId::new();
    let envelope1 = make_intent_envelope(intent1_id, IntentStatus::Active);
    insert_intent(&store, &envelope1).await;

    let proposal1_id = insert_proposal(&store, intent1_id).await;
    let cap1_id = insert_capability(&store, intent1_id, proposal1_id).await;
    insert_execution(
        &store,
        intent1_id,
        proposal1_id,
        cap1_id,
        ExecutionState::Committed,
    )
    .await;

    // Intent 2 has no execution
    let intent2_id = IntentId::new();
    let envelope2 = make_intent_envelope(intent2_id, IntentStatus::Active);
    insert_intent(&store, &envelope2).await;

    // Query using list_intents_with_exec_state
    let repo = store.intents();
    let (items, next_cursor) = repo
        .list_intents_with_exec_state(None, &[], None, 100)
        .await
        .unwrap();

    assert_eq!(items.len(), 2, "should return 2 intents");

    // Find each intent by ID
    let intent1_item = items
        .iter()
        .find(|(i, _)| i.intent_id == intent1_id)
        .unwrap();
    let intent2_item = items
        .iter()
        .find(|(i, _)| i.intent_id == intent2_id)
        .unwrap();

    assert!(intent1_item.1.is_some(), "intent1 should have exec_state");
    assert_eq!(intent1_item.1.as_ref().unwrap(), "Committed");
    assert!(
        intent2_item.1.is_none(),
        "intent2 should have no exec_state"
    );
    assert!(next_cursor.is_none(), "should have no next cursor");
}

#[tokio::test]
async fn list_intents_with_exec_state_filters_by_status() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    // Insert an Active intent with execution
    let active_id = IntentId::new();
    let active_envelope = make_intent_envelope(active_id, IntentStatus::Active);
    insert_intent(&store, &active_envelope).await;

    let proposal_id = insert_proposal(&store, active_id).await;
    let cap_id = insert_capability(&store, active_id, proposal_id).await;
    insert_execution(
        &store,
        active_id,
        proposal_id,
        cap_id,
        ExecutionState::Running,
    )
    .await;

    // Insert a Closed intent (no execution needed)
    let closed_id = IntentId::new();
    let closed_envelope = make_intent_envelope(closed_id, IntentStatus::Closed);
    insert_intent(&store, &closed_envelope).await;

    // Query only Active intents
    let repo = store.intents();
    let (items, next_cursor) = repo
        .list_intents_with_exec_state(None, &[IntentStatus::Active], None, 100)
        .await
        .unwrap();

    assert_eq!(items.len(), 1, "should return only 1 Active intent");
    assert_eq!(items[0].0.intent_id, active_id);
    assert!(next_cursor.is_none());
}
