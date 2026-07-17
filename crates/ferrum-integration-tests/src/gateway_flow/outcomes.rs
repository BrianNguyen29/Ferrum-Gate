use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::{PdpEngine, StaticPdpEngine};
use ferrum_proto::{
    ApprovalMode, Decision, EffectType, EvaluateOutcomeResponse, EvaluateProposalResponse,
    ExecutionState, IntentEnvelope, IntentStatus, OutcomeClause, OutcomeReport, RiskTier,
    RollbackClass, TimeBudget, TrustContextSummary,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{
    CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, SqliteStore, StoreFacade,
};
use std::sync::Arc;

mod support;
use support::*;

/// Verify that a compile request with approval_mode=DraftOnly flows through
/// the gateway and results in AllowDraftOnly when evaluated.
#[tokio::test]
async fn test_gateway_compile_draft_only_flow() {
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

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // Step 1: compile intent with DraftOnly approval mode
    let compile_req = ferrum_proto::IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "draft-only-test".to_string(),
        goal: "test draft-only intent".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: Vec::new(),
        requested_resource_scope: Vec::new(),
        requested_risk_tier: Some(ferrum_proto::RiskTier::Medium),
        approval_mode: Some(ferrum_proto::ApprovalMode::DraftOnly),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/intents/compile")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&compile_req).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("compile request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "compile endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let compiled: ferrum_proto::IntentCompileResponse =
        serde_json::from_slice(&body).expect("valid json");

    assert_eq!(
        compiled.envelope.approval_mode,
        ferrum_proto::ApprovalMode::DraftOnly,
        "compiled intent should have DraftOnly approval mode, got: {:?}",
        compiled.envelope.approval_mode
    );

    let intent_id = compiled.envelope.intent_id;

    // Step 2: evaluate a proposal using that intent — should get AllowDraftOnly
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "draft proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Medium,
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

    let response = tower::ServiceExt::oneshot(router, request)
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
    let evaluated: EvaluateProposalResponse = serde_json::from_slice(&body).expect("valid json");

    assert_eq!(
        evaluated.decision,
        Decision::AllowDraftOnly,
        "DraftOnly intent should result in AllowDraftOnly, got: {:?}",
        evaluated.decision
    );
}

// ---------------------------------------------------------------------------
// WS3: Draft-only bypass regression test
// ---------------------------------------------------------------------------

/// Verify that a draft-only intent CANNOT reach prepare success by bypassing
/// evaluate. This is a regression test for WS3 (draft-only not revalidated at prepare).
///
/// The attack scenario:
/// 1. A DraftOnly intent is created
/// 2. An execution record is created directly (bypassing evaluate)
/// 3. Prepare is called on that execution
///
/// Expected: prepare should reject with PolicyDenied before attempting preparation.
#[tokio::test]
async fn test_draft_only_intent_cannot_reach_prepare_by_bypassing_evaluate() {
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

    // Step 1: Create a DraftOnly intent directly in the store (bypassing compile endpoint)
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "draft-only-bypass-test".to_string(),
        goal: "test draft-only bypass".to_string(),
        normalized_goal: "test draft-only bypass".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::DraftOnly, // Key: this is DraftOnly
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: Vec::new(),
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

    // Step 2: Create a proposal directly (bypassing evaluate)
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "bypass proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Medium,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: now,
        owner_actor_id: None,
    };
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    // Step 3: Mint a capability to satisfy foreign key constraints
    // We need a valid capability_id in the database for the execution record.
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: Vec::new(),
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };
    let capability_response = cap
        .mint(mint_request)
        .await
        .expect("mint capability should succeed");
    store
        .capabilities()
        .insert(&capability_response.lease)
        .await
        .expect("capability insert should succeed");

    // Step 4: Create an execution record directly (bypassing authorize)
    // This simulates an attacker who created an execution record without going through
    // the proper evaluate -> authorize -> prepare flow.
    let execution_id = ferrum_proto::ExecutionId::new();
    let execution = ferrum_proto::ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id: capability_response.lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow, // Pretend it was allowed
        state: ferrum_proto::ExecutionState::Authorized, // At authorized state, ready for prepare
        started_at: now,
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
        owner_actor_id: None,
    };
    store
        .executions()
        .insert(&execution)
        .await
        .expect("execution insert should succeed");

    // Step 5: Call prepare on the execution - this should REJECT because the
    // intent is DraftOnly and DraftOnly intents cannot proceed to prepare.
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("prepare request should complete");

    // EXPECTED: prepare should be rejected with 403 Forbidden
    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "prepare should reject draft-only intent with FORBIDDEN, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error_response: ferrum_proto::ApiError =
        serde_json::from_slice(&body).expect("valid error json");

    assert!(
        matches!(
            error_response.code,
            ferrum_proto::ApiErrorCode::PolicyDenied
        ),
        "error code should be PolicyDenied, got: {:?}",
        error_response.code
    );
    assert!(
        error_response.message.contains("draft-only"),
        "error message should mention draft-only, got: {}",
        error_response.message
    );
}

