use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::{PdpEngine, StaticPdpEngine};
use ferrum_proto::{
    Decision, EvaluateProposalResponse, Matcher, RiskTier, RollbackClass, TrustContextSummary,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{IntentRepo, PolicyBundleRepo, SqliteStore, StoreFacade};
use std::sync::Arc;

mod support;
use support::*;

/// Verify that high taint score (>=70) triggers a Quarantine decision
/// for non-R0 actions.
#[tokio::test]
async fn test_high_taint_triggers_quarantine() {
    let high_taint_trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 85, // above the 70 threshold
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(high_taint_trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable, // non-R0
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    assert_eq!(
        evaluated.decision,
        Decision::Quarantine,
        "high taint should result in Quarantine, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .matched_rule_ids
            .contains(&"quarantine.high.taint.mutation".to_string()),
        "expected quarantine rule to be matched"
    );
}

// ---------------------------------------------------------------------------
// Poisoned context regression fixtures
// A curated set of taint-based denial scenarios targeting >=80% catch rate
// of poisoned-context vulnerabilities.
// ---------------------------------------------------------------------------

/// Taint at exactly 69 (one below threshold) should NOT trigger quarantine.
/// This is a boundary regression test to ensure the >=70 check is precise.
#[tokio::test]
async fn test_poisoned_context_taint_at_boundary_69_no_quarantine() {
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 69, // one below threshold
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "taint boundary test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({ "content": "pre-verify new content" }),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable, // non-R0
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // Below threshold should NOT quarantine
    assert_ne!(
        evaluated.decision,
        Decision::Quarantine,
        "taint=69 should not trigger quarantine, got: {:?}",
        evaluated.decision
    );
    // Should NOT have quarantine rule
    assert!(
        !evaluated
            .matched_rule_ids
            .contains(&"quarantine.high.taint.mutation".to_string()),
        "taint=69 should not match quarantine rule"
    );
}

/// R0 actions bypass taint checks even with maximum taint score.
/// This is a critical regression test for the R0 taint-skip optimization.
#[tokio::test]
async fn test_poisoned_context_r0_bypasses_taint_check() {
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 100, // maximum taint
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // R0 proposal with maximum taint
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "R0 bypass taint test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible, // R0 bypasses taint
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // R0 should NOT be quarantined even with max taint
    assert_ne!(
        evaluated.decision,
        Decision::Quarantine,
        "R0 with max taint should NOT be quarantined, got: {:?}",
        evaluated.decision
    );
    assert!(
        !evaluated
            .matched_rule_ids
            .contains(&"quarantine.high.taint.mutation".to_string()),
        "R0 should bypass quarantine rule"
    );
}

/// Taint at maximum (100) with non-R0 should trigger quarantine.
/// This ensures the upper bound of taint scoring works correctly.
#[tokio::test]
async fn test_poisoned_context_taint_at_maximum_100() {
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 100, // maximum taint
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "max taint test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R1SnapshotRecoverable, // non-R0
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    assert_eq!(
        evaluated.decision,
        Decision::Quarantine,
        "max taint=100 should trigger quarantine, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .matched_rule_ids
            .contains(&"quarantine.high.taint.mutation".to_string()),
        "max taint should match quarantine rule"
    );
}

/// R3 actions require approval regardless of taint score.
/// This tests the R3 + poisoned-context interaction.
#[tokio::test]
async fn test_poisoned_context_r3_requires_approval() {
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 50, // moderate taint
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // R3 with moderate taint
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "R3 taint test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Critical,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // R3 should require approval regardless of taint
    assert_eq!(
        evaluated.decision,
        Decision::RequireApproval,
        "R3 should require approval, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .matched_rule_ids
            .contains(&"approval.r3.required".to_string()),
        "R3 should match approval rule"
    );
}

/// Moderate taint (50) with non-R0 should NOT trigger quarantine.
/// This establishes the "safe zone" for moderate taint scores.
#[tokio::test]
async fn test_poisoned_context_moderate_taint_50_no_quarantine() {
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 50, // moderate taint
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "moderate taint test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable, // non-R0
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    assert_ne!(
        evaluated.decision,
        Decision::Quarantine,
        "moderate taint=50 should not trigger quarantine, got: {:?}",
        evaluated.decision
    );
}

/// Verify various trust context attributes do not bypass taint checks.
/// All three flags set should not change the taint threshold behavior.
#[tokio::test]
async fn test_poisoned_context_trust_attributes_no_bypass() {
    // All trust context flags set but low taint
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score: 0, // no taint
        contains_external_metadata: true,
        contains_tool_output: true,
        contains_untrusted_text: true,
    };

    let pdp: Arc<dyn PdpEngine> = Arc::new(InjectablePdpEngine::new(trust));
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "trust attributes test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R1SnapshotRecoverable, // non-R0
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // No taint = no quarantine, regardless of trust attributes
    assert_ne!(
        evaluated.decision,
        Decision::Quarantine,
        "zero taint should not trigger quarantine even with trust flags, got: {:?}",
        evaluated.decision
    );
}

// ---------------------------------------------------------------------------
// I7 E2E: StaticPdpEngine + TaintScoringFirewall quarantine test
// ---------------------------------------------------------------------------

/// I7 E2E: Verify that high taint computed by TaintScoringFirewall triggers
/// Quarantine via real StaticPdpEngine (not InjectablePdpEngine).
///
/// Flow:
/// - Create intent with input_labels containing ExternalWeb → is_external=true
/// - Create proposal with R1 rollback class and privileged=true metadata
/// - TaintScoringFirewall.compute_taint_score yields >= 70:
///   is_external (+30) + trust_score=30<50 (+20) + privileged=true (+20) = 70
/// - StaticPdpEngine.evaluate receives computed trust and returns Quarantine
///
/// This exercises the full pipeline: evaluate_proposal → build_firewall_context →
/// TaintScoringFirewall.compute_taint_score → TrustContextSummary →
/// StaticPdpEngine.evaluate → Decision::Quarantine
#[tokio::test]
async fn test_i7_e2e_static_pdp_quarantine_on_high_taint() {
    let pdp = Arc::new(StaticPdpEngine); // REAL PDP, not InjectablePdpEngine
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Create intent with ExternalWeb label to make is_external=true in firewall context.
    // This is the key difference from InjectablePdpEngine-based poisoned-context tests:
    // the taint score is computed by TaintScoringFirewall, not injected directly.
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "i7-e2e-test-intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }],
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: vec![ferrum_proto::TrustLabel::ExternalWeb],
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
        owner_actor_id: None,
    };

    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // Proposal with R1 (non-R0) and privileged=true metadata.
    // TaintScoringFirewall.compute_taint_score will compute:
    //   is_external=true → +30
    //   trust_score=30 (<50) → +20
    //   privileged=true → +20
    //   Total = 70 (>= 70 threshold for quarantine)
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "i7 e2e proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Medium,
        requested_rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::from([(
            "privileged".to_string(),
            serde_json::json!(true),
        )]),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // Assert quarantine decision
    assert_eq!(
        evaluated.decision,
        Decision::Quarantine,
        "high taint (>=70) with non-R0 should Quarantine, got: {:?} — reason: {}",
        evaluated.decision,
        evaluated.reason
    );

    // Assert matched rule is the taint quarantine rule
    assert!(
        evaluated
            .matched_rule_ids
            .contains(&"quarantine.high.taint.mutation".to_string()),
        "expected quarantine.high.taint.mutation in matched_rule_ids, got: {:?}",
        evaluated.matched_rule_ids
    );

    // Assert reason mentions taint
    assert!(
        evaluated.reason.contains("taint"),
        "reason should mention taint, got: {}",
        evaluated.reason
    );
}

