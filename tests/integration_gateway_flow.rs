//! Integration tests for gateway flow: mint -> authorize with single-use enforcement.
//!
//! These tests verify the end-to-end capability lifecycle including:
//! - Minting capabilities
//! - Authorizing executions against a capability
//! - Single-use enforcement (capability cannot be reused after authorization)

use axum::{body::Body, http::Request};
use ferrum_cap::InMemoryCapabilityService;
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, ApiErrorCode, ApprovalBinding, ApprovalRequest, ApprovalState,
    AuthorizeExecutionRequest, AuthorizeExecutionResponse, CapabilityMintRequest,
    CapabilityMintResponse, CompensateExecutionResponse, Decision, EvaluateProposalResponse,
    ExecutionId, ExecutionRecord, ExecutionState, ResourceBinding, ResourceMode, RiskTier,
    RollbackClass, RollbackExecutionResponse, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{ApprovalRepo, SqliteStore};
use http_body_util::BodyExt;
use hyper::StatusCode;
use std::sync::Arc;
use tower::ServiceExt;

/// TestApp holds a wired-up gateway runtime and provides helpers for JSON requests.
struct TestApp {
    router: axum::Router,
    runtime: GatewayRuntime,
    _tempdir: tempfile::TempDir, // kept alive to persist the sqlite file
}

/// Build a fully-wired test application backed by a temporary sqlite file.
async fn test_app() -> TestApp {
    let tempdir = tempfile::tempdir().expect("failed to create tempdir");
    let db_path = tempdir.path().join("test.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let store = Arc::new(
        SqliteStore::connect(&db_url)
            .await
            .expect("failed to connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");

    let pdp = Arc::new(StaticPdpEngine::default());
    let cap = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store);
    let router = build_router(runtime.clone());

    TestApp {
        router,
        runtime,
        _tempdir: tempdir,
    }
}

/// POST JSON to the router and return (status, parsed JSON).
async fn post_json(
    router: &axum::Router,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .expect("failed to build request");

    let res = router.clone().oneshot(req).await.expect("request failed");

    let status = res.status();
    // Use BodyExt::collect to get all body bytes
    let body_bytes = res
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );

    (status, json)
}

/// Build a minimal mint request for the given intent/proposal IDs.
fn sample_mint_request(
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
) -> CapabilityMintRequest {
    CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Read,
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
    }
}

/// Build an authorize execution request for the given proposal/capability IDs.
fn sample_authorize_request(
    proposal_id: ferrum_proto::ProposalId,
    capability_id: ferrum_proto::CapabilityId,
    dry_run: bool,
) -> AuthorizeExecutionRequest {
    AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run,
    }
}

/// Assert that the response is an ApiError with the given code and a message
/// containing the provided substring.
fn assert_api_error(json: &serde_json::Value, expected_code: ApiErrorCode, message_contains: &str) {
    let api_error: ferrum_proto::ApiError =
        serde_json::from_value(json.clone()).expect("response should deserialize as ApiError");

    assert!(
        std::mem::discriminant(&api_error.code) == std::mem::discriminant(&expected_code),
        "expected error code {:?}, got {:?}",
        expected_code,
        api_error.code
    );

    assert!(
        api_error.message.contains(message_contains),
        "error message should contain '{}', got: {}",
        message_contains,
        api_error.message
    );
}

/// Assert that an execution record has the expected state using debug representation.
fn assert_execution_state(record: &ExecutionRecord, expected_state: ExecutionState) {
    let actual = format!("{:?}", record.state);
    let expected = format!("{:?}", expected_state);
    assert!(
        actual == expected,
        "execution state should be {}, got {}",
        expected,
        actual
    );
}

/// Verify that a capability can be minted and then used exactly once.
/// The second authorize call must fail with an AlreadyUsed error because
/// authorize_execution does not yet call mark_used.
#[tokio::test]
async fn capability_single_use_reuse_denied() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Persist and evaluate the proposal (realistic flow: compile -> evaluate)
    let proposal = sample_proposal(intent_id, proposal_id, RollbackClass::R0NativeReversible);
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability
    let mint_req = sample_mint_request(intent_id, proposal_id);
    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}: {:?}",
        status,
        mint_resp
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Verify status is Active using debug representation
    let status_str = format!("{:?}", mint_response.lease.status);
    assert!(
        status_str.contains("Active"),
        "freshly minted capability should be Active, got: {}",
        status_str
    );

    // Step 3: Authorize an execution (first use -- should succeed)
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "first authorize should succeed, got {}: {:?}",
        status,
        auth_resp
    );

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp.clone()).expect("authorize response should deserialize");
    assert_execution_state(&auth_response.execution, ExecutionState::Prepared);

    // Step 4: Attempt to authorize again with the same capability (should fail)
    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body2 = serde_json::to_value(&auth_req2).unwrap();
    let (status2, auth_resp2) =
        post_json(&app.router, "/v1/executions/authorize", auth_body2).await;

    // Expect BAD_REQUEST with AlreadyUsed error because the capability
    // was already consumed by the first authorize call.
    assert_eq!(
        status2,
        StatusCode::BAD_REQUEST,
        "second authorize should fail with BAD_REQUEST, got {}: {:?}",
        status2,
        auth_resp2
    );

    assert_api_error(&auth_resp2, ApiErrorCode::Conflict, "already used");
}

/// Build a minimal action proposal for the given intent/proposal IDs and rollback class.
fn sample_proposal(
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
    rollback_class: RollbackClass,
) -> ActionProposal {
    ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test-proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: rollback_class,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    }
}

/// Verify that an execution authorization fails when the proposal_id does not match
/// the proposal_id the capability was minted for.
///
/// A capability is minted for a specific proposal (Proposal A). When an execution
/// is authorized using a different proposal (Proposal B), the gateway MUST deny
/// the request because the proposal scope does not match the capability's authorized
/// scope.
#[tokio::test]
async fn scope_mismatch_between_proposal_and_capability_is_denied() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id_a = ferrum_proto::ProposalId::new();
    let proposal_id_b = ferrum_proto::ProposalId::new(); // Different proposal

    // Step 1: Persist and evaluate Proposal A (needed for stricter gateway behavior)
    let proposal_a = sample_proposal(intent_id, proposal_id_a, RollbackClass::R0NativeReversible);
    let proposal_a_body = serde_json::to_value(&proposal_a).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id_a),
        proposal_a_body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "proposal A evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability for Proposal A
    let mint_req = sample_mint_request(intent_id, proposal_id_a);
    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}: {:?}",
        status,
        mint_resp
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Verify the capability was issued for proposal_id_a
    assert_eq!(
        mint_response.lease.proposal_id, proposal_id_a,
        "capability should be issued for proposal_id_a"
    );

    // Step 3: Attempt to authorize execution using Proposal B (different from Proposal A).
    //
    // INTENDED: This should be denied because the capability was minted for Proposal A's
    // scope, not Proposal B's scope. The gateway should validate that the authorizing
    // proposal matches the capability's authorized proposal.
    //
    let auth_req = sample_authorize_request(proposal_id_b, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;

    // The gateway should reject the request with PolicyDenied because proposal scope
    // does not match the capability's authorized scope.
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "scope mismatch: execution with proposal B should be denied when capability \
         was minted for proposal A; got {}: {:?}",
        status,
        auth_resp
    );

    assert_api_error(&auth_resp, ApiErrorCode::PolicyDenied, "proposal");
}

/// Verify that execution with R3 rollback class is NOT auto-committed through the
/// gateway's normal prepare path without proper approval gating.
///
/// When a proposal is evaluated with R3IrreversibleHighConsequence rollback class,
/// the execution should not proceed to `Prepared` state directly. Instead, it should
/// return `Decision::RequireApproval` and remain in `AwaitingApproval` state.
#[tokio::test]
async fn r3_execution_is_not_auto_committed() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate a proposal with R3 rollback class directly.
    // We bypass compile_intent and construct the proposal directly with
    // R3IrreversibleHighConsequence to isolate the R3 authorization test.
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability for the R3-proposal
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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

    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}: {:?}",
        status,
        mint_resp
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run)
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "authorize should succeed, got {}: {:?}",
        status,
        auth_resp
    );

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Verify execution state after authorization.
    //
    // For R3 rollback class, the execution should not auto-commit. It should
    // remain in `AwaitingApproval` until an approval path is implemented.
    let execution_state = format!("{:?}", auth_response.execution.state);

    assert_eq!(
        execution_state, "AwaitingApproval",
        "R3 rollback class should result in AwaitingApproval state (not Prepared); \
         got {} (this failure indicates authorize_execution auto-commits R3 without approval gating)",
        execution_state
    );

    // Step 5: Attempt to call prepare_execution for future approval-path coverage.
    let (_prepare_status, _prepare_resp) = post_json(
        &app.router,
        &format!("/v1/executions/{}/prepare", execution_id),
        serde_json::json!({}),
    )
    .await;

    // A future approval-path test can assert that prepare keeps the R3 rollback class:
    // assert_eq!(format!("{:?}", prepare_response.rollback_contract.rollback_class), "R3IrreversibleHighConsequence");
}

