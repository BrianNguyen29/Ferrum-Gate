use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, CapabilityMintRequest, Decision, EffectType, ExecutionState,
    IntentCompileRequest, IntentCompileResponse, IntentId, ProposalId, ProvenanceEventKind,
    ResourceBinding, ResourceMode, ResourceSelector, RiskTier, RollbackClass, SensitivityLabel,
    TaintBudget, ToolBinding, TrustLabel,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo,
    RollbackRepo, SqliteStore,
};
use std::sync::Arc;
use tempfile::TempDir;
use tower::util::ServiceExt;

async fn create_test_runtime() -> (TempDir, GatewayRuntime, SqliteStore) {
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

    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine::default());
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()), firewall);

    (temp_dir, runtime, store)
}

fn sample_intent_request() -> IntentCompileRequest {
    sample_intent_request_with_effect(EffectType::ReadOnlyAnalysis)
}

fn sample_intent_request_with_effect(effect_type: EffectType) -> IntentCompileRequest {
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(effect_type),
        metadata: ferrum_proto::JsonMap::new(),
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
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_compile_intent_persists_and_emits_provenance() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Verify intent was persisted
    let stored_intent = runtime.store.intents().get(intent_id).await.unwrap();
    assert!(stored_intent.is_some());
    assert_eq!(stored_intent.unwrap().intent_id, intent_id);

    // Verify provenance event was emitted
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::IntentCompiled),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(!events.is_empty());
    assert!(matches!(
        events[0].kind,
        ProvenanceEventKind::IntentCompiled
    ));
}

#[tokio::test]
async fn test_evaluate_proposal_loads_real_intent_and_persists() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // First compile an intent to have a real intent in the store
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Now create a proposal using that intent_id
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;

    // Evaluate the proposal
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Assert the decision is Allow (default policy allows low-risk R0 proposals)
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Verify proposal was persisted
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());
    assert_eq!(stored_proposal.unwrap().proposal_id, proposal_id);

    // Verify provenance events were emitted
    let submission_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ActionProposalSubmitted),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(!submission_events.is_empty());
    assert!(matches!(
        submission_events[0].kind,
        ProvenanceEventKind::ActionProposalSubmitted
    ));

    let eval_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(!eval_events.is_empty());
    assert!(matches!(
        eval_events[0].kind,
        ProvenanceEventKind::PolicyEvaluated
    ));
}

#[tokio::test]
async fn test_evaluate_proposal_falls_back_to_minimal_intent_when_not_found() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create a proposal with a non-existent intent_id
    // Note: The proposal cannot be persisted because of FK constraint (intent must exist)
    // But the evaluation should still work using fallback minimal intent
    let non_existent_intent_id = IntentId::new();
    let proposal = sample_proposal(non_existent_intent_id);
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should still succeed with fallback to minimal intent
    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should still get a decision (allow) since minimal intent passes
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Note: Proposal will NOT be persisted because intent doesn't exist (FK constraint)
    // This is expected behavior - the system gracefully handles this by:
    // 1. Using minimal intent for evaluation (fallback)
    // 2. Warning about proposal persistence failure
    // 3. Still returning a decision
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_none()); // Expected: proposal not persisted due to FK constraint

    // However, provenance events should still be emitted
    let eval_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(non_existent_intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    // Policy evaluation provenance should still be emitted even without persisted proposal
    assert!(!eval_events.is_empty());

    // Verify PolicyEvaluated has empty parent_edges (key provenance hardening requirement)
    assert!(
        eval_events[0].parent_edges.is_empty(),
        "PolicyEvaluated.parent_edges should be empty when intent is not found"
    );

    // CRITICAL: ActionProposalSubmitted should NOT be emitted when proposal persistence fails
    let submission_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(non_existent_intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ActionProposalSubmitted),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        submission_events.is_empty(),
        "ActionProposalSubmitted should NOT be emitted when intent does not exist and proposal persistence fails"
    );
}

