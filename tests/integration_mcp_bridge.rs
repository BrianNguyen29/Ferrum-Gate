//! Integration test for the MCP bridge against a local gateway runtime.
//!
//! This test proves that `McpBridge::ingest` works end-to-end:
//! 1. Creates a local gateway runtime with an execution containing internal events
//! 2. Starts a real HTTP server on a random port
//! 3. Uses `McpBridge` to ingest an external MCP event
//! 4. Verifies lineage shows both internal and external events in the same execution chain

use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_integrations_mcp::types::{SessionStarted, ToolCallCompleted, ToolCallStarted};
use ferrum_integrations_mcp::{McpBridge, McpRuntimeEvent};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    AuthorizeExecutionRequest, CapabilityMintRequest, EffectType, IntentCompileRequest, JsonMap,
    LineageQueryRequest, ProvenanceEdgeType, ProvenanceEventKind, ResourceBinding, ResourceMode,
    RiskTier, RollbackClass, TaintBudget, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::spawn;
use tower::util::ServiceExt;

/// Creates a test gateway runtime with an in-memory SQLite store.
///
/// The returned `TempDir` must be kept alive for the duration of the test
/// since the runtime holds a connection to the SQLite database.
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

/// Creates an execution with internal provenance events by calling the gateway endpoints.
async fn create_execution_with_events(
    runtime: &GatewayRuntime,
) -> (ferrum_proto::ExecutionId, ferrum_proto::EventId) {
    let path = "/tmp/test-path".to_string();

    // Compile intent
    let req = IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
        agent_plan_summary: None,
        trusted_context: JsonMap::new(),
        raw_inputs: vec![],
        requested_resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: path.clone(),
            mode: ResourceMode::Write,
            content_hash: None,
        }],
        requested_risk_tier: Some(RiskTier::Medium),
        effect_type: Some(EffectType::FileMutation),
        metadata: JsonMap::new(),
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
    let compile_resp: ferrum_proto::IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Create proposal
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 1,
        title: "Test proposal".to_string(),
        tool_name: "fs.write".to_string(),
        server_name: "workspace".to_string(),
        raw_arguments: serde_json::json!({"path": path, "content": "hello"}),
        expected_effect: "write a file".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: vec![],
        metadata: JsonMap::new(),
        created_at: chrono::Utc::now(),
    };
    let proposal_id = proposal.proposal_id;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!("/v1/proposals/{}/evaluate", proposal_id))
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&proposal).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

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
            path: path.clone(),
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
        metadata: JsonMap::new(),
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

    // Authorize execution
    let auth_req = AuthorizeExecutionRequest {
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

    // Query to get the first internal event (parent event)
    let query_req = ferrum_proto::ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
        ..Default::default()
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/query")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&query_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let query_resp: ferrum_proto::ProvenanceQueryResponse = serde_json::from_slice(&body).unwrap();
    let parent_event_id = query_resp
        .events
        .first()
        .expect("expected at least one internal event")
        .event_id;

    (execution_id, parent_event_id)
}

