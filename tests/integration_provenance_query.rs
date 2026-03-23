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
    EffectType, ExternalEventIngestRequest, HashChainRef, IntentCompileRequest, JsonMap, ObjectRef,
    ObjectType, ProvenanceEdge, ProvenanceEdgeType, ProvenanceEvent, ProvenanceEventKind,
    ProvenanceEventResponse, ProvenanceQueryRequest, ResourceBinding, ResourceMode, RiskTier,
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

    // Query with non-existent execution_id should return empty events
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(ferrum_proto::ExecutionId::new()),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
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
        query_resp.events.is_empty(),
        "expected empty events for non-existent execution"
    );
}

/// Test: query by time window returns events within range
#[tokio::test]
async fn test_provenance_query_by_time_window() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Query with full time range (should return events)
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
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
    let event_count = query_resp.events.len();
    assert!(event_count > 0, "expected events in full time range");

    // Query with narrow time window in the past (should return empty)
    let past_time = chrono::Utc::now() - chrono::Duration::hours(1);
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: Some(past_time),
        until: Some(past_time + chrono::Duration::minutes(5)),
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
    // Narrow window in the past should not contain our recent events
    assert!(
        query_resp.events.is_empty(),
        "expected empty events for narrow past time window"
    );

    // Query with since only (from past to now)
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: Some(past_time),
        until: None,
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
    // Since past to now should include our events
    assert_eq!(
        query_resp.events.len(),
        event_count,
        "expected all events when querying from past to now"
    );
}

/// Test: reject unknown JSON fields with fail-closed behavior
#[tokio::test]
async fn test_provenance_query_rejects_unknown_fields() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    // Valid request (should succeed)
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: None,
        capability_id: None,
        event_kind: None,
        terminal_only: None,
        since: None,
        until: None,
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

    // Empty query should succeed (returns all events)
    assert_eq!(response.status(), 200);

    // Request with unknown field "unknown_field" should be rejected
    let invalid_json = r#"{"execution_id": null, "unknown_field": "should fail"}"#;
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/query")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(invalid_json.to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be rejected (400 or similar) due to deny_unknown_fields
    assert!(
        response.status() == axum::http::StatusCode::BAD_REQUEST
            || response.status() == axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422 for unknown fields, got {}",
        response.status()
    );

    // Request with another unknown field "extraneous_param" should be rejected
    let invalid_json = r#"{"intent_id": null, "extraneous_param": 123}"#;
    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/provenance/query")
                .method(axum::http::Method::POST)
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(invalid_json.to_string())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status() == axum::http::StatusCode::BAD_REQUEST
            || response.status() == axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422 for unknown fields, got {}",
        response.status()
    );
}

/// Test: query by event_kind filter
#[tokio::test]
async fn test_provenance_query_by_event_kind() {
    let (_temp_dir, runtime, _store) = create_test_runtime().await;

    let (execution_id, _intent_id, _proposal_id) = create_execution_with_events(&runtime).await;

    // Query by event_kind = IntentCompiled
    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: Some(ProvenanceEventKind::IntentCompiled),
        terminal_only: None,
        since: None,
        until: None,
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

    // All events should be IntentCompiled
    for event in &query_resp.events {
        assert!(
            matches!(event.kind, ProvenanceEventKind::IntentCompiled),
            "expected IntentCompiled events"
        );
    }
}

/// Test: query can return terminal events only
#[tokio::test]
async fn test_provenance_query_terminal_only() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    let (execution_id, intent_id, proposal_id) = create_execution_with_events(&runtime).await;

    store
        .provenance()
        .append_event(&ferrum_proto::ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ProvenanceEventKind::SideEffectCommitted,
            occurred_at: chrono::Utc::now(),
            actor: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::Gateway,
                actor_id: "gateway".to_string(),
                display_name: Some("Ferrum Gateway".to_string()),
            },
            object: ferrum_proto::ObjectRef {
                object_type: ferrum_proto::ObjectType::SideEffect,
                object_id: execution_id.to_string(),
                summary: Some("terminal query test".to_string()),
            },
            intent_id: Some(intent_id),
            proposal_id: Some(proposal_id),
            execution_id: Some(execution_id),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            parent_edges: Vec::new(),
            hash_chain: ferrum_proto::HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
        })
        .await
        .unwrap();

    let query_req = ProvenanceQueryRequest {
        intent_id: None,
        proposal_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        terminal_only: Some(true),
        since: None,
        until: None,
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

    assert!(!query_resp.events.is_empty(), "expected terminal events");
    assert!(query_resp.events.iter().all(|event| {
        matches!(
            event.kind,
            ProvenanceEventKind::SideEffectCommitted
                | ProvenanceEventKind::SideEffectCompensated
                | ProvenanceEventKind::SideEffectRolledBack
                | ProvenanceEventKind::ApprovalDenied
                | ProvenanceEventKind::Quarantined
                | ProvenanceEventKind::ErrorRaised
        )
    }));
}

#[tokio::test]
async fn test_get_provenance_event_returns_ancestry_and_descendants() {
    let (_temp_dir, runtime, store) = create_test_runtime().await;

    let root = sample_provenance_event(ProvenanceEventKind::IntentCompiled, Vec::new());
    let middle =
        sample_provenance_event(ProvenanceEventKind::ToolCallPrepared, vec![root.event_id]);
    let leaf = sample_provenance_event(ProvenanceEventKind::ErrorRaised, vec![middle.event_id]);

    store.provenance().append_event(&root).await.unwrap();
    store.provenance().append_event(&middle).await.unwrap();
    store.provenance().append_event(&leaf).await.unwrap();

    let app = build_router(runtime.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri(format!(
                    "/v1/provenance/events/{}?ancestry=true&descendants=true",
                    middle.event_id
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

    assert_eq!(event_resp.event.event_id, middle.event_id);

    let ancestry = event_resp.ancestry.expect("expected ancestry in response");
    let ancestry_ids: Vec<_> = ancestry.into_iter().map(|event| event.event_id).collect();
    assert_eq!(ancestry_ids, vec![root.event_id]);

    let descendants = event_resp
        .descendants
        .expect("expected descendants in response");
    let descendant_ids: Vec<_> = descendants
        .into_iter()
        .map(|event| event.event_id)
        .collect();
    assert_eq!(descendant_ids, vec![leaf.event_id]);
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