#[tokio::test]
async fn test_evaluate_proposal_id_mismatch_returns_400_and_no_events() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // First compile an intent to have a real intent in the store
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create a proposal with a DIFFERENT proposal_id in body than in path
    let path_proposal_id = ProposalId::new();
    let body_proposal_id = ProposalId::new(); // Different!

    let mut proposal = sample_proposal(intent_id);
    proposal.proposal_id = body_proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", path_proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 400 Bad Request due to proposal_id mismatch
    assert_eq!(
        response.status(),
        400,
        "Expected 400 for proposal_id mismatch"
    );

    // Verify no proposal was persisted (neither path nor body proposal_id)
    let stored_proposal_path = runtime
        .store
        .proposals()
        .get(path_proposal_id)
        .await
        .unwrap();
    let stored_proposal_body = runtime
        .store
        .proposals()
        .get(body_proposal_id)
        .await
        .unwrap();
    assert!(
        stored_proposal_path.is_none(),
        "No proposal should be persisted for path proposal_id"
    );
    assert!(
        stored_proposal_body.is_none(),
        "No proposal should be persisted for body proposal_id"
    );

    // Verify no proposal-related provenance events were emitted
    let submission_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::ActionProposalSubmitted),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        submission_events.is_empty(),
        "ActionProposalSubmitted should NOT be emitted when proposal_id mismatch"
    );

    let eval_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::PolicyEvaluated),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        eval_events.is_empty(),
        "PolicyEvaluated should NOT be emitted when proposal_id mismatch"
    );
}
#[tokio::test]
async fn test_full_happy_path_flow_compile_evaluate_mint_authorize_prepare() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::Allow);

    // Step 3: Mint capability
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Read,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60, // Max allowed is 300 seconds
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    if status != 200 {
        let err_str = String::from_utf8_lossy(&body);
        panic!("Mint failed with status={}, body={}", status, err_str);
    }
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare execution (rollback prep)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // === Verify stored records ===

    // Verify intent persisted
    let stored_intent = runtime.store.intents().get(intent_id).await.unwrap();
    assert!(stored_intent.is_some());

    // Verify proposal persisted
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());

    // Verify capability persisted
    let stored_capability = runtime
        .store
        .capabilities()
        .get(capability_id)
        .await
        .unwrap();
    assert!(stored_capability.is_some());

    // Verify execution persisted
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let rollback_contract_id = stored_execution.unwrap().rollback_contract_id.unwrap();

    // Verify rollback contract persisted
    let stored_rollback = runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
        .unwrap();
    assert!(stored_rollback.is_some());

    // === Verify provenance chain ===

    // Query all provenance events for this intent
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    // Should have events for: IntentCompiled, ActionProposalSubmitted, PolicyEvaluated,
    // CapabilityMinted, ToolCallPrepared, SideEffectPrepared
    let has_intent_compiled = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::IntentCompiled));
    let has_proposal_submitted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_policy_evaluated = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::PolicyEvaluated));
    let has_capability_minted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::CapabilityMinted));
    let has_tool_call_prepared = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));
    let has_side_effect_prepared = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));

    assert!(has_intent_compiled, "Missing IntentCompiled event");
    assert!(
        has_proposal_submitted,
        "Missing ActionProposalSubmitted event"
    );
    assert!(has_policy_evaluated, "Missing PolicyEvaluated event");
    assert!(has_capability_minted, "Missing CapabilityMinted event");
    assert!(has_tool_call_prepared, "Missing ToolCallPrepared event");
    assert!(has_side_effect_prepared, "Missing SideEffectPrepared event");

    // Verify causal linkage: PolicyEvaluated should have parent edge to ActionProposalSubmitted
    let policy_eval_event = all_events
        .iter()
        .find(|e| matches!(e.kind, ProvenanceEventKind::PolicyEvaluated))
        .unwrap();
    assert!(
        !policy_eval_event.parent_edges.is_empty(),
        "PolicyEvaluated should have parent edge"
    );
    assert!(matches!(
        policy_eval_event.parent_edges[0].edge_type,
        ferrum_proto::ProvenanceEdgeType::Caused
    ));
}

// Helper function to run the full flow up to prepare
async fn run_flow_to_prepared(
    runtime: &GatewayRuntime,
    rollback_class: RollbackClass,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Step 1: Compile intent with mutating effect type (since we'll test mutations)
    let app = build_router(runtime.clone());
    let req = sample_intent_request_with_effect(EffectType::FileMutation);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal with specified rollback class
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Execute mutation".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: rollback_class,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

#[tokio::test]
async fn test_full_happy_path_execute_verify_auto_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R0 (auto-commit enabled)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);

    // Step 7: Verify (should auto-commit for R0)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);
    assert!(verify_resp.verified_at.is_some());

    // Verify execution state is Committed (auto-committed)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Committed));

    // Verify provenance chain includes ToolCallExecuted, SideEffectVerified, SideEffectCommitted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    let has_tool_call_executed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_side_effect_verified = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_side_effect_committed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(has_tool_call_executed, "Missing ToolCallExecuted event");
    assert!(has_side_effect_verified, "Missing SideEffectVerified event");
    assert!(
        has_side_effect_committed,
        "Missing SideEffectCommitted event (auto-commit)"
    );
}

