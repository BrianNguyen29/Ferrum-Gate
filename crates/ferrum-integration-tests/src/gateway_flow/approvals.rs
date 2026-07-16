use ferrum_cap::{CapabilityError, CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::{PdpEngine, StaticPdpEngine};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, IntentRepo, ProposalRepo, SqliteStore, StoreFacade,
};
use std::sync::Arc;
#[allow(unused_imports)]
use tower::ServiceExt;

mod support;
use support::*;

use ferrum_testkit::ApprovalFixture;
/// Test that approval_binding=None skips I6 validation (backward compatibility).
#[tokio::test]
async fn test_i6_none_binding_skips_validation() {
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

    // Setup: intent and proposal
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Mint capability with approval_binding=None
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
        approval_binding: None, // None = skip I6 validation
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should succeed with approval_binding=None
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
        .expect("authorize should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "authorize with approval_binding=None should succeed, got: {:?}",
        response.status()
    );
}

/// Test that a valid approval binding with matching digest succeeds.
#[tokio::test]
async fn test_i6_valid_binding_succeeds() {
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

    // Setup: intent and proposal
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    let proposal_digest = proposal.canonical_action_digest();
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create and insert a Granted approval with matching digest
    let approval_id = ferrum_proto::ApprovalId::new();
    let now = chrono::Utc::now();
    let approval = ApprovalFixture::new()
        .with_id(approval_id)
        .with_intent_id(intent_id)
        .with_proposal_id(proposal_id)
        .with_action_digest(proposal_digest.clone())
        .with_state(ferrum_proto::ApprovalState::Granted)
        .with_now(now)
        .build();
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    // Mint capability with approval_binding=Some
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id,
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: proposal_digest, // Must match proposal digest
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should succeed with valid binding
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
        .expect("authorize should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "authorize with valid approval binding should succeed, got: {:?}",
        response.status()
    );
}

