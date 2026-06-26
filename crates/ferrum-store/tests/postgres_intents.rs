#![cfg(feature = "postgres")]

//! Live DB-backed tests for PostgresIntentRepo, PostgresProposalRepo,
//! PostgresExecutionRepo, PostgresCapabilityRepo, PostgresRollbackRepo,
//! PostgresApprovalRepo, PostgresProvenanceRepo, PostgresLedgerRepo, and
//! PostgresPolicyBundleRepo.
//!
//! These tests run against a local Docker PostgreSQL when available and skip
//! gracefully when the database is unreachable. The DSN is taken from
//! `docker-compose.postgres.yml`.

use chrono::Utc;
use ferrum_proto::{
    ActionProposal, ActionType, ActorRef, ActorType, ApprovalId, ApprovalRequest, ApprovalState,
    CapabilityId, CapabilityLease, CapabilityStatus, Decision, EffectType, EventId, ExecutionId,
    ExecutionRecord, ExecutionState, HashChainRef, IntentEnvelope, IntentId, IntentStatus, JsonMap,
    LifecycleOutboxRecord, LifecycleOutboxStatus, ObjectRef, ObjectType, OutcomeClause,
    PolicyBundle, PolicyBundleId, PrincipalId, ProposalId, ProvenanceEdge, ProvenanceEdgeType,
    ProvenanceEvent, ProvenanceEventKind, ProvenanceQueryRequest, RiskTier, RollbackClass,
    RollbackContract, RollbackContractId, RollbackState, RollbackTarget, Timestamp,
};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerEntry, LedgerRepo,
    LifecycleOutboxRepo, PolicyBundleRepo, ProposalRepo, ProvenanceRepo, RollbackRepo, StoreFacade,
    postgres::PostgresStore,
};
use std::sync::Arc;

const TEST_DSN: &str =
    "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test";

static PG_TEST_MUTEX: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

fn pg_lock() -> &'static tokio::sync::Mutex<()> {
    PG_TEST_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn ts_offset(seconds: i64) -> Timestamp {
    Utc::now() + chrono::Duration::seconds(seconds)
}

fn make_test_intent(intent_id: IntentId, status: IntentStatus) -> IntentEnvelope {
    let now = ts_offset(0);
    IntentEnvelope {
        intent_id,
        principal_id: PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: vec![],
        resource_scope: vec![],
        risk_tier: RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
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
        status,
        created_at: now,
        expires_at: now + chrono::Duration::minutes(15),
    }
}

/// Attempt to connect to the local Postgres and bootstrap the schema.
/// Returns `None` if the database is unreachable so tests can skip.
/// Tests are serialized via a global lock to avoid concurrent table drops.
/// The returned guard must be held for the entire test body.
async fn setup() -> Option<(PostgresStore, tokio::sync::MutexGuard<'static, ()>)> {
    let guard = pg_lock().lock().await;

    let store = match PostgresStore::connect(TEST_DSN).await {
        Ok(s) => s,
        Err(_) => return None,
    };

    // Clean slate for each test
    let _ = sqlx::query("DROP TABLE IF EXISTS executions CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS proposals CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS capabilities CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS rollback_contracts CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS lifecycle_outbox CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS approvals CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS provenance_edges CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS provenance_events CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS ledger_entries CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS policy_bundles CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS intents CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS _schema_version CASCADE")
        .execute(store.pool())
        .await;

    if let Err(e) = store.apply_embedded_migrations().await {
        eprintln!("apply_embedded_migrations failed: {}", e);
        return None;
    }

    Some((store, guard))
}

#[tokio::test]
async fn postgres_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let intent_id = IntentId::new();
    let intent = make_test_intent(intent_id, IntentStatus::Active);
    let repo = store.intents();

    repo.insert(&intent).await.unwrap();
    let fetched = repo.get(intent_id).await.unwrap();

    assert!(fetched.is_some(), "expected intent to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.intent_id, intent_id);
    assert_eq!(fetched.normalized_goal, intent.normalized_goal);
}

#[tokio::test]
async fn postgres_update_changes_fields() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let intent_id = IntentId::new();
    let mut intent = make_test_intent(intent_id, IntentStatus::Active);
    let repo = store.intents();

    repo.insert(&intent).await.unwrap();

    intent.normalized_goal = "updated goal".to_string();
    intent.status = IntentStatus::Closed;
    repo.update(&intent).await.unwrap();

    let fetched = repo
        .get(intent_id)
        .await
        .unwrap()
        .expect("intent should exist");
    assert_eq!(fetched.normalized_goal, "updated goal");
    assert!(
        matches!(fetched.status, IntentStatus::Closed),
        "status should be Closed"
    );
}

#[tokio::test]
async fn postgres_update_status() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let intent_id = IntentId::new();
    let intent = make_test_intent(intent_id, IntentStatus::Active);
    let repo = store.intents();

    repo.insert(&intent).await.unwrap();
    repo.update_status(intent_id, IntentStatus::Quarantined)
        .await
        .unwrap();

    let fetched = repo
        .get(intent_id)
        .await
        .unwrap()
        .expect("intent should exist");
    assert!(
        matches!(fetched.status, IntentStatus::Quarantined),
        "status should be Quarantined"
    );
}

#[tokio::test]
async fn postgres_list_by_status() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.intents();
    let id1 = IntentId::new();
    let id2 = IntentId::new();

    repo.insert(&make_test_intent(id1, IntentStatus::Active))
        .await
        .unwrap();
    repo.insert(&make_test_intent(id2, IntentStatus::Closed))
        .await
        .unwrap();

    let active = repo.list_by_status(IntentStatus::Active).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].intent_id, id1);

    let closed = repo.list_by_status(IntentStatus::Closed).await.unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].intent_id, id2);
}

#[tokio::test]
async fn postgres_list_intents_pagination() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.intents();
    let id1 = IntentId::new();
    let id2 = IntentId::new();
    let id3 = IntentId::new();

    repo.insert(&make_test_intent(id1, IntentStatus::Active))
        .await
        .unwrap();
    repo.insert(&make_test_intent(id2, IntentStatus::Active))
        .await
        .unwrap();
    repo.insert(&make_test_intent(id3, IntentStatus::Active))
        .await
        .unwrap();

    let (items, cursor) = repo.list_intents(None, &[], None, 2).await.unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        cursor.is_some(),
        "should have next cursor when more items exist"
    );

    let (items2, cursor2) = repo
        .list_intents(None, &[], cursor.as_deref(), 2)
        .await
        .unwrap();
    assert_eq!(items2.len(), 1);
    assert!(cursor2.is_none(), "should have no next cursor on last page");

    // Filter by status
    let (filtered, _) = repo
        .list_intents(None, &[IntentStatus::Active], None, 10)
        .await
        .unwrap();
    assert_eq!(filtered.len(), 3);

    // Filter by intent_id
    let (single, _) = repo.list_intents(Some(id1), &[], None, 10).await.unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].intent_id, id1);
}

