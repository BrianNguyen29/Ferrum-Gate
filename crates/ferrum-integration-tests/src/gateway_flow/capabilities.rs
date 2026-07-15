use ferrum_cap::{CapabilityError, CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::{PdpEngine, StaticPdpEngine};
use ferrum_proto::{RiskTier, RollbackClass};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{
    CapabilityRepo, ExecutionRepo, IntentRepo, LifecycleOutboxRepo, ProposalRepo, SqliteStore,
    StoreFacade,
};
use std::sync::Arc;

mod support;
use support::*;

/// Verify that a capability marked as Used cannot be used again.
/// This tests the `mark_used` -> `AlreadyUsed` behavior.
#[tokio::test]
async fn test_single_use_capability_cannot_be_reused() {
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    let request = ferrum_proto::CapabilityMintRequest {
        intent_id: ferrum_proto::IntentId::new(),
        proposal_id: ferrum_proto::ProposalId::new(),
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

    // Mint the capability
    let response = cap.mint(request).await.expect("mint should succeed");
    let capability_id = response.lease.capability_id;

    // First mark_used should succeed
    let first_result = cap.mark_used(capability_id).await;
    assert!(first_result.is_ok(), "first mark_used should succeed");
    let lease = first_result.unwrap();
    assert!(
        matches!(lease.status, ferrum_proto::CapabilityStatus::Used),
        "status should be set to Used, got: {:?}",
        lease.status
    );

    // Second mark_used should fail with AlreadyUsed
    let second_result = cap.mark_used(capability_id).await;
    assert!(
        matches!(second_result, Err(CapabilityError::AlreadyUsed)),
        "second mark_used should fail with AlreadyUsed, got: {:?}",
        second_result
    );
}

/// Verify that a capability cannot be reused across two authorize_execution calls
/// through the gateway HTTP endpoint. This proves single-use enforcement at the
/// gateway integration level.
#[tokio::test]
async fn test_single_use_capability_cannot_be_reused_via_gateway() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
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

    // Pre-insert intent to satisfy FK constraint before evaluate writes proposal synchronously
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

    // Step 1: Evaluate a proposal to create it in the database
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({ "content": "new content written by execute" }),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: noop_binding_metadata(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };
    let proposal_id = proposal.proposal_id;

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
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "evaluate endpoint should return 200"
    );

    // Step 2: Mint a capability using the proposal from evaluate via HTTP
    let mint_request = ferrum_proto::CapabilityMintRequest {
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
            serde_json::to_vec(&mint_request).unwrap(),
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
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Step 3: First authorize_execution call should succeed
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
        .expect("first authorize request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "first authorize should return 200, got: {:?}",
        response.status()
    );

    // Step 4: Second authorize_execution call with same capability should fail with Conflict
    let auth_request2 = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request2 = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request2).unwrap(),
        ))
        .unwrap();

    let response2 = tower::ServiceExt::oneshot(router.clone(), request2)
        .await
        .expect("second authorize request should succeed (network level)");

    assert_eq!(
        response2.status(),
        axum::http::StatusCode::CONFLICT,
        "second authorize should return 409 Conflict, got: {:?}",
        response2.status()
    );

    // Verify error body has explicit error code semantics (WS2 adversarial assertion)
    let error_body = axum::body::to_bytes(response2.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error_response: ferrum_proto::ApiError =
        serde_json::from_slice(&error_body).expect("error body should be valid ApiError JSON");
    assert!(
        matches!(error_response.code, ferrum_proto::ApiErrorCode::Conflict),
        "error code should be Conflict, got: {:?}",
        error_response.code
    );
    assert!(
        !error_response.message.is_empty(),
        "error message should be non-empty, got: {:?}",
        error_response.message
    );

    // Verify capability remains in Used state after failed reuse attempt
    // (proves no state corruption: used capability is not revived)
    let cap_lease = cap
        .get(capability_id)
        .await
        .expect("capability should still be accessible after reuse failure");
    assert!(
        matches!(cap_lease.status, ferrum_proto::CapabilityStatus::Used),
        "capability status should remain Used after failed reuse, got: {:?}",
        cap_lease.status
    );
}

