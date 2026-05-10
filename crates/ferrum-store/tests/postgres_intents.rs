#![cfg(feature = "postgres")]

//! Live DB-backed tests for PostgresIntentRepo.
//!
//! These tests run against a local Docker PostgreSQL when available and skip
//! gracefully when the database is unreachable. The DSN is taken from
//! `docker-compose.postgres.yml`.

use chrono::Utc;
use ferrum_proto::{
    CapabilityId, EffectType, ExecutionId, IntentEnvelope, IntentId, IntentStatus, JsonMap,
    OutcomeClause, PrincipalId, ProposalId, RiskTier, Timestamp,
};
use ferrum_store::{IntentRepo, postgres::PostgresStore};

const TEST_DSN: &str =
    "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test";

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
async fn setup() -> Option<PostgresStore> {
    let store = match PostgresStore::connect(TEST_DSN).await {
        Ok(s) => s,
        Err(_) => return None,
    };

    // Clean slate for each test
    let _ = sqlx::query("DROP TABLE IF EXISTS executions CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS intents CASCADE")
        .execute(store.pool())
        .await;

    if let Err(e) = store.apply_intent_migration().await {
        eprintln!("apply_intent_migration failed: {}", e);
        return None;
    }

    Some(store)
}

#[tokio::test]
async fn postgres_insert_and_get_roundtrip() {
    let store = match setup().await {
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
    let store = match setup().await {
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
    let store = match setup().await {
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
    let store = match setup().await {
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
    let store = match setup().await {
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
    let store = match setup().await {
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
    let store = match setup().await {
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
    let store = match setup().await {
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