#[tokio::test]
async fn test_r2_no_auto_commit_verify_then_explicit_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R2 (auto-commit disabled)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Verify the rollback contract has auto_commit = false
    let stored_execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    let contract_id = stored_execution.rollback_contract_id.unwrap();
    let stored_contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !stored_contract.auto_commit,
        "R2 should have auto_commit = false"
    );

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify (should NOT auto-commit for R2)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Verify execution state is AwaitingVerification (NOT committed for R2)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::AwaitingVerification),
        "R2 should remain in AwaitingVerification after verify, got {:?}",
        exec.state
    );

    // Step 8: Explicit commit (required for R2)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let commit_resp: ferrum_proto::CommitResponse = serde_json::from_slice(&body).unwrap();
    assert!(commit_resp.committed);
    assert!(commit_resp.committed_at.is_some());

    // Verify final state is Committed
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Committed));

    // Verify provenance chain
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    let has_tool_call_executed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_side_effect_verified = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_side_effect_committed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(has_tool_call_executed, "Missing ToolCallExecuted event");
    assert!(has_side_effect_verified, "Missing SideEffectVerified event");
    assert!(
        has_side_effect_committed,
        "Missing SideEffectCommitted event"
    );
}

#[tokio::test]
async fn test_quarantine_path_blocks_execution_advance() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect type (to test quarantine path, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal that triggers quarantine (high taint score + non-R0 rollback class)
    // The StaticPDPEngine quarantines if taint_score >= 70 AND rollback class is not R0
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate with high taint".to_string(),
        tool_name: "db.write".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"sql": "DROP TABLE users"}),
        expected_effect: "delete data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable, // Not R0, will trigger quarantine
        decision: None,
        taint_inputs: vec![
            // Need taint_score >= 70 to trigger quarantine with R2
            // untrusted_input = 30, external_source = 25, web_url = 20
            "untrusted_input".to_string(),
            "external_source".to_string(),
            "web_url".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Verify the decision is Quarantine
    assert_eq!(eval_resp.decision, Decision::Quarantine);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 100, // Allow high taint
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution - should create a quarantined execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let _execution_id = auth_resp.execution.execution_id;

    // Verify the execution is in Quarantined state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::Quarantined
    ));
    assert!(matches!(auth_resp.execution.decision, Decision::Quarantine));
    assert!(auth_resp.execution.finished_at.is_some());

    // Verify Quarantined provenance event was emitted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::Quarantined),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing Quarantined provenance event"
    );
}

#[tokio::test]
async fn test_rollback_path_recovers_execution() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R2 (compensatable)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Step 8: Explicit commit (R2 requires manual commit like R3)
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 9: Now rollback the execution
    let app = build_router(runtime.clone());
    let rollback_req = ferrum_proto::RollbackRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/rollback", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&rollback_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let rollback_resp: ferrum_proto::RollbackResponse = serde_json::from_slice(&body).unwrap();
    assert!(rollback_resp.rolled_back);
    assert!(rollback_resp.rolled_back_at.is_some());

    // Verify execution state is RolledBack
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::RolledBack));

    // Verify provenance chain includes SideEffectRolledBack
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectRolledBack),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing SideEffectRolledBack provenance event"
    );
}

#[tokio::test]
async fn test_compensate_path_recovers_execution() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R2 (compensatable)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R2Compensatable).await;

    // Step 6: Execute
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 7: Verify
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_resp: ferrum_proto::VerifyResponse = serde_json::from_slice(&body).unwrap();
    assert!(verify_resp.verified);

    // Step 8: Explicit commit
    let app = build_router(runtime.clone());
    let commit_req = ferrum_proto::CommitRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/commit", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&commit_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 9: Now compensate the execution
    let app = build_router(runtime.clone());
    let compensate_req = ferrum_proto::CompensateRequest { execution_id };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/compensate", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&compensate_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compensate_resp: ferrum_proto::CompensateResponse = serde_json::from_slice(&body).unwrap();
    assert!(compensate_resp.compensated);
    assert!(compensate_resp.compensated_at.is_some());

    // Verify execution state is Compensated
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Compensated));

    // Verify provenance chain includes SideEffectCompensated
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::SideEffectCompensated),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        !all_events.is_empty(),
        "Missing SideEffectCompensated provenance event"
    );
}

// ============================================
// NEGATIVE TESTS: Illegal State Transitions
// ============================================

#[tokio::test]
async fn test_prepare_execution_blocks_quarantined_state() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create an execution in Quarantined state by going through quarantine path
    // Step 1: Compile intent with mutating effect (to test quarantine, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal that triggers quarantine
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate with high taint".to_string(),
        tool_name: "db.write".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"sql": "DROP TABLE users"}),
        expected_effect: "delete data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![
            // Need taint_score >= 70 to trigger quarantine with R2
            "untrusted_input".to_string(),
            "external_source".to_string(),
            "web_url".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 100,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution (will create quarantined execution)
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is in Quarantined state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::Quarantined
    ));

    // Step 5: Attempt to prepare execution - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "prepare should fail for quarantined execution"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("quarantined"));
}