#[tokio::test]
async fn postgres_list_intents_with_exec_state_no_execution() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.intents();
    let intent_id = IntentId::new();
    repo.insert(&make_test_intent(intent_id, IntentStatus::Active))
        .await
        .unwrap();

    let (items, cursor) = repo
        .list_intents_with_exec_state(None, &[], None, 100)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.intent_id, intent_id);
    assert!(
        items[0].1.is_none(),
        "exec_state should be None when no execution exists"
    );
    assert!(cursor.is_none());
}

#[tokio::test]
async fn postgres_list_intents_with_exec_state_with_execution() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.intents();
    let intent_id = IntentId::new();
    let intent = make_test_intent(intent_id, IntentStatus::Active);
    repo.insert(&intent).await.unwrap();

    // Insert execution directly via SQL since PostgresExecutionRepo is a skeleton.
    let exec_id = ExecutionId::new();
    let raw_json = serde_json::json!({
        "execution_id": exec_id.to_string(),
        "intent_id": intent_id.to_string(),
        "proposal_id": ProposalId::new().to_string(),
        "capability_id": CapabilityId::new().to_string(),
        "decision": "Allow",
        "state": "Running",
        "started_at": ts_offset(0).to_rfc3339(),
    });
    sqlx::query(
        "INSERT INTO executions \
         (execution_id, intent_id, proposal_id, capability_id, decision, state, started_at, raw_json) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(exec_id.to_string())
    .bind(intent_id.to_string())
    .bind(ProposalId::new().to_string())
    .bind(CapabilityId::new().to_string())
    .bind("Allow")
    .bind("Running")
    .bind(ts_offset(0).to_rfc3339())
    .bind(raw_json.to_string())
    .execute(store.pool())
    .await
    .unwrap();

    let (items, cursor) = repo
        .list_intents_with_exec_state(None, &[], None, 100)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.intent_id, intent_id);
    assert_eq!(
        items[0].1.as_deref(),
        Some("Running"),
        "exec_state should be Running"
    );
    assert!(cursor.is_none());
}

#[tokio::test]
async fn postgres_list_intents_with_exec_state_filters_by_status() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.intents();
    let active_id = IntentId::new();
    let closed_id = IntentId::new();

    repo.insert(&make_test_intent(active_id, IntentStatus::Active))
        .await
        .unwrap();
    repo.insert(&make_test_intent(closed_id, IntentStatus::Closed))
        .await
        .unwrap();

    let exec_id = ExecutionId::new();
    let raw_json = serde_json::json!({
        "execution_id": exec_id.to_string(),
        "intent_id": active_id.to_string(),
        "proposal_id": ProposalId::new().to_string(),
        "capability_id": CapabilityId::new().to_string(),
        "decision": "Allow",
        "state": "Committed",
        "started_at": ts_offset(0).to_rfc3339(),
    });
    sqlx::query(
        "INSERT INTO executions \
         (execution_id, intent_id, proposal_id, capability_id, decision, state, started_at, raw_json) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(exec_id.to_string())
    .bind(active_id.to_string())
    .bind(ProposalId::new().to_string())
    .bind(CapabilityId::new().to_string())
    .bind("Allow")
    .bind("Committed")
    .bind(ts_offset(0).to_rfc3339())
    .bind(raw_json.to_string())
    .execute(store.pool())
    .await
    .unwrap();

    let (items, _) = repo
        .list_intents_with_exec_state(None, &[IntentStatus::Active], None, 100)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0.intent_id, active_id);
    assert_eq!(items[0].1.as_deref(), Some("Committed"));
}

fn make_test_proposal(
    proposal_id: ProposalId,
    intent_id: IntentId,
    step_index: u32,
) -> ActionProposal {
    ActionProposal {
        proposal_id,
        intent_id,
        step_index,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "ReadOnlyAnalysis".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: vec![],
        metadata: JsonMap::new(),
        created_at: ts_offset(0),
    }
}

#[tokio::test]
async fn postgres_proposal_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let proposal = make_test_proposal(proposal_id, intent_id, 0);
    let repo = store.proposals();

    repo.insert(&proposal).await.unwrap();
    let fetched = repo.get(proposal_id).await.unwrap();

    assert!(fetched.is_some(), "expected proposal to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.proposal_id, proposal_id);
    assert_eq!(fetched.intent_id, intent_id);
    assert_eq!(fetched.server_name, "test-server");
    assert_eq!(fetched.tool_name, "test-tool");
}

#[tokio::test]
async fn postgres_proposal_list_by_intent() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.proposals();
    let intent1 = IntentId::new();
    let intent2 = IntentId::new();
    let p1 = ProposalId::new();
    let p2 = ProposalId::new();
    let p3 = ProposalId::new();

    repo.insert(&make_test_proposal(p1, intent1, 0))
        .await
        .unwrap();
    repo.insert(&make_test_proposal(p2, intent1, 1))
        .await
        .unwrap();
    repo.insert(&make_test_proposal(p3, intent2, 0))
        .await
        .unwrap();

    let for_intent1 = repo.list_by_intent(intent1).await.unwrap();
    assert_eq!(for_intent1.len(), 2);
    assert_eq!(for_intent1[0].proposal_id, p1);
    assert_eq!(for_intent1[1].proposal_id, p2);

    let for_intent2 = repo.list_by_intent(intent2).await.unwrap();
    assert_eq!(for_intent2.len(), 1);
    assert_eq!(for_intent2[0].proposal_id, p3);
}

fn make_test_execution(
    execution_id: ExecutionId,
    intent_id: IntentId,
    proposal_id: ProposalId,
    capability_id: CapabilityId,
    state: ExecutionState,
) -> ExecutionRecord {
    ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state,
        started_at: ts_offset(0),
        finished_at: None,
        result_digest: None,
        metadata: JsonMap::new(),
    }
}

#[tokio::test]
async fn postgres_execution_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.executions();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let exec_id = ExecutionId::new();
    let exec = make_test_execution(
        exec_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Proposed,
    );

    repo.insert(&exec).await.unwrap();
    let fetched = repo.get(exec_id).await.unwrap();

    assert!(fetched.is_some(), "expected execution to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.execution_id, exec_id);
    assert_eq!(fetched.intent_id, intent_id);
    assert_eq!(fetched.capability_id, cap_id);
}

#[tokio::test]
async fn postgres_execution_update_changes_fields() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.executions();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let exec_id = ExecutionId::new();
    let exec = make_test_execution(
        exec_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Proposed,
    );

    repo.insert(&exec).await.unwrap();

    let mut updated = exec.clone();
    updated.decision = Decision::Deny;
    let finished = Some(ts_offset(10));
    updated.finished_at = finished;
    updated.result_digest = Some("digest-123".to_string());
    repo.update(&updated).await.unwrap();

    let fetched = repo
        .get(exec_id)
        .await
        .unwrap()
        .expect("execution should exist");
    assert!(
        matches!(fetched.decision, Decision::Deny),
        "decision should be Deny"
    );
    assert_eq!(fetched.finished_at, finished);
    assert_eq!(fetched.result_digest, Some("digest-123".to_string()));
}

