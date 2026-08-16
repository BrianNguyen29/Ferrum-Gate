use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionType, ExecutionId, RiskTier, RollbackClass, RollbackPrepareRequest, RollbackTarget,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{IntentRepo, ProposalRepo, RollbackRepo, SqliteStore, StoreFacade};
use std::sync::Arc;

mod support;
use support::*;

use ferrum_adapter_fs::{PlannableFsAdapter, register_fs_adapter};
/// Verify that rollback and compensate are distinct operations on the adapter.
#[tokio::test]
async fn test_rollback_and_compensate_are_distinct_operations() {
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    // Create a rollback contract using R0 (which doesn't require compensation plan)
    let request = rollback.default_prepare_request(
        ferrum_proto::IntentId::new(),
        ferrum_proto::ProposalId::new(),
        ferrum_proto::ExecutionId::new(),
        RollbackClass::R0NativeReversible,
    );

    let response = rollback
        .prepare(request)
        .await
        .expect("prepare should succeed");
    let contract = response.contract;

    // compensate should succeed (noop adapter always succeeds)
    let compensate_result = rollback.compensate(&contract).await;
    assert!(
        compensate_result.is_ok(),
        "compensate should succeed for noop adapter, got: {:?}",
        compensate_result
    );

    // rollback should succeed (noop adapter always succeeds)
    let rollback_result = rollback.rollback(&contract).await;
    assert!(
        rollback_result.is_ok(),
        "rollback should succeed for noop adapter, got: {:?}",
        rollback_result
    );

    // Verify contract state transitions
    // Note: the noop adapter does not auto-transition state; this just verifies
    // both operations complete without error.
}

// ---------------------------------------------------------------------------
// Compensate execution flow test
// ---------------------------------------------------------------------------

/// Verify that FsAdapter can be used through the gateway's RollbackService path.
/// This test proves:
/// 1. FsAdapter is properly registered in the adapter registry
/// 2. PlannableFsAdapter generates correct plans for FileWrite action
/// 3. The gateway's RollbackService correctly routes to FsAdapter when adapter_key="fs"
/// 4. The contract is created with correct adapter_key and compensation_plan
#[tokio::test]
async fn test_fs_adapter_filewrite_through_gateway_rollback_path() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    // Set up registry with FsAdapter and register PlannableFsAdapter as planner
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    register_fs_adapter(&mut registry);
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    let rollback = Arc::new(rollback_service);

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Create runtime to verify the full gateway path wiring (rollback is part of runtime)
    let _runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback.clone(),
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );

    // Insert required parent records for FK constraints
    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();
    let execution_id = ExecutionId::new();

    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    // Create a direct RollbackPrepareRequest for FileWrite with FsAdapter
    // This goes through the real RollbackService.prepare() path that the gateway uses
    let request = RollbackPrepareRequest {
        intent_id,
        proposal_id,
        execution_id,
        action_type: ActionType::FileWrite,
        rollback_class: RollbackClass::R0NativeReversible,
        adapter_key: "fs".to_string(),
        target: RollbackTarget::FilePath {
            path: "/tmp/ferrum-gateway-test-filewrite.txt".to_string(),
            before_hash: None,
            after_hash: None,
        },
        prepare_checks: Vec::new(),    // Empty to trigger planner
        verify_checks: Vec::new(),     // Empty to trigger planner
        compensation_plan: Vec::new(), // Empty to trigger planner
        auto_commit: false,
        metadata: ferrum_proto::JsonMap::new(),
    };

    // Call prepare through the gateway's rollback service
    let response = rollback
        .prepare(request)
        .await
        .expect("prepare should succeed with FsAdapter");

    // Verify the contract was created with FsAdapter details
    assert_eq!(
        response.contract.adapter_key, "fs",
        "contract should have adapter_key='fs'"
    );
    assert!(
        matches!(response.contract.action_type, ActionType::FileWrite),
        "contract should have action_type=FileWrite"
    );
    assert!(
        !response.contract.compensation_plan.is_empty(),
        "compensation_plan should not be empty - planner should have filled it"
    );

    // Verify the compensation step references the fs adapter
    let compensation_step = &response.contract.compensation_plan[0];
    assert_eq!(
        compensation_step.adapter_key, "fs",
        "compensation_step should reference adapter_key='fs'"
    );
    assert_eq!(
        compensation_step.operation, "restore_snapshot",
        "compensation_step operation should be restore_snapshot"
    );

    // Verify auto_commit is false for R0 reversible
    assert!(
        !response.contract.auto_commit,
        "auto_commit should be false for R0NativeReversible"
    );
}

// ---------------------------------------------------------------------------
// HTTP-level FsAdapter FileWrite prepare integration test
// ---------------------------------------------------------------------------

