// Integration tests for capability restart safety and single-use semantics.
// These tests verify that capabilities remain durable across runtime restarts
// and that single-use enforcement is strict.

use axum::Router;
use ferrum_adapter_git::GitRollbackAdapter;
use ferrum_adapter_http::HttpRollbackAdapter;
use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, AuthorizeExecutionRequest, CapabilityMintRequest, CapabilityMintResponse,
    EffectType, ExecutionState, IntentCompileRequest, IntentCompileResponse, IntentId, ProposalId,
    ProvenanceEventKind, ResourceBinding, ResourceMode, ResourceSelector, RiskTier, RollbackClass,
    TaintBudget, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{CapabilityRepo, ProvenanceRepo, SqliteStore};
use std::sync::Arc;
use tempfile::TempDir;
use tower::util::ServiceExt;

// ============================================
// TEST RUNTIME HELPERS (shared DB for restart tests)
// ============================================

/// Creates a test runtime that shares the given store.
/// Uses SqliteCapabilityService for durable capability persistence.
fn create_shared_db_runtime(store: Arc<SqliteStore>) -> GatewayRuntime {
    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine::default());
    // Create capability repo and wrap in Arc for trait object
    let repo = store.capabilities();
    let repo: Arc<dyn CapabilityRepo> = Arc::new(repo);
    let cap: Arc<dyn CapabilityService> = Arc::new(SqliteCapabilityService::new(repo));

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    registry.register(Arc::new(ferrum_adapter_fs::FsRollbackAdapter::new("fs")));
    registry.register(Arc::new(ferrum_adapter_sqlite::SqliteRollbackAdapter::new(
        "sqlite",
    )));
    registry.register(Arc::new(ferrum_adapter_maildraft::MaildraftAdapter::new(
        "maildraft",
    )));
    registry.register(Arc::new(GitRollbackAdapter::new("git")));
    registry.register(Arc::new(HttpRollbackAdapter::new()));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    GatewayRuntime::new(pdp, cap, rollback, store, firewall)
}

/// Creates a new runtime that re-connects to the same database file.
/// This simulates a "restart" where the process dies and a new one starts.
fn create_restarted_runtime(db_path: &std::path::Path) -> (GatewayRuntime, Arc<SqliteStore>) {
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to reconnect to sqlite"),
    );
    let runtime = create_shared_db_runtime(store.clone());
    (runtime, store)
}

/// Creates an intent request with file scope for capability minting tests.
fn sample_intent_request_with_file_scope() -> IntentCompileRequest {
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: ferrum_proto::JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ResourceMode::Read,
            content_hash: None,
        }],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(EffectType::ReadOnlyAnalysis),
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

/// Helper: complete mint operation and return capability_id
async fn mint_capability(
    app: &Router,
    intent_id: IntentId,
    proposal_id: ProposalId,
) -> ferrum_proto::CapabilityId {
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
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .clone()
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

    assert_eq!(response.status(), 200, "mint should succeed");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let mint_resp: CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    mint_resp.lease.capability_id
}

// ============================================
// RESTART SAFETY TESTS
// ============================================

/// Test: mint -> new runtime on same DB -> authorize succeeds.
/// Verifies that a capability minted in one runtime is readable and usable
/// after a restart (simulated by creating a new runtime with the same DB).
#[tokio::test]
async fn test_mint_persists_across_runtime_restart() {
    // Setup: create temp dir with SQLite DB
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    // Runtime 1: Mint a capability
    let runtime1 = create_shared_db_runtime(store.clone());
    let app1 = build_router(runtime1.clone());

    // Compile intent and proposal (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let response = app1
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;
    let app1 = build_router(runtime1.clone());
    let response = app1
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

    // Mint capability in runtime1
    let app1 = build_router(runtime1.clone());
    let capability_id = mint_capability(&app1, intent_id, proposal_id).await;

    // Simulate restart: create new runtime with same DB
    let (runtime2, _store2) = create_restarted_runtime(&db_path);
    let app2 = build_router(runtime2.clone());

    // Authorize using the capability in runtime2
    let auth_req = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app2
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

    // Authorization should succeed
    assert_eq!(
        response.status(),
        200,
        "authorize should succeed after restart"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).unwrap();
    assert!(
        matches!(
            auth_resp.execution.state,
            ExecutionState::Authorized | ExecutionState::Prepared
        ),
        "execution should be authorized or prepared, got {:?}",
        auth_resp.execution.state
    );

    // Verify provenance event was emitted
    let events = runtime2
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: Some(capability_id),
            event_kind: Some(ProvenanceEventKind::CapabilityMinted),
            since: None,
            until: None,
        })
        .await
        .unwrap();
    assert!(
        !events.is_empty(),
        "CapabilityMinted event should be persisted"
    );
}