/// Verify that capability authorize can recover an active persisted capability
/// after in-memory state loss (fresh InMemoryCapabilityService).
/// Flow:
/// 1. runtime1 mint: persists capability as Active
/// 2. runtime2 (fresh memory) first authorize: succeeds, marks Used
/// 3. runtime2 second authorize: fails with AlreadyUsed (single-use enforced)
#[tokio::test]
async fn test_capability_durable_after_in_memory_state_loss() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    // Shared store across both runtimes
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

    // Step 1: Create runtime1 and router, mint capability
    let cap1: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());
    let runtime1 = GatewayRuntime::new(
        pdp.clone(),
        cap1.clone(),
        rollback.clone(),
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router1 = build_router(runtime1);

    // Evaluate proposal via runtime1
    let proposal = ferrum_proto::ActionProposal {
        proposal_id: ferrum_proto::ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({ "content": "new content for verify test" }),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: noop_binding_metadata(),
        created_at: chrono::Utc::now(),
        owner_actor_id: None,
    };
    let proposal_id = proposal.proposal_id;

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&proposal).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router1.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    // Mint capability via runtime1
    let mint_request = ferrum_proto::CapabilityMintRequest {
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
            serde_json::to_vec(&mint_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router1.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Verify the capability is Active in store
    let stored_cap = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("store lookup should succeed")
        .expect("capability should exist in store");
    assert!(
        matches!(stored_cap.status, ferrum_proto::CapabilityStatus::Active),
        "minted capability should be Active, got: {:?}",
        stored_cap.status
    );

    // Step 2: Create runtime2 with FRESH InMemoryCapabilityService (simulating state loss)
    let cap2: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());
    let runtime2 = GatewayRuntime::new(
        pdp,
        cap2.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router2 = build_router(runtime2);

    // Step 3: First authorize via runtime2 should succeed (falls back to Active persisted capability)
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

    let response = tower::ServiceExt::oneshot(router2.clone(), request)
        .await
        .expect("first authorize request should succeed (network level)");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "first authorize after state loss should return 200, got: {:?}",
        response.status()
    );

    // Verify the capability is now Used in store
    let stored_cap_after = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("store lookup should succeed")
        .expect("capability should exist in store");
    assert!(
        matches!(
            stored_cap_after.status,
            ferrum_proto::CapabilityStatus::Used
        ),
        "after authorize, capability should be Used, got: {:?}",
        stored_cap_after.status
    );

    // Step 4: Second authorize via runtime2 should fail with Conflict (AlreadyUsed)
    let auth_request2 = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request2 = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request2).unwrap(),
        ))
        .unwrap();

    let response2 = tower::ServiceExt::oneshot(router2.clone(), request2)
        .await
        .expect("second authorize request should succeed (network level)");

    assert_eq!(
        response2.status(),
        axum::http::StatusCode::CONFLICT,
        "second authorize should return 409 Conflict, got: {:?}",
        response2.status()
    );

    // Verify error body has AlreadyUsed semantics
    let error_body = axum::body::to_bytes(response2.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error_response: ferrum_proto::ApiError =
        serde_json::from_slice(&error_body).expect("error body should be valid ApiError JSON");
    assert!(
        matches!(error_response.code, ferrum_proto::ApiErrorCode::Conflict),
        "error code should be Conflict, got: {:?}",
        error_response.code
    );
}

// ---------------------------------------------------------------------------
// Durable expired-Active capability authorize test (P0 regression)
// ---------------------------------------------------------------------------

/// Verify that a durable (persisted) capability that is still `Active` but
/// already expired surfaces `Expired` (HTTP 400 / `CapabilityExpired`), NOT
/// `AlreadyUsed` (409 Conflict), when authorized after in-memory state loss.
///
/// The expired check in `get_capability_for_authorize` short-circuits before
/// the durable single-use CAS, so no execution record and no lifecycle outbox
/// entry may be created, and the capability must remain Active (not consumed).
#[tokio::test]
async fn test_authorize_durable_expired_active_capability_returns_expired() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);

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

    // Seed intent + proposal (foreign keys for the capability row).
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");
    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");

    // Insert a capability that is Active but already expired, directly into the
    // durable store. This is the state a lease is in after surviving in-memory
    // state loss past its TTL.
    let capability_id = ferrum_proto::CapabilityId::new();
    let now = chrono::Utc::now();
    let lease = ferrum_proto::CapabilityLease {
        capability_id,
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
        issued_by: "test".to_string(),
        policy_bundle_id: ferrum_proto::PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status: ferrum_proto::CapabilityStatus::Active,
        issued_at: now - chrono::Duration::seconds(120),
        expires_at: now - chrono::Duration::seconds(60), // already expired
        revoked_at: None,
        metadata: ferrum_proto::JsonMap::new(),
        owner_actor_id: None,
    };
    store
        .capabilities()
        .insert(&lease)
        .await
        .expect("capability insert should succeed");

    // Fresh in-memory capability service (simulates state loss): authorize must
    // fall back to the durable store and observe the expired Active lease.
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());
    let runtime = GatewayRuntime::new(
        pdp,
        cap,
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

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

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("authorize request should succeed (network level)");

    // Expired maps to 400 Bad Request, not 409 Conflict (AlreadyUsed).
    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "expired durable capability should return 400 Bad Request (Expired), got: {:?}",
        response.status()
    );

    let error_body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error_response: ferrum_proto::ApiError =
        serde_json::from_slice(&error_body).expect("error body should be valid ApiError JSON");
    assert!(
        matches!(
            error_response.code,
            ferrum_proto::ApiErrorCode::CapabilityExpired
        ),
        "error code should be CapabilityExpired (not Conflict/AlreadyUsed), got: {:?}",
        error_response.code
    );

    // The expired check short-circuits before the durable CAS: no execution and
    // no lifecycle outbox entry may be created.
    let executions = store
        .executions()
        .list_by_capability(capability_id)
        .await
        .expect("list executions by capability should succeed");
    assert!(
        executions.is_empty(),
        "no execution record may be created for an expired capability, got: {}",
        executions.len()
    );

    let pending_outbox = store
        .lifecycle_outbox()
        .list_by_status(ferrum_proto::LifecycleOutboxStatus::PendingProvenance, 100)
        .await
        .expect("list pending outbox should succeed");
    assert!(
        pending_outbox.is_empty(),
        "no lifecycle outbox entry may be created for an expired capability, got: {}",
        pending_outbox.len()
    );

    // The capability itself must remain Active+expired (not flipped to Used).
    let stored = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("capability lookup should succeed")
        .expect("capability should exist");
    assert!(
        matches!(stored.status, ferrum_proto::CapabilityStatus::Active),
        "expired capability must remain Active (not consumed), got: {:?}",
        stored.status
    );
}

