//! Integration tests for provenance lineage query endpoint.
//!
//! These tests hit the HTTP endpoint and verify the lineage reconstruction
//! behavior.  Note: in-memory sqlite is connection-local, so full event
//! injection tests would require a file-based database; these tests verify
//! the endpoint shape and fail-soft empty-lineage behavior.

use ferrum_cap::InMemoryCapabilityService;
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ApiErrorCode, EventId, ExecutionId, LineageDirection, LineageQueryRequest,
    ProvenanceQueryRequest,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{SqliteStore, StoreFacade};
use hyper::body::to_bytes;
use hyper::{Method, StatusCode};
use std::net::TcpListener;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Spawn a test server backed by an in-memory sqlite store.
async fn spawn_test_server() -> (String, JoinHandle<()>) {
    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("failed to connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");

    let pdp = Arc::new(StaticPdpEngine::default());
    let cap = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let runtime = GatewayRuntime::new(
        pdp,
        cap,
        rollback,
        Arc::new(store) as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
    let addr = listener.local_addr().expect("failed to get local addr");
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("server error");
    });

    (base_url, handle)
}

/// Verify that requesting lineage for an unknown execution returns 200
/// with an empty events array (fail-soft behavior).
#[tokio::test]
async fn test_lineage_endpoint_returns_empty_for_unknown_execution() {
    let (base_url, handle) = spawn_test_server().await;

    let execution_id = uuid::Uuid::new_v4();
    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage/{}", base_url, execution_id);
    let req = hyper::Request::builder()
        .method(Method::GET)
        .uri(&uri)
        .body(hyper::Body::empty())
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    // Fail-soft: return 200 with empty lineage rather than 404
    assert_eq!(res.status(), StatusCode::OK);

    let body = to_bytes(res.into_body())
        .await
        .expect("failed to read body");
    let response: serde_json::Value = serde_json::from_slice(&body).expect("invalid json response");

    // Response must match OpenAPI schema shape: { execution_id, events }
    assert!(
        response.get("execution_id").is_some(),
        "response missing execution_id"
    );
    assert!(response.get("events").is_some(), "response missing events");
    let events = response.get("events").unwrap();
    assert!(
        events.is_array(),
        "events must be an array, got: {}",
        events
    );

    handle.abort();
}

/// Verify that passing an invalid UUID format returns 400.
#[tokio::test]
async fn test_lineage_endpoint_rejects_invalid_uuid() {
    let (base_url, handle) = spawn_test_server().await;

    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage/not-a-uuid", base_url);
    let req = hyper::Request::builder()
        .method(Method::GET)
        .uri(&uri)
        .body(hyper::Body::empty())
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let body = to_bytes(res.into_body())
        .await
        .expect("failed to read body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("invalid error json");

    assert_eq!(
        error.get("code").and_then(|c| c.as_str()),
        Some("ValidationError"),
        "expected ValidationError code"
    );

    handle.abort();
}

/// Verify that a valid UUID returns 200 with proper Content-Type.
#[tokio::test]
async fn test_lineage_endpoint_returns_correct_content_type() {
    let (base_url, handle) = spawn_test_server().await;

    let execution_id = uuid::Uuid::new_v4();
    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage/{}", base_url, execution_id);
    let req = hyper::Request::builder()
        .method(Method::GET)
        .uri(&uri)
        .body(hyper::Body::empty())
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    assert_eq!(res.status(), StatusCode::OK);

    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .expect("content-type header missing");
    assert!(
        content_type.contains("application/json"),
        "expected application/json, got: {}",
        content_type
    );

    handle.abort();
}

/// Verify that querying lineage with a non-existent event_id returns 404.
#[tokio::test]
async fn test_lineage_query_returns_404_for_nonexistent_event() {
    let (base_url, handle) = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();
    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage", base_url);
    let request_body = serde_json::json!({
        "event_id": event_id.to_string(),
        "direction": "ancestors",
        "max_hops": 3
    });
    let req = hyper::Request::builder()
        .method(Method::POST)
        .uri(&uri)
        .header("content-type", "application/json")
        .body(hyper::Body::from(
            serde_json::to_string(&request_body).unwrap(),
        ))
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let body = to_bytes(res.into_body())
        .await
        .expect("failed to read body");
    let error: serde_json::Value = serde_json::from_slice(&body).expect("invalid error json");
    assert_eq!(
        error.get("code").and_then(|c| c.as_str()),
        Some("NotFound"),
        "expected NotFound error code"
    );

    handle.abort();
}

/// Verify that lineage query with default direction returns 404 for unknown event.
#[tokio::test]
async fn test_lineage_query_accepts_default_direction() {
    let (base_url, handle) = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();
    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage", base_url);

    // Request without explicit direction (should default to "ancestors")
    let request_body = serde_json::json!({
        "event_id": event_id.to_string(),
        "max_hops": 3
    });
    let req = hyper::Request::builder()
        .method(Method::POST)
        .uri(&uri)
        .header("content-type", "application/json")
        .body(hyper::Body::from(
            serde_json::to_string(&request_body).unwrap(),
        ))
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    // 404 since event doesn't exist, but the request format should be valid
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    handle.abort();
}

/// Verify that lineage query with invalid event_id format returns 400.
#[tokio::test]
async fn test_lineage_query_rejects_invalid_event_id_format() {
    let (base_url, handle) = spawn_test_server().await;

    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage", base_url);
    let request_body = serde_json::json!({
        "event_id": "not-a-valid-uuid",
        "direction": "ancestors",
        "max_hops": 3
    });
    let req = hyper::Request::builder()
        .method(Method::POST)
        .uri(&uri)
        .header("content-type", "application/json")
        .body(hyper::Body::from(
            serde_json::to_string(&request_body).unwrap(),
        ))
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    handle.abort();
}

/// Verify that lineage query accepts all direction variants.
#[tokio::test]
async fn test_lineage_query_accepts_all_directions() {
    let (base_url, handle) = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();
    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage", base_url);

    for direction in &["ancestors", "descendants", "both"] {
        let request_body = serde_json::json!({
            "event_id": event_id.to_string(),
            "direction": direction,
            "max_hops": 3
        });
        let req = hyper::Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("content-type", "application/json")
            .body(hyper::Body::from(
                serde_json::to_string(&request_body).unwrap(),
            ))
            .expect("failed to build request");

        let res = client.request(req).await.expect("request failed");
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "direction {} should be accepted",
            direction
        );
    }

    handle.abort();
}

/// Verify that lineage query with max_hops clamped to valid range.
#[tokio::test]
async fn test_lineage_query_handles_max_hops_clamping() {
    let (base_url, handle) = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();
    let client = hyper::Client::new();
    let uri = format!("{}/v1/provenance/lineage", base_url);

    // Test with max_hops = 0 (should be clamped to 1)
    let request_body = serde_json::json!({
        "event_id": event_id.to_string(),
        "direction": "ancestors",
        "max_hops": 0
    });
    let req = hyper::Request::builder()
        .method(Method::POST)
        .uri(&uri)
        .header("content-type", "application/json")
        .body(hyper::Body::from(
            serde_json::to_string(&request_body).unwrap(),
        ))
        .expect("failed to build request");

    let res = client.request(req).await.expect("request failed");
    // 404 since event doesn't exist, but clamping should work (not 500)
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    handle.abort();
}
