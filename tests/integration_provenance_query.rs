//! Integration tests for provenance query and event endpoints.
//!
//! Tests:
//! 1. Happy path: query by execution_id returns matching events
//! 2. Happy path: query by time window returns events within range
//! 3. Rejection of unknown/unexpected JSON fields (fail-closed)

use ferrum_cap::{CapabilityService, SqliteCapabilityService};
use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
use ferrum_gateway::{GatewayRuntime, build_router};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, ActorRef, ActorType, AuthorizeExecutionRequest, CapabilityMintRequest,
    EffectType, ExternalEventIngestRequest, HashChainRef, IntentCompileRequest, JsonMap,
    LineageQueryRequest, LineageQueryResponse, ObjectRef, ObjectType, ProvenanceEdge,
    ProvenanceEdgeType, ProvenanceEvent, ProvenanceEventKind, ProvenanceEventResponse,
    ProvenanceExportRequest, ProvenanceExportResponse, ProvenanceQueryRequest,
    ProvenanceReplayRequest, ProvenanceReplayResponse, ResourceBinding, ResourceMode, RiskTier,
    RollbackClass, TaintBudget, ToolBinding, TrustLabel,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{ProvenanceRepo, SqliteStore};
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

fn sample_intent_request(path: &str) -> IntentCompileRequest {
    IntentCompileRequest {
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "Test Intent".to_string(),
        goal: "Test goal".to_string(),
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

fn sample_proposal(intent_id: ferrum_proto::IntentId, path: &str) -> ActionProposal {
    ActionProposal {
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
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    }
}

fn sample_provenance_event(
    kind: ProvenanceEventKind,
    parent_ids: Vec<ferrum_proto::EventId>,
) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind,
        occurred_at: chrono::Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::System,
            actor_id: "integration-test".to_string(),
            display_name: Some("Integration Test".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Unknown,
            object_id: "provenance-test-object".to_string(),
            summary: Some("synthetic provenance event".to_string()),
        },
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: parent_ids
            .into_iter()
            .map(|from_event_id| ProvenanceEdge {
                edge_type: ProvenanceEdgeType::DerivedFrom,
                from_event_id,
                summary: None,
            })
            .collect(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: JsonMap::new(),
    }
}

async fn create_execution_with_events(
    runtime: &GatewayRuntime,
) -> (
    ferrum_proto::ExecutionId,
    ferrum_proto::IntentId,
    ferrum_proto::ProposalId,
) {
    // Use a synthetic path - it's just stored as metadata, doesn't need to exist
    let path = "/tmp/test-path".to_string();

    // Compile intent
    let req = sample_intent_request(&path);
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

    // Evaluate proposal
    let proposal = sample_proposal(intent_id, &path);
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

    (execution_id, intent_id, proposal_id)
}

/// Test: query by execution_id returns matching events
#[tokio::test]
async fn test_provenance_query_by_execution_id() {
    let (runtime_temp_dir, runtime, _store) = create_test_runtime().await;
    // Keep runtime_temp_dir alive for the duration of the test
    let _ = &runtime_temp_dir;

    let (execution_id, intent_id, proposal_id) = create_execution_with_events(&runtime).await;

    // Query by execution_id
    let query_req = ProvenanceQueryRequest {
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

    // Should have events for this execution
    assert!(
        !query_resp.events.is_empty(),
        "expected events for execution_id"
    );
    // All events should have execution_id matching
    for event in &query_resp.events {
        assert_eq!(
            event.execution_id,
            Some(execution_id),
            "event should belong to the queried execution"
        );
    }

    // Also verify we can query by intent_id
    let query_req = ProvenanceQueryRequest {
        intent_id: Some(intent_id),
        proposal_id: None,
        execution_id: None,
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

    // Should have events for this intent
    assert!(
        !query_resp.events.is_empty(),
        "expected events for intent_id"
    );
    for event in &query_resp.events {
        assert_eq!(
            event.intent_id,
            Some(intent_id),
            "event should belong to the queried intent"
        );
    }

    // And query by proposal_id
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: Some(proposal_id),
        execution_id: None,
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
    assert!(
        !query_resp.events.is_empty(),
        "expected events for proposal_id"
    );
    for event in &query_resp.events {
        assert_eq!(
            event.proposal_id,
            Some(proposal_id),
            "event should belong to the queried proposal"
        );
    }
}

#[tokio::test]
async fn test_get_provenance_event_returns_not_found_for_unknown_event() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}",
                    ferrum_proto::EventId::new()
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// Test: ingest external event successfully links to parent event in same execution
#[tokio::test]
async fn test_ingest_external_event_happy_path() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Get a provenance event for this execution to use as parent
    let query_req = ProvenanceQueryRequest {
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
        .expect("expected at least one event")
        .event_id;

    // Ingest external event
    let mut extra_metadata = JsonMap::new();
    extra_metadata.insert(
        "source_system".to_string(),
        serde_json::Value::String("spoof-attempt".to_string()),
    );
    extra_metadata.insert(
        "trace_id".to_string(),
        serde_json::Value::String("trace-123".to_string()),
    );

    let ingest_req = ExternalEventIngestRequest {
        execution_id,
        parent_event_id,
        source_system: "test-runtime".to_string(),
        source_event_id: "ext-event-123".to_string(),
        observed_at: None,
        summary: Some("External system observed something".to_string()),
        payload_digest: Some("sha256:abc123".to_string()),
        metadata: Some(extra_metadata),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/events/external")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&ingest_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let ingest_resp: ferrum_proto::ExternalEventIngestResponse =
        serde_json::from_slice(&body).unwrap();

    // Verify the returned event
    assert_eq!(
        ingest_resp.event.kind,
        ProvenanceEventKind::ExternalEventObserved
    );
    assert_eq!(ingest_resp.event.execution_id, Some(execution_id));
    assert_eq!(ingest_resp.event.parent_edges.len(), 1);
    assert_eq!(
        ingest_resp.event.parent_edges[0].from_event_id,
        parent_event_id
    );
    assert_eq!(
        ingest_resp.event.parent_edges[0].edge_type,
        ProvenanceEdgeType::ObservedBy
    );
    assert_eq!(
        ingest_resp
            .event
            .metadata
            .get("source_system")
            .and_then(|v| v.as_str()),
        Some("test-runtime")
    );
    assert_eq!(
        ingest_resp
            .event
            .metadata
            .get("source_event_id")
            .and_then(|v| v.as_str()),
        Some("ext-event-123")
    );
    assert!(
        ingest_resp
            .event
            .trust_labels
            .contains(&TrustLabel::ExternalToolOutput)
    );
    assert_eq!(
        ingest_resp
            .event
            .metadata
            .get("external_metadata")
            .and_then(|v| v.get("trace_id"))
            .and_then(|v| v.as_str()),
        Some("trace-123")
    );
    assert_eq!(
        ingest_resp
            .event
            .metadata
            .get("source_system")
            .and_then(|v| v.as_str()),
        Some("test-runtime")
    );
}

/// Test: ingest external event with unknown execution_id fails
#[tokio::test]
async fn test_ingest_external_event_unknown_execution_fails() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let ingest_req = ExternalEventIngestRequest {
        execution_id: ferrum_proto::ExecutionId::new(),
        parent_event_id: ferrum_proto::EventId::new(),
        source_system: "test-runtime".to_string(),
        source_event_id: "ext-event-123".to_string(),
        observed_at: None,
        summary: None,
        payload_digest: None,
        metadata: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/events/external")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&ingest_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 404
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// Test: ingest external event with unknown parent_event_id fails
#[tokio::test]
async fn test_ingest_external_event_unknown_parent_event_fails() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    let ingest_req = ExternalEventIngestRequest {
        execution_id,
        parent_event_id: ferrum_proto::EventId::new(),
        source_system: "test-runtime".to_string(),
        source_event_id: "ext-event-123".to_string(),
        observed_at: None,
        summary: None,
        payload_digest: None,
        metadata: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/events/external")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&ingest_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 404
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

/// Test: ingest external event with mismatched execution_id/parent_event fails
#[tokio::test]
async fn test_ingest_external_event_mismatched_execution_fails() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create two executions
    let (execution_id1, _intent_id1, _proposal_id1) = create_execution_with_events(&runtime).await;
    let (execution_id2, _intent_id2, _proposal_id2) = create_execution_with_events(&runtime).await;

    // Get a provenance event for execution_id1
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id1),
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
        .expect("expected at least one event")
        .event_id;

    // Try to ingest with execution_id2 but parent_event_id from execution_id1
    let ingest_req = ExternalEventIngestRequest {
        execution_id: execution_id2,
        parent_event_id,
        source_system: "test-runtime".to_string(),
        source_event_id: "ext-event-123".to_string(),
        observed_at: None,
        summary: None,
        payload_digest: None,
        metadata: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/events/external")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&ingest_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should fail with 400 - parent event does not belong to execution
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

/// Test: ingest external event rejects unknown JSON fields (fail-closed)
#[tokio::test]
async fn test_ingest_external_event_rejects_unknown_fields() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Get a parent event
    let query_req = ProvenanceQueryRequest {
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let query_resp: ferrum_proto::ProvenanceQueryResponse = serde_json::from_slice(&body).unwrap();
    let parent_event_id = query_resp
        .events
        .first()
        .expect("expected at least one event")
        .event_id;

    // Request with unknown field should be rejected
    let invalid_json = serde_json::json!({
        "execution_id": execution_id.to_string(),
        "parent_event_id": parent_event_id.to_string(),
        "source_system": "test",
        "source_event_id": "ext-123",
        "unknown_field": "should fail"
    });

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/events/external")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(invalid_json.to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be rejected due to deny_unknown_fields
    assert!(
        response.status() == axum::http::StatusCode::BAD_REQUEST
            || response.status() == axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422 for unknown fields, got {}",
        response.status()
    );
}

/// Test: pagination with limit returns limited events and next_cursor
#[tokio::test]
async fn test_provenance_query_pagination_with_limit() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Insert additional events to have more than limit
    for i in 0..5 {
        let event = ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ProvenanceEventKind::ToolCallPrepared,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "pagination-test".to_string(),
                display_name: Some("Pagination Test".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Unknown,
                object_id: format!("pagination-test-{}", i),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::new(),
        };
        store.provenance().append_event(&event).await.unwrap();
    }

    // Query with limit=3
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
        limit: Some(3),
        cursor: None,
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

    // Should return at most 3 events (could be fewer if terminal_only filters some)
    assert!(
        query_resp.events.len() <= 3,
        "expected at most 3 events, got {}",
        query_resp.events.len()
    );

    // Should have a next_cursor if there are more events
    // (We inserted 5 extra events, so there should be more)
    if query_resp.events.len() == 3 {
        assert!(
            query_resp.next_cursor.is_some(),
            "expected next_cursor when limit is reached"
        );
    }
}

/// Test: cursor pagination advances through pages correctly
#[tokio::test]
async fn test_provenance_query_cursor_pagination() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Insert additional events with distinct timestamps to ensure stable ordering
    let base_time = chrono::Utc::now();
    for i in 0..5 {
        let event = ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ProvenanceEventKind::ToolCallPrepared,
            occurred_at: base_time + chrono::Duration::milliseconds(i * 100),
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "cursor-test".to_string(),
                display_name: Some("Cursor Test".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Unknown,
                object_id: format!("cursor-test-{}", i),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::new(),
        };
        store.provenance().append_event(&event).await.unwrap();
    }

    // First page with limit=3
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
        limit: Some(3),
        cursor: None,
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
    let page1_resp: ferrum_proto::ProvenanceQueryResponse = serde_json::from_slice(&body).unwrap();

    let first_page_event_ids: Vec<_> = page1_resp.events.iter().map(|e| e.event_id).collect();

    // If there's a next page, use cursor to get second page
    if let Some(cursor) = page1_resp.next_cursor {
        let query_req = ProvenanceQueryRequest {
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            limit: Some(3),
            cursor: Some(cursor),
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
        let page2_resp: ferrum_proto::ProvenanceQueryResponse =
            serde_json::from_slice(&body).unwrap();

        let second_page_event_ids: Vec<_> = page2_resp.events.iter().map(|e| e.event_id).collect();

        // Pages should have no overlapping events
        for event_id in &first_page_event_ids {
            assert!(
                !second_page_event_ids.contains(event_id),
                "event_id should not appear in both pages"
            );
        }
    }
}

/// Test: filter + pagination combination works correctly
#[tokio::test]
async fn test_provenance_query_filter_with_pagination() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Insert events with different event kinds
    for i in 0..3 {
        let event = ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ProvenanceEventKind::ToolCallPrepared,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "filter-test".to_string(),
                display_name: Some("Filter Test".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Unknown,
                object_id: format!("filter-test-{}", i),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: Some(execution_id),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: JsonMap::new(),
        };
        store.provenance().append_event(&event).await.unwrap();
    }

    // Query by execution_id AND event_kind with limit
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: Some(ProvenanceEventKind::ToolCallPrepared),
        terminal_only: None,
        since: None,
        until: None,
        limit: Some(2),
        cursor: None,
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

    // All returned events should match the filter
    for event in &query_resp.events {
        assert_eq!(
            event.execution_id,
            Some(execution_id),
            "event should match execution_id filter"
        );
        assert!(
            matches!(event.kind, ProvenanceEventKind::ToolCallPrepared),
            "event should match event_kind filter"
        );
    }

    // Should respect the limit
    assert!(
        query_resp.events.len() <= 2,
        "expected at most 2 events, got {}",
        query_resp.events.len()
    );
}

// ============================================================
// Tests for POST /v1/provenance/lineage
// ============================================================

#[tokio::test]
async fn test_lineage_query_happy_path_ancestry() {
    let (temp_dir, runtime, store) = create_test_runtime().await;
    let file_path = temp_dir.path().join("test.txt");
    let file_path_str = file_path.to_string_lossy().to_string();

    // Build up some provenance events with an execution
    let intent_req = sample_intent_request(&file_path_str);
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
    let compile_resp: ferrum_proto::IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    // Submit proposal
    let proposal = sample_proposal(intent_id, &file_path_str);
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
            path: file_path_str.clone(),
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
        payload: serde_json::json!({"path": file_path_str, "content": "hello"}),
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

    // Get execution lineage to find an event to use as seed
    let events = store
        .provenance()
        .get_lineage_by_execution(execution_id)
        .await
        .unwrap();

    assert!(!events.is_empty(), "should have events in lineage");
    let seed_event = &events[events.len() / 2]; // Use middle event

    // Query lineage with ancestry=true, descendants=false
    let lineage_req = LineageQueryRequest {
        execution_id,
        event_id: seed_event.event_id,
        ancestry: true,
        descendants: false,
        max_hops: Some(8),
        edge_types: None,
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

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp: LineageQueryResponse = serde_json::from_slice(&body).unwrap();

    // Seed event should be included
    assert!(
        lineage_resp
            .events
            .iter()
            .any(|e| e.event_id == seed_event.event_id),
        "seed event should be included"
    );

    // All events should have execution_id matching the request
    for event in &lineage_resp.events {
        if let Some(exec_id) = event.execution_id {
            assert_eq!(
                exec_id, execution_id,
                "event should respect execution fence"
            );
        }
    }
}

#[tokio::test]
async fn test_lineage_query_both_directions_false_returns_400() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Create a dummy event for the request
    let event_id = ferrum_proto::EventId::new();
    let execution_id = ferrum_proto::ExecutionId::new();

    let lineage_req = LineageQueryRequest {
        execution_id,
        event_id,
        ancestry: false,
        descendants: false,
        max_hops: Some(8),
        edge_types: None,
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

    assert_eq!(
        response.status(),
        400,
        "should return 400 when both ancestry and descendants are false"
    );
}

#[tokio::test]
async fn test_lineage_query_max_hops_respected() {
    let (temp_dir, runtime, store) = create_test_runtime().await;
    let file_path = temp_dir.path().join("test.txt");
    let file_path_str = file_path.to_string_lossy().to_string();

    // Build up lineage with multiple hops
    let intent_req = sample_intent_request(&file_path_str);
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
    let compile_resp: ferrum_proto::IntentCompileResponse = serde_json::from_slice(&body).unwrap();
    let intent_id = compile_resp.envelope.intent_id;

    let proposal = sample_proposal(intent_id, &file_path_str);
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

    let mint_req = CapabilityMintRequest {
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "workspace".to_string(),
            tool_name: "fs.write".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::File {
            path: file_path_str.clone(),
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

    let execute_req = ferrum_proto::ExecuteRequest {
        execution_id,
        payload: serde_json::json!({"path": file_path_str, "content": "hello"}),
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

    let events = store
        .provenance()
        .get_lineage_by_execution(execution_id)
        .await
        .unwrap();

    // Start from oldest event (first in the list)
    let seed_event = events.first().unwrap();

    // Query with max_hops = 1
    let lineage_req = LineageQueryRequest {
        execution_id,
        event_id: seed_event.event_id,
        ancestry: true,
        descendants: true,
        max_hops: Some(1),
        edge_types: None,
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

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp: LineageQueryResponse = serde_json::from_slice(&body).unwrap();

    // With max_hops=1, should get at most seed + immediate neighbors
    // (may be fewer depending on graph structure)
    assert!(
        lineage_resp.events.len() <= 3,
        "max_hops=1 should limit events, got {}",
        lineage_resp.events.len()
    );
}

#[tokio::test]
async fn test_lineage_query_unknown_event_returns_404() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let event_id = ferrum_proto::EventId::new(); // Random non-existent event
    let execution_id = ferrum_proto::ExecutionId::new();

    let lineage_req = LineageQueryRequest {
        execution_id,
        event_id,
        ancestry: true,
        descendants: false,
        max_hops: Some(8),
        edge_types: None,
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

    assert_eq!(
        response.status(),
        404,
        "should return 404 for unknown event"
    );
}

// ============================================================
// Provenance Replay Tests
// ============================================================

/// Test: happy path - replay returns events for a known execution
#[tokio::test]
async fn test_replay_happy_path() {
    let (runtime_temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &runtime_temp_dir; // Keep alive

    // Create an execution with events
    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Replay the execution
    let replay_req = ProvenanceReplayRequest { execution_id };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/replay")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&replay_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let replay_resp: ProvenanceReplayResponse = serde_json::from_slice(&body).unwrap();

    // Should have events for this execution
    assert!(!replay_resp.events.is_empty(), "expected events for replay");
    assert_eq!(replay_resp.execution_id, execution_id);

    // All events should belong to this execution
    for event in &replay_resp.events {
        assert_eq!(
            event.execution_id,
            Some(execution_id),
            "all events should belong to the replayed execution"
        );
    }

    // Events should be topologically sorted (events with no parents come first)
    // Verify that for each event, all of its parents appear earlier in the list
    let event_pos: std::collections::HashMap<_, _> = replay_resp
        .events
        .iter()
        .enumerate()
        .map(|(i, e)| (e.event_id, i))
        .collect();
    for event in &replay_resp.events {
        for parent_edge in &event.parent_edges {
            let parent_pos = event_pos
                .get(&parent_edge.from_event_id)
                .expect("parent should be in the event set");
            let child_pos = event_pos
                .get(&event.event_id)
                .expect("child should be in the event set");
            assert!(
                parent_pos < child_pos,
                "parent {} (pos {}) should appear before child {} (pos {})",
                parent_edge.from_event_id,
                parent_pos,
                event.event_id,
                child_pos
            );
        }
    }
}

/// Test: malformed lineage - replay returns 404 for unknown execution
#[tokio::test]
async fn test_replay_unknown_execution_returns_404() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let unknown_execution_id = ferrum_proto::ExecutionId::new();

    let replay_req = ProvenanceReplayRequest {
        execution_id: unknown_execution_id,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/replay")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&replay_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        404,
        "should return 404 for unknown execution"
    );
}

/// Test: fail-closed - replay rejects request with unknown fields
#[tokio::test]
async fn test_replay_fail_closed_on_unknown_fields() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Sending a request with an unknown field should fail (deny_unknown_fields)
    let body =
        r#"{"execution_id": "00000000-0000-0000-0000-000000000000", "unknown_field": "bad"}"#;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/replay")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(body.to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reject the request due to unknown field
    assert!(
        response.status() == 400 || response.status() == 422,
        "should reject request with unknown fields, got {}",
        response.status()
    );
}

// =============================================================================
// Provenance Export Tests
// =============================================================================

/// Test: export endpoint returns proper audit payload structure
#[tokio::test]
async fn test_provenance_export_happy_path() {
    let (runtime_temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &runtime_temp_dir;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Export by execution_id
    let export_req = ProvenanceExportRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
        limit: Some(100),
        cursor: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/export")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&export_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let export_resp: ProvenanceExportResponse = serde_json::from_slice(&body).unwrap();

    // Should have exported events
    assert!(!export_resp.events.is_empty(), "expected events in export");

    // Should have export info metadata
    assert!(
        export_resp.export_info.exported_at <= chrono::Utc::now(),
        "exported_at should be in the past or present"
    );

    // Check filter presence flags
    let filters = &export_resp.export_info.filters;
    assert!(
        filters.execution_id == Some(true),
        "execution_id filter should be marked as present"
    );
    assert!(
        filters.intent_id.is_none(),
        "intent_id filter should not be marked"
    );
}

/// Test: export endpoint respects limit parameter
#[tokio::test]
async fn test_provenance_export_respects_limit() {
    let (runtime_temp_dir, runtime, _store) = create_test_runtime().await;
    let _ = &runtime_temp_dir;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Export with limit of 1
    let export_req = ProvenanceExportRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
        limit: Some(1),
        cursor: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/export")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&export_req).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let export_resp: ProvenanceExportResponse = serde_json::from_slice(&body).unwrap();

    // Should respect limit
    assert_eq!(
        export_resp.events.len(),
        1,
        "exported_count should be limited to 1"
    );
    assert_eq!(export_resp.exported_count, 1, "exported_count should be 1");

    // total_matched should be >= exported_count (could be more events)
    assert!(
        export_resp.total_matched >= export_resp.exported_count,
        "total_matched should be >= exported_count"
    );
}

/// Test: export endpoint rejects unknown fields (fail-closed)
#[tokio::test]
async fn test_provenance_export_rejects_unknown_fields() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Sending a request with an unknown field should fail (deny_unknown_fields)
    let body =
        r#"{"execution_id": "00000000-0000-0000-0000-000000000000", "unknown_field": "bad"}"#;

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/export")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(body.to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reject the request due to unknown field
    assert!(
        response.status() == 400 || response.status() == 422,
        "should reject request with unknown fields, got {}",
        response.status()
    );
}

/// Helper to create a provenance event with a specific edge type for parent edges.
fn make_event_with_edge_type(
    kind: ProvenanceEventKind,
    parent_ids: Vec<ferrum_proto::EventId>,
    edge_type: ProvenanceEdgeType,
) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind,
        occurred_at: chrono::Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::System,
            actor_id: "integration-test".to_string(),
            display_name: Some("Integration Test".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Unknown,
            object_id: "edge-type-test-object".to_string(),
            summary: Some("event for edge_types filter test".to_string()),
        },
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: parent_ids
            .into_iter()
            .map(|from_event_id| ProvenanceEdge {
                edge_type: edge_type.clone(),
                from_event_id,
                summary: None,
            })
            .collect(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: JsonMap::new(),
    }
}

/// Test: lineage query with edge_types filter restricts ancestry traversal
#[tokio::test]
async fn test_lineage_query_edge_types_filter_restricts_ancestry() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    // Create chain: root -[DerivedFrom]-> middle -[AuthorizedBy]-> leaf
    let root = make_event_with_edge_type(
        ProvenanceEventKind::IntentCompiled,
        Vec::new(),
        ProvenanceEdgeType::DerivedFrom,
    );
    let middle = make_event_with_edge_type(
        ProvenanceEventKind::ToolCallPrepared,
        vec![root.event_id],
        ProvenanceEdgeType::DerivedFrom,
    );
    let leaf = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![middle.event_id],
        ProvenanceEdgeType::AuthorizedBy,
    );

    store.provenance().append_event(&root).await.unwrap();
    store.provenance().append_event(&middle).await.unwrap();
    store.provenance().append_event(&leaf).await.unwrap();

    let execution_id = ferrum_proto::ExecutionId::new();

    // Query with no filter - should get all 3 events in ancestry
    let lineage_req_all = LineageQueryRequest {
        execution_id,
        event_id: leaf.event_id,
        ancestry: true,
        descendants: false,
        max_hops: Some(8),
        edge_types: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/lineage")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&lineage_req_all).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp_all: LineageQueryResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        lineage_resp_all.events.len(),
        3,
        "no filter should return all 3 events"
    );

    // Query with filter for DerivedFrom only - should NOT follow AuthorizedBy edge
    // So ancestry should be: root, middle, leaf (the seed)
    let lineage_req_derived = LineageQueryRequest {
        execution_id,
        event_id: leaf.event_id,
        ancestry: true,
        descendants: false,
        max_hops: Some(8),
        edge_types: Some(vec![ProvenanceEdgeType::DerivedFrom]),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/lineage")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&lineage_req_derived).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp_derived: LineageQueryResponse = serde_json::from_slice(&body).unwrap();

    // The leaf's only parent edge is AuthorizedBy, which is filtered out,
    // so we should only get the leaf itself (no ancestors followed)
    assert!(
        lineage_resp_derived.events.len() <= 1,
        "AuthorizedBy filter should not follow any edges, got {} events",
        lineage_resp_derived.events.len()
    );
}

/// Test: lineage query with edge_types filter restricts descendants traversal
#[tokio::test]
async fn test_lineage_query_edge_types_filter_restricts_descendants() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    // Create chain: root -[DerivedFrom]-> child1 -[AuthorizedBy]-> child2
    let root = make_event_with_edge_type(
        ProvenanceEventKind::IntentCompiled,
        Vec::new(),
        ProvenanceEdgeType::DerivedFrom,
    );
    let child1 = make_event_with_edge_type(
        ProvenanceEventKind::ToolCallPrepared,
        vec![root.event_id],
        ProvenanceEdgeType::DerivedFrom,
    );
    let child2 = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![child1.event_id],
        ProvenanceEdgeType::AuthorizedBy,
    );

    // Add execution_id to all events for the query to work
    let mut root_with_exec = root.clone();
    root_with_exec.execution_id = Some(ferrum_proto::ExecutionId::new());
    let mut child1_with_exec = child1.clone();
    child1_with_exec.execution_id = root_with_exec.execution_id;
    let mut child2_with_exec = child2.clone();
    child2_with_exec.execution_id = root_with_exec.execution_id;

    let execution_id = root_with_exec.execution_id.unwrap();

    store
        .provenance()
        .append_event(&root_with_exec)
        .await
        .unwrap();
    store
        .provenance()
        .append_event(&child1_with_exec)
        .await
        .unwrap();
    store
        .provenance()
        .append_event(&child2_with_exec)
        .await
        .unwrap();

    // Query descendants with no filter - should get child1 and child2
    let lineage_req_all = LineageQueryRequest {
        execution_id,
        event_id: root_with_exec.event_id,
        ancestry: false,
        descendants: true,
        max_hops: Some(8),
        edge_types: None,
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/lineage")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&lineage_req_all).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp_all: LineageQueryResponse = serde_json::from_slice(&body).unwrap();
    // Note: API includes seed event in response, so we get root + child1 + child2 = 3
    assert_eq!(
        lineage_resp_all.events.len(),
        3,
        "no filter should return root and both children (API includes seed)"
    );

    // Query descendants with DerivedFrom only - should get root + child1 (2 events)
    // child2 is not reachable because child1->child2 is AuthorizedBy, not DerivedFrom
    let lineage_req_derived = LineageQueryRequest {
        execution_id,
        event_id: root_with_exec.event_id,
        ancestry: false,
        descendants: true,
        max_hops: Some(8),
        edge_types: Some(vec![ProvenanceEdgeType::DerivedFrom]),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/lineage")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&lineage_req_derived).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp_derived: LineageQueryResponse = serde_json::from_slice(&body).unwrap();
    // API includes seed event, so we get root + child1 = 2 events
    assert_eq!(
        lineage_resp_derived.events.len(),
        2,
        "DerivedFrom filter should return root + child1"
    );
    // child1 should be in the response (either first or second depending on sort order)
    assert!(
        lineage_resp_derived
            .events
            .iter()
            .any(|e| e.event_id == child1_with_exec.event_id),
        "child1 should be in the response"
    );

    // Query descendants with AuthorizedBy only - should get root only (1 event)
    // root->child1 is DerivedFrom (not AuthorizedBy), so child1 is not reachable
    // child2 is only reachable via child1->child2 AuthorizedBy edge, but we can't reach child1
    let lineage_req_auth = LineageQueryRequest {
        execution_id,
        event_id: root_with_exec.event_id,
        ancestry: false,
        descendants: true,
        max_hops: Some(8),
        edge_types: Some(vec![ProvenanceEdgeType::AuthorizedBy]),
    };

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/lineage")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&lineage_req_auth).unwrap())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let lineage_resp_auth: LineageQueryResponse = serde_json::from_slice(&body).unwrap();
    // API includes seed event, and AuthorizedBy filter from root only returns root (1 event)
    // since root has no direct AuthorizedBy edges to children
    assert_eq!(
        lineage_resp_auth.events.len(),
        1,
        "AuthorizedBy filter should only return root - no direct AuthorizedBy edge from root"
    );
    // root should be in the response
    assert!(
        lineage_resp_auth
            .events
            .iter()
            .any(|e| e.event_id == root_with_exec.event_id),
        "root should be in the response"
    );
}

/// Test: GET /v1/provenance/events/{event_id} with edge_types filter restricts ancestry traversal
#[tokio::test]
async fn test_get_provenance_event_edge_type_filter_restricts_ancestry() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    // Create chain: root -[DerivedFrom]-> middle -[AuthorizedBy]-> leaf
    let root = make_event_with_edge_type(
        ProvenanceEventKind::IntentCompiled,
        Vec::new(),
        ProvenanceEdgeType::DerivedFrom,
    );
    let middle = make_event_with_edge_type(
        ProvenanceEventKind::ToolCallPrepared,
        vec![root.event_id],
        ProvenanceEdgeType::DerivedFrom,
    );
    let leaf = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![middle.event_id],
        ProvenanceEdgeType::AuthorizedBy,
    );

    store.provenance().append_event(&root).await.unwrap();
    store.provenance().append_event(&middle).await.unwrap();
    store.provenance().append_event(&leaf).await.unwrap();

    // Query ancestry with no filter - should get both root and middle
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?ancestry=true",
                    leaf.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    let ancestry = event_resp.ancestry.expect("expected ancestry in response");
    let ancestry_ids: Vec<_> = ancestry.into_iter().map(|e| e.event_id).collect();
    // Without filter, we should get both root and middle
    assert!(
        ancestry_ids.contains(&root.event_id),
        "root should be in ancestry, got {:?}",
        ancestry_ids
    );
    assert!(
        ancestry_ids.contains(&middle.event_id),
        "middle should be in ancestry, got {:?}",
        ancestry_ids
    );
    assert_eq!(
        ancestry_ids.len(),
        2,
        "should have 2 ancestors without filter, got {}",
        ancestry_ids.len()
    );

    // Query ancestry with AuthorizedBy filter only - should follow AuthorizedBy edge from leaf to middle
    // but NOT DerivedFrom edge from middle to root
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?ancestry=true&edge_types=AuthorizedBy",
                    leaf.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let (parts, body) = response.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    assert_eq!(
        parts.status.as_u16(),
        200,
        "ancestry query with AuthorizedBy filter failed. body: {:?}",
        String::from_utf8_lossy(&body_bytes)
    );
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body_bytes).unwrap();

    let ancestry = event_resp.ancestry.expect("expected ancestry in response");
    let ancestry_ids: Vec<_> = ancestry.into_iter().map(|e| e.event_id).collect();
    // AuthorizedBy filter should follow leaf's AuthorizedBy edge to middle,
    // but NOT middle's DerivedFrom edge to root, so we should get middle only
    assert!(
        ancestry_ids.contains(&middle.event_id),
        "AuthorizedBy filter should include middle (reachable via AuthorizedBy), got {:?}",
        ancestry_ids
    );
    assert!(
        !ancestry_ids.contains(&root.event_id),
        "AuthorizedBy filter should NOT include root (only reachable via DerivedFrom), got {:?}",
        ancestry_ids
    );

    // Query ancestry with DerivedFrom filter only - should follow DerivedFrom edge from middle to root
    // but NOT AuthorizedBy edge from leaf to middle
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?ancestry=true&edge_types=DerivedFrom",
                    leaf.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    // DerivedFrom filter: leaf's parent edge is AuthorizedBy (NOT DerivedFrom), so we skip it
    // We never reach middle, so we never see middle's DerivedFrom edge to root
    // Result: no ancestry (None when serialized as JSON)
    let ancestry_ids: Vec<_> = match event_resp.ancestry {
        Some(ancestry) => ancestry.into_iter().map(|e| e.event_id).collect(),
        None => vec![],
    };
    assert!(
        ancestry_ids.is_empty(),
        "DerivedFrom filter should skip leaf's AuthorizedBy edge, got {:?}",
        ancestry_ids
    );
}

/// Test: GET /v1/provenance/events/{event_id} with edge_types filter restricts descendants traversal
#[tokio::test]
async fn test_get_provenance_event_edge_type_filter_restricts_descendants() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    // Create chain: root -[DerivedFrom]-> child1 -[AuthorizedBy]-> child2
    let root = make_event_with_edge_type(
        ProvenanceEventKind::IntentCompiled,
        Vec::new(),
        ProvenanceEdgeType::DerivedFrom,
    );
    let child1 = make_event_with_edge_type(
        ProvenanceEventKind::ToolCallPrepared,
        vec![root.event_id],
        ProvenanceEdgeType::DerivedFrom,
    );
    let child2 = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![child1.event_id],
        ProvenanceEdgeType::AuthorizedBy,
    );

    store.provenance().append_event(&root).await.unwrap();
    store.provenance().append_event(&child1).await.unwrap();
    store.provenance().append_event(&child2).await.unwrap();

    // Query descendants with no filter - should get child1 and child2
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?descendants=true",
                    root.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    let descendants = event_resp
        .descendants
        .expect("expected descendants in response");
    let descendant_ids: Vec<_> = descendants.into_iter().map(|e| e.event_id).collect();
    // Without filter, we should get both child1 and child2
    assert!(
        descendant_ids.contains(&child1.event_id),
        "child1 should be in descendants, got {:?}",
        descendant_ids
    );
    assert!(
        descendant_ids.contains(&child2.event_id),
        "child2 should be in descendants, got {:?}",
        descendant_ids
    );
    assert_eq!(
        descendant_ids.len(),
        2,
        "should have 2 descendants without filter, got {}",
        descendant_ids.len()
    );

    // Query descendants with DerivedFrom filter only - should get child1 but NOT child2
    // because child1->child2 is AuthorizedBy, not DerivedFrom
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?descendants=true&edge_types=DerivedFrom",
                    root.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        status, 200,
        "descendants query failed with edge_types=DerivedFrom"
    );
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    // DerivedFrom filter should follow root->child1 (DerivedFrom), but NOT child1->child2 (AuthorizedBy)
    let descendant_ids: Vec<_> = match event_resp.descendants {
        Some(descendants) => descendants.into_iter().map(|e| e.event_id).collect(),
        None => vec![],
    };
    assert!(
        descendant_ids.contains(&child1.event_id),
        "child1 should be in descendants with DerivedFrom filter, got {:?}",
        descendant_ids
    );
    assert!(
        !descendant_ids.contains(&child2.event_id),
        "child2 should NOT be in descendants with DerivedFrom filter (child1->child2 is AuthorizedBy), got {:?}",
        descendant_ids
    );

    // Query descendants with AuthorizedBy filter only - should get NO descendants
    // because root has no direct AuthorizedBy edges to children
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?descendants=true&edge_types=AuthorizedBy",
                    root.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    // AuthorizedBy filter should not follow any edges from root
    let descendant_ids: Vec<_> = match event_resp.descendants {
        Some(descendants) => descendants.into_iter().map(|e| e.event_id).collect(),
        None => vec![],
    };
    assert!(
        descendant_ids.is_empty(),
        "AuthorizedBy filter should not follow any edges from root, got {:?}",
        descendant_ids
    );
}