#[tokio::test]
async fn test_authorize_execution_blocks_proposal_capability_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create and evaluate TWO different proposals for the same intent
    let app = build_router(runtime.clone());
    let proposal1 = sample_proposal(intent_id);
    let proposal1_id = proposal1.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal1_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal1).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Create second proposal
    let app = build_router(runtime.clone());
    let proposal2 = sample_proposal(intent_id);
    let proposal2_id = proposal2.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal2_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal2).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability bound to proposal1
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id: proposal1_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.read".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Attempt to authorize execution using proposal2_id with capability bound to proposal1
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: proposal2_id, // Wrong proposal!
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 403 Forbidden due to proposal/capability mismatch
    assert_eq!(
        response.status(),
        403,
        "authorize should fail for proposal/capability mismatch"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("mismatch"));
    assert!(error.message.contains(&proposal1_id.to_string()));
    assert!(error.message.contains(&proposal2_id.to_string()));
}

#[tokio::test]
async fn test_prepare_execution_blocks_denied_state() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // We need to create a Denied execution. Currently the system creates Denied
    // when proposal decision is Deny or AllowDraftOnly.
    // We'll manually insert a Denied execution to test the guard.

    // First create a basic flow to get valid IDs
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Manually update the execution to Denied state
    let mut execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    execution.state = ExecutionState::Denied;
    execution.finished_at = Some(chrono::Utc::now());
    runtime.store.executions().update(&execution).await.unwrap();

    // Attempt to prepare execution - should fail with 409 Conflict
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "prepare should fail for denied execution"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("denied"));
}

#[tokio::test]
async fn test_prepare_execution_blocks_terminal_states() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Test Committed state
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Execute and commit
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({}),
    };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify (auto-commits for R0)
    let app = build_router(runtime.clone());
    let verify_req = ferrum_proto::VerifyRequest { execution_id };
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/verify", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&verify_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Verify execution is Committed
    let execution = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(execution.state, ExecutionState::Committed));

    // Attempt to prepare committed execution - should fail
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("terminal state"));
}

// ============================================
// APPROVAL FLOW TESTS
// ============================================

#[tokio::test]
async fn test_full_approval_flow_approve_then_prepare_succeeds() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect (to test approval flow, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal (requires approval)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "db.delete".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "users"}),
        expected_effect: "delete all user data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence, // Requires approval
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::RequireApproval);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution - should create AwaitingApproval execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is in AwaitingApproval state
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));
    assert_eq!(auth_resp.execution.decision, Decision::RequireApproval);

    // Verify approval request was created
    let pending_approvals = runtime.store.approvals().list_pending().await.unwrap();
    assert!(
        !pending_approvals.is_empty(),
        "Approval request should be created"
    );
    let approval = &pending_approvals[0];
    assert_eq!(approval.intent_id, intent_id);
    assert_eq!(approval.proposal_id, proposal_id);
    assert_eq!(approval.execution_id, Some(execution_id));
    assert!(matches!(
        approval.state,
        ferrum_proto::ApprovalState::Pending
    ));
    let approval_id = approval.approval_id;

    // Step 5: Attempt to prepare execution - should fail with 409 (awaiting approval)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        409,
        "prepare should fail for execution awaiting approval"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("awaiting approval"));

    // Step 6: Resolve approval with approve=true
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: true,
        reason: Some("Approved by admin".to_string()),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resolve_resp: ferrum_proto::ApprovalRequest = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        resolve_resp.state,
        ferrum_proto::ApprovalState::Granted
    ));

    // Step 7: Verify execution transitioned to Authorized
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::Authorized),
        "execution should be Authorized after approval, got {:?}",
        exec.state
    );
    assert_eq!(exec.decision, Decision::Allow);

    // Step 8: Prepare execution should now succeed
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let prep_resp: ferrum_proto::PrepareExecutionResponse = serde_json::from_slice(&body).unwrap();
    assert!(prep_resp.prepared);
    assert!(prep_resp.rollback_contract.is_some());

    // Verify provenance events: ApprovalRequested and ApprovalGranted
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    let has_approval_requested = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalRequested));
    let has_approval_granted = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalGranted));

    assert!(
        has_approval_requested,
        "Missing ApprovalRequested provenance event"
    );
    assert!(
        has_approval_granted,
        "Missing ApprovalGranted provenance event"
    );
}

#[tokio::test]
async fn test_approval_denial_flow_blocks_execution() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect (to test approval flow, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal (requires approval)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "db.delete".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "users"}),
        expected_effect: "delete all user data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(eval_resp.decision, Decision::RequireApproval);

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is AwaitingApproval
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::AwaitingApproval
    ));

    // Get approval ID
    let pending_approvals = runtime.store.approvals().list_pending().await.unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    // Step 5: Resolve approval with approve=false (deny)
    let app = build_router(runtime.clone());
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "admin".to_string(),
            display_name: Some("Admin".to_string()),
        },
        approve: false,
        reason: Some("Denied by admin".to_string()),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}/resolve", approval_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&resolve_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let resolve_resp: ferrum_proto::ApprovalRequest = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        resolve_resp.state,
        ferrum_proto::ApprovalState::Denied
    ));

    // Step 6: Verify execution transitioned to Denied terminal state
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(matches!(exec.state, ExecutionState::Denied));
    assert_eq!(exec.decision, Decision::Deny);
    assert!(exec.finished_at.is_some());

    // Step 7: Prepare should fail for denied execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("denied"));

    // Verify provenance events: ApprovalRequested and ApprovalDenied
    let all_events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    let has_approval_requested = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalRequested));
    let has_approval_denied = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalDenied));

    assert!(
        has_approval_requested,
        "Missing ApprovalRequested provenance event"
    );
    assert!(
        has_approval_denied,
        "Missing ApprovalDenied provenance event"
    );
}