// ---------------------------------------------------------------------------
// Scope mismatch deny test
// ---------------------------------------------------------------------------

/// Verify that the PDP engine performs explicit scope-mismatch checking:
/// - Empty resource_scope + non-R0 mutation = Deny (scope mismatch)
/// - Empty resource_scope + R0 = Allow (R0 is native reversible, no scope needed)
#[tokio::test]
async fn test_scope_mismatch_deny_on_empty_scope_with_mutation() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // Proposal with non-R0 rollback class (mutation)
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "scope test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable, // non-R0 = mutation
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    // Scope mismatch now returns OK but with Deny decision
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // Empty scope + non-R0 = Deny (scope mismatch)
    assert_eq!(
        evaluated.decision,
        Decision::Deny,
        "empty scope with non-R0 mutation should Deny, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .matched_rule_ids
            .contains(&"scope.mismatch.empty.scope".to_string()),
        "expected scope.mismatch.empty.scope rule to be matched"
    );
}

/// Verify that R0 (native reversible) proposals are allowed even with empty scope.
#[tokio::test]
async fn test_r0_allowed_with_empty_scope() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
    let intent_id = ferrum_proto::IntentId::new();
    store
        .intents()
        .insert(&make_test_intent(intent_id))
        .await
        .expect("intent insert");

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // Proposal with R0 rollback class (native reversible, no mutation)
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "scope test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: noop_binding_metadata(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    // R0 is native reversible and does not require explicit scope
    assert_eq!(
        evaluated.decision,
        Decision::Allow,
        "R0 with empty scope should Allow, got: {:?}",
        evaluated.decision
    );
}