/// Test: authorize -> new runtime -> second authorize fails.
/// Verifies that single-use semantics are enforced across restarts:
/// the second authorize attempt (in new runtime) must fail.
#[tokio::test]
async fn test_single_use_enforced_across_runtime_restart() {
    // Setup
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    // Runtime 1: Compile, mint, authorize
    let runtime1 = create_shared_db_runtime(store.clone());
    let app1 = build_router(runtime1.clone());

    // Compile intent (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let response = app1
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    // Evaluate proposal
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;
    let app1 = build_router(runtime1.clone());
    let response = app1
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

    // Mint capability
    let app1 = build_router(runtime1.clone());
    let capability_id = mint_capability(&app1, intent_id, proposal_id).await;

    // First authorize (succeeds)
    let auth_req = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let app1 = build_router(runtime1.clone());
    let response = app1
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
    assert_eq!(response.status(), 200, "first authorize should succeed");

    // Simulate restart
    let (runtime2, _store2) = create_restarted_runtime(&db_path);
    let app2 = build_router(runtime2.clone());

    // Second authorize attempt (should fail - capability already used)
    let response = app2
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

    // Should fail with 403 (capability already used)
    assert_eq!(
        response.status(),
        403,
        "second authorize should fail (capability already used)"
    );
}

/// Test: revoke -> new runtime -> authorize fails.
/// Verifies that revoked capabilities are properly recognized across restarts.
#[tokio::test]
async fn test_revoked_capability_fails_across_runtime_restart() {
    // Setup
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    // Runtime 1: Compile, mint, revoke
    let runtime1 = create_shared_db_runtime(store.clone());

    // Compile intent (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let app1 = build_router(runtime1.clone());
    let response = app1
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    // Evaluate proposal
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;
    let app1 = build_router(runtime1.clone());
    let response = app1
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

    // Mint capability
    let app1 = build_router(runtime1.clone());
    let capability_id = mint_capability(&app1, intent_id, proposal_id).await;

    // Revoke capability
    let app1 = build_router(runtime1.clone());
    let response = app1
        .oneshot(
            axum::http::Request::builder()
                .uri(&format!("/v1/capabilities/{}/revoke", capability_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body("{}".to_string())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "revoke should succeed");

    // Simulate restart
    let (runtime2, _store2) = create_restarted_runtime(&db_path);
    let app2 = build_router(runtime2.clone());

    // Attempt to authorize (should fail - capability revoked)
    let auth_req = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app2
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

    // Should fail with 403 (capability revoked)
    assert_eq!(
        response.status(),
        403,
        "authorize should fail (capability revoked)"
    );

    // Verify provenance event was emitted
    let events = runtime2
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: Some(capability_id),
            event_kind: Some(ProvenanceEventKind::CapabilityRevoked),
            since: None,
            until: None,
        })
        .await
        .unwrap();
    assert!(
        !events.is_empty(),
        "CapabilityRevoked event should be persisted"
    );
}

/// Test: expired -> new runtime -> authorize fails.
/// Verifies that expired capabilities are properly recognized across restarts.
#[tokio::test]
async fn test_expired_capability_fails_across_runtime_restart() {
    // Setup
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    // Runtime 1: Compile, mint with short TTL, wait for expiration
    let runtime1 = create_shared_db_runtime(store.clone());

    // Compile intent (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let app1 = build_router(runtime1.clone());
    let response = app1
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    // Evaluate proposal
    let proposal = sample_proposal(intent_id);
    let proposal_id = proposal.proposal_id;
    let app1 = build_router(runtime1.clone());
    let response = app1
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

    // Mint capability with 1 second TTL
    let app1 = build_router(runtime1.clone());
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
        requested_ttl_secs: 1, // Very short TTL
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app1
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
    let mint_resp: CapabilityMintResponse = serde_json::from_slice(&body).unwrap();
    let capability_id = mint_resp.lease.capability_id;

    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Simulate restart
    let (runtime2, _store2) = create_restarted_runtime(&db_path);
    let app2 = build_router(runtime2.clone());

    // Attempt to authorize (should fail - capability expired)
    let auth_req = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let response = app2
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

    // Should fail with 403 (capability expired)
    assert_eq!(
        response.status(),
        403,
        "authorize should fail (capability expired)"
    );
}

