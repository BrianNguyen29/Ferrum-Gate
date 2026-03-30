//! Process-level integration tests for ferrumctl revoke-capability command.
//!
//! These tests spawn the **real** `ferrumctl` binary (`env!("CARGO_BIN_EXE_ferrumctl")`)
//! against a local gateway runtime to prove end-to-end CLI behavior.
//!
//! Coverage:
//! - `server revoke-capability` — revoke a capability via CLI
//!
//! Each test:
//! 1. Spins up a local gateway HTTP server on a random port
//! 2. Seeds a capability via the REST API flow (intent → proposal → capability mint)
//! 3. Spawns `ferrumctl` as a child process with FERRUMCTL_SERVER_URL
//! 4. Validates the CLI output / exit code against the known capability ID

use ferrum_adapter_fs::FsRollbackAdapter;
use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, CapabilityMintRequest, EffectType, IntentCompileRequest, IntentCompileResponse,
    ProposalId, ResourceBinding, ResourceMode, ResourceSelector, RiskTier, RollbackClass,
    TaintBudget, ToolBinding,
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
// Test runtime bootstrap (reused from process_rollback.rs)
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

    let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine::default());
    let cap: Arc<dyn CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    registry.register(Arc::new(FsRollbackAdapter::new("fs")));
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

// ---------------------------------------------------------------------------
// Helper: seed a capability via the full intent → proposal → mint flow
// ---------------------------------------------------------------------------

/// Seeds a capability through the flow: intent → proposal → capability mint.
/// Returns the capability_id.
async fn seed_capability(client: &Client, server_url: &str) -> ferrum_proto::CapabilityId {
    // Step 1: Compile intent
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
        requested_risk_tier: Some(RiskTier::Medium),
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

    // Step 2: Create and evaluate proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Execute mutation".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": "/tmp/test.txt", "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R2Compensatable,
        decision: None,
        taint_inputs: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let resp = client
        .post(&format!(
            "{}/v1/proposals/{}/evaluate",
            server_url, proposal_id
        ))
        .json(&proposal)
        .send()
        .await
        .expect("proposal evaluate failed");
    assert_eq!(resp.status(), 200);

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

    let resp = client
        .post(&format!("{}/v1/capabilities/mint", server_url))
        .json(&mint_req)
        .send()
        .await
        .expect("capability mint failed");
    assert_eq!(resp.status(), 200);
    let mint_resp: ferrum_proto::CapabilityMintResponse =
        resp.json().await.expect("failed to parse mint response");
    mint_resp.lease.capability_id
}

// ---------------------------------------------------------------------------
// ferrumctl binary wrapper (reused from process_rollback.rs)
// ---------------------------------------------------------------------------

/// Runs the `ferrumctl` binary with the given arguments and FERRUMCTL_SERVER_URL.
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

/// Test: `ferrumctl server revoke-capability` revokes a valid capability and exits with code 0.
#[tokio::test]
async fn test_process_revoke_capability_via_binary() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir; // keep alive for test duration

    let (server_url, server_handle) = start_local_server(runtime).await;
    let client = Client::new();

    // Seed a capability using the intent → proposal → mint flow
    let capability_id = seed_capability(&client, &server_url).await;

    // Spawn ferrumctl revoke-capability
    let output = run_ferrumctl(
        &server_url,
        ["revoke-capability", &capability_id.to_string(), "--json"],
    )
    .await;

    // Verify exit code 0
    assert!(
        output.status.success(),
        "ferrumctl should exit 0, got {}. stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify output mentions the capability_id and shows ok: true
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(&capability_id.to_string()),
        "stdout should contain capability_id. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("ok") || stdout.contains("revoked"),
        "stdout should mention 'ok' or 'revoked'. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    server_handle.abort();
}

/// Test: `ferrumctl server revoke-capability` fails for non-existent capability.
#[tokio::test]
async fn test_process_revoke_capability_not_found() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let _client = Client::new();

    // Use a random capability_id that doesn't exist
    let fake_capability_id = ferrum_proto::CapabilityId::new();

    let output = run_ferrumctl(
        &server_url,
        [
            "revoke-capability",
            &fake_capability_id.to_string(),
            "--json",
        ],
    )
    .await;

    // Should fail (non-zero exit)
    assert!(
        !output.status.success(),
        "ferrumctl should exit non-zero for non-existent capability"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found")
            || stderr.contains("404")
            || stderr.contains("NotFound")
            || stderr.contains("revoked"),
        "stderr should mention not found or revoked. stderr: {}",
        stderr
    );

    server_handle.abort();
}

/// Test: `ferrumctl server revoke-capability` fails for invalid UUID.
#[tokio::test]
async fn test_process_revoke_capability_invalid_uuid() {
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    let (server_url, server_handle) = start_local_server(runtime).await;
    let _client = Client::new();

    let output = run_ferrumctl(
        &server_url,
        ["revoke-capability", "not-a-valid-uuid", "--json"],
    )
    .await;

    // Should fail (non-zero exit)
    assert!(
        !output.status.success(),
        "ferrumctl should exit non-zero for invalid UUID"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("UUID"),
        "stderr should mention invalid UUID. stderr: {}",
        stderr
    );

    server_handle.abort();
}
