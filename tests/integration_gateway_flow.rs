use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, CapabilityMintRequest, Decision, ExecutionState, IntentCompileRequest,
    IntentCompileResponse, IntentId, ProposalId, ProvenanceEventKind, ResourceBinding,
    ResourceMode, RiskTier, RollbackClass, TaintBudget, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{
    CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo, RollbackRepo,
    SqliteStore,
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

    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()));

    (temp_dir, runtime, store)
}

fn sample_intent_request() -> IntentCompileRequest {
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
    // Step 1: Compile intent
    let app = build_router(runtime.clone());
    let req = sample_intent_request();
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
async fn test_r3_no_auto_commit_verify_then_explicit_commit() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Run flow to prepare with R3 (auto-commit disabled)
    let (intent_id, _proposal_id, execution_id) =
        run_flow_to_prepared(&runtime, RollbackClass::R3IrreversibleHighConsequence).await;

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
        "R3 should have auto_commit = false"
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

    // Step 7: Verify (should NOT auto-commit for R3)
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

    // Verify execution state is AwaitingVerification (NOT committed for R3)
    let stored_execution = runtime.store.executions().get(execution_id).await.unwrap();
    assert!(stored_execution.is_some());
    let exec = stored_execution.unwrap();
    assert!(
        matches!(exec.state, ExecutionState::AwaitingVerification),
        "R3 should remain in AwaitingVerification after verify, got {:?}",
        exec.state
    );

    // Step 8: Explicit commit (required for R3)
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