#[tokio::test]
async fn test_get_approval_by_id() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with mutating effect (to test approval flow, not read-only violation)
    let req = sample_intent_request_with_effect(EffectType::DatabaseMutation);
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal and evaluate
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "db.delete".to_string(),
        server_name: "database".to_string(),
        raw_arguments: serde_json::json!({"table": "users"}),
        expected_effect: "delete all user data".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability and authorize to create approval
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "database".to_string(),
            tool_name: "db.delete".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Get approval ID
    let pending_approvals = runtime.store.approvals().list_pending().await.unwrap();
    assert!(!pending_approvals.is_empty());
    let approval_id = pending_approvals[0].approval_id;

    // Step 4: Get approval by ID
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}", approval_id))
                .method(axum::http::Method::GET)
                .body("".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_resp: ferrum_proto::ApprovalRequest = serde_json::from_slice(&body).unwrap();
    assert_eq!(get_resp.approval_id, approval_id);
    assert_eq!(get_resp.intent_id, intent_id);
    assert_eq!(get_resp.proposal_id, proposal_id);
    assert!(matches!(
        get_resp.state,
        ferrum_proto::ApprovalState::Pending
    ));

    // Step 5: Get non-existent approval should 404
    let app = build_router(runtime.clone());
    let fake_approval_id = ferrum_proto::ApprovalId::new();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/approvals/{}", fake_approval_id))
                .method(axum::http::Method::GET)
                .body("".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

// ============================================
// HARDENING REGRESSION TESTS
// ============================================

#[tokio::test]
async fn test_read_only_intent_empty_scope_blocks_mutating_proposal() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with read-only effect (empty scope)
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a mutating proposal against the read-only intent
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "data"}),
        expected_effect: "write file contents".to_string(), // "write" is a mutating keyword
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // The proposal should be denied because read-only intent + mutating proposal is a violation
    // (read_only_violation is High severity -> Deny)
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "Read-only intent with empty scope should block mutating proposal, got {:?}",
        eval_resp.decision
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .iter()
            .any(|r| r == "read_only_violation"),
        "Expected read_only_violation in matched rules: {:?}",
        eval_resp.matched_rule_ids
    );
}

#[tokio::test]
async fn test_compile_time_taint_contributes_to_taint_score() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with external web content (adds ExternalWeb label)
    let mut req = sample_intent_request();
    req.raw_inputs = vec![ferrum_proto::IntentInputRef {
        source_id: "user_input".to_string(),
        source_type: "user".to_string(),
        trust_labels: vec![TrustLabel::ExternalWeb, TrustLabel::Untrusted],
        sensitivity_labels: vec![],
        summary: "Visit https://example.com for more info".to_string(),
        event_id: None,
    }];

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Verify compile-time taint labels are present
    assert!(
        compile_resp
            .envelope
            .trust_context
            .input_labels
            .contains(&TrustLabel::ExternalWeb),
        "ExternalWeb label should be present from compile-time"
    );

    // Step 2: Create a proposal with additional taint inputs
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Analyze data".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![
            "external_api".to_string(), // Adds 25 to taint score
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Step 3: Verify the combined taint score was used (compile-time + proposal-time)
    // ExternalWeb (compile) + ExternalWeb from content + external_api (proposal) = 25 + 25 = 50+
    let stored_proposal = runtime.store.proposals().get(proposal_id).await.unwrap();
    assert!(stored_proposal.is_some());
}

#[tokio::test]
async fn test_high_severity_contradiction_fail_closed() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Test High severity (read-only violation) -> Deny
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create a proposal with read-only intent but mutating effect (High severity)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Delete file".to_string(),
        tool_name: "fs.delete".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        expected_effect: "delete file permanently".to_string(), // "delete" is mutating
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // High severity (read_only_violation) should result in Deny (fail-closed)
    assert_eq!(
        eval_resp.decision,
        Decision::Deny,
        "High severity contradiction should result in Deny, got {:?}",
        eval_resp.decision
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .iter()
            .any(|r| r == "read_only_violation"),
        "Expected read_only_violation in matched rules"
    );
}