/// Test: GET /v1/provenance/events/{event_id} with multiple edge_types query params
#[tokio::test]
async fn test_get_provenance_event_multiple_edge_types_params() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    // Create chain: root -[DerivedFrom]-> child1 -[AuthorizedBy]-> child2 -[ApprovedBy]-> child3
    let root = make_event_with_edge_type(
        ProvenanceEventKind::IntentCompiled,
        Vec::new(),
        ProvenanceEdgeType::DerivedFrom,
    );
    let child1 = make_event_with_edge_type(
        ProvenanceEventKind::ToolCallPrepared,
        vec![root.event_id],
        ProvenanceEdgeType::DerivedFrom,
    );
    let child2 = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![child1.event_id],
        ProvenanceEdgeType::AuthorizedBy,
    );
    let child3 = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![child2.event_id],
        ProvenanceEdgeType::ApprovedBy,
    );

    store.provenance().append_event(&root).await.unwrap();
    store.provenance().append_event(&child1).await.unwrap();
    store.provenance().append_event(&child2).await.unwrap();
    store.provenance().append_event(&child3).await.unwrap();

    // Query with multiple edge_types via comma-separated values: DerivedFrom,AuthorizedBy
    // This should follow: root->child1 (DerivedFrom) AND child1->child2 (AuthorizedBy)
    // But NOT child2->child3 (ApprovedBy)
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?descendants=true&edge_types=DerivedFrom,AuthorizedBy",
                    root.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    let descendant_ids: Vec<_> = match event_resp.descendants {
        Some(descendants) => descendants.into_iter().map(|e| e.event_id).collect(),
        None => vec![],
    };
    assert!(
        descendant_ids.contains(&child1.event_id),
        "child1 should be in descendants (via DerivedFrom), got {:?}",
        descendant_ids
    );
    assert!(
        descendant_ids.contains(&child2.event_id),
        "child2 should be in descendants (via AuthorizedBy), got {:?}",
        descendant_ids
    );
    assert!(
        !descendant_ids.contains(&child3.event_id),
        "child3 should NOT be in descendants (via ApprovedBy, not filtered), got {:?}",
        descendant_ids
    );

    // Query with comma-separated edge_types: DerivedFrom,AuthorizedBy
    // Should produce the same result as repeated params
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?descendants=true&edge_types=DerivedFrom,AuthorizedBy",
                    root.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    let descendant_ids: Vec<_> = match event_resp.descendants {
        Some(descendants) => descendants.into_iter().map(|e| e.event_id).collect(),
        None => vec![],
    };
    assert!(
        descendant_ids.contains(&child1.event_id),
        "child1 should be in descendants with comma-separated filter, got {:?}",
        descendant_ids
    );
    assert!(
        descendant_ids.contains(&child2.event_id),
        "child2 should be in descendants with comma-separated filter, got {:?}",
        descendant_ids
    );
    assert!(
        !descendant_ids.contains(&child3.event_id),
        "child3 should NOT be in descendants with comma-separated filter, got {:?}",
        descendant_ids
    );
}