/// Test: McpBridge::ingest creates an external event linked to internal events in the same execution.
///
/// This proves:
/// - The MCP bridge correctly POSTs to the gateway external-event ingest endpoint
/// - The ingested event is persisted with the correct execution_id and parent_event_id
/// - Lineage query by execution_id shows both internal and external events in the same chain
#[tokio::test]
async fn test_mcp_bridge_ingest_creates_linked_external_event() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir; // Keep alive for test duration

    // Create execution with internal events
    let (execution_id, parent_event_id) = create_execution_with_events(&runtime).await;

    // Start a real HTTP server on a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://127.0.0.1:{}", addr.port());

    // Spawn the server in the background
    let runtime_clone = runtime.clone();
    let server_handle = spawn(async move {
        let app = build_router(runtime_clone);
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create MCP bridge pointing to our local server
    let bridge = McpBridge::new(&server_url, "mcp-test-bridge").expect("bridge should construct");

    // Ingest an external MCP event using the bridge
    let mcp_event = McpRuntimeEvent::ToolCallCompleted(ToolCallCompleted {
        tool_name: "bash".to_string(),
        input_json: r#"{"cmd": "echo hello"}"#.into(),
        output_json: Some(r#"{"stdout":"hello\n"}"#.into()),
        exit_code: 0,
        completed_at: None,
        summary: Some("echo hello completed".to_string()),
    });

    let ingested_event = bridge
        .ingest(execution_id, parent_event_id, mcp_event)
        .await
        .expect("ingest should succeed");

    // Verify the ingested event has correct properties
    assert_eq!(
        ingested_event.kind,
        ProvenanceEventKind::ExternalEventObserved,
        "ingested event should be ExternalEventObserved"
    );
    assert_eq!(
        ingested_event.execution_id,
        Some(execution_id),
        "ingested event should belong to the same execution"
    );
    assert_eq!(
        ingested_event.parent_edges.len(),
        1,
        "ingested event should have exactly one parent edge"
    );
    assert_eq!(
        ingested_event.parent_edges[0].from_event_id, parent_event_id,
        "ingested event should reference the internal parent event"
    );
    assert_eq!(
        ingested_event.parent_edges[0].edge_type,
        ProvenanceEdgeType::ObservedBy,
        "edge type should be ObservedBy for external events"
    );

    // Verify the ingested event has expected metadata
    let source_system = ingested_event
        .metadata
        .get("source_system")
        .and_then(|v| v.as_str());
    assert_eq!(
        source_system,
        Some("mcp-test-bridge"),
        "source_system should be the bridge-configured value"
    );

    // Verify the source_event_id was set
    let source_event_id = ingested_event
        .metadata
        .get("source_event_id")
        .and_then(|v| v.as_str());
    assert!(source_event_id.is_some(), "source_event_id should be set");
    assert!(
        source_event_id.unwrap().starts_with("mcp.tool.completed:"),
        "source_event_id should follow the expected format"
    );

    // Query lineage starting from the ingested event to verify chain
    let lineage_req = LineageQueryRequest {
        execution_id,
        event_id: ingested_event.event_id,
        ancestry: true,
        descendants: true,
        max_hops: Some(8),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/lineage")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&lineage_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200, "lineage query should succeed");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp: ferrum_proto::LineageQueryResponse = serde_json::from_slice(&body).unwrap();

    // Verify both internal and external events are in the lineage
    let event_ids: Vec<_> = lineage_resp.events.iter().map(|e| e.event_id).collect();
    assert!(
        event_ids.contains(&ingested_event.event_id),
        "ingested external event should appear in lineage"
    );
    assert!(
        event_ids.contains(&parent_event_id),
        "internal parent event should appear in lineage"
    );

    // Verify the ingested event's source_event_id matches the metadata
    let ingested_source_id = lineage_resp
        .events
        .iter()
        .find(|e| e.event_id == ingested_event.event_id)
        .and_then(|e| e.metadata.get("source_event_id"))
        .and_then(|v| v.as_str());
    assert_eq!(
        ingested_source_id, source_event_id,
        "lineage should contain the same source_event_id"
    );

    // Shutdown the server
    server_handle.abort();
}

/// Test: McpBridge::ingest works with different MCP event types
#[tokio::test]
async fn test_mcp_bridge_ingest_multiple_event_types() {
    // Create gateway runtime
    let (temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &temp_dir;

    // Create execution with internal events
    let (execution_id, parent_event_id) = create_execution_with_events(&runtime).await;

    // Start HTTP server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://127.0.0.1:{}", addr.port());

    let runtime_clone = runtime.clone();
    let server_handle = spawn(async move {
        let app = build_router(runtime_clone);
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create MCP bridge
    let bridge = McpBridge::new(&server_url, "mcp-test").expect("bridge should construct");

    // Ingest a ToolCallStarted event
    let started_event = McpRuntimeEvent::ToolCallStarted(ToolCallStarted {
        tool_name: "read_file".to_string(),
        input_json: r#"{"path": "/etc/passwd"}"#.into(),
        started_at: None,
        summary: None,
    });

    let started_ingested = bridge
        .ingest(execution_id, parent_event_id, started_event)
        .await
        .expect("ingest ToolCallStarted should succeed");

    assert_eq!(
        started_ingested.kind,
        ProvenanceEventKind::ExternalEventObserved
    );

    // Ingest a SessionStarted event
    let session_event = McpRuntimeEvent::SessionStarted(SessionStarted {
        session_id: "sess-abc123".to_string(),
        transport_type: Some("stdio".into()),
        started_at: None,
        summary: None,
    });

    let session_ingested = bridge
        .ingest(execution_id, started_ingested.event_id, session_event)
        .await
        .expect("ingest SessionStarted should succeed");

    assert_eq!(
        session_ingested.kind,
        ProvenanceEventKind::ExternalEventObserved
    );
    // SessionStarted should have ToolCallStarted as its parent
    assert_eq!(
        session_ingested.parent_edges[0].from_event_id,
        started_ingested.event_id
    );

    // Shutdown
    server_handle.abort();
}