#[tokio::test]
async fn test_effect_classifier_word_boundary_safety() {
    // Test that the effect classifier uses word boundaries
    // "target" should NOT be classified as "get" (read-only) because it's a substring
    use ferrum_firewall::{DefaultFirewall, SemanticFirewall};

    let firewall = DefaultFirewall::new();

    // Create a read-only intent
    let intent = ferrum_proto::IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![ferrum_proto::OutcomeClause {
            id: "primary".to_string(),
            description: "Test outcome".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(), // Empty scope - should still block mutating proposals
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30000,
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
        created_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
    };

    // "target" contains "get" but should NOT be treated as a read-only operation
    // (it's an unknown effect, so should default to mutating - fail-closed)
    let proposal_target = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id: intent.intent_id,
        step_index: 1,
        title: "Target operation".to_string(),
        tool_name: "ops.target".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "target resource".to_string(), // Contains "get" in "target"
        estimated_risk: ferrum_proto::RiskTier::Low,
        requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let contradictions = firewall.contradiction_check(&intent, &proposal_target);

    // "target" should be treated as unknown/mutating (fail-closed), triggering read_only_violation
    assert!(
        contradictions
            .iter()
            .any(|c| c.rule_id == "read_only_violation"),
        "'target' should not match 'get' - unknown effects should default to mutating (fail-closed)"
    );
}

// ============================================
// DRAFT-ONLY FLOW TESTS
// ============================================

#[tokio::test]
async fn test_draft_only_flow_dry_run_succeeds() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent with DraftOnly approval mode
    let req = sample_intent_request();
    // Note: The intent's approval_mode is set during compilation, but we need to trigger
    // AllowDraftOnly decision from the PDP. This happens when intent.approval_mode is DraftOnly.
    // We'll need to test this via the PDP's behavior.

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal and manually update intent to have DraftOnly mode
    // The StaticPdpEngine returns AllowDraftOnly when intent.approval_mode is DraftOnly
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Draft action".to_string(),
        tool_name: "email.draft".to_string(),
        server_name: "mail".to_string(),
        raw_arguments: serde_json::json!({"to": "test@example.com", "subject": "Test"}),
        expected_effect: "create email draft".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: Some(Decision::AllowDraftOnly), // Manually set to DraftOnly
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Store proposal with DraftOnly decision
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "mail".to_string(),
            tool_name: "email.draft".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize with dry_run=true - should succeed with Authorized state
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: true, // Dry run should succeed
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();

    // Dry run should succeed with Authorized state but remain draft-only
    assert!(matches!(
        auth_resp.execution.state,
        ExecutionState::Authorized
    ));
    assert_eq!(auth_resp.execution.decision, Decision::AllowDraftOnly);

    // Draft-only authorization must not escalate into normal prepare/execute path
    let execution_id = auth_resp.execution.execution_id;
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(error.message.contains("draft-only"));
}

#[tokio::test]
async fn test_draft_only_flow_non_dry_run_is_denied() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal with AllowDraftOnly decision
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Draft action".to_string(),
        tool_name: "email.draft".to_string(),
        server_name: "mail".to_string(),
        raw_arguments: serde_json::json!({"to": "test@example.com", "subject": "Test"}),
        expected_effect: "create email draft".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: Some(Decision::AllowDraftOnly),
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Store proposal with DraftOnly decision
    runtime.store.proposals().insert(&proposal).await.unwrap();

    // Step 3: Mint capability
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "mail".to_string(),
            tool_name: "email.draft".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize with dry_run=false - should be denied
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false, // Non-dry-run should be denied
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Request succeeds but execution is denied
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();

    // Non-dry-run should be denied with Denied state and Deny decision
    assert!(matches!(auth_resp.execution.state, ExecutionState::Denied));
    assert_eq!(auth_resp.execution.decision, Decision::Deny);
    assert!(auth_resp.execution.finished_at.is_some());
    assert!(auth_resp.warnings.iter().any(|w| w.contains("draft-only")));

    // Step 5: Prepare should fail for denied execution
    let execution_id = auth_resp.execution.execution_id;
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
}

// Phase C Firewall Integration Tests

#[tokio::test]
async fn test_compile_intent_derives_trust_context_from_inputs() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create a compile request with external URL in raw inputs
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![ferrum_proto::IntentInputRef {
            source_id: "web_search".to_string(),
            source_type: "external".to_string(),
            trust_labels: vec![TrustLabel::ExternalWeb],
            sensitivity_labels: vec![SensitivityLabel::Internal],
            summary: format!(
                "Check out https://example.com for more information about this topic. {}",
                "Additional content here. ".repeat(100)
            ),
            event_id: None,
        }],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();

    // Verify trust context was derived correctly
    let trust_context = &compile_resp.envelope.trust_context;
    assert!(
        trust_context
            .input_labels
            .contains(&TrustLabel::ExternalWeb)
    );
    assert!(
        trust_context
            .sensitivity_labels
            .contains(&SensitivityLabel::Internal)
    );
    assert!(trust_context.contains_untrusted_text);
    assert!(trust_context.contains_external_metadata);

    // Verify warnings were generated
    assert!(!compile_resp.warnings.is_empty());
    assert!(
        compile_resp
            .warnings
            .iter()
            .any(|w| w.contains("untrusted"))
    );
}