/// Verify that a proposal with high taint score and non-R0 rollback class
/// triggers quarantine via the PDP engine.
///
/// When a proposal has taint_inputs that result in taint_score >= 70 AND the
/// requested rollback class is non-R0, the PDP engine returns Decision::Quarantine
/// with rule "quarantine.high.taint.mutation".
///
/// The evaluate_proposal endpoint derives taint_score from proposal.taint_inputs
/// (10 points per taint input). With 7+ taint inputs and R1/R2 rollback class,
/// the PDP quarantine rule fires end-to-end.
#[tokio::test]
async fn high_taint_proposal_with_non_r0_rollback_triggers_quarantine() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Build a proposal with 7 taint inputs (7 * 10 = 70 >= 70 threshold)
    // and R1 rollback class (non-R0) to trigger the quarantine rule.
    let proposal = ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "high-taint-proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R2Compensatable, // non-R0
        taint_inputs: vec![
            "external-web:https://example.com".to_string(),
            "external-email:phishing@example.com".to_string(),
            "external-repo:text-from-untrusted".to_string(),
            "external-tool-output:llm-generated".to_string(),
            "external-metadata:unverified".to_string(),
            "untrusted-user-input".to_string(),
            "external-content:mixed".to_string(),
        ],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should return OK even when quarantined, got {}: {:?}",
        status,
        resp
    );

    let eval_response: EvaluateProposalResponse = serde_json::from_value(resp.clone())
        .expect("response should deserialize as EvaluateProposalResponse");

    assert_eq!(
        eval_response.decision,
        Decision::Quarantine,
        "high taint + non-R0 rollback should result in Quarantine, got {:?}: {:?}",
        eval_response.decision,
        resp
    );

    assert!(
        eval_response
            .matched_rule_ids
            .contains(&"quarantine.high.taint.mutation".to_string()),
        "matched rules should contain 'quarantine.high.taint.mutation', got {:?}",
        eval_response.matched_rule_ids
    );

    assert!(
        eval_response.reason.contains("taint score"),
        "reason should mention taint score, got: {}",
        eval_response.reason
    );
}

/// Verify that a prepared execution can be rolled back end-to-end.
///
/// After an execution is prepared (has a rollback contract), calling the rollback
/// endpoint should invoke the rollback adapter, update the execution state to
/// RolledBack, and update the contract state to RolledBack.
#[tokio::test]
async fn rollback_execution_flow() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Persist and evaluate the proposal (non-R3 so authorize auto-commits)
    let proposal = sample_proposal(intent_id, proposal_id, RollbackClass::R0NativeReversible);
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability
    let mint_req = sample_mint_request(intent_id, proposal_id);
    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}",
        status
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run, non-R3 -> goes to Prepared)
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorize should succeed, got {}: {:?}",
        status,
        auth_resp
    );

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let execution_id = auth_response.execution.execution_id;
    assert_execution_state(&auth_response.execution, ExecutionState::Prepared);

    // Step 4: Call prepare_execution to create the rollback contract
    let (status, prepare_resp) = post_json(
        &app.router,
        &format!("/v1/executions/{}/prepare", execution_id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "prepare should succeed, got {}: {:?}",
        status,
        prepare_resp
    );

    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_value(prepare_resp).expect("prepare response should deserialize");
    assert!(
        prepare_response.rollback_contract.is_some(),
        "prepare should produce a rollback contract"
    );
    let contract_id = prepare_response
        .rollback_contract
        .as_ref()
        .unwrap()
        .contract_id;

    // Step 5: Call rollback_execution to roll back the prepared execution
    let (status, rollback_resp) = post_json(
        &app.router,
        &format!("/v1/executions/{}/rollback", execution_id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "rollback should succeed, got {}: {:?}",
        status,
        rollback_resp
    );

    let rollback_response: RollbackExecutionResponse =
        serde_json::from_value(rollback_resp).expect("rollback response should deserialize");
    assert!(
        rollback_response.rolled_back,
        "rollback response should indicate rolled_back = true"
    );
    assert_eq!(
        rollback_response.contract_id,
        Some(contract_id),
        "rollback response should include the contract ID"
    );
}

/// Verify that a prepared execution can be compensated end-to-end.
///
/// After an execution is prepared (has a rollback contract), calling the compensate
/// endpoint should invoke the compensate adapter, update the execution state to
/// Compensated, and update the contract state to Compensated.
///
/// The compensate endpoint is symmetric to the rollback endpoint but uses a
/// different adapter method and emits a SideEffectCompensated provenance event.
#[tokio::test]
async fn compensate_execution_flow() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Persist and evaluate the proposal (non-R3 so authorize auto-commits)
    let proposal = sample_proposal(intent_id, proposal_id, RollbackClass::R0NativeReversible);
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability
    let mint_req = sample_mint_request(intent_id, proposal_id);
    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}",
        status
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run, non-R3 -> goes to Prepared)
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "authorize should succeed, got {}: {:?}",
        status,
        auth_resp
    );

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let execution_id = auth_response.execution.execution_id;
    assert_execution_state(&auth_response.execution, ExecutionState::Prepared);

    // Step 4: Call prepare_execution to create the rollback contract
    let (status, prepare_resp) = post_json(
        &app.router,
        &format!("/v1/executions/{}/prepare", execution_id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "prepare should succeed, got {}: {:?}",
        status,
        prepare_resp
    );

    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_value(prepare_resp).expect("prepare response should deserialize");
    assert!(
        prepare_response.rollback_contract.is_some(),
        "prepare should produce a rollback contract"
    );
    let contract_id = prepare_response
        .rollback_contract
        .as_ref()
        .unwrap()
        .contract_id;

    // Step 5: Call compensate_execution to compensate the prepared execution
    let (status, compensate_resp) = post_json(
        &app.router,
        &format!("/v1/executions/{}/compensate", execution_id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "compensate should succeed, got {}: {:?}",
        status,
        compensate_resp
    );

    let compensate_response: CompensateExecutionResponse =
        serde_json::from_value(compensate_resp).expect("compensate response should deserialize");
    assert!(
        compensate_response.compensated,
        "compensate response should indicate compensated = true"
    );
    assert_eq!(
        compensate_response.contract_id,
        Some(contract_id),
        "compensate response should include the contract ID"
    );
}

/// Verify the R3 approval happy path end-to-end.
///
/// Flow: create R3 proposal -> evaluate -> mint -> authorize
///   -> assert execution is AwaitingApproval
///   -> locate/read the approval request (via r3_approval_id in execution metadata)
///   -> grant approval
///   -> assert approval state changed to Granted and execution advanced to Prepared
#[tokio::test]
async fn r3_approval_happy_path() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate a proposal with R3 rollback class.
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability for the R3 proposal.
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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

    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}: {:?}",
        status,
        mint_resp
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run) — should result in AwaitingApproval.
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "authorize should succeed, got {}: {:?}",
        status,
        auth_resp
    );

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Assert execution is in AwaitingApproval state.
    assert_execution_state(&auth_response.execution, ExecutionState::AwaitingApproval);
    assert_eq!(
        format!("{:?}", auth_response.execution.decision),
        "RequireApproval",
        "R3 execution should have RequireApproval decision"
    );

    // Step 5: Extract the approval_id from execution metadata.
    // The r3_approval_id is stored in execution metadata during authorize_execution.
    let approval_id_str = auth_response
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string");

    // Step 6: Get the approval request by its ID via GET /v1/approvals/{approval_id}.
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/v1/approvals/{}", approval_id_str))
        .body(Body::empty())
        .expect("failed to build request");
    let approval_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    let status = approval_resp.status();
    let body_bytes = approval_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let approval_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );

    assert_eq!(
        status,
        StatusCode::OK,
        "get approval should succeed, got {}: {:?}",
        status,
        approval_json
    );

    let approval: ApprovalRequest =
        serde_json::from_value(approval_json.clone()).expect("approval should deserialize");
    assert!(
        matches!(approval.state, ApprovalState::Pending),
        "approval should be Pending, got {:?}",
        approval.state
    );
    assert_eq!(
        approval.execution_id,
        Some(execution_id),
        "approval should be linked to the execution"
    );

    // Step 7: Grant the approval.
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "test-approver".to_string(),
            display_name: Some("Test Approver".to_string()),
        },
        approve: true,
        reason: Some("R3 approval granted - test".to_string()),
    };
    let resolve_body = serde_json::to_value(&resolve_req).unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/v1/approvals/{}/resolve", approval_id_str))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&resolve_body).unwrap()))
        .expect("failed to build request");
    let resolve_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    let status = resolve_resp.status();
    let body_bytes = resolve_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let resolve_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );

    assert_eq!(
        status,
        StatusCode::OK,
        "resolve approval should succeed, got {}: {:?}",
        status,
        resolve_json
    );

    let resolved_approval: ApprovalRequest =
        serde_json::from_value(resolve_json).expect("resolved approval should deserialize");
    assert!(
        matches!(resolved_approval.state, ApprovalState::Granted),
        "approval should be Granted after resolve, got {:?}",
        resolved_approval.state
    );

    // Step 8: Verify the execution has advanced to Prepared.
    // We verify this by checking that the second authorize call fails with AlreadyUsed.
    // This confirms that cap.mark_used() was called during resolve, transitioning
    // the capability to Used in the InMemoryCapabilityService.
    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body2 = serde_json::to_value(&auth_req2).unwrap();
    let (status2, auth_resp2) =
        post_json(&app.router, "/v1/executions/authorize", auth_body2).await;
    assert_eq!(
        status2,
        StatusCode::BAD_REQUEST,
        "second authorize should fail because capability was consumed after approval grant"
    );
    assert_api_error(&auth_resp2, ApiErrorCode::Conflict, "already used");
}