#[tokio::test]
async fn postgres_execution_update_state_valid_transition() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.executions();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let exec_id = ExecutionId::new();
    let exec = make_test_execution(
        exec_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Proposed,
    );

    repo.insert(&exec).await.unwrap();
    repo.update_state(exec_id, ExecutionState::Running)
        .await
        .unwrap();

    let fetched = repo
        .get(exec_id)
        .await
        .unwrap()
        .expect("execution should exist");
    assert!(
        matches!(fetched.state, ExecutionState::Running),
        "state should be Running"
    );
}

#[tokio::test]
async fn postgres_execution_update_state_invalid_transition() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.executions();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let exec_id = ExecutionId::new();
    let exec = make_test_execution(
        exec_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Committed,
    );

    repo.insert(&exec).await.unwrap();
    let err = repo
        .update_state(exec_id, ExecutionState::Running)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ferrum_store::StoreError::InvalidState(_)),
        "expected InvalidState error for transition out of terminal state, got: {}",
        err
    );
}

#[tokio::test]
async fn postgres_execution_list_by_intent() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.executions();
    let intent1 = IntentId::new();
    let intent2 = IntentId::new();
    let e1 = ExecutionId::new();
    let e2 = ExecutionId::new();
    let e3 = ExecutionId::new();

    repo.insert(&make_test_execution(
        e1,
        intent1,
        ProposalId::new(),
        CapabilityId::new(),
        ExecutionState::Proposed,
    ))
    .await
    .unwrap();
    repo.insert(&make_test_execution(
        e2,
        intent1,
        ProposalId::new(),
        CapabilityId::new(),
        ExecutionState::Running,
    ))
    .await
    .unwrap();
    repo.insert(&make_test_execution(
        e3,
        intent2,
        ProposalId::new(),
        CapabilityId::new(),
        ExecutionState::Proposed,
    ))
    .await
    .unwrap();

    let for_intent1 = repo.list_by_intent(intent1).await.unwrap();
    assert_eq!(for_intent1.len(), 2);
    assert_eq!(for_intent1[0].execution_id, e2);
    assert_eq!(for_intent1[1].execution_id, e1);

    let for_intent2 = repo.list_by_intent(intent2).await.unwrap();
    assert_eq!(for_intent2.len(), 1);
    assert_eq!(for_intent2[0].execution_id, e3);
}

#[tokio::test]
async fn postgres_execution_list_by_capability() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.executions();
    let cap1 = CapabilityId::new();
    let cap2 = CapabilityId::new();
    let e1 = ExecutionId::new();
    let e2 = ExecutionId::new();
    let e3 = ExecutionId::new();

    repo.insert(&make_test_execution(
        e1,
        IntentId::new(),
        ProposalId::new(),
        cap1,
        ExecutionState::Proposed,
    ))
    .await
    .unwrap();
    repo.insert(&make_test_execution(
        e2,
        IntentId::new(),
        ProposalId::new(),
        cap1,
        ExecutionState::Running,
    ))
    .await
    .unwrap();
    repo.insert(&make_test_execution(
        e3,
        IntentId::new(),
        ProposalId::new(),
        cap2,
        ExecutionState::Proposed,
    ))
    .await
    .unwrap();

    let for_cap1 = repo.list_by_capability(cap1).await.unwrap();
    assert_eq!(for_cap1.len(), 2);
    assert_eq!(for_cap1[0].execution_id, e2);
    assert_eq!(for_cap1[1].execution_id, e1);

    let for_cap2 = repo.list_by_capability(cap2).await.unwrap();
    assert_eq!(for_cap2.len(), 1);
    assert_eq!(for_cap2[0].execution_id, e3);
}

fn make_test_capability(
    capability_id: CapabilityId,
    intent_id: IntentId,
    proposal_id: ProposalId,
    status: CapabilityStatus,
) -> CapabilityLease {
    CapabilityLease {
        capability_id,
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
        policy_bundle_id: PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status,
        issued_at: ts_offset(0),
        expires_at: ts_offset(3600),
        revoked_at: None,
        metadata: JsonMap::new(),
    }
}

#[tokio::test]
async fn postgres_capability_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.capabilities();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let cap = make_test_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);

    repo.insert(&cap).await.unwrap();
    let fetched = repo.get(cap_id).await.unwrap();

    assert!(fetched.is_some(), "expected capability to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.capability_id, cap_id);
    assert_eq!(fetched.intent_id, intent_id);
    assert_eq!(fetched.tool_binding.server_name, "test-server");
    assert_eq!(fetched.tool_binding.tool_name, "test-tool");
}

#[tokio::test]
async fn postgres_capability_update_changes_fields() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.capabilities();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let mut cap = make_test_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);

    repo.insert(&cap).await.unwrap();

    let revoked_at = Some(ts_offset(10));
    cap.status = CapabilityStatus::Revoked;
    cap.revoked_at = revoked_at;
    repo.update(&cap).await.unwrap();

    let fetched = repo
        .get(cap_id)
        .await
        .unwrap()
        .expect("capability should exist");
    assert!(
        matches!(fetched.status, CapabilityStatus::Revoked),
        "status should be Revoked"
    );
    assert_eq!(fetched.revoked_at, revoked_at);
}

#[tokio::test]
async fn postgres_capability_update_status_valid_transition() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.capabilities();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let cap = make_test_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);

    repo.insert(&cap).await.unwrap();
    repo.update_status(cap_id, CapabilityStatus::Used)
        .await
        .unwrap();

    let fetched = repo
        .get(cap_id)
        .await
        .unwrap()
        .expect("capability should exist");
    assert!(
        matches!(fetched.status, CapabilityStatus::Used),
        "status should be Used"
    );
}

#[tokio::test]
async fn postgres_capability_update_status_invalid_transition() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.capabilities();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let cap_id = CapabilityId::new();
    let cap = make_test_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Used);

    repo.insert(&cap).await.unwrap();
    let err = repo
        .update_status(cap_id, CapabilityStatus::Active)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ferrum_store::StoreError::InvalidState(_)),
        "expected InvalidState error for transition out of terminal state, got: {}",
        err
    );
}

#[tokio::test]
async fn postgres_capability_list_by_intent() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.capabilities();
    let intent1 = IntentId::new();
    let intent2 = IntentId::new();
    let c1 = CapabilityId::new();
    let c2 = CapabilityId::new();
    let c3 = CapabilityId::new();

    repo.insert(&make_test_capability(
        c1,
        intent1,
        ProposalId::new(),
        CapabilityStatus::Active,
    ))
    .await
    .unwrap();

    // Small sleep to ensure distinct issued_at ordering
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    repo.insert(&make_test_capability(
        c2,
        intent1,
        ProposalId::new(),
        CapabilityStatus::Used,
    ))
    .await
    .unwrap();

    repo.insert(&make_test_capability(
        c3,
        intent2,
        ProposalId::new(),
        CapabilityStatus::Active,
    ))
    .await
    .unwrap();

    let for_intent1 = repo.list_by_intent(intent1).await.unwrap();
    assert_eq!(for_intent1.len(), 2);
    assert_eq!(for_intent1[0].capability_id, c2);
    assert_eq!(for_intent1[1].capability_id, c1);

    let for_intent2 = repo.list_by_intent(intent2).await.unwrap();
    assert_eq!(for_intent2.len(), 1);
    assert_eq!(for_intent2[0].capability_id, c3);
}