// ---------------------------------------------------------------------------
// R3 no auto-commit test
// ---------------------------------------------------------------------------

/// Verify that `authorize_execution` rejects (403 IntegrityMismatch) when the
/// request's `proposal_id` does not match the capability lease's
/// `proposal_id`, before any durable capability mutation.
///
/// Critical invariant: a holder of one capability must not be able to
/// authorize an unrelated proposal. The guard must run before
/// `mark_capability_used_durable` so the capability remains Active / usable.
#[tokio::test]
async fn test_authorize_rejects_proposal_capability_mismatch() {
    let pdp: Arc<dyn PdpEngine> = Arc::new(StaticPdpEngine);
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

    // Setup: one intent and two distinct proposals (A = lease, B = request).
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_a = ferrum_proto::ProposalId::new();
    let proposal_a_record = make_test_proposal(intent_id, proposal_a);
    store
        .proposals()
        .insert(&proposal_a_record)
        .await
        .expect("proposal A insert");
    seed_policy_evaluated(&store, &proposal_a_record).await;

    let proposal_b = ferrum_proto::ProposalId::new();
    let proposal_b_record = make_test_proposal(intent_id, proposal_b);
    store
        .proposals()
        .insert(&proposal_b_record)
        .await
        .expect("proposal B insert");
    seed_policy_evaluated(&store, &proposal_b_record).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Mint a capability bound to proposal A.
    let mint_request = ferrum_proto::CapabilityMintRequest {
        intent_id,
        proposal_id: proposal_a,
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
            serde_json::to_vec(&mint_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read mint body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;
    let lease_proposal_id = cap_response.lease.proposal_id;
    assert_eq!(
        lease_proposal_id, proposal_a,
        "minted capability should be bound to proposal A"
    );

    // Authorize with the MISMATCHED proposal B but the same capability_id.
    // Guard must reject (403 IntegrityMismatch) BEFORE the durable
    // single-use mark, leaving the capability Active and reusable.
    let auth_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: proposal_b,
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
        .expect("authorize request should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize should return FORBIDDEN on proposal/capability mismatch, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read authorize error body");
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::IntegrityMismatch),
        "error code should be IntegrityMismatch on proposal/capability mismatch, got: {:?}",
        error.code
    );

    // Capability must remain Active (not Used) after the rejected authorize
    // — the guard runs before `mark_capability_used_durable`.
    let lease_after = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("store capability get")
        .expect("capability should still exist");
    assert!(
        matches!(lease_after.status, ferrum_proto::CapabilityStatus::Active),
        "capability must remain Active after rejected mismatched authorize, got: {:?}",
        lease_after.status
    );

    // Sanity: a follow-up authorize with the MATCHED proposal A still
    // succeeds (proves the capability was not consumed by the rejected call).
    let auth_request_match = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: proposal_a,
        capability_id,
        dry_run: false,
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&auth_request_match).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("authorize request should complete");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "matched authorize (proposal A) should succeed after rejected mismatch, got: {:?}",
        response.status()
    );
}

// ---------------------------------------------------------------------------
// I6 Approval Binding Digest Validation Tests
// ---------------------------------------------------------------------------

/// Verify that the HTTP capability mint endpoint rejects a requested TTL above
/// the 300-second safety limit with a 400 Bad Request / ValidationError.
#[tokio::test]
async fn test_gateway_rejects_capability_ttl_over_300() {
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

    let proposal = make_test_proposal(intent_id, ferrum_proto::ProposalId::new());
    let proposal_id = proposal.proposal_id;
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    let mint_request = ferrum_proto::CapabilityMintRequest {
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
        requested_ttl_secs: 301,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let request = axum::http::Request::builder()
        .method(axum::http::Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&mint_request).unwrap(),
        ))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed (network level)");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "gateway should reject TTL > 300 with 400 Bad Request, got: {:?}",
        response.status()
    );

    let error_body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error_response: ferrum_proto::ApiError =
        serde_json::from_slice(&error_body).expect("error body should be valid ApiError JSON");
    assert!(
        matches!(
            error_response.code,
            ferrum_proto::ApiErrorCode::ValidationError
        ),
        "error code should be ValidationError for TTL too long, got: {:?}",
        error_response.code
    );
    assert!(
        !error_response.message.is_empty(),
        "error message should be non-empty, got: {:?}",
        error_response.message
    );
}