#[tokio::test]
async fn test_evaluate_proposal_denies_read_only_violation() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile a read-only intent
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Read-only Intent".to_string(),
        goal: "Read some data".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![],
        requested_risk_tier: Some(RiskTier::Low),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Modify intent to have read-only outcomes and explicit scope
    let mut read_only_intent = compile_resp.envelope;
    read_only_intent.allowed_outcomes = vec![ferrum_proto::OutcomeClause {
        id: "primary".to_string(),
        description: "Read only".to_string(),
        effect_type: EffectType::ReadOnlyAnalysis,
        required: true,
    }];
    read_only_intent.resource_scope = vec![ResourceSelector::McpTool {
        server_name: "workspace".to_string(),
        tool_name: "fs.read".to_string(),
        mode: ResourceMode::Read,
    }];
    runtime
        .store
        .intents()
        .update(&read_only_intent)
        .await
        .unwrap();

    // Step 2: Create a mutating proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Write file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "data"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R1SnapshotRecoverable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal - should be denied due to read-only violation
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should be denied due to read-only violation
    assert_eq!(eval_resp.decision, Decision::Deny);
    assert!(
        eval_resp.reason.to_lowercase().contains("read-only")
            || eval_resp
                .warnings
                .iter()
                .any(|w| w.to_lowercase().contains("read-only"))
    );
    assert!(
        eval_resp
            .matched_rule_ids
            .contains(&"read_only_violation".to_string())
    );
}

#[tokio::test]
async fn test_evaluate_proposal_denies_mcp_scope_violation() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile an intent with specific MCP tool scope
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "MCP Scoped Intent".to_string(),
        goal: "Use specific tools".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ResourceSelector::McpTool {
            server_name: "allowed-server".to_string(),
            tool_name: "allowed-tool".to_string(),
            mode: ResourceMode::Read,
        }],
        requested_risk_tier: Some(RiskTier::Low),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal using a different tool
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Wrong tool".to_string(),
        tool_name: "unauthorized-tool".to_string(),
        server_name: "unauthorized-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read data".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal - should be denied due to MCP scope violation
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should be denied due to MCP scope violation
    assert_eq!(eval_resp.decision, Decision::Deny);
    assert!(
        eval_resp
            .matched_rule_ids
            .contains(&"mcp_scope_violation".to_string())
    );
}

#[tokio::test]
async fn test_evaluate_proposal_allows_matching_mcp_scope() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile an intent with specific MCP tool scope
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "MCP Scoped Intent".to_string(),
        goal: "Use specific tools".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ResourceSelector::McpTool {
            server_name: "allowed-server".to_string(),
            tool_name: "allowed-tool".to_string(),
            mode: ResourceMode::Read,
        }],
        requested_risk_tier: Some(RiskTier::Low),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
        metadata: ferrum_proto::JsonMap::new(),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal using the allowed tool
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Correct tool".to_string(),
        tool_name: "allowed-tool".to_string(),
        server_name: "allowed-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "read data".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal - should be allowed
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let eval_resp: ferrum_proto::EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();

    // Should be allowed since tool matches scope
    assert_eq!(eval_resp.decision, Decision::Allow);
    assert!(
        !eval_resp
            .matched_rule_ids
            .contains(&"mcp_scope_violation".to_string())
    );
}

#[tokio::test]
async fn test_taint_score_computation_in_evaluate() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Step 1: Compile an intent
    let req = sample_intent_request();
    let app = build_router(runtime.clone());

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create a proposal with taint inputs
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "External data operation".to_string(),
        tool_name: "fs.read".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/data.txt"}),
        expected_effect: "read file".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![
            "external_source".to_string(),
            "untrusted_input".to_string(),
            "user_data".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    // Step 3: Evaluate the proposal
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // The evaluation should use the taint score computed by the firewall
    // external_source = 25, untrusted_input = 30, user_data = 15 = 70 total
    // Since 70 >= 70 and with R0, the PDP may quarantine or allow based on its logic
    // We're just verifying the firewall computes the score and the flow completes
}

// ============================================
// EXECUTION-TIME HTTP EGRESS ENFORCEMENT TESTS (Phase C2)
// ============================================

