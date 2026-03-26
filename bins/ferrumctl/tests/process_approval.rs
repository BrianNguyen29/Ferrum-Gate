//! Process-level integration tests for ferrumctl approval commands.
//!
//! These tests spawn the **real** `ferrumctl` binary (`env!("CARGO_BIN_EXE_ferrumctl")`)
//! against a local gateway runtime to prove end-to-end CLI behavior.
//!
//! Coverage:
//! - `server resolve-approval` — single approval resolution via CLI
//! - `server resolve-approval-bulk` — bulk approval resolution via CLI
//! - `server watch-approvals` — polling watch for pending approvals
//!
//! Each test:
//! 1. Spins up a local gateway HTTP server on a random port
//! 2. Seeds a pending approval via the REST API
//! 3. Spawns `ferrumctl` as a child process with FERRUMCTL_SERVER_URL
//! 4. Validates the CLI output / exit code against the known approval ID

use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, ApprovalId, AuthorizeExecutionRequest, CapabilityMintRequest, EffectType,
    EvaluateProposalResponse, ExecutionId, ExecutionState, IntentCompileRequest,
    IntentCompileResponse, IntentId, ProposalId, ResourceMode, ResourceSelector, RiskTier,
    RollbackClass, TaintBudget, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use reqwest::Client;
use std::env;
use std::process::Stdio;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::spawn;
use tokio::time::{Duration, sleep};

// ---------------------------------------------------------------------------
// Test runtime bootstrap (same pattern as integration_ferrumctl_approval.rs)
// ---------------------------------------------------------------------------

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
        let app: axum::Router = build_router(runtime);
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    sleep(Duration::from_millis(50)).await;

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
        metadata: ferrum_proto::JsonMap::new(),
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
    let _capability_id = mint_resp.lease.capability_id;

    // Step 4: Authorize execution (creates approval request)
    let auth_req = AuthorizeExecutionRequest {
        proposal_id: proposal.proposal_id,
        capability_id: mint_resp.lease.capability_id,
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

// ---------------------------------------------------------------------------
// ferrumctl binary wrapper
// ---------------------------------------------------------------------------

/// Runs the `ferrumctl` binary with the given arguments and FERRUMCTL_SERVER_URL.
/// Uses tokio::process::Command to avoid blocking the single-threaded tokio runtime.
async fn run_ferrumctl<I, S>(server_url: &str, args: I) -> std::process::Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let bin_path = env!("CARGO_BIN_EXE_ferrumctl");
    let mut cmd = tokio::process::Command::new(bin_path);
    cmd.env("FERRUMCTL_SERVER_URL", server_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Collect args as OsString for Command::args
    let mut args_vec: Vec<std::ffi::OsString> = vec!["server".into()];
    for arg in args {
        args_vec.push(arg.as_ref().into());
    }
    cmd.args(&args_vec);

    cmd.output().await.expect("ferrumctl process failed")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test: `ferrumctl server resolve-approval --approve` transitions approval to Granted
/// and exits with code 0.
#[tokio::test]
async fn test_process_resolve_approval_approve_via_binary() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir; // keep alive for test duration

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    // Seed a pending approval
    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Spawn ferrumctl resolve-approval --approve
    // Note: --actor-type values are case-insensitive kebab: operator, agent, user, etc.
    let output = run_ferrumctl(
        &server_url,
        [
            "resolve-approval",
            &approval_id.to_string(),
            "--approve",
            "--actor-type",
            "operator",
            "--actor-id",
            "process-test",
            "--json",
        ],
    )
    .await;

    // Verify exit code 0
    assert!(
        output.status.success(),
        "ferrumctl should exit 0, got {}. stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify output mentions the approval and shows Granted state
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("Granted") || stdout.contains("Granted"),
        "stdout should contain Granted. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Verify approval is now Granted via API
    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let updated: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse updated approval");
    assert!(
        matches!(updated.state, ferrum_proto::ApprovalState::Granted),
        "approval should be Granted after CLI resolve, got {:?}",
        updated.state
    );

    server_handle.abort();
}

/// Test: `ferrumctl server resolve-approval --deny` requires --reason and exits 0.
#[tokio::test]
async fn test_process_resolve_approval_deny_via_binary() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Spawn ferrumctl resolve-approval --deny --reason ...
    let output = run_ferrumctl(
        &server_url,
        [
            "resolve-approval",
            &approval_id.to_string(),
            "--deny",
            "--reason",
            "Not authorized by process test",
            "--actor-id",
            "process-test",
            "--json",
        ],
    )
    .await;

    assert!(
        output.status.success(),
        "ferrumctl should exit 0 on deny, got {}. stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Denied"),
        "stdout should contain Denied. stdout: {}",
        stdout
    );

    // Verify approval is now Denied via API
    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let updated: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse updated approval");
    assert!(
        matches!(updated.state, ferrum_proto::ApprovalState::Denied),
        "approval should be Denied, got {:?}",
        updated.state
    );

    server_handle.abort();
}