/// Test that Pending approval state returns 403 PolicyDenied.
#[tokio::test]
async fn test_i6_pending_approval_denied() {
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

    // Setup
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    let proposal_digest = proposal.canonical_action_digest();
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create a Pending approval (not Granted)
    let approval_id = ferrum_proto::ApprovalId::new();
    let approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: proposal_digest.clone(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Pending, // Not Granted!
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    // Mint with approval_binding
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id,
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: proposal_digest,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should fail with 403 PolicyDenied
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
        .expect("authorize should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize with Pending approval should return 403, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "error code should be PolicyDenied, got: {:?}",
        error.code
    );
}

/// Test that digest mismatch returns 403 IntegrityMismatch.
#[tokio::test]
async fn test_i6_digest_mismatch_denied() {
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

    // Setup
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create approval with different digest (mismatch)
    let approval_id = ferrum_proto::ApprovalId::new();
    let wrong_digest = "wrong-digest-value".to_string();
    let approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: wrong_digest.clone(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Granted,
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    // Mint with approval_binding containing wrong digest
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id,
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: wrong_digest, // Mismatch with proposal digest
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should fail with 403 IntegrityMismatch
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
        .expect("authorize should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize with digest mismatch should return 403, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::IntegrityMismatch),
        "error code should be IntegrityMismatch, got: {:?}",
        error.code
    );
}

/// Test that expired binding returns 403 PolicyDenied.
#[tokio::test]
async fn test_i6_expired_binding_denied() {
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

    // Setup
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    let proposal_digest = proposal.canonical_action_digest();
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create approval with matching digest
    let approval_id = ferrum_proto::ApprovalId::new();
    let approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: proposal_digest.clone(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Granted,
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    // Mint with EXPIRED approval_binding
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id,
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: proposal_digest,
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1), // EXPIRED
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should fail with 403 PolicyDenied
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
        .expect("authorize should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize with expired binding should return 403, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::PolicyDenied),
        "error code should be PolicyDenied, got: {:?}",
        error.code
    );
    assert!(
        error.message.contains("expired"),
        "error message should mention expiration, got: {}",
        error.message
    );
}

/// Test that approval not found returns 403 IntegrityMismatch.
#[tokio::test]
async fn test_i6_approval_not_found_denied() {
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

    // Setup
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Mint with approval_binding pointing to non-existent approval
    let non_existent_approval_id = ferrum_proto::ApprovalId::new();
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id: non_existent_approval_id, // Does not exist!
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: "any-digest".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should fail with 403 IntegrityMismatch
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
        .expect("authorize should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize with non-existent approval should return 403, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::IntegrityMismatch),
        "error code should be IntegrityMismatch, got: {:?}",
        error.code
    );
}

/// Test that chain is broken when approval_binding.approved_action_digest differs
/// from approval.action_digest even though approval_id is valid.
/// This breaks the binding->approval chain.
#[tokio::test]
async fn test_i6_chain_broken_digest_mismatch_between_approval_and_binding() {
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

    // Setup: intent and proposal
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    let proposal_digest = proposal.canonical_action_digest();
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create approval with action_digest matching proposal digest
    let approval_id = ferrum_proto::ApprovalId::new();
    let approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: proposal_digest.clone(), // Matches proposal digest
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Granted,
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    // Mint capability with WRONG approved_action_digest (breaks chain)
    // The binding points to a valid approval but carries wrong digest
    let wrong_digest = "definitely-wrong-digest-value".to_string();
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id, // Valid approval ID
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: wrong_digest, // WRONG - does not match approval.action_digest
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Authorize should fail with 403 IntegrityMismatch (chain broken)
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
        .expect("authorize should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "authorize with chain-broken binding should return 403, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read error body");
    let error: ferrum_proto::ApiError = serde_json::from_slice(&body).expect("valid ApiError JSON");
    assert!(
        matches!(error.code, ferrum_proto::ApiErrorCode::IntegrityMismatch),
        "error code should be IntegrityMismatch, got: {:?}",
        error.code
    );
    assert!(
        error.message.contains("binding digest"),
        "error message should mention binding digest mismatch, got: {}",
        error.message
    );
}

// ---------------------------------------------------------------------------
// M3: cancel_execution integration tests
// ---------------------------------------------------------------------------

/// Test that a valid approval binding with matching digest succeeds and enforces single-use.
#[tokio::test]
async fn test_i6_single_use_with_valid_approval_binding() {
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

    // Setup: intent and proposal
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    let proposal_digest = proposal.canonical_action_digest();
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Create and insert a Granted approval with matching digest
    let approval_id = ferrum_proto::ApprovalId::new();
    let approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: proposal_digest.clone(), // Must match proposal digest
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Granted,
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    // Mint capability with approval_binding=Some (valid)
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
        approval_binding: Some(ferrum_proto::ApprovalBinding {
            approval_id,
            approver_roles: vec!["operator".to_string()],
            approved_action_digest: proposal_digest, // Must match proposal digest
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        }),
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
        .unwrap();
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // First authorize should succeed with valid binding
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
        .expect("first authorize should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "first authorize with valid approval binding should succeed, got: {:?}",
        response.status()
    );

    // Second authorize with same capability should fail with 409 Conflict (single-use)
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

    let response2 = tower::ServiceExt::oneshot(router, request2)
        .await
        .expect("second authorize request should succeed (network level)");

    assert_eq!(
        response2.status(),
        axum::http::StatusCode::CONFLICT,
        "second authorize should return 409 Conflict (single-use), got: {:?}",
        response2.status()
    );

    // Verify error body has Conflict semantics
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

    // Verify capability remains in Used state after failed reuse attempt.
    // The gateway persists to the durable store and does not populate the
    // in-memory cache, so read authoritative state from the store.
    let cap_lease = store
        .capabilities()
        .get(capability_id)
        .await
        .expect("store read should succeed")
        .expect("capability should still be present in the durable store after reuse failure");
    assert!(
        matches!(cap_lease.status, ferrum_proto::CapabilityStatus::Used),
        "capability status should remain Used after failed reuse, got: {:?}",
        cap_lease.status
    );

    // Document the owned-mint cache boundary.
    assert!(
        matches!(cap.get(capability_id).await, Err(CapabilityError::NotFound)),
        "in-memory capability service should remain unpopulated by gateway mint"
    );
}

// ---------------------------------------------------------------------------
// Pending approvals pagination test
// ---------------------------------------------------------------------------

/// Verify pagination returns correct limit and offset results.
#[tokio::test]
async fn test_pending_approvals_pagination() {
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

    // Insert 5 pending approvals with different created_at timestamps
    let intent_base = ferrum_proto::IntentId::new();
    let proposal_base = ferrum_proto::ProposalId::new();

    // Insert parent intent and proposal to satisfy foreign key constraints
    let intent = make_test_intent(intent_base);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent should succeed");
    let proposal = make_test_proposal(intent_base, proposal_base);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("insert proposal should succeed");
    seed_policy_evaluated(&store, &proposal).await;

    for i in 0..5i64 {
        let created_at = chrono::Utc::now() - chrono::Duration::seconds(1000 - i);
        let approval = make_test_approval(intent_base, proposal_base, created_at);
        store
            .approvals()
            .insert(&approval)
            .await
            .expect("insert should succeed");
    }

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // Test 1: Default pagination (limit=50, offset=0) should return all 5
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri("/v1/approvals")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "default pagination should return 200"
    );
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        5,
        "default should return all 5 pending approvals"
    );

    // Test 2: limit=2 should return only 2 approvals
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri("/v1/approvals?limit=2")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(approvals.len(), 2, "limit=2 should return 2 approvals");

    // Test 3: offset=3 with enough items should return remaining
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri("/v1/approvals?limit=10&offset=3")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        2,
        "offset=3, limit=10 should return 2 remaining approvals"
    );

    // Test 4: offset beyond available should return empty list
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri("/v1/approvals?offset=10")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        0,
        "offset beyond items should return empty list"
    );

    // Test 5: limit exceeding max (100) should return validation error
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri("/v1/approvals?limit=200")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "limit exceeding max should return 400"
    );
}

