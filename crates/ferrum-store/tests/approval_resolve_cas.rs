//! Compare-and-swap tests for approval resolution.
//!
//! Verifies that `ApprovalRepo::resolve` atomically transitions a `Pending`,
//! unexpired approval exactly once: concurrent (or equivalent) resolvers cannot
//! both win, an expired approval cannot be resolved, and the SQLite write-queue
//! path honors the same contract.

use ferrum_proto::{
    ActorRef, ActorType, ApprovalId, ApprovalRequest, ApprovalState, IntentId, PrincipalId,
    ProposalId,
};
use ferrum_store::{ApprovalRepo, IntentRepo, ProposalRepo, SqliteStore, StoreFacade};
use ferrum_testkit::ApprovalFixture;

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
        owner_actor_id: None,
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
        owner_actor_id: None,
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
    ApprovalFixture::new()
        .with_id(approval_id)
        .with_intent_id(intent_id)
        .with_proposal_id(proposal_id)
        .with_state(state)
        .with_created_at(created_at)
        .with_expires_at(expires_at)
        .with_requested_by(ActorRef {
            actor_type: ActorType::User,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Actor".to_string()),
        })
        .build()
}

/// Insert a minimal intent + proposal so an approval can be inserted, and
/// return their ids.
async fn seed_intent_and_proposal(store: &SqliteStore) -> (IntentId, ProposalId) {
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
    (intent_id, proposal_id)
}

#[tokio::test]
async fn resolve_returns_false_when_approval_expired() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let (intent_id, proposal_id) = seed_intent_and_proposal(&store).await;

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    // Still Pending, but already expired by expires_at.
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now - chrono::Duration::hours(1),
        now - chrono::Duration::minutes(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    let result = store
        .approvals()
        .resolve(approval_id, ApprovalState::Granted, now)
        .await;
    assert!(
        matches!(result, Ok(false)),
        "expired approval must lose the CAS (Ok(false)), got: {:?}",
        result
    );

    // State must be unchanged and raw_json must remain consistent with it.
    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(
        matches!(fetched.state, ApprovalState::Pending),
        "expired approval must remain Pending, got: {:?}",
        fetched.state
    );
}

#[tokio::test]
async fn resolve_first_wins_second_is_blocked() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let (intent_id, proposal_id) = seed_intent_and_proposal(&store).await;

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now,
        now + chrono::Duration::hours(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    // First resolver wins the CAS.
    let first = store
        .approvals()
        .resolve(approval_id, ApprovalState::Granted, now)
        .await;
    assert!(
        matches!(first, Ok(true)),
        "first resolve must win: {:?}",
        first
    );

    // A subsequent resolver observes the terminal state and loses the race with
    // Ok(false) (never Err): the opposing Denied decision must not overwrite the
    // Granted winner. This is what lets the gateway map a lost CAS to 409.
    let second = store
        .approvals()
        .resolve(approval_id, ApprovalState::Denied, now)
        .await;
    assert!(
        matches!(second, Ok(false)),
        "second resolve must lose with Ok(false) after a terminal transition, got: {:?}",
        second
    );

    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Granted));
}

#[tokio::test]
async fn resolve_write_queue_only_one_wins() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let (intent_id, proposal_id) = seed_intent_and_proposal(&store).await;

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now,
        now + chrono::Duration::hours(1),
    );

    // Route through the write queue via the StoreFacade trait object.
    let facade: std::sync::Arc<dyn StoreFacade> = std::sync::Arc::new(store.clone());
    facade.approvals().insert(&approval).await.unwrap();

    let first = facade
        .approvals()
        .resolve(approval_id, ApprovalState::Granted, now)
        .await;
    assert!(
        matches!(first, Ok(true)),
        "queued first resolve must win: {:?}",
        first
    );

    let second = facade
        .approvals()
        .resolve(approval_id, ApprovalState::Denied, now)
        .await;
    assert!(
        matches!(second, Ok(false)),
        "queued second resolve must lose with Ok(false) after terminal transition, got: {:?}",
        second
    );

    let fetched = facade.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(matches!(fetched.state, ApprovalState::Granted));
}

#[tokio::test]
async fn resolve_concurrent_resolvers_exactly_one_wins() {
    // Single shared connection so all concurrent resolvers observe the same
    // in-memory database.
    let store = SqliteStore::connect_with_pool_size("sqlite::memory:", 1)
        .await
        .unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let (intent_id, proposal_id) = seed_intent_and_proposal(&store).await;

    let approval_id = ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = make_approval(
        approval_id,
        intent_id,
        proposal_id,
        ApprovalState::Pending,
        now,
        now + chrono::Duration::hours(1),
    );
    store.approvals().insert(&approval).await.unwrap();

    // Mix opposing decisions: every resolver targets a valid terminal decision,
    // but only one can win the CAS. Every loser must observe Ok(false) (never
    // Err), which is what lets the gateway map a lost race to 409 rather than 500.
    let targets = [
        ApprovalState::Granted,
        ApprovalState::Denied,
        ApprovalState::Granted,
        ApprovalState::Denied,
    ];
    let store = std::sync::Arc::new(store);
    let mut handles = Vec::new();
    for target in targets {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store.approvals().resolve(approval_id, target, now).await
        }));
    }

    let mut winners = 0;
    let mut losers = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Ok(true) => winners += 1,
            Ok(false) => losers += 1,
            Err(e) => panic!("concurrent resolver must not error on a lost CAS, got: {e:?}"),
        }
    }
    assert_eq!(
        winners, 1,
        "exactly one concurrent resolver must win the CAS, got {}",
        winners
    );
    assert_eq!(
        losers, 3,
        "every other concurrent resolver must lose with Ok(false), got {}",
        losers
    );

    // The single winner decided the outcome; the row must end in whichever
    // terminal decision won, never Pending.
    let fetched = store.approvals().get(approval_id).await.unwrap().unwrap();
    assert!(
        matches!(
            fetched.state,
            ApprovalState::Granted | ApprovalState::Denied
        ),
        "approval must end in the single winning terminal decision, got: {:?}",
        fetched.state
    );
}