// ---------------------------------------------------------------------------
// I5: Scope cannot expand beyond intent — integration test
// ---------------------------------------------------------------------------

/// Verify that authorize_execution denies when capability resource_bindings
/// exceed the intent's resource_scope (I5 invariant).
#[tokio::test]
async fn test_i5_scope_validation_resource_bindings_exceed_intent_scope() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
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

    // Create intent with scope limited to /tmp
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "i5-test-intent".to_string(),
        goal: "test I5 scope validation".to_string(),
        normalized_goal: "test I5 scope validation".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }],
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: Vec::new(),
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

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create proposal
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "i5-test-proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Medium,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    // Mint capability with resource binding OUTSIDE intent scope (/other/path instead of /tmp)
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ferrum_proto::ResourceBinding::File {
            path: "/other/path/file.txt".to_string(), // Outside /tmp scope!
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&mint_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    // Minting the capability succeeds (validation happens at authorize time)
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "mint endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize execution — should fail because resource binding is outside intent scope
    let auth_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");

    // Should be FORBIDDEN because resource bindings exceed intent scope
    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize should return FORBIDDEN when resource bindings exceed intent scope, got: {:?}",
        response.status()
    );
}

/// Verify that authorize_execution allows when capability resource_bindings
/// are within the intent's resource_scope (I5 invariant - valid subset case).
#[tokio::test]
async fn test_i5_scope_validation_resource_bindings_within_intent_scope() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
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

    // Create intent with scope /tmp
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "i5-valid-test-intent".to_string(),
        goal: "test I5 scope validation (valid case)".to_string(),
        normalized_goal: "test I5 scope validation (valid case)".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }],
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: Vec::new(),
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

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create proposal
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "i5-valid-test-proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Medium,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    // Mint capability with resource binding WITHIN intent scope (/tmp/subdir/file.txt is under /tmp)
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ferrum_proto::ResourceBinding::File {
            path: "/tmp/subdir/file.txt".to_string(), // Within /tmp scope!
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&mint_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "mint endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize execution — should succeed because resource binding is within intent scope
    let auth_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");

    // Should succeed because resource binding is within intent scope
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "authorize should return OK when resource bindings are within intent scope, got: {:?}",
        response.status()
    );
}

// ---------------------------------------------------------------------------
// Proposal/Capability Binding Guard
// ---------------------------------------------------------------------------