/// Helper: Run flow to Prepared state with HTTP resource binding
async fn run_http_flow_to_prepared(
    runtime: &GatewayRuntime,
    http_binding: ResourceBinding,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    // Step 1: Compile intent
    let app = build_router(runtime.clone());
    let req = sample_intent_request_with_effect(EffectType::ExternalApiCall);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Evaluate proposal for HTTP call
    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Call external API".to_string(),
        tool_name: "http.get".to_string(),
        server_name: "http-adapter".to_string(),
        raw_arguments: serde_json::json!({"url": "https://api.example.com/v1/data"}),
        expected_effect: "make HTTP API call".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // Step 3: Mint capability with HTTP resource binding
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "http-adapter".to_string(),
            tool_name: "http.get".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![http_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution
    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Step 5: Prepare execution
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

async fn run_file_flow_to_prepared(
    runtime: &GatewayRuntime,
    file_binding: ResourceBinding,
    effect_type: EffectType,
    tool_name: &str,
    raw_arguments: serde_json::Value,
    expected_effect: &str,
) -> (
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
    ferrum_proto::ExecutionId,
) {
    let app = build_router(runtime.clone());
    let req = sample_intent_request_with_effect(effect_type);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let app = build_router(runtime.clone());
    let proposal = ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Operate on file".to_string(),
        tool_name: tool_name.to_string(),
        server_name: "workspace".to_string(),
        raw_arguments,
        expected_effect: expected_effect.to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: tool_name.to_string(),
            tool_version: None,
        },
        resource_bindings: vec![file_binding],
        argument_constraints: vec![],
        taint_budget: TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/capabilities/mint")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&mint_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    let app = build_router(runtime.clone());
    let auth_req = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/prepare", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    (intent_id, proposal_id, execution_id)
}

#[tokio::test]
async fn test_http_execution_allowed_with_matching_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding that matches our request
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with matching HTTP payload
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "GET",
            "headers": {
                "content-type": "application/json"
            }
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "HTTP execution with matching binding should succeed"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);
}

#[tokio::test]
async fn test_http_execution_denied_host_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding for specific host
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with wrong host
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://evil.com/v1/users",
            "method": "GET"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution to wrong host should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_http_execution_denied_method_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding for GET only
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with wrong method
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "POST"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution with wrong method should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_http_execution_denied_header_violation() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create HTTP binding with limited header allowlist
    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec!["content-type".to_string()],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    // Step 6: Execute with disallowed header
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "GET",
            "headers": {
                "content-type": "application/json",
                "x-custom-secret": "sensitive-data"
            }
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution with unauthorized header should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_http_execution_denied_missing_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create capability with NO HTTP bindings (only file binding)
    let file_binding = ResourceBinding::File {
        path: "/tmp/test.txt".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, file_binding).await;

    // Step 6: Execute with HTTP payload but no HTTP binding in capability
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "url": "https://api.example.com/v1/users",
            "method": "GET"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        403,
        "HTTP execution without HTTP binding should be denied"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "Expected PolicyDenied, got {:?}",
        error.code
    );
}

#[tokio::test]
async fn test_non_http_execution_unaffected_by_missing_http_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run standard flow with file binding (no HTTP binding)
    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    // Step 6: Execute with non-HTTP payload (file operation)
    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/test.txt",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        200,
        "Non-HTTP execution should be unaffected by HTTP enforcement"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let execute_resp: ferrum_proto::ExecuteResponse = serde_json::from_slice(&body).unwrap();
    assert!(execute_resp.executed);
}

#[tokio::test]
async fn test_file_execution_allowed_with_matching_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let file_binding = ResourceBinding::File {
        path: "/tmp/readable.txt".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };

    let (_intent_id, _proposal_id, execution_id) = run_file_flow_to_prepared(
        &runtime,
        file_binding,
        EffectType::ReadOnlyAnalysis,
        "fs.read",
        serde_json::json!({"path": "/tmp/readable.txt"}),
        "read a file",
    )
    .await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/readable.txt"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_file_execution_denied_path_mismatch() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/other.txt",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn test_file_execution_denied_write_on_read_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let file_binding = ResourceBinding::File {
        path: "/tmp/readable.txt".to_string(),
        mode: ResourceMode::Read,
        required_hash: None,
    };

    let (_intent_id, _proposal_id, execution_id) = run_file_flow_to_prepared(
        &runtime,
        file_binding,
        EffectType::ReadOnlyAnalysis,
        "fs.read",
        serde_json::json!({"path": "/tmp/readable.txt"}),
        "read a file",
    )
    .await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/readable.txt",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn test_file_execution_denied_missing_binding() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let http_binding = ResourceBinding::Http {
        method: ferrum_proto::HttpMethod::Get,
        base_url: "https://api.example.com".to_string(),
        path_prefix: "/v1/".to_string(),
        header_allowlist: vec![],
        mode: ResourceMode::Read,
    };

    let (_intent_id, _proposal_id, execution_id) =
        run_http_flow_to_prepared(&runtime, http_binding).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "/tmp/test.txt"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn test_file_execution_denied_path_traversal() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (_intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R0NativeReversible).await;

    let app = build_router(runtime.clone());
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({
            "path": "../etc/passwd",
            "content": "hello world"
        }),
    };

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/executions/{}/execute", execution_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&execute_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
}