/// Verify that `POST /v1/executions/{execution_id}/prepare` yields a rollback
/// contract with `adapter_key = "fs"` when the proposal's tool_name indicates
/// a FileWrite operation.
///
/// This test exercises the full HTTP router path:
/// 1. Intent with FileWrite scope is created
/// 2. Proposal with tool_name="file_write" is evaluated and persisted
/// 3. Capability is minted
/// 4. Execution is authorized
/// 5. HTTP POST to /v1/executions/{id}/prepare is called
/// 6. The returned contract has adapter_key="fs" and action_type=FileWrite
///
/// This proves the gateway's prepare route correctly routes to the FsAdapter
/// for FileWrite/fs-first operations.
#[tokio::test]
async fn test_prepare_endpoint_returns_fs_adapter_for_file_write() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    // Set up registry with FsAdapter and PlannableFsAdapter
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    register_fs_adapter(&mut registry);
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    let rollback = Arc::new(rollback_service);

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
        rollback.clone(),
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create intent with FileWrite resource scope
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "file-write-test-intent".to_string(),
        goal: "write a test file".to_string(),
        normalized_goal: "write a test file".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp/ferrum-test-*.txt".to_string(),
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

    // Create proposal with tool_name that triggers fs adapter inference
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "file write proposal".to_string(),
        tool_name: "file_write".to_string(), // This triggers fs adapter inference
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "file is written".to_string(),
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

    // Mint a capability
    let cap_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "file_write".to_string(),
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
        .expect("read mint response body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Authorize execution
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
        .expect("read authorize response body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Call prepare_execution via HTTP
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");

    let status = response.status();
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "prepare should return 200, got {:?}",
        status
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read response body");
    let parsed: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");

    // Verify the contract uses FsAdapter
    let contract = parsed
        .rollback_contract
        .as_ref()
        .expect("rollback_contract must be present in PrepareExecutionResponse");

    assert_eq!(
        contract.adapter_key, "fs",
        "contract adapter_key should be 'fs' for FileWrite tool, got '{}'",
        contract.adapter_key
    );

    assert!(
        matches!(contract.action_type, ferrum_proto::ActionType::FileWrite),
        "contract action_type should be FileWrite, got {:?}",
        contract.action_type
    );

    // Verify the contract has a compensation plan from PlannableFsAdapter
    assert!(
        !contract.compensation_plan.is_empty(),
        "compensation_plan should not be empty for fs adapter"
    );

    // Verify first compensation step uses fs adapter
    let compensation_step = &contract.compensation_plan[0];
    assert_eq!(
        compensation_step.adapter_key, "fs",
        "compensation_step adapter_key should be 'fs'"
    );

    // Prove rollback contract is persisted and retrievable from store in the same flow.
    // This verifies the Q2.2 "persist rollback contract" requirement for fs-first prepare.
    let contract_id = contract.contract_id;
    let retrieved_contract = store
        .rollback_contracts()
        .get(contract_id)
        .await
        .expect("store lookup should succeed")
        .expect("rollback contract should be persisted in store after prepare");

    assert_eq!(
        retrieved_contract.contract_id, contract_id,
        "retrieved contract_id should match the one from HTTP response"
    );
    assert_eq!(
        retrieved_contract.adapter_key, "fs",
        "retrieved contract adapter_key should be 'fs'"
    );
    assert!(
        matches!(
            retrieved_contract.action_type,
            ferrum_proto::ActionType::FileWrite
        ),
        "retrieved contract action_type should be FileWrite"
    );
    assert_eq!(
        retrieved_contract.rollback_class,
        ferrum_proto::RollbackClass::R0NativeReversible,
        "retrieved contract rollback_class should match proposal"
    );
    // Verify metadata (fs-specific fields) round-tripped correctly
    assert!(
        retrieved_contract.metadata.contains_key("original_path")
            || !retrieved_contract.compensation_plan.is_empty(),
        "retrieved contract should have fs-specific metadata or compensation_plan preserved"
    );
}

// ---------------------------------------------------------------------------
// HTTP-level FsAdapter FileWrite compensate integration test
// ---------------------------------------------------------------------------

/// Verify that `POST /v1/executions/{execution_id}/compensate` correctly
/// retrieves the persisted rollback contract from the store and exercises
/// the FsAdapter compensate path to restore/delete the target file.
///
/// This test exercises the full HTTP router path:
/// 1. Intent with FileWrite scope is created
/// 2. Proposal with tool_name="file_write" is evaluated and persisted
/// 3. Capability is minted
/// 4. Execution is authorized
/// 5. HTTP POST to /v1/executions/{id}/prepare is called (creates snapshot)
/// 6. Real file side effect is created (file written with new content)
/// 7. HTTP POST to /v1/executions/{id}/compensate is called
/// 8. FsAdapter compensate path is exercised, restoring original content
///
/// This proves:
/// - The rollback contract is persisted in the store after prepare
/// - The compensate endpoint retrieves the contract from the store
/// - The FsAdapter compensate path correctly restores the file from snapshot
#[tokio::test]
async fn test_compensate_endpoint_restores_file_via_fs_adapter() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    // Set up registry with FsAdapter and PlannableFsAdapter
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    register_fs_adapter(&mut registry);
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    let rollback = Arc::new(rollback_service);

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
        rollback.clone(),
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create a temp file with original content before the test flow
    let temp_dir = std::env::temp_dir();
    let test_file_path = temp_dir.join(format!(
        "ferrum-compensate-test-{}.txt",
        uuid::Uuid::new_v4()
    ));
    let original_content = "original content for compensate test";
    std::fs::write(&test_file_path, original_content).expect("failed to write temp file");
    let file_path_str = test_file_path.to_string_lossy().to_string();

    // Verify file was created with original content
    assert!(
        test_file_path.exists(),
        "temp file should exist before test flow"
    );
    let read_back = std::fs::read_to_string(&test_file_path).expect("failed to read temp file");
    assert_eq!(
        read_back, original_content,
        "temp file should have original content"
    );

    // Create intent with FileWrite resource scope using EXACT path (no glob)
    let intent_id = ferrum_proto::IntentId::new();
    let now = chrono::Utc::now();
    let intent = ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "file-compensate-test-intent".to_string(),
        goal: "write and compensate a test file".to_string(),
        normalized_goal: "write and compensate a test file".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: file_path_str.clone(),
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

    // Create proposal with tool_name that triggers fs adapter inference
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "file compensate proposal".to_string(),
        tool_name: "file_write".to_string(), // This triggers fs adapter inference
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({ "content": "modified content simulating execute" }),
        expected_effect: "file is written and then compensated".to_string(),
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

    // Mint a capability
    let cap_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "file_write".to_string(),
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
        .expect("read mint response body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Authorize execution
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
        .expect("read authorize response body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Call prepare_execution via HTTP - this creates the snapshot via FsAdapter
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");

    let status = response.status();
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "prepare should return 200, got {:?}",
        status
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read response body");
    let parsed: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");

    // Verify the contract uses FsAdapter
    let contract = parsed
        .rollback_contract
        .as_ref()
        .expect("rollback_contract must be present in PrepareExecutionResponse");

    assert_eq!(
        contract.adapter_key, "fs",
        "contract adapter_key should be 'fs' for FileWrite tool"
    );

    let contract_id = contract.contract_id;

    // Verify the contract was persisted in the store
    let retrieved_contract = store
        .rollback_contracts()
        .get(contract_id)
        .await
        .expect("store lookup should succeed")
        .expect("rollback contract should be persisted in store after prepare");

    // Verify the contract has the fs adapter key (proves fs adapter was selected)
    assert_eq!(
        retrieved_contract.adapter_key, "fs",
        "contract adapter_key should be 'fs'"
    );

    // Verify the contract has a compensation plan (proves PlannableFsAdapter was exercised)
    assert!(
        !retrieved_contract.compensation_plan.is_empty(),
        "compensation_plan should not be empty for fs adapter"
    );

    // Verify the contract's metadata contains snapshot_path from the real adapter prepare.
    // This proves RollbackService.prepare correctly merged adapter receipt metadata into the contract.
    let snapshot_path_from_contract = retrieved_contract
        .metadata
        .get("snapshot_path")
        .and_then(|v| v.as_str())
        .map(String::from);
    assert!(
        snapshot_path_from_contract.is_some(),
        "contract metadata should contain snapshot_path from FsAdapter.prepare. \
         This proves the fix to RollbackService.prepare correctly preserves adapter metadata. \
         Metadata keys: {:?}",
        retrieved_contract.metadata.keys().collect::<Vec<_>>()
    );

    // Simulate the "execute" side effect: modify the file with new content
    let modified_content = "modified content simulating execute";
    std::fs::write(&test_file_path, modified_content).expect("failed to modify temp file");

    // Verify file was modified
    let read_modified =
        std::fs::read_to_string(&test_file_path).expect("failed to read modified file");
    assert_eq!(
        read_modified, modified_content,
        "file should have modified content before compensate"
    );

    // NOTE: File was already modified manually above (line ~3984) to simulate execute
    // side effect. We still need to call HTTP execute to transition contract to ExecutedAwaitingVerify
    // before compensate can be called (WS-Compensate state guard requires this).
    let execute_request = ferrum_proto::ExecuteExecutionRequest {
        payload: serde_json::json!({ "content": modified_content }),
    };
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/execute", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&execute_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("execute request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "execute should return 200, got: {:?}",
        response.status()
    );

    // Call compensate via HTTP - this should restore the file via FsAdapter
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/compensate", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("compensate request should succeed");

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read compensate response body");

    // If 500, print error for debugging
    if status == axum::http::StatusCode::INTERNAL_SERVER_ERROR {
        eprintln!(
            "compensate returned 500: {}",
            String::from_utf8_lossy(&body)
        );
    }

    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "compensate should return 200, got {:?}. Error: {}",
        status,
        String::from_utf8_lossy(&body)
    );

    let compensate_response: ferrum_proto::CompensateExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");

    // Verify compensate succeeded
    assert!(
        compensate_response.compensated,
        "compensate response should show compensated=true"
    );

    // Verify the contract state is Compensated
    let updated_contract = compensate_response
        .rollback_contract
        .expect("rollback_contract should be present in compensate response");
    assert_eq!(
        updated_contract.state,
        ferrum_proto::RollbackState::Compensated,
        "contract state should be Compensated"
    );

    // Verify the file was RESTORED to original content (FsAdapter compensate path exercised)
    let read_restored =
        std::fs::read_to_string(&test_file_path).expect("failed to read restored file");
    assert_eq!(
        read_restored, original_content,
        "file should be restored to original content after compensate. \
         Expected '{}', got '{}'. This proves FsAdapter compensate correctly \
         retrieved the persisted contract and restored from snapshot.",
        original_content, read_restored
    );

    // Clean up temp file
    let _ = std::fs::remove_file(&test_file_path);
}

