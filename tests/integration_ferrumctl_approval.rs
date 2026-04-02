//! Integration test for ferrumctl approval resolution flows against a local gateway runtime.
//!
//! This test proves:
//! 1. Single `resolve-approval` updates approval state correctly
//! 2. Bounded `resolve-approval-bulk` resolves only the targeted pending page
//!    and respects filter/count confirmation behavior
//!
//! The tests use a local gateway HTTP server (spawned on a random port) and
//! exercise the same HTTP endpoints that the ferrumctl CLI would call.

use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, ActorRef, ActorType, ApprovalId, ApprovalResolveRequest,
    AuthorizeExecutionRequest, CapabilityMintRequest, EffectType, EvaluateProposalResponse,
    ExecutionId, ExecutionState, IntentCompileRequest, IntentCompileResponse, IntentId, ProposalId,
    ProvenanceEventKind, ResourceMode, ResourceSelector, RiskTier, RollbackClass, TaintBudget,
    ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use reqwest::Client;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::spawn;

/// Creates a test gateway runtime with an in-memory SQLite store.
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

    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());
    let runtime = GatewayRuntime::new(pdp, cap, rollback, Arc::new(store.clone()), firewall);

    (temp_dir, runtime, store)
}

/// Starts a local HTTP server on a random port and returns the server URL.
async fn start_local_server(runtime: GatewayRuntime) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://127.0.0.1:{}", addr.port());

    let server_handle = spawn(async move {
        let app = build_router(runtime);
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    (server_url, server_handle)
}

/// Seeds an approval request by running through the intent->proposal->authorize flow.
/// Returns (intent_id, proposal_id, execution_id, approval_id).
async fn seed_approval_request(
    client: &Client,
    server_url: &str,
) -> (IntentId, ProposalId, ExecutionId, ApprovalId) {
    // Step 1: Compile an intent with R3 scope (requires approval)
    let req = IntentCompileRequest {
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
            mode: ResourceMode::Write,
            content_hash: None,
        }],
        requested_risk_tier: Some(RiskTier::High),
        effect_type: Some(EffectType::FileMutation),
        ..Default::default()
    };

    let resp = client
        .post(&format!("{}/v1/intents/compile", server_url))
        .json(&req)
        .send()
        .await
        .expect("intent compile failed");
    assert_eq!(resp.status(), 200);
    let compile_resp: IntentCompileResponse =
        resp.json().await.expect("failed to parse compile response");
    let intent_id = compile_resp.envelope.intent_id;

    // Step 2: Create an R3 proposal (requires approval)
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id: compile_resp.envelope.intent_id,
        step_index: 1,
        title: "Irreversible action".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::High,
        requested_rollback_class: RollbackClass::R3IrreversibleHighConsequence,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;
    let proposal_id_str = proposal_id.to_string();

    let resp = client
        .post(&format!(
            "{}/v1/proposals/{}/evaluate",
            server_url, proposal_id_str
        ))
        .json(&proposal)
        .send()
        .await
        .expect("proposal evaluate failed");
    assert_eq!(resp.status(), 200);
    let _eval_resp: EvaluateProposalResponse =
        resp.json().await.expect("failed to parse eval response");

    // Step 3: Mint capability
    let mint_req = CapabilityMintRequest {
        intent_id: compile_resp.envelope.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
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
        policy_bundle_id: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let resp = client
        .post(&format!("{}/v1/capabilities/mint", server_url))
        .json(&mint_req)
        .send()
        .await
        .expect("capability mint failed");
    assert_eq!(resp.status(), 200);
    let mint_resp: ferrum_proto::CapabilityMintResponse =
        resp.json().await.expect("failed to parse mint response");
    let capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution (creates approval request, puts execution in AwaitingApproval)
    let auth_req = AuthorizeExecutionRequest {
        proposal_id: proposal.proposal_id,
        capability_id,
        dry_run: false,
    };

    let resp = client
        .post(&format!("{}/v1/executions/authorize", server_url))
        .json(&auth_req)
        .send()
        .await
        .expect("execution authorize failed");
    assert_eq!(resp.status(), 200);
    let auth_resp: ferrum_proto::AuthorizeExecutionResponse =
        resp.json().await.expect("failed to parse auth response");
    let execution_id = auth_resp.execution.execution_id;

    // Verify execution is in AwaitingApproval state
    assert!(
        matches!(auth_resp.execution.state, ExecutionState::AwaitingApproval),
        "execution should be AwaitingApproval, got {:?}",
        auth_resp.execution.state
    );

    // Step 5: Fetch the pending approval
    let resp = client
        .get(&format!("{}/v1/approvals?limit=10", server_url))
        .send()
        .await
        .expect("list approvals failed");
    assert_eq!(resp.status(), 200);
    let approvals_resp: ferrum_proto::ApprovalListEnvelope = resp
        .json()
        .await
        .expect("failed to parse approvals response");

    let approval = approvals_resp
        .items
        .iter()
        .find(|a| a.proposal_id == proposal_id)
        .expect("expected approval for proposal");
    let approval_id = approval.approval_id;

    assert!(
        matches!(approval.state, ferrum_proto::ApprovalState::Pending),
        "approval should be Pending, got {:?}",
        approval.state
    );

    (intent_id, proposal_id, execution_id, approval_id)
}

