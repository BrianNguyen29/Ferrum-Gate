use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    LineageDirection, LineageQueryRequest, LineageQueryResponse, ProvenanceEventKind,
    ProvenanceIngestRequest, ProvenanceIngestResponse,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{PolicyBundleRepo, SqliteStore, StoreFacade};
use std::sync::Arc;
#[allow(unused_imports)]
use tower::ServiceExt;

mod support;
use support::*;

use ferrum_sync::{McpBridge, RuntimeBridge};
/// Verify that registering a bridge and ingesting provenance with matching
/// source_runtime_id succeeds and the event is stored and queryable.
#[tokio::test]
async fn test_bridge_registration_and_ingest() {
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

    // Register a bridge with source_runtime_id "mcp://test-runtime"
    let bridge = Arc::new(McpBridge::new("mcp://test-runtime"));

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![bridge.clone() as Arc<dyn RuntimeBridge>],
    );
    let router = build_router(runtime);

    // Step 1: Ingest provenance event with matching source_runtime_id
    let request = ProvenanceIngestRequest {
        source_runtime_id: "mcp://test-runtime".to_string(),
        kind: ProvenanceEventKind::ExternalEventReceived,
        description: "test bridge ingest event".to_string(),
        execution_id: None,
        intent_id: None,
        trust_labels: vec![],
        sensitivity_labels: vec![],
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/v1/provenance/ingest")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("ingest request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "ingest with registered bridge should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let ingest_response: ProvenanceIngestResponse =
        serde_json::from_slice(&body).expect("valid json");

    assert!(
        ingest_response.linked,
        "ingest response should indicate linked=true for registered bridge"
    );
    assert!(
        !ingest_response.event_id.0.is_nil(),
        "event_id should be valid"
    );

    // Step 2: Verify event is persisted in store by querying lineage
    let lineage_request = LineageQueryRequest {
        event_id: ingest_response.event_id,
        direction: LineageDirection::Ancestors,
        max_hops: 3,
    };

    let lineage_response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/v1/provenance/lineage")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&lineage_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("lineage query should succeed");

    assert_eq!(
        lineage_response.status(),
        axum::http::StatusCode::OK,
        "lineage query for existing event should return 200"
    );

    let body = axum::body::to_bytes(lineage_response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let lineage_result: LineageQueryResponse = serde_json::from_slice(&body).expect("valid json");

    // The seed event should be in the lineage response
    assert!(
        !lineage_result.events.is_empty(),
        "lineage should contain at least the seed event"
    );

    // Verify the seed event has the correct source_runtime_id
    let seed_event = lineage_result
        .events
        .iter()
        .find(|e| e.event_id == ingest_response.event_id);
    assert!(
        seed_event.is_some(),
        "seed event should be present in lineage response"
    );
    assert_eq!(
        seed_event.unwrap().source_runtime_id.as_deref(),
        Some("mcp://test-runtime"),
        "seed event should have correct source_runtime_id"
    );
}

/// Verify that ingesting provenance with an unknown (unregistered) source_runtime_id
/// fails with 400 BAD_REQUEST (fail-closed behavior).
#[tokio::test]
async fn test_bridge_ingest_unknown_source_rejected() {
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

    // No bridges registered - should fail-closed
    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Attempt to ingest with unknown source_runtime_id
    let request = ProvenanceIngestRequest {
        source_runtime_id: "mcp-unknown".to_string(),
        kind: ProvenanceEventKind::ExternalEventReceived,
        description: "test unknown source".to_string(),
        execution_id: None,
        intent_id: None,
        trust_labels: vec![],
        sensitivity_labels: vec![],
        metadata: ferrum_proto::JsonMap::new(),
    };

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/v1/provenance/ingest")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("ingest request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "ingest with unknown source_runtime_id should return 400"
    );
}

// ---------------------------------------------------------------------------
// FsAdapter FileWrite gateway-path integration test
// ---------------------------------------------------------------------------