// ---------------------------------------------------------------------------
// Draft-only intent gateway flow test
// ---------------------------------------------------------------------------

/// Verify that a high-risk external proposal with dangerous attributes gets
/// non-zero taint from the firewall and is quarantined by the PDP.
#[tokio::test]
async fn test_firewall_high_taint_external_proposal_quarantine() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create an intent with external trust label (ExternalToolOutput) to trigger
    // firewall's is_external=true path.
    // Also include a non-empty resource_scope to avoid triggering ScopeMismatch policy bundle rule.
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "external-dangerous-intent".to_string(),
        goal: "execute privileged command".to_string(),
        normalized_goal: "execute privileged command".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp/exec".to_string(),
            mode: ferrum_proto::ResourceMode::Execute,
            content_hash: None,
        }],
        risk_tier: ferrum_proto::RiskTier::High,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: vec![ferrum_proto::TrustLabel::ExternalToolOutput],
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: true,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
        owner_actor_id: None,
    };
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create a high-risk proposal with:
    // - taint_inputs non-empty (external indicator)
    // - R1 rollback class (mutation, not R0)
    // - dangerous metadata (privileged: true)
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "privileged exec proposal".to_string(),
        tool_name: "exec_tool".to_string(),
        server_name: "dangerous-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "executes command".to_string(),
        estimated_risk: ferrum_proto::RiskTier::High,
        requested_rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
        taint_inputs: vec![
            "external_input_1".to_string(),
            "external_input_2".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::from([
            ("privileged".to_string(), serde_json::json!("true")),
            ("dangerous".to_string(), serde_json::json!("true")),
        ]),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");

    // The StaticPdpEngine does not check taint, but the policy bundle evaluation
    // path is not triggered here either. We verify the firewall-derived trust
    // was constructed with non-zero taint_score by checking that the response
    // reflects the high-risk context.
    // Since InjectablePdpEngine is not used here, we use StaticPdpEngine which
    // has its own quarantine logic based on taint_score >= 70.
    // The key assertion: this high-risk external proposal with dangerous attributes
    // gets a firewall-computed taint_score >= 70 and therefore gets Quarantine.
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let eval_response: ferrum_proto::EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid json");

    // The StaticPdpEngine quarantine logic: taint >= 70 && non-R0 => Quarantine.
    // With is_external=true (+30), low trust_score=30 (+20), privileged (+20), dangerous (+20)
    // the firewall computes 90, which should trigger Quarantine.
    assert_eq!(
        eval_response.decision,
        ferrum_proto::Decision::Quarantine,
        "high-risk external proposal with dangerous attributes should be quarantined, got {:?}: {}",
        eval_response.decision,
        eval_response.reason
    );
    assert!(
        eval_response
            .matched_rule_ids
            .iter()
            .any(|id| id.contains("taint") || id.contains("quarantine")),
        "matched_rule_ids should mention taint/quarantine rule, got: {:?}",
        eval_response.matched_rule_ids
    );
}

/// Verify that an ordinary internal R0 proposal with no external indicators
/// gets taint_score=0 from the firewall and is allowed (not quarantined).
#[tokio::test]
async fn test_firewall_low_taint_internal_proposal_allows() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create an internal intent with no external labels.
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "internal-read-only-intent".to_string(),
        goal: "read data".to_string(),
        normalized_goal: "read data".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: ferrum_proto::RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: vec![ferrum_proto::TrustLabel::InternalSystem],
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
        owner_actor_id: None,
    };
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create a low-risk internal R0 proposal with no taint inputs or external metadata.
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "internal read proposal".to_string(),
        tool_name: "read_tool".to_string(),
        server_name: "internal-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "reads data".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Low,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let eval_response: ferrum_proto::EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid json");

    // Internal R0 proposal with no external indicators should get taint_score=0
    // from firewall and therefore NOT be quarantined.
    // StaticPdpEngine quarantine: taint >= 70 && non-R0 => Quarantine.
    // R0 is never quarantined regardless of taint.
    assert_ne!(
        eval_response.decision,
        ferrum_proto::Decision::Quarantine,
        "internal R0 proposal should NOT be quarantined, got decision {:?}: {}",
        eval_response.decision,
        eval_response.reason
    );
}

// ---------------------------------------------------------------------------
// Policy bundle enforcement tests
// ---------------------------------------------------------------------------