fn make_test_rollback_contract(
    contract_id: RollbackContractId,
    execution_id: ExecutionId,
) -> RollbackContract {
    let mut metadata = JsonMap::new();
    metadata.insert(
        "adapter_kind".to_string(),
        serde_json::Value::String("ferrum-adapter-fs".to_string()),
    );
    metadata.insert(
        "snapshot_path".to_string(),
        serde_json::Value::String("/tmp/ferrum-fs-snapshots/exec-123/path-hash".to_string()),
    );

    RollbackContract {
        contract_id,
        intent_id: IntentId::new(),
        proposal_id: ProposalId::new(),
        execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: RollbackClass::R1SnapshotRecoverable,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: "/tmp/test.txt".to_string(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: ts_offset(0),
        expires_at: None,
        metadata,
    }
}

async fn seed_lifecycle_outbox_graph(store: &PostgresStore) -> (ExecutionRecord, RollbackContract) {
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let capability_id = CapabilityId::new();
    let execution_id = ExecutionId::new();
    let contract_id = RollbackContractId::new();

    store
        .intents()
        .insert(&make_test_intent(intent_id, IntentStatus::Active))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(proposal_id, intent_id, 0))
        .await
        .unwrap();
    store
        .capabilities()
        .insert(&make_test_capability(
            capability_id,
            intent_id,
            proposal_id,
            CapabilityStatus::Active,
        ))
        .await
        .unwrap();

    let mut execution = ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Authorized,
        started_at: ts_offset(0),
        finished_at: None,
        result_digest: None,
        metadata: JsonMap::new(),
    };
    store.executions().insert(&execution).await.unwrap();

    let mut contract = make_test_rollback_contract(contract_id, execution_id);
    contract.intent_id = intent_id;
    contract.proposal_id = proposal_id;
    contract.state = RollbackState::PendingPrepare;
    store.rollback_contracts().insert(&contract).await.unwrap();

    execution.rollback_contract_id = Some(contract.contract_id);
    store.executions().update(&execution).await.unwrap();

    (execution, contract)
}

fn lifecycle_outbox_for(
    execution: &ExecutionRecord,
    contract: &RollbackContract,
) -> LifecycleOutboxRecord {
    LifecycleOutboxRecord::pending(
        execution.execution_id,
        Some(contract.contract_id),
        Some(ExecutionState::Authorized),
        ExecutionState::Running,
        Some(RollbackState::PendingPrepare),
        Some(RollbackState::Prepared),
        ProvenanceEventKind::SideEffectPrepared,
        format!("prepare:{}", execution.execution_id),
    )
}

fn lifecycle_action_proposal_submitted_event(execution: &ExecutionRecord) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ActionProposalSubmitted,
        occurred_at: ts_offset(0),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "postgres-test".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::Proposal,
            object_id: execution.proposal_id.to_string(),
            summary: None,
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
        metadata: JsonMap::new(),
        source_runtime_id: None,
    }
}

fn lifecycle_prepared_event(
    execution: &ExecutionRecord,
    contract: &RollbackContract,
) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::SideEffectPrepared,
        occurred_at: ts_offset(0),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "postgres-test".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: contract.contract_id.to_string(),
            summary: None,
        },
        intent_id: Some(execution.intent_id),
        proposal_id: Some(execution.proposal_id),
        execution_id: Some(execution.execution_id),
        capability_id: Some(execution.capability_id),
        rollback_contract_id: Some(contract.contract_id),
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
    }
}

#[tokio::test]
async fn postgres_lifecycle_reconciler_repairs_missing_provenance_and_edge() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };
    let store = Arc::new(store);
    let (mut execution, mut contract) = seed_lifecycle_outbox_graph(store.as_ref()).await;
    let repo = store.lifecycle_outbox();
    let outbox = lifecycle_outbox_for(&execution, &contract);
    let parent_event = lifecycle_action_proposal_submitted_event(&execution);
    store
        .provenance()
        .append_event(&parent_event)
        .await
        .unwrap();

    execution.state = ExecutionState::Running;
    contract.state = RollbackState::Prepared;
    repo.record_lifecycle_transition(&execution, Some(&contract), &outbox)
        .await
        .unwrap();

    let facade: Arc<dyn StoreFacade> = store.clone();
    let report = ferrum_store::reconcile_lifecycle_outbox(&facade, 10)
        .await
        .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.repaired_missing_provenance, 1);
    let reconciled = repo.get(outbox.outbox_id).await.unwrap().unwrap();
    assert_eq!(reconciled.status, LifecycleOutboxStatus::Reconciled);
    let event_id = reconciled.provenance_event_id.unwrap();
    let edges = store.provenance().get_edges_to(event_id).await.unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from_event_id, parent_event.event_id);
}

#[tokio::test]
async fn postgres_lifecycle_reconciler_repairs_missing_edge_for_existing_event() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };
    let store = Arc::new(store);
    let (mut execution, mut contract) = seed_lifecycle_outbox_graph(store.as_ref()).await;
    let repo = store.lifecycle_outbox();
    let outbox = lifecycle_outbox_for(&execution, &contract);
    let parent_event = lifecycle_action_proposal_submitted_event(&execution);
    let prepared_event = lifecycle_prepared_event(&execution, &contract);
    store
        .provenance()
        .append_event(&parent_event)
        .await
        .unwrap();
    store
        .provenance()
        .append_event(&prepared_event)
        .await
        .unwrap();

    execution.state = ExecutionState::Running;
    contract.state = RollbackState::Prepared;
    repo.record_lifecycle_transition(&execution, Some(&contract), &outbox)
        .await
        .unwrap();
    repo.mark_provenance_written(outbox.outbox_id, prepared_event.event_id)
        .await
        .unwrap();

    let facade: Arc<dyn StoreFacade> = store.clone();
    let report = ferrum_store::reconcile_lifecycle_outbox(&facade, 10)
        .await
        .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.already_reconciled, 1);
    let reconciled = repo.get(outbox.outbox_id).await.unwrap().unwrap();
    assert_eq!(reconciled.status, LifecycleOutboxStatus::Reconciled);
    let edges = store
        .provenance()
        .get_edges_to(prepared_event.event_id)
        .await
        .unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from_event_id, parent_event.event_id);
}