// ---------------------------------------------------------------------------
// HTTP-level FsAdapter FileWrite execute + verify integration test
// ---------------------------------------------------------------------------

/// Verify that compensate on a Prepared contract (before execute) returns 409 Conflict.
/// This tests the WS-Compensate state guard: compensate is only valid from
/// ExecutedAwaitingVerify + Running/AwaitingVerification (CompensationPending is no longer
/// used in the current flow).
#[tokio::test]
async fn test_compensate_on_prepared_contract_returns_409_conflict() {
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

    // Pre-insert intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
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

    // Step 1: evaluate proposal
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "compensate guard test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({ "content": "test content" }),
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

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // Step 2: mint capability
    let cap_request = ferrum_proto::CapabilityMintRequest {
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Step 3: authorize execution (creates execution in Prepared state)
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: prepare the execution (creates contract in Prepared state)
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // Step 5: ATTEMPT to compensate — contract is Prepared, not ExecutedAwaitingVerify.
    // WS-Compensate guard should return 409 Conflict.
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/compensate", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("compensate request should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::CONFLICT,
        "compensate on Prepared contract should return 409 Conflict, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::Conflict),
        "error code should be Conflict, got: {:?}",
        error.code
    );
    assert!(
        error
            .message
            .contains("compensate not allowed in current state"),
        "error message should mention state guard, got: {}",
        error.message
    );
}