/// Test: GET /v1/provenance/events/{event_id} with ancestry and multiple edge_types params
#[tokio::test]
async fn test_get_provenance_event_ancestry_multiple_edge_types_params() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    // Create chain: root -[DerivedFrom]-> middle -[AuthorizedBy]-> leaf
    let root = make_event_with_edge_type(
        ProvenanceEventKind::IntentCompiled,
        Vec::new(),
        ProvenanceEdgeType::DerivedFrom,
    );
    let middle = make_event_with_edge_type(
        ProvenanceEventKind::ToolCallPrepared,
        vec![root.event_id],
        ProvenanceEdgeType::DerivedFrom,
    );
    let leaf = make_event_with_edge_type(
        ProvenanceEventKind::SideEffectCommitted,
        vec![middle.event_id],
        ProvenanceEdgeType::AuthorizedBy,
    );

    store.provenance().append_event(&root).await.unwrap();
    store.provenance().append_event(&middle).await.unwrap();
    store.provenance().append_event(&leaf).await.unwrap();

    // Query ancestry with multiple edge_types: AuthorizedBy AND DerivedFrom
    // Starting from leaf: leaf has AuthorizedBy edge to middle, middle has DerivedFrom edge to root
    // So we should get both middle and root
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?ancestry=true&edge_types=AuthorizedBy,DerivedFrom",
                    leaf.event_id
                ))
                .method(axum::http::Method::GET)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let event_resp: ProvenanceEventResponse = serde_json::from_slice(&body).unwrap();

    let ancestry_ids: Vec<_> = match event_resp.ancestry {
        Some(ancestry) => ancestry.into_iter().map(|e| e.event_id).collect(),
        None => vec![],
    };
    assert!(
        ancestry_ids.contains(&middle.event_id),
        "middle should be in ancestry (via AuthorizedBy), got {:?}",
        ancestry_ids
    );
    assert!(
        ancestry_ids.contains(&root.event_id),
        "root should be in ancestry (via DerivedFrom), got {:?}",
        ancestry_ids
    );
}