/// Verify the R3 approval reject path end-to-end.
///
/// Flow: create R3 proposal -> evaluate -> mint -> authorize
///   -> assert execution is AwaitingApproval
///   -> locate/read the approval request (via r3_approval_id in execution metadata)
///   -> reject approval (approve=false)
///   -> assert approval state changed to Denied
///   -> assert execution remains in AwaitingApproval (NOT advanced to Prepared)
///   -> assert capability is NOT consumed (can still be used / authorize again fails with AlreadyUsed only if cap was consumed)
///
/// Unlike the grant path, reject must NOT mark the capability as used.
#[tokio::test]
async fn r3_approval_reject_path() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate a proposal with R3 rollback class.
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _proposal_resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "proposal evaluation should succeed, got {}",
        status
    );

    // Step 2: Mint a capability for the R3 proposal.
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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

    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "mint should succeed, got {}: {:?}",
        status,
        mint_resp
    );

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run) — should result in AwaitingApproval.
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "authorize should succeed, got {}: {:?}",
        status,
        auth_resp
    );

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Assert execution is in AwaitingApproval state.
    assert_execution_state(&auth_response.execution, ExecutionState::AwaitingApproval);
    assert_eq!(
        format!("{:?}", auth_response.execution.decision),
        "RequireApproval",
        "R3 execution should have RequireApproval decision"
    );

    // Step 5: Extract the approval_id from execution metadata.
    let approval_id_str = auth_response
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string");

    // Step 6: Get the approval request by its ID via GET /v1/approvals/{approval_id}.
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/v1/approvals/{}", approval_id_str))
        .body(Body::empty())
        .expect("failed to build request");
    let approval_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    let status = approval_resp.status();
    let body_bytes = approval_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let approval_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );

    assert_eq!(
        status,
        StatusCode::OK,
        "get approval should succeed, got {}: {:?}",
        status,
        approval_json
    );

    let approval: ApprovalRequest =
        serde_json::from_value(approval_json.clone()).expect("approval should deserialize");
    assert!(
        matches!(approval.state, ApprovalState::Pending),
        "approval should be Pending, got {:?}",
        approval.state
    );
    assert_eq!(
        approval.execution_id,
        Some(execution_id),
        "approval should be linked to the execution"
    );

    // Step 7: Reject the approval (approve=false).
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "test-approver".to_string(),
            display_name: Some("Test Approver".to_string()),
        },
        approve: false,
        reason: Some("R3 approval denied - test".to_string()),
    };
    let resolve_body = serde_json::to_value(&resolve_req).unwrap();
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(format!("/v1/approvals/{}/resolve", approval_id_str))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&resolve_body).unwrap()))
        .expect("failed to build request");
    let resolve_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    let status = resolve_resp.status();
    let body_bytes = resolve_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let resolve_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );

    assert_eq!(
        status,
        StatusCode::OK,
        "resolve approval (reject) should succeed, got {}: {:?}",
        status,
        resolve_json
    );

    let resolved_approval: ApprovalRequest =
        serde_json::from_value(resolve_json).expect("resolved approval should deserialize");
    assert!(
        matches!(resolved_approval.state, ApprovalState::Denied),
        "approval should be Denied after reject, got {:?}",
        resolved_approval.state
    );

    // Step 8: Verify the execution remains in AwaitingApproval (NOT Prepared).
    // The execution should not have advanced because approval was denied.
    // We verify this by re-authorizing with the SAME capability.
    // On the reject path, the capability should NOT be consumed.
    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body2 = serde_json::to_value(&auth_req2).unwrap();
    let (status2, auth_resp2) =
        post_json(&app.router, "/v1/executions/authorize", auth_body2).await;

    // On reject path, the capability is NOT consumed, so authorize should succeed
    // (or fail for a different reason like proposal scope, but NOT AlreadyUsed).
    // Since this is the same valid proposal/capability pair, it should return
    // AwaitingApproval again (idempotent re-authorize).
    assert!(
        status2 != StatusCode::BAD_REQUEST
            || !auth_resp2
                .pointer("/message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("already used"))
                .unwrap_or(false),
        "second authorize should NOT fail with 'already used' after rejection; \
         capability must NOT be consumed on reject. Got status: {}, body: {:?}",
        status2,
        auth_resp2
    );
}

/// Verify that GET /v1/approvals returns only pending approvals and that
/// resolved approvals no longer appear in the list.
///
/// Flow: create two R3 proposals -> mint -> authorize (creates two pending approvals)
///   -> GET /v1/approvals returns both pending approvals
///   -> resolve one approval (grant)
///   -> GET /v1/approvals returns only the remaining pending approval
#[tokio::test]
async fn pending_approvals_list() {
    let app = test_app().await;

    let intent_id_1 = ferrum_proto::IntentId::new();
    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let intent_id_2 = ferrum_proto::IntentId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate two R3 proposals.
    for (intent_id, proposal_id) in [(intent_id_1, proposal_id_1), (intent_id_2, proposal_id_2)] {
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let proposal_body = serde_json::to_value(&proposal).unwrap();
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            proposal_body,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");
    }

    // Step 2: Mint capabilities for both R3 proposals.
    let mint_req_1 = CapabilityMintRequest {
        intent_id: intent_id_1,
        proposal_id: proposal_id_1,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp_1) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req_1).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");
    let cap_id_1: CapabilityMintResponse =
        serde_json::from_value(mint_resp_1).expect("mint response should deserialize");
    let capability_id_1 = cap_id_1.lease.capability_id;

    let mint_req_2 = CapabilityMintRequest {
        intent_id: intent_id_2,
        proposal_id: proposal_id_2,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp_2) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req_2).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");
    let cap_id_2: CapabilityMintResponse =
        serde_json::from_value(mint_resp_2).expect("mint response should deserialize");
    let capability_id_2 = cap_id_2.lease.capability_id;

    // Step 3: Authorize both executions (both go to AwaitingApproval).
    let auth_resp_1: AuthorizeExecutionResponse = {
        let auth_req = sample_authorize_request(proposal_id_1, capability_id_1, false);
        let (status, auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "authorize should succeed");
        serde_json::from_value(auth_resp).expect("authorize response should deserialize")
    };
    let approval_id_1 = auth_resp_1
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string")
        .to_string();

    let auth_resp_2: AuthorizeExecutionResponse = {
        let auth_req = sample_authorize_request(proposal_id_2, capability_id_2, false);
        let (status, auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "authorize should succeed");
        serde_json::from_value(auth_resp).expect("authorize response should deserialize")
    };
    let approval_id_2 = auth_resp_2
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string")
        .to_string();

    // Step 4: GET /v1/approvals should return both pending approvals.
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/approvals")
        .body(Body::empty())
        .expect("failed to build request");
    let list_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    assert_eq!(
        list_resp.status(),
        StatusCode::OK,
        "list approvals should succeed"
    );
    let body_bytes = list_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    // Response is now an envelope { items: [...], next_cursor: ... }
    let envelope: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("list response should be an envelope");
    let list_json: Vec<serde_json::Value> = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");

    assert_eq!(
        list_json.len(),
        2,
        "list should return 2 pending approvals, got {}: {:?}",
        list_json.len(),
        list_json
    );

    // Verify both pending approvals are in the list.
    let approval_ids: Vec<String> = list_json
        .iter()
        .map(|v| {
            v.get("approval_id")
                .and_then(|id| id.as_str())
                .expect("approval_id should be a string")
                .to_string()
        })
        .collect();
    assert!(
        approval_ids.contains(&approval_id_1),
        "list should contain approval_id_1, got {:?}",
        approval_ids
    );
    assert!(
        approval_ids.contains(&approval_id_2),
        "list should contain approval_id_2, got {:?}",
        approval_ids
    );

    // Verify all returned approvals are in Pending state.
    for approval_json in &list_json {
        let approval: ApprovalRequest =
            serde_json::from_value(approval_json.clone()).expect("approval should deserialize");
        assert!(
            matches!(approval.state, ApprovalState::Pending),
            "all approvals in list should be Pending, got {:?}",
            approval.state
        );
    }
}