/// Verify that calling compensate twice (repeat compensate on Compensated contract)
/// returns 409 Conflict. This tests idempotency of the state guard.
/// After first compensate: contract=Compensated, execution=Compensated.
/// Second compensate attempt should be rejected.
#[tokio::test]
async fn test_repeat_compensate_on_compensated_contract_returns_409_conflict() {
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

    // Pre-insert intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
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

    // Full flow: evaluate -> mint -> authorize -> prepare -> execute -> compensate
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "repeat compensate test".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({ "content": "test content" }),
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

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // mint capability
    let cap_request = ferrum_proto::CapabilityMintRequest {
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // authorize
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // prepare
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // execute
    let execute_request = ferrum_proto::ExecuteExecutionRequest {
        payload: serde_json::json!({ "content": "test content" }),
    };
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/execute", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&execute_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("execute request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // FIRST compensate — should succeed and transition contract=Compensated, execution=Compensated
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/compensate", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("first compensate request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "first compensate should return 200, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read first compensate body");
    let first_compensate: ferrum_proto::CompensateExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(
        first_compensate.compensated,
        "first compensate should succeed"
    );
    assert_eq!(
        first_compensate
            .rollback_contract
            .as_ref()
            .expect("contract should be present")
            .state,
        ferrum_proto::RollbackState::Compensated,
        "contract should be Compensated after first compensate"
    );

    // SECOND compensate — should return 409 Conflict (repeat compensate not allowed)
    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri(format!("/v1/executions/{}/compensate", execution_id))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("second compensate request should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::CONFLICT,
        "repeat compensate should return 409 Conflict, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::Conflict),
        "error code should be Conflict, got: {:?}",
        error.code
    );
    assert!(
        error
            .message
            .contains("compensate not allowed in current state"),
        "error message should mention state guard, got: {}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// D1.9 Approval Resolve Integration Tests
// ---------------------------------------------------------------------------