// ---------------------------------------------------------------------------
// Pending approvals filter by proposal_id test
// ---------------------------------------------------------------------------

/// Verify that proposal_id filter returns only pending approvals for that proposal.
#[tokio::test]
async fn test_pending_approvals_filtered_by_proposal_id() {
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

    // Create two different proposals
    let intent_base = ferrum_proto::IntentId::new();
    let proposal_a = ferrum_proto::ProposalId::new();
    let proposal_b = ferrum_proto::ProposalId::new();

    // Insert parent intent and proposals to satisfy foreign key constraints
    let intent = make_test_intent(intent_base);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("insert intent should succeed");
    let prop_a = make_test_proposal(intent_base, proposal_a);
    store
        .proposals()
        .insert(&prop_a)
        .await
        .expect("insert proposal_a should succeed");
    let prop_b = make_test_proposal(intent_base, proposal_b);
    store
        .proposals()
        .insert(&prop_b)
        .await
        .expect("insert proposal_b should succeed");

    // Insert 3 pending approvals for proposal_a and 2 for proposal_b
    for i in 0..3i64 {
        let created_at = chrono::Utc::now() - chrono::Duration::seconds(100 - i);
        let approval = make_test_approval(intent_base, proposal_a, created_at);
        store
            .approvals()
            .insert(&approval)
            .await
            .expect("insert should succeed");
    }
    for i in 0..2i64 {
        let created_at = chrono::Utc::now() - chrono::Duration::seconds(200 - i);
        let approval = make_test_approval(intent_base, proposal_b, created_at);
        store
            .approvals()
            .insert(&approval)
            .await
            .expect("insert should succeed");
    }

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let router = build_router(runtime);

    // Test 1: Filter by proposal_a - should return only 3 approvals for proposal_a
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri(format!("/v1/approvals?proposal_id={}", proposal_a))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "filtered request should return 200"
    );
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        3,
        "filter by proposal_a should return 3 approvals"
    );
    // Verify all returned approvals are for proposal_a
    for approval in approvals {
        let returned_proposal_id = approval
            .get("proposal_id")
            .expect("proposal_id field")
            .as_str()
            .expect("string");
        assert_eq!(
            returned_proposal_id,
            proposal_a.to_string(),
            "all returned approvals should be for proposal_a"
        );
    }

    // Test 2: Filter by proposal_b - should return only 2 approvals for proposal_b
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri(format!("/v1/approvals?proposal_id={}", proposal_b))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        2,
        "filter by proposal_b should return 2 approvals"
    );

    // Test 3: Filter composes with limit - should return only 2 of the 3 from proposal_a
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri(format!("/v1/approvals?proposal_id={}&limit=2", proposal_a))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        2,
        "filter with limit=2 should return 2 approvals"
    );

    // Test 4: Filter composes with offset - should skip the first approval for proposal_a
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri(format!(
            "/v1/approvals?proposal_id={}&limit=10&offset=1",
            proposal_a
        ))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        2,
        "filter with offset=1 should return 2 approvals (1 skipped)"
    );

    // Test 5: Invalid proposal_id should return 400
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri("/v1/approvals?proposal_id=not-a-uuid")
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("request should succeed");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "invalid proposal_id should return 400"
    );

    // Test 6: proposal_id for proposal with no approvals returns empty list
    let empty_proposal = ferrum_proto::ProposalId::new();
    let request = axum::http::Request::builder()
        .method(axum::http::Method::GET)
        .uri(format!("/v1/approvals?proposal_id={}", empty_proposal))
        .header("content-type", "application/json")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("request should succeed");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid json");
    let approvals = parsed
        .get("items")
        .expect("items field")
        .as_array()
        .expect("array");
    assert_eq!(
        approvals.len(),
        0,
        "proposal_id with no approvals should return empty list"
    );
}