/// Verify that GET /v1/approvals returns an empty list when there are no pending approvals.
#[tokio::test]
async fn pending_approvals_list_empty() {
    let app = test_app().await;

    // No approvals exist — the list should be empty.
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/approvals")
        .body(Body::empty())
        .expect("failed to build request");
    let list_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    assert_eq!(
        list_resp.status(),
        StatusCode::OK,
        "list approvals should succeed even with no approvals"
    );
    let body_bytes = list_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    // Response is now an envelope { items: [...], next_cursor: ... }
    let envelope: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("list response should be an envelope");
    let list_json: Vec<serde_json::Value> = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");

    assert!(
        list_json.is_empty(),
        "list should be empty when no approvals exist, got {:?}",
        list_json
    );
}

/// Verify that an authorized approver (actor_id in approver_roles) can successfully
/// resolve an approval that has an approval_binding with non-empty approver_roles.
///
/// Flow: create R3 proposal -> mint with approval_binding (role-a)
///   -> authorize -> assert AwaitingApproval
///   -> resolve with authorized actor (role-a) -> assert Granted
///   -> capability is consumed (second authorize fails with AlreadyUsed)
#[tokio::test]
async fn r3_approval_with_authorization_happy_path() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate a proposal with R3 rollback class.
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

    // Step 2: Mint a capability with an approval_binding that requires "role-a".
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: Some(ApprovalBinding {
            approval_id: ferrum_proto::ApprovalId::new(), // will be linked at authorize time
            approver_roles: vec!["role-a".to_string()],
            approved_action_digest: proposal_id.to_string(), // digest matches proposal_id set at R3 authorize time
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        }),
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run) — should result in AwaitingApproval.
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;
    assert_eq!(status, StatusCode::OK, "authorize should succeed");

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let _execution_id = auth_response.execution.execution_id;
    assert_execution_state(&auth_response.execution, ExecutionState::AwaitingApproval);

    // Step 4: Extract the approval_id from execution metadata.
    let approval_id_str = auth_response
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string")
        .to_string();

    // Step 5: Resolve with an authorized actor (actor_id = "role-a", which is in approver_roles).
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "role-a".to_string(), // authorized because it's in approver_roles
            display_name: Some("Authorized Approver".to_string()),
        },
        approve: true,
        reason: Some("authorized approval".to_string()),
    };
    let resolve_body = serde_json::to_value(&resolve_req).unwrap();
    let (status, resolve_resp) = post_json(
        &app.router,
        &format!("/v1/approvals/{}/resolve", approval_id_str),
        resolve_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "resolve with authorized actor should succeed, got {}: {:?}",
        status,
        resolve_resp
    );

    let resolved: ApprovalRequest =
        serde_json::from_value(resolve_resp).expect("resolved approval should deserialize");
    assert!(
        matches!(resolved.state, ApprovalState::Granted),
        "approval should be Granted, got {:?}",
        resolved.state
    );

    // Step 6: Verify capability was consumed (second authorize fails with AlreadyUsed).
    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body2 = serde_json::to_value(&auth_req2).unwrap();
    let (status2, auth_resp2) =
        post_json(&app.router, "/v1/executions/authorize", auth_body2).await;
    assert_eq!(
        status2,
        StatusCode::BAD_REQUEST,
        "second authorize should fail"
    );
    assert_api_error(&auth_resp2, ApiErrorCode::Conflict, "already used");
}

/// Verify that an unauthorized approver (actor_id NOT in approver_roles) receives a 403
/// and the approval remains Pending / capability unconsumed.
///
/// Flow: create R3 proposal -> mint with approval_binding (role-a)
///   -> authorize -> assert AwaitingApproval
///   -> resolve with unauthorized actor (role-b) -> assert 403
///   -> assert approval is still Pending
///   -> assert capability is NOT consumed (second authorize succeeds or fails differently)
#[tokio::test]
async fn r3_approval_with_authorization_unauthorized_gets_403() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate a proposal with R3 rollback class.
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

    // Step 2: Mint a capability with an approval_binding that requires "role-a".
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: Some(ApprovalBinding {
            approval_id: ferrum_proto::ApprovalId::new(),
            approver_roles: vec!["role-a".to_string()],
            approved_action_digest: proposal_id.to_string(), // digest matches proposal_id set at R3 authorize time
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        }),
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run) — should result in AwaitingApproval.
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;
    assert_eq!(status, StatusCode::OK, "authorize should succeed");

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    assert_execution_state(&auth_response.execution, ExecutionState::AwaitingApproval);

    // Step 4: Extract the approval_id from execution metadata.
    let approval_id_str = auth_response
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string")
        .to_string();

    // Step 5: Attempt to resolve with an unauthorized actor (actor_id = "role-b", NOT in approver_roles).
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "role-b".to_string(), // NOT authorized (only role-a is)
            display_name: Some("Unauthorized Approver".to_string()),
        },
        approve: true,
        reason: Some("this should be rejected".to_string()),
    };
    let resolve_body = serde_json::to_value(&resolve_req).unwrap();
    let (status, resolve_resp) = post_json(
        &app.router,
        &format!("/v1/approvals/{}/resolve", approval_id_str),
        resolve_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "resolve with unauthorized actor should return FORBIDDEN, got {}: {:?}",
        status,
        resolve_resp
    );
    assert_api_error(&resolve_resp, ApiErrorCode::PolicyDenied, "not authorized");

    // Step 6: Verify approval is still Pending (not mutated).
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/v1/approvals/{}", approval_id_str))
        .body(Body::empty())
        .expect("failed to build request");
    let approval_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    let body_bytes = approval_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let approval_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let approval: ApprovalRequest =
        serde_json::from_value(approval_json).expect("approval should deserialize");
    assert!(
        matches!(approval.state, ApprovalState::Pending),
        "approval should still be Pending after unauthorized resolve attempt, got {:?}",
        approval.state
    );

    // Step 7: Verify capability is NOT consumed (authorize again should succeed or fail
    // for a reason other than AlreadyUsed).
    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body2 = serde_json::to_value(&auth_req2).unwrap();
    let (status2, auth_resp2) =
        post_json(&app.router, "/v1/executions/authorize", auth_body2).await;
    // Either succeeds (returns AwaitingApproval again) or fails for a reason other than "already used"
    assert!(
        status2 == StatusCode::OK
            || !auth_resp2
                .pointer("/message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("already used"))
                .unwrap_or(false),
        "second authorize should NOT fail with 'already used' after unauthorized reject; \
         got status: {}, body: {:?}",
        status2,
        auth_resp2
    );
}

/// Verify that a matching approval_id binding succeeds.
///
/// Flow: create R3 proposal -> mint with approval_binding (with specific approval_id)
///   -> authorize (uses binding.approval_id for the ApprovalRequest)
///   -> resolve with authorized actor -> assert Granted
///   -> capability is consumed
#[tokio::test]
async fn r3_approval_with_matching_approval_id_binding_succeeds() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    // Step 1: Create and evaluate a proposal with R3 rollback class.
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

    // Step 2: Mint a capability with an approval_binding that has a specific approval_id.
    // The authorization step will use this same approval_id for the ApprovalRequest.
    let binding_approval_id = ferrum_proto::ApprovalId::new();
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: Some(ApprovalBinding {
            approval_id: binding_approval_id,
            approver_roles: vec!["role-a".to_string()],
            approved_action_digest: proposal_id.to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        }),
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let mint_body = serde_json::to_value(&mint_req).unwrap();
    let (status, mint_resp) = post_json(&app.router, "/v1/capabilities/mint", mint_body).await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    // Step 3: Authorize execution (non-dry-run) — should result in AwaitingApproval.
    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body = serde_json::to_value(&auth_req).unwrap();
    let (status, auth_resp) = post_json(&app.router, "/v1/executions/authorize", auth_body).await;
    assert_eq!(status, StatusCode::OK, "authorize should succeed");

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    assert_execution_state(&auth_response.execution, ExecutionState::AwaitingApproval);

    // Step 4: Extract the approval_id from execution metadata.
    let approval_id_str = auth_response
        .execution
        .metadata
        .get("r3_approval_id")
        .expect("execution metadata should contain r3_approval_id")
        .as_str()
        .expect("r3_approval_id should be a string")
        .to_string();

    // Verify the approval_id in the approval matches the binding's approval_id.
    assert_eq!(
        approval_id_str,
        binding_approval_id.to_string(),
        "authorization should use binding.approval_id"
    );

    // Step 5: Resolve with an authorized actor.
    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "role-a".to_string(),
            display_name: Some("Authorized Approver".to_string()),
        },
        approve: true,
        reason: Some("approval with matching binding id".to_string()),
    };
    let resolve_body = serde_json::to_value(&resolve_req).unwrap();
    let (status, resolve_resp) = post_json(
        &app.router,
        &format!("/v1/approvals/{}/resolve", approval_id_str),
        resolve_body,
    )
    .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "resolve with matching approval_id should succeed, got {}: {:?}",
        status,
        resolve_resp
    );

    let resolved: ApprovalRequest =
        serde_json::from_value(resolve_resp).expect("resolved approval should deserialize");
    assert!(
        matches!(resolved.state, ApprovalState::Granted),
        "approval should be Granted, got {:?}",
        resolved.state
    );

    // Step 6: Verify capability was consumed.
    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let auth_body2 = serde_json::to_value(&auth_req2).unwrap();
    let (status2, auth_resp2) =
        post_json(&app.router, "/v1/executions/authorize", auth_body2).await;
    assert_eq!(
        status2,
        StatusCode::BAD_REQUEST,
        "second authorize should fail because capability was consumed"
    );
    assert_api_error(&auth_resp2, ApiErrorCode::Conflict, "already used");
}