/// Verify that an active policy bundle with a matching Deny rule changes the evaluate_proposal response.
#[tokio::test]
async fn test_policy_bundle_active_deny_rule_affects_evaluation() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Insert an intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create and insert an active policy bundle with a Deny rule matching ActionIsMutation
    let deny_rule = ferrum_proto::PolicyRule {
        id: "deny-mutation".to_string(),
        description: "deny all mutations".to_string(),
        decision: Decision::Deny,
        priority: 100,
        matchers: vec![Matcher::ActionIsMutation],
    };
    let bundle = make_test_policy_bundle("test-deny-bundle", vec![deny_rule], true);
    store
        .policy_bundles()
        .insert(&bundle)
        .await
        .expect("bundle insert should succeed");

    // Create a mutation proposal (R3 - non-R0)
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test mutation".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate request should succeed");

    // The response should be OK but with Deny decision from the policy bundle
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid EvaluateProposalResponse JSON");

    assert_eq!(
        evaluated.decision,
        Decision::Deny,
        "decision should be Deny from policy bundle, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .reason
            .contains("policy bundle test-deny-bundle matched rule deny-mutation"),
        "reason should mention policy bundle match, got: {}",
        evaluated.reason
    );
    assert!(
        evaluated
            .matched_rule_ids
            .contains(&"policy_bundle:test-deny-bundle:deny-mutation".to_string()),
        "matched_rule_ids should contain policy_bundle prefix, got: {:?}",
        evaluated.matched_rule_ids
    );
}

/// Verify that an inactive policy bundle has no effect on evaluate_proposal.
#[tokio::test]
async fn test_policy_bundle_inactive_has_no_effect() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Insert an intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let mut intent = make_test_intent(intent_id);
    // Add a resource scope to avoid PDP scope mismatch check
    intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
        path: "/test".to_string(),
        mode: ferrum_proto::ResourceMode::Write,
        content_hash: None,
    }];
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create and insert an INACTIVE policy bundle with a Deny rule
    let deny_rule = ferrum_proto::PolicyRule {
        id: "deny-mutation".to_string(),
        description: "deny all mutations".to_string(),
        decision: Decision::Deny,
        priority: 100,
        matchers: vec![Matcher::ActionIsMutation],
    };
    let bundle = make_test_policy_bundle("inactive-deny-bundle", vec![deny_rule], false); // inactive!
    store
        .policy_bundles()
        .insert(&bundle)
        .await
        .expect("bundle insert should succeed");

    // Create an R3 mutation proposal - should NOT match the inactive bundle
    // and should fall through to PDP which returns RequireApproval for R3
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test mutation".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate request should succeed");

    // The response should NOT be Deny from the bundle since it's inactive
    // Instead it should be RequireApproval from the PDP (R3 rule)
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid EvaluateProposalResponse JSON");

    assert_ne!(
        evaluated.decision,
        Decision::Deny,
        "decision should NOT be Deny from inactive bundle, got: {:?}",
        evaluated.decision
    );
    // R3 proposals without scope should get RequireApproval from PDP scaffold
    assert_eq!(
        evaluated.decision,
        Decision::RequireApproval,
        "decision should be RequireApproval from PDP for R3, got: {:?}",
        evaluated.decision
    );
}

/// Verify that an active policy bundle with a non-matching rule falls back to PDP.
#[tokio::test]
async fn test_policy_bundle_nonmatching_rule_falls_back_to_pdp() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Insert an intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let mut intent = make_test_intent(intent_id);
    // Add a resource scope to avoid PDP scope mismatch check
    intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
        path: "/test".to_string(),
        mode: ferrum_proto::ResourceMode::Write,
        content_hash: None,
    }];
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create and insert an active policy bundle with a rule that only matches R0
    // but our proposal is R3, so it won't match
    let r0_only_rule = ferrum_proto::PolicyRule {
        id: "allow-r0".to_string(),
        description: "allow only R0".to_string(),
        decision: Decision::Allow,
        priority: 100,
        matchers: vec![Matcher::RollbackClassEquals {
            value: "R0NativeReversible".to_string(),
        }],
    };
    let bundle = make_test_policy_bundle("r0-only-bundle", vec![r0_only_rule], true);
    store
        .policy_bundles()
        .insert(&bundle)
        .await
        .expect("bundle insert should succeed");

    // Create an R3 mutation proposal - should NOT match the R0-only rule
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test mutation".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate request should succeed");

    // The proposal is R3 which doesn't match the R0-only rule,
    // so it should fall back to PDP which returns RequireApproval for R3
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid EvaluateProposalResponse JSON");

    assert_eq!(
        evaluated.decision,
        Decision::RequireApproval,
        "decision should be RequireApproval from PDP for non-matching R3, got: {:?}",
        evaluated.decision
    );
    // matched_rule_ids should NOT contain any policy_bundle prefix since no rule matched
    assert!(
        !evaluated
            .matched_rule_ids
            .iter()
            .any(|id| id.starts_with("policy_bundle:")),
        "matched_rule_ids should not contain policy_bundle prefix for non-matching rule, got: {:?}",
        evaluated.matched_rule_ids
    );
}