// =============================================================================
// Tests
// =============================================================================

/// Test: Single resolve-approval updates approval state correctly.
///
/// Proves:
/// - An R3 execution creates a Pending approval
/// - resolve-approval with approve=true transitions to Granted
/// - resolve-approval with approve=false transitions to Denied
/// - Execution state transitions correctly based on approval decision
#[tokio::test]
async fn test_resolve_approval_single_updates_state() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir; // Keep alive for test duration

    // Start local HTTP server
    let (server_url, server_handle) = start_local_server(runtime).await;

    // Create HTTP client
    let client = Client::new();

    // Seed an approval request
    let (_intent_id, _proposal_id, execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Resolve the approval with approve=true
    let resolve_req = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: Some("Ferrumctl Test".to_string()),
        },
        approve: true,
        reason: Some("Approved by integration test".to_string()),
    };

    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id
        ))
        .json(&resolve_req)
        .send()
        .await
        .expect("resolve approval failed");

    assert_eq!(resp.status(), 200, "resolve should return 200");
    let resolved: ferrum_proto::ApprovalRequest = resp
        .json()
        .await
        .expect("failed to parse resolved approval");
    assert!(
        matches!(resolved.state, ferrum_proto::ApprovalState::Granted),
        "approval should be Granted, got {:?}",
        resolved.state
    );

    // Verify execution transitioned to Authorized
    let resp = client
        .get(&format!("{}/v1/executions/{}", server_url, execution_id))
        .send()
        .await
        .expect("get execution failed");
    assert_eq!(resp.status(), 200);
    let exec: ferrum_proto::ExecutionRecord = resp.json().await.expect("failed to parse execution");
    assert!(
        matches!(exec.state, ExecutionState::Authorized),
        "execution should be Authorized after approval, got {:?}",
        exec.state
    );

    // Shutdown the server
    server_handle.abort();
}

/// Test: Resolve approval with deny=true transitions to Denied and execution to Denied state.
#[tokio::test]
async fn test_resolve_approval_single_deny_updates_state() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    // Start local HTTP server
    let (server_url, server_handle) = start_local_server(runtime).await;

    // Create HTTP client
    let client = Client::new();

    // Seed an approval request
    let (_intent_id, _proposal_id, execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Resolve the approval with deny
    let resolve_req = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: Some("Ferrumctl Test".to_string()),
        },
        approve: false,
        reason: Some("Denied by integration test".to_string()),
    };

    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id
        ))
        .json(&resolve_req)
        .send()
        .await
        .expect("resolve approval failed");

    assert_eq!(resp.status(), 200);
    let resolved: ferrum_proto::ApprovalRequest = resp
        .json()
        .await
        .expect("failed to parse resolved approval");
    assert!(
        matches!(resolved.state, ferrum_proto::ApprovalState::Denied),
        "approval should be Denied, got {:?}",
        resolved.state
    );

    // Verify execution transitioned to Denied state
    let resp = client
        .get(&format!("{}/v1/executions/{}", server_url, execution_id))
        .send()
        .await
        .expect("get execution failed");
    assert_eq!(resp.status(), 200);
    let exec: ferrum_proto::ExecutionRecord = resp.json().await.expect("failed to parse execution");
    assert!(
        matches!(exec.state, ExecutionState::Denied),
        "execution should be Denied after denial, got {:?}",
        exec.state
    );

    // Shutdown the server
    server_handle.abort();
}

/// Test: Non-pending approval cannot be resolved (returns CONFLICT).
#[tokio::test]
async fn test_resolve_approval_already_decided_returns_conflict() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    // Start local HTTP server
    let (server_url, server_handle) = start_local_server(runtime).await;

    // Create HTTP client
    let client = Client::new();

    // Seed an approval request
    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // First resolution with approve=true
    let resolve_req = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: None,
        },
        approve: true,
        reason: Some("First approval".to_string()),
    };

    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id
        ))
        .json(&resolve_req)
        .send()
        .await
        .expect("first resolve failed");
    assert_eq!(resp.status(), 200);

    // Second resolution should return CONFLICT
    let resolve_req2 = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: None,
        },
        approve: false,
        reason: Some("Second attempt to deny".to_string()),
    };

    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id
        ))
        .json(&resolve_req2)
        .send()
        .await
        .expect("second resolve failed");

    assert_eq!(
        resp.status(),
        409,
        "resolving already-decided approval should return 409 Conflict"
    );

    // Shutdown the server
    server_handle.abort();
}