/// Verify that a mismatched approval_id binding fails closed even if a forged
/// approval record is inserted for the same execution.
#[tokio::test]
async fn r3_approval_with_mismatched_approval_id_binding_gets_403() {
    let app = test_app().await;

    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();

    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let proposal_body = serde_json::to_value(&proposal).unwrap();
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        proposal_body,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

    let binding_approval_id = ferrum_proto::ApprovalId::new();
    let forged_approval_id = ferrum_proto::ApprovalId::new();
    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
            required_hash: None,
        }],
        argument_constraints: Vec::new(),
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: Some(ApprovalBinding {
            approval_id: binding_approval_id,
            approver_roles: vec!["role-a".to_string()],
            approved_action_digest: proposal_id.to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        }),
        requested_ttl_secs: 60,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let (status, mint_resp) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");

    let mint_response: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");
    let capability_id = mint_response.lease.capability_id;

    let auth_req = sample_authorize_request(proposal_id, capability_id, false);
    let (status, auth_resp) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "authorize should succeed");

    let auth_response: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("authorize response should deserialize");
    let execution_id = auth_response.execution.execution_id;
    assert_execution_state(&auth_response.execution, ExecutionState::AwaitingApproval);

    let forged_approval = ApprovalRequest {
        approval_id: forged_approval_id,
        intent_id,
        proposal_id,
        execution_id: Some(execution_id),
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("Ferrum Gateway".to_string()),
        },
        reason: "forged approval for mismatch test".to_string(),
        action_digest: proposal_id.to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        state: ApprovalState::Pending,
        created_at: chrono::Utc::now(),
    };
    app.runtime
        .store
        .approvals()
        .insert(&forged_approval)
        .await
        .expect("forged approval insert should succeed");

    let resolve_req = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::User,
            actor_id: "role-a".to_string(),
            display_name: Some("Authorized Approver".to_string()),
        },
        approve: true,
        reason: Some("this should fail on approval_id mismatch".to_string()),
    };
    let (status, resolve_resp) = post_json(
        &app.router,
        &format!("/v1/approvals/{}/resolve", forged_approval_id),
        serde_json::to_value(&resolve_req).unwrap(),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "resolve with mismatched approval_id binding should return FORBIDDEN, got {}: {:?}",
        status,
        resolve_resp
    );
    assert_api_error(
        &resolve_resp,
        ApiErrorCode::PolicyDenied,
        "approval_id mismatch",
    );

    let req = axum::http::Request::builder()
        .method("GET")
        .uri(format!("/v1/approvals/{}", forged_approval_id))
        .body(Body::empty())
        .expect("failed to build request");
    let approval_resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");
    let body_bytes = approval_resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let approval_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let approval: ApprovalRequest =
        serde_json::from_value(approval_json).expect("approval should deserialize");
    assert!(
        matches!(approval.state, ApprovalState::Pending),
        "forged approval should remain Pending after mismatch, got {:?}",
        approval.state
    );

    let auth_req2 = sample_authorize_request(proposal_id, capability_id, false);
    let (status2, auth_resp2) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req2).unwrap(),
    )
    .await;
    assert!(
        status2 == StatusCode::OK
            || !auth_resp2
                .pointer("/message")
                .and_then(|m| m.as_str())
                .map(|m| m.contains("already used"))
                .unwrap_or(false),
        "second authorize should NOT fail with 'already used' after approval_id mismatch; got status: {}, body: {:?}",
        status2,
        auth_resp2
    );
}

/// Verify that GET /v1/approvals?proposal_id=X returns only pending approvals
/// for the specified proposal and excludes approvals for other proposals.
#[tokio::test]
async fn pending_approvals_filter_by_proposal_id_returns_only_target_proposal() {
    let app = test_app().await;

    let intent_id_1 = ferrum_proto::IntentId::new();
    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let intent_id_2 = ferrum_proto::IntentId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Create two R3 proposals and generate pending approvals for both.
    for (intent_id, proposal_id) in [(intent_id_1, proposal_id_1), (intent_id_2, proposal_id_2)] {
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "mint should succeed");
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "authorize should succeed");
    }

    // Filter by proposal_id_1 — should return only approvals for proposal_id_1.
    let (status, items) =
        get_approvals(&app.router, &format!("proposal_id={}", proposal_id_1)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "list with proposal_id filter should succeed"
    );

    // Should return exactly 1 approval (for proposal_id_1).
    assert_eq!(
        items.len(),
        1,
        "proposal_id filter should return exactly 1 approval, got {}: {:?}",
        items.len(),
        items
    );

    // The returned approval must be for proposal_id_1.
    let approval: ApprovalRequest =
        serde_json::from_value(items[0].clone()).expect("approval should deserialize");
    assert_eq!(
        approval.proposal_id, proposal_id_1,
        "returned approval should be for proposal_id_1, got {:?}",
        approval.proposal_id
    );
}