#[tokio::test]
async fn postgres_lifecycle_reconciler_marks_missing_parent_for_operator_review() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };
    let store = Arc::new(store);
    let (mut execution, mut contract) = seed_lifecycle_outbox_graph(store.as_ref()).await;
    let repo = store.lifecycle_outbox();
    let outbox = lifecycle_outbox_for(&execution, &contract);

    execution.state = ExecutionState::Running;
    contract.state = RollbackState::Prepared;
    repo.record_lifecycle_transition(&execution, Some(&contract), &outbox)
        .await
        .unwrap();

    let facade: Arc<dyn StoreFacade> = store.clone();
    let report = ferrum_store::reconcile_lifecycle_outbox(&facade, 10)
        .await
        .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.needs_operator_review, 1);
    let review = repo.get(outbox.outbox_id).await.unwrap().unwrap();
    assert_eq!(review.status, LifecycleOutboxStatus::NeedsOperatorReview);
    assert_eq!(
        review.last_error.as_deref(),
        Some("required parent edge is ambiguous or missing")
    );
}

#[tokio::test]
async fn postgres_lifecycle_claims_do_not_overlap_before_lease_expiry() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };
    let (execution, contract) = seed_lifecycle_outbox_graph(&store).await;
    let repo = store.lifecycle_outbox();
    let outbox = lifecycle_outbox_for(&execution, &contract);
    repo.enqueue_lifecycle_transition(&outbox).await.unwrap();

    let first = repo
        .claim_pending_reconciliation(10, "node-a", chrono::Duration::minutes(5))
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].record.outbox_id, outbox.outbox_id);

    let second = repo
        .claim_pending_reconciliation(10, "node-b", chrono::Duration::minutes(5))
        .await
        .unwrap();
    assert!(second.is_empty());
}

#[tokio::test]
async fn postgres_lifecycle_concurrent_claims_are_disjoint_and_fenced() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };
    let (execution, contract) = seed_lifecycle_outbox_graph(&store).await;
    let repo = store.lifecycle_outbox();
    for index in 0..6 {
        let mut outbox = lifecycle_outbox_for(&execution, &contract);
        outbox.idempotency_key = format!("concurrent-claim-{index}");
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();
    }

    let repo_a = repo.clone();
    let repo_b = repo.clone();
    let (claims_a, claims_b) = tokio::join!(
        repo_a.claim_pending_reconciliation(3, "node-a", chrono::Duration::minutes(5)),
        repo_b.claim_pending_reconciliation(3, "node-b", chrono::Duration::minutes(5))
    );
    let claims_a = claims_a.unwrap();
    let claims_b = claims_b.unwrap();
    assert_eq!(claims_a.len(), 3);
    assert_eq!(claims_b.len(), 3);

    let ids_a = claims_a
        .iter()
        .map(|claim| claim.record.outbox_id)
        .collect::<std::collections::HashSet<_>>();
    let ids_b = claims_b
        .iter()
        .map(|claim| claim.record.outbox_id)
        .collect::<std::collections::HashSet<_>>();
    assert!(ids_a.is_disjoint(&ids_b));

    let stale_claim = claims_a[0].clone();
    sqlx::query(
        "UPDATE lifecycle_outbox
         SET reconciliation_lease_expires_at = NOW() - INTERVAL '1 second'
         WHERE outbox_id = $1",
    )
    .bind(stale_claim.record.outbox_id.to_string())
    .execute(store.pool())
    .await
    .unwrap();
    let replacement = repo
        .claim_pending_reconciliation(1, "node-c", chrono::Duration::minutes(5))
        .await
        .unwrap();
    assert_eq!(replacement.len(), 1);
    assert_eq!(
        replacement[0].record.outbox_id,
        stale_claim.record.outbox_id
    );
    assert!(replacement[0].lease.generation > stale_claim.lease.generation);
    assert!(
        !repo
            .mark_reconciled_claimed(&stale_claim.lease, JsonMap::new())
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn postgres_rollback_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.rollback_contracts();
    let exec_id = ExecutionId::new();
    let contract_id = RollbackContractId::new();
    let contract = make_test_rollback_contract(contract_id, exec_id);

    repo.insert(&contract).await.unwrap();
    let fetched = repo.get(contract_id).await.unwrap();

    assert!(fetched.is_some(), "expected contract to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.contract_id, contract_id);
    assert_eq!(fetched.execution_id, exec_id);
    assert_eq!(fetched.adapter_key, "fs");
    assert!(
        matches!(fetched.state, RollbackState::Prepared),
        "state should be Prepared"
    );
}

#[tokio::test]
async fn postgres_rollback_update_changes_fields() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.rollback_contracts();
    let exec_id = ExecutionId::new();
    let contract_id = RollbackContractId::new();
    let mut contract = make_test_rollback_contract(contract_id, exec_id);

    repo.insert(&contract).await.unwrap();

    contract.state = RollbackState::ExecutedAwaitingVerify;
    contract.auto_commit = true;
    repo.update(&contract).await.unwrap();

    let fetched = repo
        .get(contract_id)
        .await
        .unwrap()
        .expect("contract should exist");
    assert!(
        matches!(fetched.state, RollbackState::ExecutedAwaitingVerify),
        "state should be ExecutedAwaitingVerify"
    );
    assert!(fetched.auto_commit, "auto_commit should be true");
}

#[tokio::test]
async fn postgres_rollback_update_state() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.rollback_contracts();
    let exec_id = ExecutionId::new();
    let contract_id = RollbackContractId::new();
    let contract = make_test_rollback_contract(contract_id, exec_id);

    repo.insert(&contract).await.unwrap();
    repo.update_state(contract_id, RollbackState::Verified)
        .await
        .unwrap();

    let fetched = repo
        .get(contract_id)
        .await
        .unwrap()
        .expect("contract should exist");
    assert!(
        matches!(fetched.state, RollbackState::Verified),
        "state should be Verified"
    );
}

#[tokio::test]
async fn postgres_rollback_list_by_execution() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.rollback_contracts();
    let exec1 = ExecutionId::new();
    let exec2 = ExecutionId::new();
    let c1 = RollbackContractId::new();
    let c2 = RollbackContractId::new();
    let c3 = RollbackContractId::new();

    repo.insert(&make_test_rollback_contract(c1, exec1))
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    repo.insert(&make_test_rollback_contract(c2, exec1))
        .await
        .unwrap();
    repo.insert(&make_test_rollback_contract(c3, exec2))
        .await
        .unwrap();

    let for_exec1 = repo.list_by_execution(exec1).await.unwrap();
    assert_eq!(for_exec1.len(), 2);
    assert_eq!(for_exec1[0].contract_id, c2);
    assert_eq!(for_exec1[1].contract_id, c1);

    let for_exec2 = repo.list_by_execution(exec2).await.unwrap();
    assert_eq!(for_exec2.len(), 1);
    assert_eq!(for_exec2[0].contract_id, c3);
}

#[tokio::test]
async fn postgres_rollback_metadata_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.rollback_contracts();
    let exec_id = ExecutionId::new();
    let contract_id = RollbackContractId::new();
    let contract = make_test_rollback_contract(contract_id, exec_id);

    repo.insert(&contract).await.unwrap();

    let fetched = repo
        .get(contract_id)
        .await
        .unwrap()
        .expect("contract should exist");
    assert_eq!(
        fetched
            .metadata
            .get("adapter_kind")
            .and_then(|v| v.as_str()),
        Some("ferrum-adapter-fs")
    );
    assert_eq!(
        fetched
            .metadata
            .get("snapshot_path")
            .and_then(|v| v.as_str()),
        Some("/tmp/ferrum-fs-snapshots/exec-123/path-hash")
    );
}

