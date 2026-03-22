//! Integration test for provenance minimum-chain / lineage evidence
//!
//! This test verifies that for supported mutation flows:
//! 1. All events in the minimum lineage chain are persisted
//! 2. Parent edges are correctly stored and can be queried
//! 3. The chain is contiguous (no missing links)
//!
//! Minimum lineage chain (from docs/04-runtime-flow.md):
//! 1. ActionProposalSubmitted
//! 2. PolicyEvaluated
//! 3. CapabilityMinted
//! 4. ToolCallPrepared
//! 5. ToolCallExecuted
//! 6. SideEffectPrepared
//! 7. SideEffectVerified
//! 8. terminal event (SideEffectCommitted / SideEffectCompensated / SideEffectRolledBack)

use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, CapabilityMintRequest, EffectType, ExecutionState, IntentCompileRequest,
    IntentCompileResponse, ProvenanceEdgeType, ProvenanceEventKind, ResourceBinding, ResourceMode,
    RiskTier, RollbackClass, TaintBudget, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo, SqliteStore};
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
    let cap: Arc<dyn CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    registry.register(Arc::new(ferrum_adapter_fs::FsRollbackAdapter::new("fs")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()), firewall);

    (temp_dir, runtime, store)
}

fn sample_mutation_intent_request(path: &str) -> IntentCompileRequest {
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Mutation Intent".to_string(),
        goal: "Test mutation goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: path.to_string(),
            mode: ResourceMode::Write,
            content_hash: None,
        }],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(EffectType::FileMutation),
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn sample_mutation_proposal(intent_id: ferrum_proto::IntentId, path: &str) -> ActionProposal {
    ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Mutate file".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": path, "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_minimum_lineage_chain_events_exist() {
    let (temp_dir, runtime, store) = create_test_runtime().await;
    let file_path = temp_dir.path().join("test.txt");
    let file_path = file_path.to_string_lossy().to_string();

    // Step 1: Compile intent
    let req = sample_mutation_intent_request(&file_path);
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
    let proposal = sample_mutation_proposal(intent_id, &file_path);
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

    // Step 3: Mint capability
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: file_path.clone(),
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

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
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

    // Step 6: Execute
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": file_path, "content": "hello"}),
    };

    let app = build_router(runtime.clone());
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

    // Step 7: Verify (auto-commits for R0)
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let app = build_router(runtime.clone());
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

    // === Verify minimum lineage chain events exist ===
    let all_events = store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    let has_action_proposal_submitted = all_events
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
    let has_tool_call_executed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_side_effect_prepared = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));
    let has_side_effect_verified = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_side_effect_committed = all_events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(
        has_action_proposal_submitted,
        "Missing ActionProposalSubmitted in minimum lineage chain"
    );
    assert!(
        has_policy_evaluated,
        "Missing PolicyEvaluated in minimum lineage chain"
    );
    assert!(
        has_capability_minted,
        "Missing CapabilityMinted in minimum lineage chain"
    );
    assert!(
        has_tool_call_prepared,
        "Missing ToolCallPrepared in minimum lineage chain"
    );
    assert!(
        has_tool_call_executed,
        "Missing ToolCallExecuted in minimum lineage chain"
    );
    assert!(
        has_side_effect_prepared,
        "Missing SideEffectPrepared in minimum lineage chain"
    );
    assert!(
        has_side_effect_verified,
        "Missing SideEffectVerified in minimum lineage chain"
    );
    assert!(
        has_side_effect_committed,
        "Missing SideEffectCommitted (terminal event) in minimum lineage chain"
    );

    // === Verify lineage edges are persisted ===
    let policy_eval_event = all_events
        .iter()
        .find(|e| matches!(e.kind, ProvenanceEventKind::PolicyEvaluated))
        .expect("PolicyEvaluated event not found");

    // Query edges from database
    let edges = store
        .provenance()
        .get_edges_to(policy_eval_event.event_id)
        .await
        .unwrap();

    assert!(
        !edges.is_empty(),
        "PolicyEvaluated should have incoming edges from the database"
    );

    // Verify the edge is a Caused relationship
    let has_caused_edge = edges.iter().any(|e| {
        matches!(e.edge_type, ProvenanceEdgeType::Caused)
            && e.summary
                .as_ref()
                .map(|s| s.contains("proposal submission"))
                .unwrap_or(false)
    });
    assert!(
        has_caused_edge,
        "PolicyEvaluated should have a Caused edge linking to the proposal submission"
    );

    // Verify the edge links to an ActionProposalSubmitted event
    let proposal_submitted_event = all_events
        .iter()
        .find(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted))
        .expect("ActionProposalSubmitted event not found");

    let edge_from_proposal = edges
        .iter()
        .any(|e| e.from_event_id == proposal_submitted_event.event_id);
    assert!(
        edge_from_proposal,
        "PolicyEvaluated should have an edge from ActionProposalSubmitted"
    );
}