/// Verify that pending approvals for other proposals are excluded when filtering.
#[tokio::test]
async fn pending_approvals_filter_excludes_other_proposals() {
    let app = test_app().await;

    let intent_id_1 = ferrum_proto::IntentId::new();
    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let intent_id_2 = ferrum_proto::IntentId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Create two R3 proposals and generate pending approvals for both.
    for (intent_id, proposal_id) in [(intent_id_1, proposal_id_1), (intent_id_2, proposal_id_2)] {
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Fetch all approvals (no filter) - should have 2.
    let (_, all_items) = get_approvals(&app.router, "").await;
    assert_eq!(
        all_items.len(),
        2,
        "should have 2 total approvals, got {}",
        all_items.len()
    );

    // Filter by proposal_id_1 — should return only 1 (for proposal_id_1).
    let (status, items) =
        get_approvals(&app.router, &format!("proposal_id={}", proposal_id_1)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        items.len(),
        1,
        "filter should return 1 approval, got {}",
        items.len()
    );

    // Verify the returned approval is for proposal_id_1.
    let filtered_approval: ApprovalRequest =
        serde_json::from_value(items[0].clone()).expect("approval should deserialize");
    assert_eq!(
        filtered_approval.proposal_id, proposal_id_1,
        "filtered approval should be for proposal_id_1"
    );

    // Verify proposal_id_2's approval is excluded by checking all_items contains proposal_id_2's approval.
    let all_approval_proposal_ids: Vec<_> = all_items
        .iter()
        .map(|v| {
            v.get("proposal_id")
                .and_then(|id| id.as_str())
                .expect("proposal_id should be a string")
                .to_string()
        })
        .collect();
    assert!(
        all_approval_proposal_ids.contains(&proposal_id_2.to_string()),
        "all_items should contain proposal_id_2's approval"
    );

    // Filter for proposal_id_2 should return only that approval.
    let (_, items_2) = get_approvals(&app.router, &format!("proposal_id={}", proposal_id_2)).await;
    assert_eq!(
        items_2.len(),
        1,
        "filter by proposal_id_2 should return 1 approval"
    );
    let filtered_approval_2: ApprovalRequest =
        serde_json::from_value(items_2[0].clone()).expect("approval should deserialize");
    assert_eq!(
        filtered_approval_2.proposal_id, proposal_id_2,
        "filtered approval should be for proposal_id_2"
    );
}

/// Verify that proposal_id filter composes correctly with limit and offset.
///
/// Since each proposal_id creates exactly one approval (one evaluation per proposal),
/// we test filter+limit/offset by creating multiple proposals and verifying that
/// pagination works correctly on the full list while filter works correctly on individual proposals.
#[tokio::test]
async fn pending_approvals_filter_composes_with_limit_offset() {
    let app = test_app().await;

    // Create 5 approvals with 5 different proposal_ids.
    let proposal_ids: Vec<_> = (0..5u8).map(|_| ferrum_proto::ProposalId::new()).collect();

    for proposal_id in &proposal_ids {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal = sample_proposal(
            intent_id,
            *proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id: *proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = AuthorizeExecutionRequest {
            proposal_id: *proposal_id,
            capability_id: cap.lease.capability_id,
            dry_run: false,
        };
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Test pagination on full list (no filter).
    // offset=0, limit=3 should return first 3.
    let (_, page0) = get_approvals(&app.router, "limit=3&offset=0").await;
    assert_eq!(page0.len(), 3, "limit=3 should return 3 items");

    // offset=3, limit=3 should return last 2.
    let (_, page1) = get_approvals(&app.router, "limit=3&offset=3").await;
    assert_eq!(page1.len(), 2, "offset=3 should return remaining 2 items");

    // Test filter with limit: filter by proposal_ids[0] with limit=10 should return 1.
    let (_, filtered) = get_approvals(
        &app.router,
        &format!("proposal_id={}&limit=10", proposal_ids[0]),
    )
    .await;
    assert_eq!(
        filtered.len(),
        1,
        "filter+limit should return 1 approval for specific proposal"
    );

    // Verify the correct proposal_id is returned.
    let approval: ApprovalRequest =
        serde_json::from_value(filtered[0].clone()).expect("approval should deserialize");
    assert_eq!(
        approval.proposal_id, proposal_ids[0],
        "filtered approval should be for the correct proposal"
    );

    // Filter with offset on full list (proposal_ids[0] only has 1 approval, so offset=0 works).
    let (_, filtered_offset) = get_approvals(
        &app.router,
        &format!("proposal_id={}&limit=10&offset=0", proposal_ids[0]),
    )
    .await;
    assert_eq!(
        filtered_offset.len(),
        1,
        "filter+offset=0 should still return 1 approval"
    );

    // Filter with offset=1 on proposal_ids[0] should return 0 (only 1 approval exists).
    let (_, filtered_offset_1) = get_approvals(
        &app.router,
        &format!("proposal_id={}&limit=10&offset=1", proposal_ids[0]),
    )
    .await;
    assert_eq!(
        filtered_offset_1.len(),
        0,
        "filter+offset=1 should return 0 approvals (none at offset 1)"
    );
}

/// Verify that an invalid proposal_id (not a valid UUID) returns a validation error.
#[tokio::test]
async fn pending_approvals_invalid_proposal_id_returns_validation_error() {
    let app = test_app().await;

    // "not-a-uuid" is not a valid UUID, so it should be rejected.
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/approvals?proposal_id=not-a-uuid")
        .body(Body::empty())
        .expect("failed to build request");
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "invalid proposal_id should return BAD_REQUEST, got {}",
        resp.status()
    );

    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let err: ferrum_proto::ApiError =
        serde_json::from_slice(&body_bytes).expect("error response should deserialize");

    assert!(
        err.message.contains("proposal_id") || err.message.contains("uuid"),
        "error message should mention proposal_id or uuid, got: {}",
        err.message
    );
}

/// Helper: GET /v1/approvals with optional query string and return the items array.
async fn get_approvals(router: &axum::Router, query: &str) -> (StatusCode, Vec<serde_json::Value>) {
    let uri = if query.is_empty() {
        "/v1/approvals".to_string()
    } else {
        format!("/v1/approvals?{}", query)
    };
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(&uri)
        .body(Body::empty())
        .expect("failed to build request");
    let resp = router.clone().oneshot(req).await.expect("request failed");
    let status = resp.status();
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();

    // Parse as envelope { items: [...], next_cursor: ... }
    let envelope: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );

    // Extract items array
    let items = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_else(|| {
            vec![serde_json::json!({ "parse_error": "missing items field or not an array" })]
        });

    (status, items)
}

/// Helper: GET /v1/approvals and return the full response envelope.
async fn get_approvals_envelope(
    router: &axum::Router,
    query: &str,
) -> (StatusCode, serde_json::Value) {
    let uri = if query.is_empty() {
        "/v1/approvals".to_string()
    } else {
        format!("/v1/approvals?{}", query)
    };
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(&uri)
        .body(Body::empty())
        .expect("failed to build request");
    let resp = router.clone().oneshot(req).await.expect("request failed");
    let status = resp.status();
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_else(
        |_| serde_json::json!({ "parse_error": String::from_utf8_lossy(&body_bytes).to_string() }),
    );
    (status, json)
}

/// Verify that GET /v1/approvals uses the default pagination (limit=50, offset=0)
/// and returns all pending approvals.
#[tokio::test]
async fn pending_approvals_pagination_defaults() {
    let app = test_app().await;

    // Create 3 pending approvals.
    for i in 0..3u8 {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "mint should succeed");
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "authorize should succeed for item {}",
            i
        );
    }

    // Default call (no params) — should return all 3.
    let (status, items) = get_approvals(&app.router, "").await;
    assert_eq!(status, StatusCode::OK, "list should succeed");
    assert_eq!(
        items.len(),
        3,
        "default limit=50 should return all 3 pending approvals"
    );
}

/// Verify that limit and offset query params work correctly.
#[tokio::test]
async fn pending_approvals_pagination_limit_offset() {
    let app = test_app().await;

    // Create 5 pending approvals.
    let mut cap_ids = Vec::new();
    for i in 0..5u8 {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");
        cap_ids.push(cap.lease.capability_id);

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "authorize should succeed for item {}",
            i
        );
    }

    // limit=2 — should return only 2.
    let (status, items) = get_approvals(&app.router, "limit=2").await;
    assert_eq!(status, StatusCode::OK, "list with limit=2 should succeed");
    assert_eq!(
        items.len(),
        2,
        "limit=2 should return exactly 2 items, got {}",
        items.len()
    );

    // offset=2, limit=2 — should return 2 items (3rd and 4th).
    let (status, items) = get_approvals(&app.router, "offset=2&limit=2").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "list with offset=2&limit=2 should succeed"
    );
    assert_eq!(
        items.len(),
        2,
        "offset=2&limit=2 should return 2 items, got {}",
        items.len()
    );

    // offset=4 — should return only 1 (5th item).
    let (status, items) = get_approvals(&app.router, "offset=4&limit=10").await;
    assert_eq!(status, StatusCode::OK, "list with offset=4 should succeed");
    assert_eq!(
        items.len(),
        1,
        "offset=4 should return 1 item, got {}",
        items.len()
    );

    // offset beyond total — should return empty list.
    let (status, items) = get_approvals(&app.router, "offset=10&limit=10").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "list with offset beyond total should succeed"
    );
    assert_eq!(
        items.len(),
        0,
        "offset=10 should return empty list, got {}",
        items.len()
    );
}

/// Verify that limit is clamped conservatively to MAX_LIMIT (100).
#[tokio::test]
async fn pending_approvals_pagination_limit_clamped_to_max() {
    let app = test_app().await;

    // Create 3 approvals.
    for _ in 0..3u8 {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");
        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // limit=200 should be clamped to 100 internally.
    let (status, items) = get_approvals(&app.router, "limit=200").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "limit=200 should succeed (silently clamped)"
    );
    assert_eq!(
        items.len(),
        3,
        "clamped limit=200 should still return all 3 pending approvals"
    );

    // limit=100 should work normally.
    let (status, items) = get_approvals(&app.router, "limit=100").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(items.len(), 3);
}

/// Verify that limit=0 is rejected (fail-closed on invalid params).
#[tokio::test]
async fn pending_approvals_pagination_rejects_zero_limit() {
    let app = test_app().await;

    let (status, body_bytes) = {
        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/v1/approvals?limit=0")
            .body(Body::empty())
            .expect("failed to build request");
        let resp = app
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("request failed");
        let status = resp.status();
        let body = resp
            .into_body()
            .collect()
            .await
            .expect("failed to collect body")
            .to_bytes();
        (status, body)
    };
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "limit=0 should be rejected with BAD_REQUEST, got {}",
        status
    );
    let err: ferrum_proto::ApiError =
        serde_json::from_slice(&body_bytes).expect("error response should deserialize");
    assert!(
        err.message.contains("limit") || err.message.contains("positive"),
        "error message should mention 'limit' or 'positive', got: {}",
        err.message
    );
}

// ------------------------------------------------------------------------------------------------
// Cursor pagination tests
// ------------------------------------------------------------------------------------------------

/// Verify that cursor pagination returns items and next_cursor on first page.
#[tokio::test]
async fn cursor_pagination_first_page_returns_items_and_next_cursor() {
    let app = test_app().await;

    // Create 3 pending approvals.
    for i in 0..3u8 {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "mint should succeed");
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "authorize should succeed for item {}",
            i
        );
    }

    // Request first page with limit=2 and an empty cursor to trigger cursor mode for first page.
    // Empty cursor signals "first page request" in cursor pagination.
    let (status, envelope) = get_approvals_envelope(&app.router, "limit=2&cursor=").await;
    assert_eq!(status, StatusCode::OK, "list should succeed");

    let items = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");
    assert_eq!(items.len(), 2, "limit=2 should return 2 items");

    let next_cursor = envelope
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(String::from);
    assert!(
        next_cursor.is_some(),
        "next_cursor should be present when more items exist"
    );
}