// ---------------------------------------------------------------------------
// Outcome evaluation integration tests
// ---------------------------------------------------------------------------

/// Test that resolving a pending approval as granted returns 200 OK.
#[tokio::test]
async fn test_resolve_approval_granted_success() {
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

    // Setup: intent + proposal + approval (FK constraints)
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let approval = make_test_approval(intent_id, proposal_id, chrono::Utc::now());
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    let resolve_request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-operator".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        approve: true,
        reason: Some("approved for testing".to_string()),
        mfa_factor: None,
    };

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(format!("/v1/approvals/{}/resolve", approval.approval_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&resolve_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("resolve request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "resolve pending approval as granted should return 200 OK, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let result: ferrum_proto::ApprovalRequest =
        serde_json::from_slice(&body).expect("valid ApprovalRequest JSON");
    assert!(
        matches!(result.state, ferrum_proto::ApprovalState::Granted),
        "approval state should be Granted, got: {:?}",
        result.state
    );
}

/// Test that resolving a pending approval as denied returns 200 OK.
#[tokio::test]
async fn test_resolve_approval_denied_success() {
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

    // Setup: intent + proposal + approval (FK constraints)
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let approval = make_test_approval(intent_id, proposal_id, chrono::Utc::now());
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    let resolve_request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-operator".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        approve: false,
        reason: Some("not approved".to_string()),
        mfa_factor: None,
    };

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(format!("/v1/approvals/{}/resolve", approval.approval_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&resolve_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("resolve request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "resolve pending approval as denied should return 200 OK, got: {:?}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let result: ferrum_proto::ApprovalRequest =
        serde_json::from_slice(&body).expect("valid ApprovalRequest JSON");
    assert!(
        matches!(result.state, ferrum_proto::ApprovalState::Denied),
        "approval state should be Denied, got: {:?}",
        result.state
    );
}

/// Test that resolving an already-granted approval returns 409 Conflict.
#[tokio::test]
async fn test_resolve_approval_conflict_already_granted() {
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

    // Setup: intent + proposal + already-granted approval
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let approval_id = ferrum_proto::ApprovalId::new();
    let granted_approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "already granted".to_string(),
        action_digest: "test-digest".to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Granted,
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&granted_approval)
        .await
        .expect("approval insert");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    let resolve_request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-operator".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        approve: true,
        reason: None,
        mfa_factor: None,
    };

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(format!("/v1/approvals/{}/resolve", approval_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&resolve_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("resolve request should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::CONFLICT,
        "resolve already-granted approval should return 409 Conflict, got: {:?}",
        response.status()
    );
}

/// Test that resolving an already-denied approval returns 409 Conflict.
#[tokio::test]
async fn test_resolve_approval_conflict_already_denied() {
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

    // Setup: intent + proposal + already-denied approval
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let approval_id = ferrum_proto::ApprovalId::new();
    let denied_approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "already denied".to_string(),
        action_digest: "test-digest".to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Denied,
        created_at: chrono::Utc::now(),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&denied_approval)
        .await
        .expect("approval insert");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    let resolve_request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-operator".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        approve: true,
        reason: None,
        mfa_factor: None,
    };

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(format!("/v1/approvals/{}/resolve", approval_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&resolve_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("resolve request should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::CONFLICT,
        "resolve already-denied approval should return 409 Conflict, got: {:?}",
        response.status()
    );
}

/// Test that resolving an expired approval returns 403 Forbidden.
#[tokio::test]
async fn test_resolve_approval_forbidden_expired() {
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

    // Setup: intent + proposal + expired pending approval
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    // Create approval that expired in the past
    let approval_id = ferrum_proto::ApprovalId::new();
    let expired_approval = ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "expired approval".to_string(),
        action_digest: "test-digest".to_string(),
        expires_at: chrono::Utc::now() - chrono::Duration::hours(1), // Already expired
        state: ferrum_proto::ApprovalState::Pending,                 // Still Pending but expired
        created_at: chrono::Utc::now() - chrono::Duration::hours(2),
        resolver_evidence_version: None,
        owner_actor_id: None,
    };
    store
        .approvals()
        .insert(&expired_approval)
        .await
        .expect("approval insert");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    let resolve_request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-operator".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        approve: true,
        reason: None,
        mfa_factor: None,
    };

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(format!("/v1/approvals/{}/resolve", approval_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&resolve_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("resolve request should complete");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "resolve expired approval should return 403 Forbidden, got: {:?}",
        response.status()
    );
}

/// Test that resolving a pending approval emits a provenance event via the REST API.
#[tokio::test]
async fn test_resolve_approval_provenance_event_emitted() {
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

    // Setup: intent + proposal + approval (FK constraints)
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_test_intent(intent_id);
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert");

    let proposal_id = ferrum_proto::ProposalId::new();
    let proposal = make_test_proposal(intent_id, proposal_id);
    store
        .proposals()
        .insert(&proposal)
        .await
        .expect("proposal insert");
    seed_policy_evaluated(&store, &proposal).await;

    let approval = make_test_approval(intent_id, proposal_id, chrono::Utc::now());
    let approval_id = approval.approval_id;
    store
        .approvals()
        .insert(&approval)
        .await
        .expect("approval insert");

    let runtime = GatewayRuntime::new(
        pdp,
        cap.clone(),
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime.clone());

    let resolve_request = ferrum_proto::ApprovalResolveRequest {
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-operator".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        approve: true,
        reason: None,
        mfa_factor: None,
    };

    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(axum::http::Method::POST)
                .uri(format!("/v1/approvals/{}/resolve", approval_id))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&resolve_request).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("resolve request should succeed");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "resolve should return 200 OK, got: {:?}",
        response.status()
    );

    // Query provenance via the REST API to verify event was emitted
    let query_request = ferrum_proto::ProvenanceQueryRequest {
        intent_id: Some(intent_id),
        execution_id: None,
        capability_id: None,
        event_kind: Some(ferrum_proto::ProvenanceEventKind::ApprovalGranted),
        since: None,
        until: None,
        edge_types: Vec::new(),
    };

    let query_response = router
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
        "Expected at least one ApprovalGranted provenance event after resolve, got empty"
    );

    let event = &provenance_result.events[0];
    assert!(
        matches!(
            event.kind,
            ferrum_proto::ProvenanceEventKind::ApprovalGranted
        ),
        "Expected ApprovalGranted provenance event, got: {:?}",
        event.kind
    );
}

// ---------------------------------------------------------------------------
// POL-4: Policy bundle activation provenance audit test
// ---------------------------------------------------------------------------