/// Verify TaintAtLeast matcher in active policy bundle.
#[tokio::test]
async fn test_policy_bundle_taint_at_least_matcher() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Insert an intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create and insert an active policy bundle with a Quarantine rule matching high taint
    let quarantine_rule = ferrum_proto::PolicyRule {
        id: "high-taint-quarantine".to_string(),
        description: "quarantine high taint".to_string(),
        decision: Decision::Quarantine,
        priority: 100,
        matchers: vec![Matcher::TaintAtLeast { value: 50 }],
    };
    let bundle = make_test_policy_bundle("taint-bundle", vec![quarantine_rule], true);
    store
        .policy_bundles()
        .insert(&bundle)
        .await
        .expect("bundle insert should succeed");

    // Create an R3 proposal (mutation) with external metadata to trigger firewall-derived taint >= 50.
    // The firewall computes taint based on is_external (+30) and low trust_score (+20), totaling 50.
    // This ensures TaintAtLeast { value: 50 } matcher matches on firewall-derived taint.

    // Insert a high-taint trust context for this intent
    let high_taint_intent_id = ferrum_proto::IntentId::new();
    let high_taint_intent = make_test_intent(high_taint_intent_id);
    store
        .intents()
        .insert(&high_taint_intent)
        .await
        .expect("intent insert should succeed");

    // Create proposal for the high-taint intent with external metadata to trigger firewall taint
    let high_taint_proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id: high_taint_intent_id,
        step_index: 0,
        title: "test mutation high taint".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R1SnapshotRecoverable,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::from([("source".to_string(), serde_json::json!("test"))]),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&high_taint_proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid EvaluateProposalResponse JSON");

    // The firewall-derived taint (50) is >= 50, so TaintAtLeast matcher should match
    // and the bundle rule should return Quarantine
    assert_eq!(
        evaluated.decision,
        Decision::Quarantine,
        "decision should be Quarantine for high taint, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .reason
            .contains("policy bundle taint-bundle matched rule high-taint-quarantine"),
        "reason should mention policy bundle match, got: {}",
        evaluated.reason
    );
}

/// Verify ScopeMismatch matcher in active policy bundle.
#[tokio::test]
async fn test_policy_bundle_scope_mismatch_matcher() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Insert an intent with EMPTY resource scope (no scope = scope mismatch possible)
    let intent_id = ferrum_proto::IntentId::new();
    let mut intent = make_test_intent(intent_id);
    intent.resource_scope = Vec::new(); // Empty scope
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Create and insert an active policy bundle with a Deny rule matching ScopeMismatch
    let scope_deny_rule = ferrum_proto::PolicyRule {
        id: "deny-scope-mismatch".to_string(),
        description: "deny scope mismatch".to_string(),
        decision: Decision::Deny,
        priority: 100,
        matchers: vec![Matcher::ScopeMismatch],
    };
    let bundle = make_test_policy_bundle("scope-bundle", vec![scope_deny_rule], true);
    store
        .policy_bundles()
        .insert(&bundle)
        .await
        .expect("bundle insert should succeed");

    // Create a mutation proposal (R3 - non-R0) - this should trigger ScopeMismatch
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test mutation".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate request should succeed");

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let evaluated: EvaluateProposalResponse =
        serde_json::from_slice(&body).expect("valid EvaluateProposalResponse JSON");

    // ScopeMismatch: empty scope + mutation = true, so bundle should Deny
    assert_eq!(
        evaluated.decision,
        Decision::Deny,
        "decision should be Deny for scope mismatch, got: {:?}",
        evaluated.decision
    );
    assert!(
        evaluated
            .reason
            .contains("policy bundle scope-bundle matched rule deny-scope-mismatch"),
        "reason should mention policy bundle match, got: {}",
        evaluated.reason
    );
}

// ---------------------------------------------------------------------------
// Verify endpoint invalid-state 409 tests
// ---------------------------------------------------------------------------