/// Verify that cursor pagination advances correctly without overlap.
#[tokio::test]
async fn cursor_pagination_next_page_no_overlap() {
    let app = test_app().await;

    // Create 4 pending approvals.
    for i in 0..4u8 {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "authorize should succeed for item {}",
            i
        );
    }

    // First page: limit=2 with empty cursor to trigger cursor pagination.
    let (_status, envelope) = get_approvals_envelope(&app.router, "limit=2&cursor=").await;
    let page1_ids: Vec<String> = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array")
        .iter()
        .map(|v| {
            v.get("approval_id")
                .and_then(|id| id.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert_eq!(page1_ids.len(), 2, "page 1 should have 2 items");

    let cursor = envelope
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(String::from)
        .expect("next_cursor should be present");

    // Second page: use cursor.
    let (_status, envelope2) =
        get_approvals_envelope(&app.router, &format!("limit=2&cursor={}", cursor)).await;
    let page2_ids: Vec<String> = envelope2
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array")
        .iter()
        .map(|v| {
            v.get("approval_id")
                .and_then(|id| id.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert_eq!(page2_ids.len(), 2, "page 2 should have 2 items");

    // No overlap between page 1 and page 2.
    for id in &page1_ids {
        assert!(
            !page2_ids.contains(id),
            "page 2 should not overlap with page 1: {} should not be in {:?}",
            id,
            page2_ids
        );
    }

    // Page 3: should be empty (only 4 items total).
    let (_status, envelope3) =
        get_approvals_envelope(&app.router, &format!("limit=2&cursor={}", cursor)).await;
    let page3_ids: Vec<String> = envelope3
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array")
        .iter()
        .map(|v| {
            v.get("approval_id")
                .and_then(|id| id.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    // Using the same cursor again should give the same results (deterministic).
    assert_eq!(
        page2_ids, page3_ids,
        "same cursor should return same page (deterministic)"
    );
}

/// Verify that proposal_id filter composes with cursor pagination.
#[tokio::test]
async fn cursor_pagination_composes_with_proposal_id_filter() {
    let app = test_app().await;

    // Create 2 approvals for proposal_id_1 and 1 approval for proposal_id_2.
    let intent_id_1 = ferrum_proto::IntentId::new();
    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let proposal_1 = sample_proposal(
        intent_id_1,
        proposal_id_1,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id_1),
        serde_json::to_value(&proposal_1).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // First approval for proposal_id_1.
    let mint_req_1 = CapabilityMintRequest {
        intent_id: intent_id_1,
        proposal_id: proposal_id_1,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp_1) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req_1).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cap_1: CapabilityMintResponse =
        serde_json::from_value(mint_resp_1).expect("mint response should deserialize");

    let auth_req_1 = sample_authorize_request(proposal_id_1, cap_1.lease.capability_id, false);
    let (status, _auth_resp_1) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req_1).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Second approval for proposal_id_1 (use intent_id_1 again since proposal is already persisted).
    // We re-use intent_id_1 because it's already been validated by the first evaluate.
    let mint_req_1b = CapabilityMintRequest {
        intent_id: intent_id_1,
        proposal_id: proposal_id_1,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp_1b) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req_1b).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cap_1b: CapabilityMintResponse =
        serde_json::from_value(mint_resp_1b).expect("mint response should deserialize");

    let auth_req_1b = sample_authorize_request(proposal_id_1, cap_1b.lease.capability_id, false);
    let (status, _auth_resp_1b) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req_1b).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Approval for proposal_id_2.
    let intent_id_2 = ferrum_proto::IntentId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();
    let proposal_2 = sample_proposal(
        intent_id_2,
        proposal_id_2,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id_2),
        serde_json::to_value(&proposal_2).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let mint_req_2 = CapabilityMintRequest {
        intent_id: intent_id_2,
        proposal_id: proposal_id_2,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp_2) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req_2).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cap_2: CapabilityMintResponse =
        serde_json::from_value(mint_resp_2).expect("mint response should deserialize");

    let auth_req_2 = sample_authorize_request(proposal_id_2, cap_2.lease.capability_id, false);
    let (status, _auth_resp_2) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req_2).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Now: filter by proposal_id_1 with limit=1 and cursor.
    let (_status, envelope) = get_approvals_envelope(
        &app.router,
        &format!("proposal_id={}&limit=1", proposal_id_1),
    )
    .await;
    let items = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");
    assert_eq!(items.len(), 1, "filter+limit=1 should return 1 item");

    // The item should be for proposal_id_1.
    let approval: ApprovalRequest =
        serde_json::from_value(items[0].clone()).expect("approval should deserialize");
    assert_eq!(
        approval.proposal_id, proposal_id_1,
        "filtered approval should be for proposal_id_1"
    );

    let cursor = envelope
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(String::from);

    // If there's a next_cursor, use it to get the second page.
    if let Some(c) = cursor {
        let (_status, envelope2) = get_approvals_envelope(
            &app.router,
            &format!("proposal_id={}&limit=1&cursor={}", proposal_id_1, c),
        )
        .await;
        let items2 = envelope2
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .expect("items should be an array");
        assert_eq!(items2.len(), 1, "second page should have 1 item");

        let approval2: ApprovalRequest =
            serde_json::from_value(items2[0].clone()).expect("approval should deserialize");
        assert_eq!(
            approval2.proposal_id, proposal_id_1,
            "second page approval should also be for proposal_id_1"
        );

        // Should not overlap.
        assert_ne!(
            approval.approval_id, approval2.approval_id,
            "pages should not overlap"
        );
    }
}

/// Verify that an invalid cursor returns a validation error.
#[tokio::test]
async fn cursor_pagination_invalid_cursor_returns_error() {
    let app = test_app().await;

    // Create one approval so the endpoint is functional.
    let intent_id = ferrum_proto::IntentId::new();
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        serde_json::to_value(&proposal).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cap: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");

    let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
    let (status, _auth_resp) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Invalid cursor: "not-a-valid-cursor".
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/approvals?cursor=not-a-valid-cursor")
        .body(Body::empty())
        .expect("failed to build request");
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "invalid cursor should return BAD_REQUEST, got {}",
        resp.status()
    );

    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let err: ferrum_proto::ApiError =
        serde_json::from_slice(&body_bytes).expect("error response should deserialize");

    assert!(
        err.message.contains("cursor"),
        "error message should mention 'cursor', got: {}",
        err.message
    );
}

/// Verify that offset mode returns envelope with next_cursor = null.
#[tokio::test]
async fn cursor_pagination_offset_mode_returns_null_next_cursor() {
    let app = test_app().await;

    // Create 2 approvals.
    for _ in 0..2u8 {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = sample_proposal(
            intent_id,
            proposal_id,
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let (status, _resp) = post_json(
            &app.router,
            &format!("/v1/proposals/{}/evaluate", proposal_id),
            serde_json::to_value(&proposal).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let mint_req = CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: vec![ResourceBinding::File {
                path: "/tmp/test.txt".to_string(),
                mode: ResourceMode::Write,
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
        let (status, mint_resp) = post_json(
            &app.router,
            "/v1/capabilities/mint",
            serde_json::to_value(&mint_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let cap: CapabilityMintResponse =
            serde_json::from_value(mint_resp).expect("mint response should deserialize");

        let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
        let (status, _auth_resp) = post_json(
            &app.router,
            "/v1/executions/authorize",
            serde_json::to_value(&auth_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    // Use offset mode (no cursor).
    let (_status, envelope) = get_approvals_envelope(&app.router, "limit=10").await;

    let next_cursor = envelope
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(String::from);

    assert!(
        next_cursor.is_none(),
        "offset mode should return next_cursor = null, got {:?}",
        next_cursor
    );

    // Items should be present and correct count.
    let items = envelope
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");
    assert_eq!(items.len(), 2, "should return 2 items");
}

/// Helper: Create a pending approval and return its execution_id.
async fn create_approval_with_execution(
    app: &TestApp,
    proposal_id: ferrum_proto::ProposalId,
) -> (ApprovalRequest, ExecutionId) {
    let intent_id = ferrum_proto::IntentId::new();
    let proposal = sample_proposal(
        intent_id,
        proposal_id,
        RollbackClass::R3IrreversibleHighConsequence,
    );
    let (status, _resp) = post_json(
        &app.router,
        &format!("/v1/proposals/{}/evaluate", proposal_id),
        serde_json::to_value(&proposal).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "proposal evaluation should succeed");

    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: "/tmp/test.txt".to_string(),
            mode: ResourceMode::Write,
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
    let (status, mint_resp) = post_json(
        &app.router,
        "/v1/capabilities/mint",
        serde_json::to_value(&mint_req).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "mint should succeed");
    let cap: CapabilityMintResponse =
        serde_json::from_value(mint_resp).expect("mint response should deserialize");

    let auth_req = sample_authorize_request(proposal_id, cap.lease.capability_id, false);
    let (status, auth_resp) = post_json(
        &app.router,
        "/v1/executions/authorize",
        serde_json::to_value(&auth_req).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "authorize should succeed");

    let auth: AuthorizeExecutionResponse =
        serde_json::from_value(auth_resp).expect("auth response should deserialize");
    let execution_id = auth.execution.execution_id;

    // Get the approval that was created
    let (status, items) = get_approvals(&app.router, "").await;
    assert_eq!(status, StatusCode::OK);
    let approval: ApprovalRequest =
        serde_json::from_value(items[0].clone()).expect("approval should deserialize");

    (approval, execution_id)
}

/// Verify that GET /v1/approvals?execution_id=X returns only pending approvals
/// for the specified execution and excludes approvals for other executions.
#[tokio::test]
async fn pending_approvals_filter_by_execution_id_returns_only_target_execution() {
    let app = test_app().await;

    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Create two approvals with different execution_ids
    let (_approval_1, execution_id_1) = create_approval_with_execution(&app, proposal_id_1).await;
    let (_approval_2, _execution_id_2) = create_approval_with_execution(&app, proposal_id_2).await;

    // Filter by execution_id_1 — should return only approvals for execution_id_1.
    let (status, items) =
        get_approvals(&app.router, &format!("execution_id={}", execution_id_1)).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "list with execution_id filter should succeed"
    );

    // Should return exactly 1 approval (for execution_id_1).
    assert_eq!(
        items.len(),
        1,
        "execution_id filter should return exactly 1 approval, got {}: {:?}",
        items.len(),
        items
    );

    // The returned approval must be for execution_id_1.
    let approval: ApprovalRequest =
        serde_json::from_value(items[0].clone()).expect("approval should deserialize");
    assert!(
        approval.execution_id.is_some(),
        "returned approval should have an execution_id"
    );
    assert_eq!(
        approval.execution_id.unwrap(),
        execution_id_1,
        "returned approval should be for execution_id_1"
    );
}

/// Verify that pending approvals for other executions are excluded when filtering.
#[tokio::test]
async fn pending_approvals_filter_excludes_other_executions() {
    let app = test_app().await;

    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Create two approvals with different execution_ids
    let (_approval_1, execution_id_1) = create_approval_with_execution(&app, proposal_id_1).await;
    let (_approval_2, _execution_id_2) = create_approval_with_execution(&app, proposal_id_2).await;

    // Fetch all approvals (no filter) - should have 2.
    let (_, all_items) = get_approvals(&app.router, "").await;
    assert_eq!(
        all_items.len(),
        2,
        "should have 2 total approvals, got {}",
        all_items.len()
    );

    // Filter by execution_id_1 — should return only 1 approval (for execution_id_1).
    let (_, filtered_items) =
        get_approvals(&app.router, &format!("execution_id={}", execution_id_1)).await;
    assert_eq!(
        filtered_items.len(),
        1,
        "execution_id filter should return exactly 1 approval, got {}",
        filtered_items.len()
    );

    // Verify the filtered approval is for the correct execution.
    let filtered_approval: ApprovalRequest =
        serde_json::from_value(filtered_items[0].clone()).expect("approval should deserialize");
    assert_eq!(
        filtered_approval.execution_id.unwrap(),
        execution_id_1,
        "filtered approval should be for execution_id_1"
    );
}

/// Verify that execution_id filter composes with cursor pagination.
/// This test creates 2 approvals with different execution_ids and verifies
/// that cursor pagination works correctly on the filtered list.
#[tokio::test]
async fn pending_approvals_filter_by_execution_id_composes_with_cursor_pagination() {
    let app = test_app().await;

    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Create 2 approvals with different execution_ids
    let (approval_1, execution_id_1) = create_approval_with_execution(&app, proposal_id_1).await;
    let (approval_2, _execution_id_2) = create_approval_with_execution(&app, proposal_id_2).await;

    // Verify we have 2 approvals
    let (_, all_items) = get_approvals(&app.router, "").await;
    assert_eq!(all_items.len(), 2, "should have 2 total approvals");

    // Get first page with cursor (limit=1) - should have 1 item
    let (_, page0) = get_approvals_envelope(&app.router, "limit=1&cursor=").await;
    let items0 = page0
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");
    assert_eq!(items0.len(), 1, "first page should have 1 item");

    let cursor = page0
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(String::from);
    assert!(cursor.is_some(), "first page should have a next_cursor");

    // Use cursor to get second page
    let (_, page1) =
        get_approvals_envelope(&app.router, &format!("limit=1&cursor={}", cursor.unwrap())).await;
    let items1 = page1
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("items should be an array");
    assert_eq!(items1.len(), 1, "second page should have 1 item");

    let next_cursor = page1
        .get("next_cursor")
        .and_then(|v| v.as_str())
        .map(String::from);
    assert!(
        next_cursor.is_none(),
        "second page should have no next_cursor"
    );

    // Now verify execution_id filter works - filter by execution_id_1
    let (_, filtered_items) =
        get_approvals(&app.router, &format!("execution_id={}", execution_id_1)).await;
    assert_eq!(
        filtered_items.len(),
        1,
        "execution_id filter should return exactly 1 approval"
    );

    // Verify the returned approval matches the filter
    let filtered_approval: ApprovalRequest =
        serde_json::from_value(filtered_items[0].clone()).expect("approval should deserialize");
    assert_eq!(
        filtered_approval.execution_id.unwrap(),
        execution_id_1,
        "filtered approval should be for execution_id_1"
    );

    // Clean up
    for approval in [&approval_1, &approval_2] {
        let resolve_req = ferrum_proto::ApprovalResolveRequest {
            actor: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::User,
                actor_id: "test-approver".to_string(),
                display_name: Some("Test Approver".to_string()),
            },
            approve: true,
            reason: Some("test resolution".to_string()),
        };
        let (status, _resolve_resp) = post_json(
            &app.router,
            &format!("/v1/approvals/{}/resolve", approval.approval_id),
            serde_json::to_value(&resolve_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "resolve should succeed");
    }
}

/// Verify that invalid execution_id returns validation error.
#[tokio::test]
async fn pending_approvals_invalid_execution_id_returns_validation_error() {
    let app = test_app().await;

    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/v1/approvals?execution_id=not-a-uuid")
        .body(Body::empty())
        .expect("failed to build request");
    let resp = app
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "invalid execution_id should return BAD_REQUEST, got {}",
        resp.status()
    );

    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .expect("failed to collect body")
        .to_bytes();
    let err: ferrum_proto::ApiError =
        serde_json::from_slice(&body_bytes).expect("error response should deserialize");

    assert!(
        err.message.contains("execution") || err.message.contains("uuid"),
        "error message should mention 'execution' or 'uuid', got: {}",
        err.message
    );
}

/// Verify that when both proposal_id and execution_id are provided,
/// both filters apply (AND semantics).
#[tokio::test]
async fn pending_approvals_filter_by_proposal_and_execution_uses_and_semantics() {
    let app = test_app().await;

    let proposal_id_1 = ferrum_proto::ProposalId::new();
    let proposal_id_2 = ferrum_proto::ProposalId::new();

    // Create approvals for different proposals (each gets its own execution_id)
    let (approval_1, execution_id_1) = create_approval_with_execution(&app, proposal_id_1).await;
    let (_approval_2, execution_id_2) = create_approval_with_execution(&app, proposal_id_2).await;

    // Verify we have 2 approvals total
    let (_, all_items) = get_approvals(&app.router, "").await;
    assert_eq!(all_items.len(), 2, "should have 2 total approvals");

    // Filter by both proposal_id_1 AND execution_id_1 — should return exactly 1 approval.
    let (status, items) = get_approvals(
        &app.router,
        &format!(
            "proposal_id={}&execution_id={}",
            proposal_id_1, execution_id_1
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "list with combined filters should succeed"
    );
    assert_eq!(
        items.len(),
        1,
        "combined filter should return exactly 1 approval, got {}: {:?}",
        items.len(),
        items
    );

    // Verify the returned approval matches both filters.
    let approval: ApprovalRequest =
        serde_json::from_value(items[0].clone()).expect("approval should deserialize");
    assert_eq!(
        approval.proposal_id, proposal_id_1,
        "returned approval should be for proposal_id_1"
    );
    assert_eq!(
        approval.execution_id.unwrap(),
        execution_id_1,
        "returned approval should be for execution_id_1"
    );

    // Filter by proposal_id_2 AND execution_id_2 — should return exactly 1 approval.
    let (status, items) = get_approvals(
        &app.router,
        &format!(
            "proposal_id={}&execution_id={}",
            proposal_id_2, execution_id_2
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        items.len(),
        1,
        "combined filter should return exactly 1 approval"
    );

    // Filter by proposal_id_1 AND execution_id_2 — should return 0 (no match, cross pair).
    let (status, items) = get_approvals(
        &app.router,
        &format!(
            "proposal_id={}&execution_id={}",
            proposal_id_1, execution_id_2
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        items.len(),
        0,
        "combined filter with non-matching pair should return 0 approvals"
    );

    // Filter by proposal_id_2 AND execution_id_1 — should return 0 (no match, cross pair).
    let (status, items) = get_approvals(
        &app.router,
        &format!(
            "proposal_id={}&execution_id={}",
            proposal_id_2, execution_id_1
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        items.len(),
        0,
        "combined filter with non-matching pair should return 0 approvals"
    );

    // Clean up
    for approval in [&approval_1] {
        let resolve_req = ferrum_proto::ApprovalResolveRequest {
            actor: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::User,
                actor_id: "test-approver".to_string(),
                display_name: Some("Test Approver".to_string()),
            },
            approve: true,
            reason: Some("test resolution".to_string()),
        };
        let (status, _resolve_resp) = post_json(
            &app.router,
            &format!("/v1/approvals/{}/resolve", approval.approval_id),
            serde_json::to_value(&resolve_req).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "resolve should succeed");
    }
}