/// Verify that activating and deactivating a policy bundle emits provenance
/// events (PolicyBundleActivated / PolicyBundleDeactivated).
#[tokio::test]
async fn test_policy_bundle_active_switch_emits_provenance() {
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

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Use a UUID string so policy_bundle_id parsing succeeds
    let bundle_uuid = uuid::Uuid::new_v4();
    let bundle_id = bundle_uuid.to_string();
    let bundle = make_test_policy_bundle(&bundle_id, vec![], false);
    store
        .policy_bundles()
        .insert(&bundle)
        .await
        .expect("bundle insert should succeed");

    // Activate the bundle
    let activate_request = ferrum_proto::SetPolicyBundleActiveRequest { active: true };
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::PUT)
                .uri(format!("/v1/policy-bundles/{}/active", bundle_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&activate_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("activate request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "activate should return 200 OK, got: {:?}",
        response.status()
    );

    // Query provenance for PolicyBundleActivated
    let query_request = ferrum_proto::ProvenanceQueryRequest {
        intent_id: None,
        execution_id: None,
        capability_id: None,
        event_kind: Some(ferrum_proto::ProvenanceEventKind::PolicyBundleActivated),
        since: None,
        until: None,
        edge_types: Vec::new(),
    };
    let query_response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/v1/provenance/query")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&query_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("provenance query should succeed");
    assert_eq!(
        query_response.status(),
        axum::http::StatusCode::OK,
        "provenance query should return 200 OK, got: {:?}",
        query_response.status()
    );

    let body = axum::body::to_bytes(query_response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let provenance_result: ferrum_proto::ProvenanceQueryResponse =
        serde_json::from_slice(&body).expect("valid ProvenanceQueryResponse JSON");
    assert!(
        !provenance_result.events.is_empty(),
        "Expected at least one PolicyBundleActivated provenance event, got empty"
    );

    let activated_event = &provenance_result.events[0];
    assert!(
        matches!(
            activated_event.kind,
            ferrum_proto::ProvenanceEventKind::PolicyBundleActivated
        ),
        "Expected PolicyBundleActivated provenance event, got: {:?}",
        activated_event.kind
    );
    assert_eq!(
        activated_event.object.object_id, bundle_id,
        "object_id should match bundle_id"
    );
    assert!(
        matches!(
            activated_event.object.object_type,
            ferrum_proto::ObjectType::PolicyBundle
        ),
        "object_type should be PolicyBundle"
    );
    assert_eq!(
        activated_event.policy_bundle_id,
        Some(ferrum_proto::PolicyBundleId(bundle_uuid)),
        "policy_bundle_id should be parsed from UUID bundle_id"
    );
    assert_eq!(
        activated_event.metadata.get("active"),
        Some(&serde_json::json!(true)),
        "metadata should contain active=true"
    );

    // Deactivate the bundle
    let deactivate_request = ferrum_proto::SetPolicyBundleActiveRequest { active: false };
    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::PUT)
                .uri(format!("/v1/policy-bundles/{}/active", bundle_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&deactivate_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("deactivate request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "deactivate should return 200 OK, got: {:?}",
        response.status()
    );

    // Query provenance for PolicyBundleDeactivated
    let query_request = ferrum_proto::ProvenanceQueryRequest {
        intent_id: None,
        execution_id: None,
        capability_id: None,
        event_kind: Some(ferrum_proto::ProvenanceEventKind::PolicyBundleDeactivated),
        since: None,
        until: None,
        edge_types: Vec::new(),
    };
    let query_response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri("/v1/provenance/query")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&query_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("provenance query should succeed");
    assert_eq!(
        query_response.status(),
        axum::http::StatusCode::OK,
        "provenance query should return 200 OK, got: {:?}",
        query_response.status()
    );

    let body = axum::body::to_bytes(query_response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let provenance_result: ferrum_proto::ProvenanceQueryResponse =
        serde_json::from_slice(&body).expect("valid ProvenanceQueryResponse JSON");
    assert!(
        !provenance_result.events.is_empty(),
        "Expected at least one PolicyBundleDeactivated provenance event, got empty"
    );

    let deactivated_event = &provenance_result.events[0];
    assert!(
        matches!(
            deactivated_event.kind,
            ferrum_proto::ProvenanceEventKind::PolicyBundleDeactivated
        ),
        "Expected PolicyBundleDeactivated provenance event, got: {:?}",
        deactivated_event.kind
    );
    assert_eq!(
        deactivated_event.metadata.get("active"),
        Some(&serde_json::json!(false)),
        "metadata should contain active=false"
    );
}

// ---------------------------------------------------------------------------
// TTL-1: Gateway rejects capability TTL greater than 300 seconds
// ---------------------------------------------------------------------------