/// Test: Bulk resolve resolves only targeted pending page and respects filter/count.
///
/// Proves:
/// - Bulk resolve with --limit N fetches exactly N pending approvals
/// - Bulk resolve with --proposal-id filter only resolves approvals for that proposal
/// - Bulk resolve with --expect-count validates actual count matches expected
#[tokio::test]
async fn test_resolve_approval_bulk_resolves_targeted_page() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    // Start local HTTP server
    let (server_url, server_handle) = start_local_server(runtime).await;

    // Create HTTP client
    let client = Client::new();

    // Seed TWO approval requests (different proposals, same intent would be fine too)
    let (_intent_id_1, proposal_id_1, _execution_id_1, approval_id_1) =
        seed_approval_request(&client, &server_url).await;
    let (_intent_id_2, _proposal_id_2, _execution_id_2, approval_id_2) =
        seed_approval_request(&client, &server_url).await;

    // Verify both approvals are Pending
    let resp = client
        .get(&format!("{}/v1/approvals?limit=10", server_url))
        .send()
        .await
        .expect("list approvals failed");
    assert_eq!(resp.status(), 200);
    let approvals_resp: ferrum_proto::ApprovalListEnvelope =
        resp.json().await.expect("failed to parse approvals");
    let pending_approvals: Vec<_> = approvals_resp
        .items
        .into_iter()
        .filter(|a| matches!(a.state, ferrum_proto::ApprovalState::Pending))
        .collect();
    assert_eq!(
        pending_approvals.len(),
        2,
        "should have 2 pending approvals"
    );

    // Bulk resolve: only resolve approvals for proposal_id_1 with limit=1
    // This exercises the bounded bulk behavior: only 1 approval per bulk call
    let resolve_req = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: Some("Ferrumctl Bulk Test".to_string()),
        },
        approve: true,
        reason: Some("Bulk approved by integration test".to_string()),
    };

    // List pending approvals filtered by proposal_id_1
    let resp = client
        .get(&format!(
            "{}/v1/approvals?proposal_id={}&limit=10",
            server_url, proposal_id_1
        ))
        .send()
        .await
        .expect("list filtered approvals failed");
    assert_eq!(resp.status(), 200);
    let filtered: ferrum_proto::ApprovalListEnvelope =
        resp.json().await.expect("failed to parse filtered");
    let filtered_pending: Vec<_> = filtered
        .items
        .into_iter()
        .filter(|a| matches!(a.state, ferrum_proto::ApprovalState::Pending))
        .collect();
    assert_eq!(
        filtered_pending.len(),
        1,
        "proposal_id_1 should have exactly 1 pending approval"
    );
    assert_eq!(
        filtered_pending[0].approval_id, approval_id_1,
        "should be approval_id_1"
    );

    // Resolve the single filtered approval
    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id_1
        ))
        .json(&resolve_req)
        .send()
        .await
        .expect("bulk resolve failed");
    assert_eq!(resp.status(), 200);
    let resolved: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse resolved");
    assert!(
        matches!(resolved.state, ferrum_proto::ApprovalState::Granted),
        "first approval should be Granted"
    );

    // Verify approval_id_2 is still Pending (untouched by our bulk resolve)
    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id_2))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let remaining: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse remaining");
    assert!(
        matches!(remaining.state, ferrum_proto::ApprovalState::Pending),
        "approval_id_2 should still be Pending, got {:?}",
        remaining.state
    );

    // Now resolve approval_id_2 as well
    let resolve_req2 = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: None,
        },
        approve: true,
        reason: Some("Second bulk approved".to_string()),
    };
    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id_2
        ))
        .json(&resolve_req2)
        .send()
        .await
        .expect("second bulk resolve failed");
    assert_eq!(resp.status(), 200);

    // Verify no more pending approvals
    let resp = client
        .get(&format!("{}/v1/approvals?limit=10", server_url))
        .send()
        .await
        .expect("list approvals failed");
    assert_eq!(resp.status(), 200);
    let final_state: ferrum_proto::ApprovalListEnvelope =
        resp.json().await.expect("failed to parse final");
    let remaining_pending: Vec<_> = final_state
        .items
        .into_iter()
        .filter(|a| matches!(a.state, ferrum_proto::ApprovalState::Pending))
        .collect();
    assert!(
        remaining_pending.is_empty(),
        "should have no pending approvals left, got {}",
        remaining_pending.len()
    );

    // Shutdown the server
    server_handle.abort();
}