/// Verify that when an execution's actual effect matches an allowed outcome,
/// the evaluate-outcome endpoint returns aligned=true.
#[tokio::test]
async fn test_outcome_evaluation_aligned_flow() {
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

    // Step 1: Create and persist an intent
    let now = chrono::Utc::now();
    let intent = IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![
            OutcomeClause {
                id: "read_only".to_string(),
                description: "read only analysis".to_string(),
                effect_type: EffectType::ReadOnlyAnalysis,
                required: true,
            },
            OutcomeClause {
                id: "file_write".to_string(),
                description: "file mutation".to_string(),
                effect_type: EffectType::FileMutation,
                required: false,
            },
        ],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Medium,
        approval_mode: ApprovalMode::None,
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
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
        owner_actor_id: None,
    };
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Step 2: Create a proposal and insert it
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id: intent.intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
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
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    // Step 3: Mint a capability
    let cap_request = ferrum_proto::CapabilityMintRequest {
        intent_id: intent.intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: Vec::new(),
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&cap_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "mint endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Step 4: Authorize execution (creates execution in Prepared state)
    let auth_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "authorize endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 5: Prepare the execution
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "prepare endpoint should return 200"
    );

    // Step 6: Update execution state to Committed (simulating successful execution)
    let execution = store
        .executions()
        .get(execution_id)
        .await
        .expect("get execution should succeed")
        .expect("execution not found");
    let mut updated_execution = execution.clone();
    updated_execution.state = ExecutionState::Committed;
    store
        .executions()
        .update(&updated_execution)
        .await
        .expect("update execution should succeed");

    // Step 7: Evaluate outcome with an aligned effect
    let report = OutcomeReport {
        execution_id,
        actual_effect: EffectType::FileMutation,
        description: "file was successfully modified".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&report).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate-outcome request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate-outcome endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let result: EvaluateOutcomeResponse = serde_json::from_slice(&body).expect("valid json");

    assert!(
        result.aligned,
        "FileMutation in allowed_outcomes should result in aligned=true, got: {:?}",
        result.aligned
    );
}

/// Verify that when an execution's actual effect matches a forbidden outcome,
/// the evaluate-outcome endpoint returns aligned=false.
#[tokio::test]
async fn test_outcome_evaluation_forbidden_flow() {
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

    // Step 1: Create an intent with GitMutation as a forbidden outcome
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "read-only intent".to_string(),
        goal: "analyze repository".to_string(),
        normalized_goal: "analyze repository".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: vec![OutcomeClause {
            id: "forbid-git".to_string(),
            description: "no git mutations allowed".to_string(),
            effect_type: EffectType::GitMutation,
            required: false,
        }],
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Medium,
        approval_mode: ApprovalMode::None,
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
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
        owner_actor_id: None,
    };
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    // Step 2: Create a proposal and insert it
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read only analysis".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: noop_binding_metadata(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    let cap_request = ferrum_proto::CapabilityMintRequest {
        intent_id: proposal.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: Vec::new(),
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&cap_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "mint endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Step 4: Authorize execution (creates execution in Prepared state)
    let auth_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "authorize endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 5: Prepare the execution
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "prepare endpoint should return 200"
    );

    // Step 6: Update execution state to Committed (simulating successful execution)
    let execution = store
        .executions()
        .get(execution_id)
        .await
        .expect("get execution should succeed")
        .expect("execution not found");
    let mut updated_execution = execution.clone();
    updated_execution.state = ExecutionState::Committed;
    store
        .executions()
        .update(&updated_execution)
        .await
        .expect("update execution should succeed");

    // Step 7: Evaluate outcome with a forbidden effect (GitMutation)
    let report = OutcomeReport {
        execution_id,
        actual_effect: EffectType::GitMutation,
        description: "git commit was performed".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&report).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("evaluate-outcome request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate-outcome endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let result: EvaluateOutcomeResponse = serde_json::from_slice(&body).expect("valid json");

    assert!(
        !result.aligned,
        "GitMutation in forbidden_outcomes should result in aligned=false, got: {:?}",
        result.aligned
    );
}

// ---------------------------------------------------------------------------
// U4: Bridge registration + ingest + lineage validation integration tests
// ---------------------------------------------------------------------------