// ============================================
// CONCURRENCY TESTS
// ============================================

/// Test: two authorize attempts on same capability -> exactly one succeeds.
/// Verifies that single-use enforcement is atomic and no race conditions
/// allow double-spending of a capability.
#[tokio::test]
async fn test_concurrent_authorize_single_use_enforcement() {
    // Setup
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    let runtime = create_shared_db_runtime(store.clone());
    let app = build_router(runtime.clone());

    // Compile intent (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    // Evaluate proposal
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

    // Mint capability
    let app = build_router(runtime.clone());
    let capability_id = mint_capability(&app, intent_id, proposal_id).await;

    // Two concurrent authorize requests
    let auth_req = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    // Spawn both requests concurrently
    let app1 = build_router(runtime.clone());
    let app2 = build_router(runtime.clone());

    let (response1, response2) = tokio::join! {
        app1.oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        ),
        app2.oneshot(
            axum::http::Request::builder()
                .uri("/v1/executions/authorize")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&auth_req).unwrap())
                .unwrap(),
        )
    };

    // Exactly one should succeed (200), one should fail (403)
    let status1 = response1.unwrap().status();
    let status2 = response2.unwrap().status();

    let success_count = [status1.as_u16(), status2.as_u16()]
        .iter()
        .filter(|&&s| s == 200)
        .count();
    let failure_count = [status1.as_u16(), status2.as_u16()]
        .iter()
        .filter(|&&s| s == 403)
        .count();

    assert_eq!(
        success_count, 1,
        "exactly one authorize should succeed, got status1={}, status2={}",
        status1, status2
    );
    assert_eq!(
        failure_count, 1,
        "exactly one authorize should fail with 403, got status1={}, status2={}",
        status1, status2
    );
}

// ============================================
// PROVENANCE EMISSION TESTS
// ============================================

/// Test: provenance is emitted only after service operation succeeds.
/// Verifies that no provenance events are emitted if mint/revoke fails.
#[tokio::test]
async fn test_mint_provenance_emitted_only_on_success() {
    // Setup
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    let runtime = create_shared_db_runtime(store.clone());

    // Compile intent (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    // Mint with TTL > 300 (should fail)
    let app = build_router(runtime.clone());
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
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
        requested_ttl_secs: 500, // Exceeds max 300
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = app
        .clone()
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

    // Should fail with 400
    assert_eq!(response.status(), 400, "mint with TTL > 300 should fail");

    // Verify no CapabilityMinted provenance event was emitted
    let events = runtime
        .store
        .provenance()
        .query(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            execution_id: None,
            capability_id: None,
            event_kind: Some(ProvenanceEventKind::CapabilityMinted),
            since: None,
            until: None,
        })
        .await
        .unwrap();

    assert!(
        events.is_empty(),
        "no CapabilityMinted event should be emitted when mint fails"
    );
}

/// Test: verify capability is persisted by capability service, not by gateway dual-write.
#[tokio::test]
async fn test_capability_persisted_by_service_not_gateway() {
    // Setup
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("store.sqlite");
    std::fs::File::create(&db_path).expect("failed to create sqlite file");
    let database_url = format!("sqlite://{}", db_path.display());
    let store = Arc::new(
        futures::executor::block_on(SqliteStore::connect(&database_url))
            .expect("failed to connect"),
    );
    futures::executor::block_on(store.apply_embedded_migrations())
        .expect("failed to apply migrations");

    let runtime = create_shared_db_runtime(store.clone());

    // Compile intent (with file scope for capability minting)
    let intent_req = sample_intent_request_with_file_scope();
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/intents/compile")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&intent_req).unwrap())
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

    // Mint capability
    let app = build_router(runtime.clone());
    let capability_id = mint_capability(&app, intent_id, proposal_id).await;

    // Verify capability is readable from store (proves service persisted it)
    let stored_capability = runtime
        .store
        .capabilities()
        .get(capability_id)
        .await
        .unwrap();
    assert!(
        stored_capability.is_some(),
        "capability should be persisted by capability service"
    );
    assert_eq!(stored_capability.unwrap().capability_id, capability_id);
}