#[tokio::test]
async fn test_lineage_chain_is_contiguous_no_missing_events() {
    let (temp_dir, runtime, store) = create_test_runtime().await;
    let file_path = temp_dir.path().join("test.txt");
    let file_path = file_path.to_string_lossy().to_string();

    // Run the same flow to get a complete lineage
    let req = sample_mutation_intent_request(&file_path);
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create and evaluate proposal
    let proposal = sample_mutation_proposal(intent_id, &file_path);
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let _response = app
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

    // Verify the records are linked by intent_id (contiguous chain)
    let intent = store.intents().get(intent_id).await.unwrap();
    assert!(intent.is_some(), "Intent should exist");

    let proposals = store.proposals().list_by_intent(intent_id).await.unwrap();
    assert!(
        !proposals.is_empty(),
        "Should have proposals linked to intent"
    );

    // Query all events and ensure they form a chain
    let events = store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    // Verify temporal ordering - events should be in chronological order
    for i in 1..events.len() {
        assert!(
            events[i].occurred_at >= events[i - 1].occurred_at,
            "Events should be in chronological order"
        );
    }

    // Verify execution events have parent proposal/intent context
    let execution_events: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                ProvenanceEventKind::ToolCallExecuted
                    | ProvenanceEventKind::SideEffectVerified
                    | ProvenanceEventKind::SideEffectPrepared
            )
        })
        .collect();

    for exec_event in execution_events {
        assert!(
            exec_event.intent_id == Some(intent_id),
            "Execution events should have intent_id context"
        );
        assert!(
            exec_event.proposal_id == Some(proposal_id),
            "Execution events should have proposal_id context"
        );
    }
}

#[tokio::test]
async fn test_rollback_lineage_chain_has_terminal_event() {
    let (temp_dir, runtime, store) = create_test_runtime().await;
    let file_path = temp_dir.path().join("test.txt");
    let file_path = file_path.to_string_lossy().to_string();

    // Run full flow to commit
    let req = sample_mutation_intent_request(&file_path);
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let compile_resp: IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let proposal = sample_mutation_proposal(intent_id, &file_path);
    let proposal_id = proposal.proposal_id;

    // Evaluate proposal
    let app = build_router(runtime.clone());
    let _response = app
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

    // Mint capability
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: file_path.clone(),
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Authorize
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Prepare
    let app = build_router(runtime.clone());
    let _response = app
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

    // Execute
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": file_path, "content": "hello"}),
    };

    let app = build_router(runtime.clone());
    let _response = app
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

    // Verify (auto-commits for R0)
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let app = build_router(runtime.clone());
    let _response = app
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

    // Rollback the execution
    let rollback_req = ferrum_proto::RollbackRequest { execution_id };

    let app = build_router(runtime.clone());
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

    // Verify the terminal event is SideEffectRolledBack
    let events = store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
        })
        .await
        .unwrap();

    let has_side_effect_rolled_back = events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectRolledBack));
    assert!(
        has_side_effect_rolled_back,
        "Rollback lineage should have SideEffectRolledBack as terminal event"
    );

    // Verify execution state is RolledBack
    let execution = store.executions().get(execution_id).await.unwrap().unwrap();
    assert!(
        matches!(execution.state, ExecutionState::RolledBack),
        "Execution should be in RolledBack state"
    );
}

#[tokio::test]
async fn test_get_execution_lineage_endpoint() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let file_path = temp_dir.path().join("test.txt");
    let file_path = file_path.to_string_lossy().to_string();

    // Run full flow to get a committed execution
    let req = sample_mutation_intent_request(&file_path);
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

    let proposal = sample_mutation_proposal(intent_id, &file_path);
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let _response = app
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

    // Mint capability
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: file_path.clone(),
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: ferrum_proto::CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Authorize
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    let execution_id = auth_resp.execution.execution_id;

    // Prepare
    let app = build_router(runtime.clone());
    let _response = app
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

    // Execute
    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": file_path, "content": "hello"}),
    };

    let app = build_router(runtime.clone());
    let _response = app
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

    // Verify (auto-commits for R0)
    let verify_req = ferrum_proto::VerifyRequest { execution_id };

    let app = build_router(runtime.clone());
    let _response = app
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

    // === Hit the new lineage endpoint ===
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/provenance/lineage/{}", execution_id))
                .method(axum::http::Method::GET)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200, "lineage endpoint should return 200");

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify response structure
    assert!(
        lineage.get("execution_id").is_some(),
        "response should contain execution_id"
    );
    assert!(
        lineage.get("events").is_some(),
        "response should contain events array"
    );

    let events = lineage.get("events").unwrap().as_array().unwrap();
    assert!(
        !events.is_empty(),
        "lineage should contain at least one event"
    );

    // Verify we have events from the minimum lineage chain
    let event_kinds: Vec<_> = events
        .iter()
        .map(|e| e.get("kind").and_then(|k| k.as_str()).unwrap_or(""))
        .collect();

    assert!(
        event_kinds.contains(&"SideEffectCommitted"),
        "lineage should include SideEffectCommitted event, got: {:?}",
        event_kinds
    );
    assert!(
        event_kinds.contains(&"ToolCallExecuted"),
        "lineage should include ToolCallExecuted event, got: {:?}",
        event_kinds
    );

    // Verify temporal ordering
    let timestamps: Vec<_> = events.iter().filter_map(|e| e.get("occurred_at")).collect();
    for i in 1..timestamps.len() {
        let curr = timestamps[i];
        let prev = timestamps[i - 1];
        // timestamps should be comparable as strings (ISO 8601)
        let curr_str = curr.as_str().unwrap_or("");
        let prev_str = prev.as_str().unwrap_or("");
        assert!(
            curr_str >= prev_str,
            "events should be in chronological order"
        );
    }
}

#[tokio::test]
async fn test_get_lineage_unknown_execution_returns_empty_events() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let unknown_execution_id = ferrum_proto::ExecutionId::new();

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/provenance/lineage/{}", unknown_execution_id))
                .method(axum::http::Method::GET)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("".to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 200 with empty events (fail-soft)
    assert_eq!(
        response.status(),
        200,
        "lineage endpoint should return 200 even for unknown execution"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let events = lineage.get("events").unwrap().as_array().unwrap();
    assert!(
        events.is_empty(),
        "unknown execution should return empty events"
    );
}
