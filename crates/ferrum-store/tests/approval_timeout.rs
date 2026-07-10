//! Tests for approval timeout reconciliation.

use ferrum_proto::{
    ActorRef, ActorType, ApprovalId, ApprovalRequest, ApprovalState, IntentId, PrincipalId,
    ProposalId,
};
use ferrum_store::{ApprovalRepo, IntentRepo, ProposalRepo, SqliteStore, StoreFacade};

fn make_test_intent(intent_id: IntentId) -> ferrum_proto::IntentEnvelope {
    ferrum_proto::IntentEnvelope {
        intent_id,
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
    }
}

fn make_test_proposal(
    intent_id: IntentId,
    proposal_id: ProposalId,
) -> ferrum_proto::ActionProposal {
    ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test-effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Low,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    }
}

fn make_approval(
    approval_id: ApprovalId,
    intent_id: IntentId,
    proposal_id: ProposalId,
    state: ApprovalState,
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
) -> ApprovalRequest {
    ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ActorRef {
            actor_type: ActorType::User,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Actor".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: "test-digest".to_string(),
        expires_at,
        state,
        created_at,
    }
}

#[tokio::test]
async fn expire_stale_pending_expires_past_expires_at() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::hours(2),
        now - chrono::Duration::minutes(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();

    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].approval_id, approval_id);
    assert!(matches!(expired[0].state, ApprovalState::Expired));

    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Expired));
}

#[tokio::test]
async fn expire_stale_pending_expires_when_older_than_max_age() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    // expires_at is in the future, but created_at is older than max_age
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::hours(2),
        now + chrono::Duration::hours(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();

    assert_eq!(expired.len(), 1);
    assert!(matches!(expired[0].state, ApprovalState::Expired));
}

#[tokio::test]
async fn expire_stale_pending_leaves_fresh_pending_approvals() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::minutes(30),
        now + chrono::Duration::hours(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();

    assert!(expired.is_empty());

    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Pending));
}

#[tokio::test]
async fn expire_stale_pending_leaves_terminal_approvals() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Granted,
        now - chrono::Duration::hours(2),
        now - chrono::Duration::minutes(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();

    assert!(expired.is_empty());

    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Granted));
}

#[tokio::test]
async fn expire_stale_pending_respects_batch_size() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let now = chrono::Utc::now();
    let mut ids = Vec::new();
    for i in 0..5 {
        let approval_id = ApprovalId::new();
        ids.push(approval_id);
        let approval = make_approval(
            approval_id,
            intent_id,
            proposal_id,
            ApprovalState::Pending,
            now - chrono::Duration::hours(2) - chrono::Duration::seconds(i),
            now - chrono::Duration::minutes(1),
        );
        store.approvals().insert(&approval).await.unwrap();
    }

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 2)
        .await
        .unwrap();

    assert_eq!(expired.len(), 2);
}

#[tokio::test]
async fn expire_stale_pending_does_not_clobber_terminal_approval() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    // Old by created_at (so the reconciler considers it a candidate) but not yet
    // expired by expires_at, so an operator can still resolve it under the CAS.
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::hours(2),
        now + chrono::Duration::hours(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    // Simulate an operator resolving the approval before the reconciler runs.
    let won = store
        .approvals()
        .resolve(approval_id, ApprovalState::Granted, now)
        .await
        .unwrap();
    assert!(won, "operator resolve should win the CAS");

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();

    assert!(expired.is_empty());

    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Granted));
}

#[tokio::test]
async fn expire_stale_pending_is_idempotent() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::hours(2),
        now - chrono::Duration::minutes(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    let expired = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();
    assert_eq!(expired.len(), 1);

    let second = store
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();
    assert!(second.is_empty());
}

#[tokio::test]
async fn expire_stale_pending_via_write_queue_skips_resolved() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_test_proposal(intent_id, proposal_id))
        .await
        .unwrap();

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    // Old by created_at (reconciler candidate) but not yet expired by expires_at
    // so the queued resolve can still win the CAS.
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::hours(2),
        now + chrono::Duration::hours(1),
    );

    // Use the StoreFacade trait object so the approval repo routes writes
    // through the SQLite write queue.
    let facade: std::sync::Arc<dyn StoreFacade> = std::sync::Arc::new(store.clone());
    facade.approvals().insert(&approval).await.unwrap();

    let won = facade
        .approvals()
        .resolve(approval_id, ApprovalState::Granted, now)
        .await
        .unwrap();
    assert!(won, "queued resolve should win the CAS");

    let expired = facade
        .approvals()
        .expire_stale_pending(now, 3600, 100)
        .await
        .unwrap();

    assert!(expired.is_empty());

    let fetched = facade.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Granted));
}