fn make_test_approval(
    approval_id: ApprovalId,
    proposal_id: ProposalId,
    state: ApprovalState,
) -> ApprovalRequest {
    ApprovalRequest {
        approval_id,
        intent_id: IntentId::new(),
        proposal_id,
        execution_id: None,
        requested_by: ActorRef {
            actor_type: ActorType::User,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Actor".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: "test-digest".to_string(),
        expires_at: ts_offset(3600),
        state,
        created_at: ts_offset(0),
    }
}

#[tokio::test]
async fn postgres_approval_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let proposal_id = ProposalId::new();
    let approval_id = ApprovalId::new();
    let approval = make_test_approval(approval_id, proposal_id, ApprovalState::Pending);

    repo.insert(&approval).await.unwrap();
    let fetched = repo.get(approval_id).await.unwrap();

    assert!(fetched.is_some(), "expected approval to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.approval_id, approval_id);
    assert_eq!(fetched.proposal_id, proposal_id);
    assert_eq!(fetched.action_digest, "test-digest");
    assert!(matches!(fetched.state, ApprovalState::Pending));
}

#[tokio::test]
async fn postgres_approval_update_changes_fields() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let proposal_id = ProposalId::new();
    let approval_id = ApprovalId::new();
    let mut approval = make_test_approval(approval_id, proposal_id, ApprovalState::Pending);

    repo.insert(&approval).await.unwrap();

    approval.execution_id = Some(ExecutionId::new());
    approval.action_digest = "updated-digest".to_string();
    repo.update(&approval).await.unwrap();

    let fetched = repo
        .get(approval_id)
        .await
        .unwrap()
        .expect("approval should exist");
    assert_eq!(fetched.action_digest, "updated-digest");
    assert!(fetched.execution_id.is_some());
}

#[tokio::test]
async fn postgres_approval_resolve_valid_transition() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let proposal_id = ProposalId::new();
    let approval_id = ApprovalId::new();
    let approval = make_test_approval(approval_id, proposal_id, ApprovalState::Pending);

    repo.insert(&approval).await.unwrap();
    repo.resolve(approval_id, ApprovalState::Granted)
        .await
        .unwrap();

    let fetched = repo
        .get(approval_id)
        .await
        .unwrap()
        .expect("approval should exist");
    assert!(
        matches!(fetched.state, ApprovalState::Granted),
        "state should be Granted"
    );
}

#[tokio::test]
async fn postgres_approval_resolve_invalid_transition() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let proposal_id = ProposalId::new();
    let approval_id = ApprovalId::new();
    let approval = make_test_approval(approval_id, proposal_id, ApprovalState::Granted);

    repo.insert(&approval).await.unwrap();
    let err = repo
        .resolve(approval_id, ApprovalState::Pending)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ferrum_store::StoreError::InvalidState(_)),
        "expected InvalidState error for transition out of terminal state, got: {}",
        err
    );
}

#[tokio::test]
async fn postgres_approval_list_pending() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let p1 = ProposalId::new();
    let p2 = ProposalId::new();
    let a1 = ApprovalId::new();
    let a2 = ApprovalId::new();
    let a3 = ApprovalId::new();

    repo.insert(&make_test_approval(a1, p1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a2, p2, ApprovalState::Pending))
        .await
        .unwrap();
    repo.insert(&make_test_approval(a3, p1, ApprovalState::Granted))
        .await
        .unwrap();

    let pending = repo.list_pending().await.unwrap();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].approval_id, a2);
    assert_eq!(pending[1].approval_id, a1);
}

#[tokio::test]
async fn postgres_approval_list_pending_paginated() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let p1 = ProposalId::new();
    let a1 = ApprovalId::new();
    let a2 = ApprovalId::new();
    let a3 = ApprovalId::new();

    repo.insert(&make_test_approval(a1, p1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a2, p1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a3, p1, ApprovalState::Pending))
        .await
        .unwrap();

    let page1 = repo.list_pending_paginated(2, 0).await.unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].approval_id, a3);
    assert_eq!(page1[1].approval_id, a2);

    let page2 = repo.list_pending_paginated(2, 2).await.unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].approval_id, a1);
}

#[tokio::test]
async fn postgres_approval_list_pending_by_proposal_paginated() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let proposal1 = ProposalId::new();
    let proposal2 = ProposalId::new();
    let a1 = ApprovalId::new();
    let a2 = ApprovalId::new();
    let a3 = ApprovalId::new();

    repo.insert(&make_test_approval(a1, proposal1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a2, proposal1, ApprovalState::Pending))
        .await
        .unwrap();
    repo.insert(&make_test_approval(a3, proposal2, ApprovalState::Pending))
        .await
        .unwrap();

    let for_proposal1 = repo
        .list_pending_by_proposal_paginated(proposal1, 10, 0)
        .await
        .unwrap();
    assert_eq!(for_proposal1.len(), 2);
    assert_eq!(for_proposal1[0].approval_id, a2);
    assert_eq!(for_proposal1[1].approval_id, a1);

    let for_proposal2 = repo
        .list_pending_by_proposal_paginated(proposal2, 10, 0)
        .await
        .unwrap();
    assert_eq!(for_proposal2.len(), 1);
    assert_eq!(for_proposal2[0].approval_id, a3);
}

#[tokio::test]
async fn postgres_approval_list_pending_cursor() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let p1 = ProposalId::new();
    let a1 = ApprovalId::new();
    let a2 = ApprovalId::new();
    let a3 = ApprovalId::new();

    repo.insert(&make_test_approval(a1, p1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a2, p1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a3, p1, ApprovalState::Pending))
        .await
        .unwrap();

    // Use the newest item (a3) as cursor; should return a2 and a1
    let cursor_approval = repo.get(a3).await.unwrap().unwrap();
    let page = repo
        .list_pending_cursor(cursor_approval.created_at, a3, 10)
        .await
        .unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].approval_id, a2);
    assert_eq!(page[1].approval_id, a1);

    // Use a2 as cursor; should return only a1
    let cursor_approval = repo.get(a2).await.unwrap().unwrap();
    let page = repo
        .list_pending_cursor(cursor_approval.created_at, a2, 10)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].approval_id, a1);
}

#[tokio::test]
async fn postgres_approval_list_pending_by_proposal_cursor() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.approvals();
    let proposal1 = ProposalId::new();
    let proposal2 = ProposalId::new();
    let a1 = ApprovalId::new();
    let a2 = ApprovalId::new();
    let a3 = ApprovalId::new();

    repo.insert(&make_test_approval(a1, proposal1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a2, proposal1, ApprovalState::Pending))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    repo.insert(&make_test_approval(a3, proposal2, ApprovalState::Pending))
        .await
        .unwrap();

    // Cursor on proposal1 using a2; should return only a1 (filtered by proposal)
    let cursor_approval = repo.get(a2).await.unwrap().unwrap();
    let page = repo
        .list_pending_by_proposal_cursor(proposal1, cursor_approval.created_at, a2, 10)
        .await
        .unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].approval_id, a1);

    // Cursor on proposal2 using a3; should return nothing
    let cursor_approval = repo.get(a3).await.unwrap().unwrap();
    let page = repo
        .list_pending_by_proposal_cursor(proposal2, cursor_approval.created_at, a3, 10)
        .await
        .unwrap();
    assert!(page.is_empty());
}