/// Test: `ferrumctl server resolve-approval` fails without --approve or --deny.
#[tokio::test]
async fn test_process_resolve_approval_requires_approve_or_deny() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Spawn ferrumctl resolve-approval WITHOUT --approve or --deny
    let output = run_ferrumctl(
        &server_url,
        [
            "resolve-approval",
            &approval_id.to_string(),
            "--actor-id",
            "process-test",
        ],
    )
    .await;

    // Should fail (non-zero exit)
    assert!(
        !output.status.success(),
        "ferrumctl should exit non-zero without --approve/--deny"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("approve") || stderr.contains("deny"),
        "stderr should mention the missing flag. stderr: {}",
        stderr
    );

    server_handle.abort();
}

/// Test: `ferrumctl server watch-approvals --iterations 1` shows pending approvals.
#[tokio::test]
async fn test_process_watch_approvals_shows_pending() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Spawn ferrumctl watch-approvals --iterations 1
    let output = run_ferrumctl(
        &server_url,
        ["watch-approvals", "--iterations", "1", "--json"],
    )
    .await;

    assert!(
        output.status.success(),
        "watch-approvals should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The JSON output should contain our seeded approval_id
    assert!(
        stdout.contains(&approval_id.to_string()),
        "watch-approvals JSON output should contain the seeded approval_id. stdout: {}",
        stdout
    );

    server_handle.abort();
}

/// Test: `ferrumctl server resolve-approval-bulk` with correct flags resolves approvals.
#[tokio::test]
async fn test_process_resolve_approval_bulk_via_binary() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    // Seed ONE approval
    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Get the proposal_id for bulk filter
    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let approval: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse approval");
    let proposal_id_str = approval.proposal_id.to_string();

    // Spawn ferrumctl resolve-approval-bulk with all required flags
    let output = run_ferrumctl(
        &server_url,
        [
            "resolve-approval-bulk",
            "--proposal-id",
            &proposal_id_str,
            "--limit",
            "1",
            "--yes",
            "--expect-count",
            "1",
            "--approve",
            "--actor-id",
            "bulk-process-test",
        ],
    )
    .await;

    assert!(
        output.status.success(),
        "bulk resolve should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("RESOLVED") || stdout.contains("resolved"),
        "bulk output should indicate resolution. stdout: {}",
        stdout
    );

    // Verify approval is now Granted via API
    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let updated: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse updated approval");
    assert!(
        matches!(updated.state, ferrum_proto::ApprovalState::Granted),
        "approval should be Granted after bulk resolve, got {:?}",
        updated.state
    );

    server_handle.abort();
}

/// Test: `ferrumctl server resolve-approval-bulk` without --yes is rejected.
#[tokio::test]
async fn test_process_resolve_approval_bulk_requires_yes() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    let resp = client
        .get(&format!("{}/v1/approvals/{}", server_url, approval_id))
        .send()
        .await
        .expect("get approval failed");
    assert_eq!(resp.status(), 200);
    let approval: ferrum_proto::ApprovalRequest =
        resp.json().await.expect("failed to parse approval");
    let proposal_id_str = approval.proposal_id.to_string();

    // Missing --yes flag
    let output = run_ferrumctl(
        &server_url,
        [
            "resolve-approval-bulk",
            "--proposal-id",
            &proposal_id_str,
            "--limit",
            "1",
            "--expect-count",
            "1",
            "--approve",
        ],
    )
    .await;

    assert!(
        !output.status.success(),
        "bulk resolve without --yes should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--yes") || stderr.contains("confirm"),
        "stderr should mention missing --yes. stderr: {}",
        stderr
    );

    server_handle.abort();
}

/// Test: `ferrumctl server inspect-approvals` lists the seeded approval correctly.
#[tokio::test]
async fn test_process_inspect_approvals_shows_pending() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    let (_intent_id, _proposal_id, _execution_id, approval_id) =
        seed_approval_request(&client, &server_url).await;

    // Spawn ferrumctl inspect-approvals
    let output = run_ferrumctl(&server_url, ["inspect-approvals", "--limit", "10"]).await;

    assert!(
        output.status.success(),
        "inspect-approvals should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&approval_id.to_string()),
        "inspect-approvals output should contain the seeded approval_id. stdout: {}",
        stdout
    );

    server_handle.abort();
}