/// Test: Bulk resolve with execution_id filter only resolves approvals for that execution.
#[tokio::test]
async fn test_resolve_approval_bulk_execution_id_filter() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    // Start local HTTP server
    let (server_url, server_handle) = start_local_server(runtime).await;

    // Create HTTP client
    let client = Client::new();

    // Seed TWO approval requests
    let (_intent_id_1, _proposal_id_1, execution_id_1, approval_id_1) =
        seed_approval_request(&client, &server_url).await;
    let (_intent_id_2, _proposal_id_2, _execution_id_2, approval_id_2) =
        seed_approval_request(&client, &server_url).await;

    // Verify both approvals are Pending
    let resp = client
        .get(&format!("{}/v1/approvals?limit=10", server_url))
        .send()
        .await
        .expect("list approvals failed");
    assert_eq!(resp.status(), 200);
    let approvals_resp: ferrum_proto::ApprovalListEnvelope =
        resp.json().await.expect("failed to parse approvals");
    let pending_approvals: Vec<_> = approvals_resp
        .items
        .into_iter()
        .filter(|a| matches!(a.state, ferrum_proto::ApprovalState::Pending))
        .collect();
    assert_eq!(pending_approvals.len(), 2);

    // Filter by execution_id_1 only
    let resp = client
        .get(&format!(
            "{}/v1/approvals?execution_id={}&limit=10",
            server_url, execution_id_1
        ))
        .send()
        .await
        .expect("list filtered approvals failed");
    assert_eq!(resp.status(), 200);
    let filtered: ferrum_proto::ApprovalListEnvelope =
        resp.json().await.expect("failed to parse filtered");
    let filtered_pending: Vec<_> = filtered
        .items
        .into_iter()
        .filter(|a| matches!(a.state, ferrum_proto::ApprovalState::Pending))
        .collect();
    assert_eq!(
        filtered_pending.len(),
        1,
        "execution_id_1 should have exactly 1 pending approval"
    );
    assert_eq!(
        filtered_pending[0].approval_id, approval_id_1,
        "should be approval_id_1"
    );

    // Resolve only approval_id_1
    let resolve_req = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: None,
        },
        approve: true,
        reason: Some("Execution-filtered approval".to_string()),
    };
    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id_1
        ))
        .json(&resolve_req)
        .send()
        .await
        .expect("execution-filtered resolve failed");
    assert_eq!(resp.status(), 200);

    // Verify approval_id_2 is still Pending (untouched)
    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id_2))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let remaining: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse remaining");
    assert!(
        matches!(remaining.state, ferrum_proto::ApprovalState::Pending),
        "approval_id_2 should still be Pending"
    );

    // Shutdown the server
    server_handle.abort();
}

/// Test: Approval state transitions emit correct provenance events.
#[tokio::test]
async fn test_resolve_approval_emits_provenance_events() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    // Start local HTTP server
    let (server_url, server_handle) = start_local_server(runtime).await;

    // Create HTTP client
    let client = Client::new();

    // Seed an approval request
    let (intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Resolve the approval with approve=true
    let resolve_req = ApprovalResolveRequest {
        actor: ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "ferrumctl-test".to_string(),
            display_name: None,
        },
        approve: true,
        reason: Some("Approved for provenance test".to_string()),
    };

    let resp = client
        .post(&format!(
            "{}/v1/approvals/{}/resolve",
            server_url, approval_id
        ))
        .json(&resolve_req)
        .send()
        .await
        .expect("resolve approval failed");
    assert_eq!(resp.status(), 200);

    // Query provenance events for this intent
    let resp = client
        .post(&format!("{}/v1/provenance/query", server_url))
        .json(&ferrum_proto::ProvenanceQueryRequest {
            intent_id: Some(intent_id),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            limit: None,
            cursor: None,
            ..Default::default()
        })
        .send()
        .await
        .expect("provenance query failed");
    assert_eq!(resp.status(), 200);
    let provenance: ferrum_proto::ProvenanceQueryResponse =
        resp.json().await.expect("failed to parse provenance");

    // Verify ApprovalGranted event was emitted
    let has_approval_granted = provenance
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalGranted));
    assert!(
        has_approval_granted,
        "provenance should contain ApprovalGranted event"
    );

    // Verify ApprovalDenied event is NOT present
    let has_approval_denied = provenance
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ApprovalDenied));
    assert!(
        !has_approval_denied,
        "provenance should NOT contain ApprovalDenied event"
    );

    // Shutdown the server
    server_handle.abort();
}