fn make_test_provenance_event(
    event_id: EventId,
    kind: ProvenanceEventKind,
    occurred_at: Timestamp,
    intent_id: Option<IntentId>,
) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id,
        kind,
        occurred_at,
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Actor".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Intent,
            object_id: "test-object".to_string(),
            summary: None,
        },
        intent_id,
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
        metadata: JsonMap::new(),
        source_runtime_id: None,
    }
}

#[tokio::test]
async fn postgres_provenance_append_and_get_event_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.provenance();
    let event_id = EventId::new();
    let event = make_test_provenance_event(
        event_id,
        ProvenanceEventKind::IntentCompiled,
        ts_offset(0),
        None,
    );

    repo.append_event(&event).await.unwrap();
    let fetched = repo.get_event(event_id).await.unwrap();

    assert!(fetched.is_some(), "expected event to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.event_id, event_id);
    assert!(matches!(fetched.kind, ProvenanceEventKind::IntentCompiled));
}

#[tokio::test]
async fn postgres_provenance_append_edges_and_get_edges_to() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.provenance();
    let parent_id = EventId::new();
    let child_id = EventId::new();

    let parent = make_test_provenance_event(
        parent_id,
        ProvenanceEventKind::UserGoalReceived,
        ts_offset(0),
        None,
    );
    let child = make_test_provenance_event(
        child_id,
        ProvenanceEventKind::IntentCompiled,
        ts_offset(1),
        None,
    );

    repo.append_event(&parent).await.unwrap();
    repo.append_event(&child).await.unwrap();

    let edges = vec![ProvenanceEdge {
        edge_type: ProvenanceEdgeType::DerivedFrom,
        from_event_id: parent_id,
        to_event_id: Some(child_id),
        summary: Some("derived".to_string()),
    }];
    repo.append_edges(child_id, &edges).await.unwrap();

    let edges_to_child = repo.get_edges_to(child_id).await.unwrap();
    assert_eq!(edges_to_child.len(), 1);
    assert_eq!(edges_to_child[0].from_event_id, parent_id);
    assert!(matches!(
        edges_to_child[0].edge_type,
        ProvenanceEdgeType::DerivedFrom
    ));
    assert_eq!(edges_to_child[0].to_event_id, Some(child_id));
}

#[tokio::test]
async fn postgres_provenance_rejects_orphan_edges() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.provenance();
    let result = repo
        .append_edges(
            EventId::new(),
            &[ProvenanceEdge {
                edge_type: ProvenanceEdgeType::DerivedFrom,
                from_event_id: EventId::new(),
                to_event_id: None,
                summary: Some("orphan".to_string()),
            }],
        )
        .await;

    assert!(result.is_err(), "orphan provenance edge must be rejected");
}

#[tokio::test]
async fn postgres_append_event_with_edges_rolls_back_event_when_edge_insert_fails() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.provenance();
    let child_id = EventId::new();
    let child = make_test_provenance_event(
        child_id,
        ProvenanceEventKind::ActionProposalSubmitted,
        ts_offset(1),
        None,
    );

    let result = repo
        .append_event_with_edges(
            &child,
            &[ProvenanceEdge {
                edge_type: ProvenanceEdgeType::DerivedFrom,
                from_event_id: EventId::new(),
                to_event_id: Some(child_id),
                summary: Some("missing parent".to_string()),
            }],
        )
        .await;

    assert!(result.is_err(), "invalid edge must fail the atomic append");
    assert!(
        repo.get_event(child_id).await.unwrap().is_none(),
        "event insert must roll back when edge insert fails"
    );
}

#[tokio::test]
async fn postgres_provenance_get_edges_from() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.provenance();
    let parent_id = EventId::new();
    let child1_id = EventId::new();
    let child2_id = EventId::new();

    for (id, kind) in [
        (parent_id, ProvenanceEventKind::UserGoalReceived),
        (child1_id, ProvenanceEventKind::IntentCompiled),
        (child2_id, ProvenanceEventKind::ActionProposalSubmitted),
    ] {
        repo.append_event(&make_test_provenance_event(id, kind, ts_offset(0), None))
            .await
            .unwrap();
    }

    repo.append_edges(
        child1_id,
        &[ProvenanceEdge {
            edge_type: ProvenanceEdgeType::DerivedFrom,
            from_event_id: parent_id,
            to_event_id: Some(child1_id),
            summary: None,
        }],
    )
    .await
    .unwrap();

    repo.append_edges(
        child2_id,
        &[ProvenanceEdge {
            edge_type: ProvenanceEdgeType::Caused,
            from_event_id: parent_id,
            to_event_id: Some(child2_id),
            summary: None,
        }],
    )
    .await
    .unwrap();

    let edges = repo.get_edges_from(&[parent_id]).await.unwrap();
    assert_eq!(edges.len(), 2);
    let to_ids: Vec<_> = edges.iter().map(|e| e.to_event_id).collect();
    assert!(to_ids.contains(&Some(child1_id)));
    assert!(to_ids.contains(&Some(child2_id)));
}

#[tokio::test]
async fn postgres_provenance_query_filters() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.provenance();
    let intent1 = IntentId::new();
    let intent2 = IntentId::new();
    let e1 = EventId::new();
    let e2 = EventId::new();
    let e3 = EventId::new();

    repo.append_event(&make_test_provenance_event(
        e1,
        ProvenanceEventKind::UserGoalReceived,
        ts_offset(0),
        Some(intent1),
    ))
    .await
    .unwrap();
    repo.append_event(&make_test_provenance_event(
        e2,
        ProvenanceEventKind::IntentCompiled,
        ts_offset(10),
        Some(intent1),
    ))
    .await
    .unwrap();
    repo.append_event(&make_test_provenance_event(
        e3,
        ProvenanceEventKind::UserGoalReceived,
        ts_offset(20),
        Some(intent2),
    ))
    .await
    .unwrap();

    // Filter by intent_id
    let by_intent1 = repo
        .query(&ProvenanceQueryRequest {
            intent_id: Some(intent1),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
            edge_types: vec![],
        })
        .await
        .unwrap();
    assert_eq!(by_intent1.len(), 2);

    // Filter by kind
    let by_kind = repo
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::IntentCompiled),
            since: None,
            until: None,
            edge_types: vec![],
        })
        .await
        .unwrap();
    assert_eq!(by_kind.len(), 1);
    assert_eq!(by_kind[0].event_id, e2);

    // Filter by since
    let since = repo
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: Some(ts_offset(15)),
            until: None,
            edge_types: vec![],
        })
        .await
        .unwrap();
    assert_eq!(since.len(), 1);
    assert_eq!(since[0].event_id, e3);

    // No filters returns all in occurred_at ASC order
    let all = repo
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
            edge_types: vec![],
        })
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].event_id, e1);
    assert_eq!(all[1].event_id, e2);
    assert_eq!(all[2].event_id, e3);
}

fn make_test_ledger_entry(
    entry_id: i64,
    event_id: EventId,
    content_hash: Option<String>,
    previous_ledger_hash: Option<String>,
) -> LedgerEntry {
    LedgerEntry {
        entry_id,
        event_id,
        intent_id: None,
        execution_id: None,
        occurred_at: ts_offset(0),
        content_hash,
        previous_ledger_hash,
        raw_json: serde_json::json!({}),
    }
}

async fn seed_provenance_events_for_ledger(store: &PostgresStore, event_ids: &[EventId]) {
    let repo = store.provenance();
    for (offset, event_id) in event_ids.iter().enumerate() {
        let event = make_test_provenance_event(
            *event_id,
            ProvenanceEventKind::SideEffectVerified,
            ts_offset(offset as i64),
            None,
        );
        repo.append_event(&event).await.unwrap();
    }
}

#[tokio::test]
async fn postgres_ledger_append_and_get_by_event() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.ledger();
    let event_id = EventId::new();
    let entry = make_test_ledger_entry(0, event_id, Some("hash-1".to_string()), None);

    seed_provenance_events_for_ledger(&store, &[event_id]).await;
    repo.append(&entry).await.unwrap();
    let fetched = repo.get_by_event(event_id).await.unwrap();

    assert!(fetched.is_some(), "expected entry to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.event_id, event_id);
    assert_eq!(fetched.content_hash, Some("hash-1".to_string()));
}

#[tokio::test]
async fn postgres_ledger_list_recent_and_get_latest() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.ledger();
    let e1 = EventId::new();
    let e2 = EventId::new();
    let e3 = EventId::new();

    seed_provenance_events_for_ledger(&store, &[e1, e2, e3]).await;
    repo.append(&make_test_ledger_entry(0, e1, Some("h1".to_string()), None))
        .await
        .unwrap();
    repo.append(&make_test_ledger_entry(
        0,
        e2,
        Some("h2".to_string()),
        Some("h1".to_string()),
    ))
    .await
    .unwrap();
    repo.append(&make_test_ledger_entry(
        0,
        e3,
        Some("h3".to_string()),
        Some("h2".to_string()),
    ))
    .await
    .unwrap();

    let recent = repo.list_recent(2).await.unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].event_id, e3);
    assert_eq!(recent[1].event_id, e2);

    let latest = repo.get_latest().await.unwrap();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().event_id, e3);
}

#[tokio::test]
async fn postgres_ledger_verify_chain_valid() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.ledger();
    let e1 = EventId::new();
    let e2 = EventId::new();

    seed_provenance_events_for_ledger(&store, &[e1, e2]).await;
    repo.append(&make_test_ledger_entry(
        0,
        e1,
        Some("hash-1".to_string()),
        None,
    ))
    .await
    .unwrap();
    repo.append(&make_test_ledger_entry(
        0,
        e2,
        Some("hash-2".to_string()),
        Some("hash-1".to_string()),
    ))
    .await
    .unwrap();

    repo.verify_chain().await.unwrap();
}

#[tokio::test]
async fn postgres_ledger_verify_chain_broken() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.ledger();
    let e1 = EventId::new();
    let e2 = EventId::new();

    seed_provenance_events_for_ledger(&store, &[e1, e2]).await;
    repo.append(&make_test_ledger_entry(
        0,
        e1,
        Some("hash-1".to_string()),
        None,
    ))
    .await
    .unwrap();
    repo.append(&make_test_ledger_entry(
        0,
        e2,
        Some("hash-2".to_string()),
        Some("wrong-hash".to_string()),
    ))
    .await
    .unwrap();

    let err = repo.verify_chain().await.unwrap_err();
    assert!(
        matches!(err, ferrum_store::StoreError::InvalidState(_)),
        "expected InvalidState for broken chain, got: {}",
        err
    );
    let msg = err.to_string();
    assert!(
        msg.contains("broken chain"),
        "error should mention broken chain: {}",
        msg
    );
}

fn make_test_policy_bundle(bundle_id: &str, active: bool) -> PolicyBundle {
    PolicyBundle {
        bundle_id: bundle_id.to_string(),
        version: "0.1.0".to_string(),
        rules: vec![ferrum_proto::PolicyRule {
            id: "rule-1".to_string(),
            description: "test rule".to_string(),
            decision: Decision::Allow,
            priority: 1,
            matchers: vec![],
        }],
        active,
        content_hash: Some(format!("hash-{}", bundle_id)),
        created_at: ts_offset(0),
        updated_at: ts_offset(0),
    }
}

#[tokio::test]
async fn postgres_policy_bundle_insert_and_get_roundtrip() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.policy_bundles();
    let bundle = make_test_policy_bundle("bundle-1", false);

    repo.insert(&bundle).await.unwrap();
    let fetched = repo.get("bundle-1").await.unwrap();

    assert!(fetched.is_some(), "expected bundle to be found");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.bundle_id, "bundle-1");
    assert_eq!(fetched.version, "0.1.0");
    assert!(!fetched.active);
}

#[tokio::test]
async fn postgres_policy_bundle_get_by_content_hash() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.policy_bundles();
    let bundle = make_test_policy_bundle("bundle-hash", false);

    repo.insert(&bundle).await.unwrap();
    let fetched = repo.get_by_content_hash("hash-bundle-hash").await.unwrap();

    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().bundle_id, "bundle-hash");
}

#[tokio::test]
async fn postgres_policy_bundle_update_and_delete() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.policy_bundles();
    let mut bundle = make_test_policy_bundle("bundle-update", false);

    repo.insert(&bundle).await.unwrap();

    bundle.version = "0.2.0".to_string();
    repo.update(&bundle).await.unwrap();

    let fetched = repo.get("bundle-update").await.unwrap().unwrap();
    assert_eq!(fetched.version, "0.2.0");

    repo.delete("bundle-update").await.unwrap();
    let deleted = repo.get("bundle-update").await.unwrap();
    assert!(deleted.is_none());
}

#[tokio::test]
async fn postgres_policy_bundle_list_and_active() {
    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres live test: database not reachable");
            return;
        }
    };

    let repo = store.policy_bundles();
    let b1 = make_test_policy_bundle("bundle-a", false);
    let mut b2 = make_test_policy_bundle("bundle-b", false);
    b2.created_at = ts_offset(10);
    b2.updated_at = ts_offset(10);

    repo.insert(&b1).await.unwrap();
    repo.insert(&b2).await.unwrap();

    let all = repo.list().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].bundle_id, "bundle-b");
    assert_eq!(all[1].bundle_id, "bundle-a");

    let active = repo.list_active().await.unwrap();
    assert!(active.is_empty());

    repo.set_active("bundle-b", true).await.unwrap();

    let active = repo.list_active().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].bundle_id, "bundle-b");
    assert!(active[0].active);
}
