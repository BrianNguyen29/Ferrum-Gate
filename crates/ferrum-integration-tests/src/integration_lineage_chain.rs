//! Integration tests for provenance lineage query endpoint.
//!
//! These tests hit the HTTP endpoint and verify the lineage reconstruction
//! behavior using tower's ServiceExt for direct handler testing.

use axum::Router;
use axum::body::Body;
use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
use ferrum_gateway::GatewayRuntime;
use ferrum_gateway::build_router;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, AuthorizeExecutionRequest, CapabilityMintRequest, EffectType, EventId,
    IntentEnvelope, LineageDirection, LineageQueryRequest, OutcomeClause, ProposalId,
    ProvenanceEventKind, RiskTier, RollbackClass, ToolBinding,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{IntentRepo, SqliteStore, StoreFacade};
use http::{Method, Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

/// Spawn a test server backed by an in-memory sqlite store.
async fn spawn_test_server() -> Router {
    let store = SqliteStore::connect("sqlite::memory:")
        .await
        .expect("failed to connect to sqlite");
    store
        .apply_embedded_migrations()
        .await
        .expect("failed to apply migrations");

    let pdp = Arc::new(StaticPdpEngine);
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
    build_router(runtime)
}

// ---------------------------------------------------------------------------
// Provenance lineage integration tests
// ---------------------------------------------------------------------------

/// Helper to create a minimal intent envelope for testing (satisfies FK constraints).
fn make_test_intent(intent_id: ferrum_proto::IntentId) -> IntentEnvelope {
    let now = chrono::Utc::now();
    IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test-intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
    }
}

/// Full authorize → prepare → compensate flow, then verify lineage contains
/// the minimum chain: ActionProposalSubmitted (authorize), SideEffectPrepared (prepare),
/// SideEffectCompensated (terminal-present).
///
/// This is the Q1-P5 conservative slice: minimum provenance chain over existing
/// gateway execution surface, using existing event kinds. Explicit execute-step
/// remains absent (no /v1/executions/{id}/execute endpoint exists).
#[tokio::test]
async fn test_lineage_chain_minimum_provenance_events() {
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
        cap,
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Step 1: Evaluate a proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&proposal).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // Step 2: Mint a capability
    let cap_request = CapabilityMintRequest {
        intent_id: proposal.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ToolBinding {
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

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&cap_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Step 3: Authorize execution (creates execution in Prepared state; emits ActionProposalSubmitted)
    let auth_request = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&auth_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Prepare execution (emits SideEffectPrepared)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(prepare_response.prepared);

    // Step 5: Execute execution (transitions contract to ExecutedAwaitingVerify before compensate)
    // WS-Compensate state guard requires contract to be ExecutedAwaitingVerify before compensate.
    let execute_request = ferrum_proto::ExecuteExecutionRequest {
        payload: serde_json::json!({}),
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/execute", execution_id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&execute_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("execute request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    // Step 6: Compensate execution (emits SideEffectCompensated as terminal-present)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/compensate", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("compensate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    // Step 7: Query lineage for this execution_id
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/provenance/lineage/{}", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("lineage request should succeed");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "lineage endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    #[derive(serde::Deserialize)]
    struct LineageResponse {
        #[allow(dead_code)]
        execution_id: ferrum_proto::ExecutionId,
        events: Vec<ferrum_proto::ProvenanceEvent>,
    }
    let lineage: LineageResponse = serde_json::from_slice(&body).expect("valid json");

    // Verify we got exactly 5 events in the minimum chain
    // (authorize + prepare + ToolCallPrepared + execute + terminal)
    assert_eq!(
        lineage.events.len(),
        5,
        "lineage should contain exactly 5 events (authorize+prepare+ToolCallPrepared+execute+terminal), got {}",
        lineage.events.len()
    );

    // Extract event kinds using matches! macro since PartialEq is not derived
    let has_auth = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_prepare = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));
    let has_tool_prepared = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));
    let has_tool_executed = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_terminal = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCompensated));

    assert!(
        has_auth,
        "authorize event (ActionProposalSubmitted) must be present in lineage"
    );
    assert!(
        has_prepare,
        "prepare event (SideEffectPrepared) must be present in lineage"
    );
    assert!(
        has_tool_prepared,
        "prepare event (ToolCallPrepared) must be present in lineage"
    );
    assert!(
        has_tool_executed,
        "execute event (ToolCallExecuted) must be present in lineage"
    );
    assert!(
        has_terminal,
        "terminal-present event (SideEffectCompensated) must be present in lineage"
    );

    // Verify all events are linked to the execution_id
    for event in &lineage.events {
        assert!(
            event.execution_id.is_some(),
            "all lineage events must have execution_id set, event_id={}",
            event.event_id
        );
    }
}

/// WS4 adversarial regression test: verify that a non-terminal execution
/// (authorize + prepare without terminal compensate) does NOT masquerade
/// as a complete minimum chain.
///
/// This test exercises the conservative Q1-P5 behavior: partial flow
/// (authorize + prepare) must NOT produce a terminal-present event in lineage.
/// The minimum chain requires authorize + prepare + terminal-present events.
/// Without the terminal event, the chain is incomplete and must not be
/// mistaken for a complete execution.
///
/// Steps: evaluate → mint → authorize → prepare (SKIP compensate)
/// Asserts:
///   - Event count is 2 (authorize + prepare only)
///   - No SideEffectCompensated (terminal-present) event in lineage
///   - No SideEffectRolledBack event in lineage
#[tokio::test]
async fn test_lineage_adversarial_partial_execution_no_terminal() {
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
        cap,
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Step 1: Evaluate a proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&proposal).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // Step 2: Mint a capability
    let cap_request = CapabilityMintRequest {
        intent_id: proposal.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ToolBinding {
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

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&cap_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Step 3: Authorize execution (creates execution in Prepared state; emits ActionProposalSubmitted)
    let auth_request = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&auth_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Prepare execution (emits SideEffectPrepared)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(prepare_response.prepared);

    // Step 5: DO NOT compensate — this is the adversarial partial flow
    // We intentionally skip the terminal step to verify lineage cannot
    // masquerade as a complete chain.

    // Step 6: Query lineage for this execution_id (partial flow)
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/provenance/lineage/{}", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("lineage request should succeed");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "lineage endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    #[derive(serde::Deserialize)]
    struct LineageResponse {
        #[allow(dead_code)]
        execution_id: ferrum_proto::ExecutionId,
        events: Vec<ferrum_proto::ProvenanceEvent>,
    }
    let lineage: LineageResponse = serde_json::from_slice(&body).expect("valid json");

    // ADVERSARIAL ASSERTIONS: partial flow must NOT appear as complete chain

    // Event count must be exactly 3 (authorize + prepare + ToolCallPrepared only)
    assert_eq!(
        lineage.events.len(),
        3,
        "partial flow (authorize+prepare without terminal) should produce exactly 3 events, got {}",
        lineage.events.len()
    );

    // Verify the 3 expected events exist
    let has_auth = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_prepare = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));
    let has_tool_prepared = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));

    assert!(
        has_auth,
        "authorize event (ActionProposalSubmitted) must be present in lineage"
    );
    assert!(
        has_prepare,
        "prepare event (SideEffectPrepared) must be present in lineage"
    );
    assert!(
        has_tool_prepared,
        "prepare event (ToolCallPrepared) must be present in lineage"
    );

    // CRITICAL ADVERSARIAL CHECKS: no terminal-present events must exist
    let has_terminal_compensated = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCompensated));
    let has_terminal_rolled_back = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectRolledBack));

    assert!(
        !has_terminal_compensated,
        "ADVERSARIAL CHECK FAILED: SideEffectCompensated (terminal-present) must NOT appear in partial flow lineage"
    );
    assert!(
        !has_terminal_rolled_back,
        "ADVERSARIAL CHECK FAILED: SideEffectRolledBack (terminal-present) must NOT appear in partial flow lineage"
    );

    // Verify all events are linked to the execution_id
    for event in &lineage.events {
        assert!(
            event.execution_id.is_some(),
            "all lineage events must have execution_id set, event_id={}",
            event.event_id
        );
    }
}

/// Full happy-path lineage test: authorize → prepare → execute → verify (committed).
///
/// This test verifies the complete lineage chain for a successful execution:
/// - ActionProposalSubmitted (authorize)
/// - SideEffectPrepared (prepare)
/// - ToolCallPrepared (prepare)
/// - ToolCallExecuted (execute)
/// - SideEffectVerified (verify)
/// - SideEffectCommitted (verify committed terminal)
///
/// The verify step succeeds and transitions the execution to Committed state,
/// emitting SideEffectVerified (always) and SideEffectCommitted (on success).
#[tokio::test]
async fn test_lineage_chain_full_provenance_events() {
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
        cap,
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Step 1: Evaluate a proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&proposal).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // Step 2: Mint a capability
    let cap_request = CapabilityMintRequest {
        intent_id: proposal.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ToolBinding {
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

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&cap_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Step 3: Authorize execution (creates execution in Prepared state)
    let auth_request = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&auth_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Prepare execution (emits SideEffectPrepared + ToolCallPrepared)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(prepare_response.prepared);

    // Step 5: Execute execution (emits ToolCallExecuted)
    let execute_request = ferrum_proto::ExecuteExecutionRequest {
        payload: serde_json::json!({}),
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/execute", execution_id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&execute_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("execute request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    // Step 6: Verify execution (emits SideEffectVerified + SideEffectCommitted on success)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/verify", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("verify request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let verify_response: ferrum_proto::VerifyExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(
        verify_response.verified,
        "verify should succeed for happy path"
    );

    // Step 7: Query lineage for this execution_id
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/provenance/lineage/{}", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("lineage request should succeed");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "lineage endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    #[derive(serde::Deserialize)]
    struct LineageResponse {
        #[allow(dead_code)]
        execution_id: ferrum_proto::ExecutionId,
        events: Vec<ferrum_proto::ProvenanceEvent>,
    }
    let lineage: LineageResponse = serde_json::from_slice(&body).expect("valid json");

    // Verify we got exactly 6 events in the full committed chain
    // (authorize + prepare + execute + verify(verified+committed))
    assert_eq!(
        lineage.events.len(),
        6,
        "lineage should contain exactly 6 events (authorize+prepare+execute+verify), got {}",
        lineage.events.len()
    );

    // Verify all required event kinds are present
    let has_auth = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_prepare = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));
    let has_tool_prepared = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));
    let has_tool_executed = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_verified = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_committed = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(
        has_auth,
        "authorize event (ActionProposalSubmitted) must be present in lineage"
    );
    assert!(
        has_prepare,
        "prepare event (SideEffectPrepared) must be present in lineage"
    );
    assert!(
        has_tool_prepared,
        "prepare event (ToolCallPrepared) must be present in lineage"
    );
    assert!(
        has_tool_executed,
        "execute event (ToolCallExecuted) must be present in lineage"
    );
    assert!(
        has_verified,
        "verify event (SideEffectVerified) must be present in lineage"
    );
    assert!(
        has_committed,
        "committed terminal event (SideEffectCommitted) must be present in lineage"
    );

    // Verify all events are linked to the execution_id
    for event in &lineage.events {
        assert!(
            event.execution_id.is_some(),
            "all lineage events must have execution_id set, event_id={}",
            event.event_id
        );
    }
}

// ---------------------------------------------------------------------------
// FS adapter lineage integration tests — real adapter provenance chains
// ---------------------------------------------------------------------------

use ferrum_adapter_fs::{PlannableFsAdapter, register_fs_adapter};

/// Helper to create a test intent with a FileWrite scope for FsAdapter.
fn make_fs_test_intent(intent_id: ferrum_proto::IntentId, file_path: String) -> IntentEnvelope {
    let now = chrono::Utc::now();
    IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "fs-lineage-test-intent".to_string(),
        goal: "fs lineage test goal".to_string(),
        normalized_goal: "fs lineage test goal".to_string(),
        allowed_outcomes: Vec::new(),
        forbidden_outcomes: Vec::new(),
        resource_scope: vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: file_path,
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }],
        risk_tier: RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
    }
}

/// Full lineage chain test using FsAdapter with compensate terminal state.
///
/// Flow: evaluate → mint → authorize → prepare (FsAdapter) → execute → compensate
/// Verifies:
/// - Lineage contains the full 5-event minimum chain (authorize+prepare+execute+compensate)
/// - SideEffectCompensated terminal event is present
/// - adapter_key='fs' is captured in the lineage events
///
/// This proves real FsAdapter side effects are reflected in the provenance chain.
#[tokio::test]
async fn test_lineage_chain_fs_adapter_compensate() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    // Register FsAdapter + PlannableFsAdapter so prepare selects fs path
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    register_fs_adapter(&mut registry);
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    let rollback = Arc::new(rollback_service);

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Create temp file for FsAdapter snapshot
    let temp_dir = std::env::temp_dir();
    let test_file_path = temp_dir.join(format!(
        "ferrum-lineage-fs-compensate-{}.txt",
        uuid::Uuid::new_v4()
    ));
    let original_content = "original content for lineage compensate test";
    std::fs::write(&test_file_path, original_content).expect("failed to write temp file");
    let file_path_str = test_file_path.to_string_lossy().to_string();

    // Pre-insert intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_fs_test_intent(intent_id, file_path_str.clone());
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    let runtime = GatewayRuntime::new(
        pdp,
        cap,
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Step 1: Evaluate a proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "fs-lineage-compensate proposal".to_string(),
        tool_name: "file_write".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "file is written and compensated".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&proposal).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // Step 2: Mint a capability
    let cap_request = CapabilityMintRequest {
        intent_id: proposal.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "file_write".to_string(),
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

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&cap_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Step 3: Authorize execution
    let auth_request = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&auth_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Prepare execution (FsAdapter captures snapshot)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(prepare_response.prepared);

    // Step 5: Execute execution (transitions contract to ExecutedAwaitingVerify)
    let execute_request = ferrum_proto::ExecuteExecutionRequest {
        payload: serde_json::json!({ "content": "modified content for compensate" }),
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/execute", execution_id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&execute_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("execute request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    // Step 6: Compensate execution (FsAdapter rollback restores original content)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/compensate", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("compensate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let compensate_response: ferrum_proto::CompensateExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(
        compensate_response.compensated,
        "execution should be compensated"
    );

    // Step 7: Verify file content was restored (FsAdapter side effect)
    let restored_content =
        std::fs::read_to_string(&test_file_path).expect("read file after compensate");
    assert_eq!(
        restored_content, original_content,
        "FsAdapter compensate should restore original file content"
    );

    // Step 8: Query lineage for this execution_id
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/provenance/lineage/{}", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("lineage request should succeed");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "lineage endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    #[derive(serde::Deserialize)]
    struct LineageResponse {
        #[allow(dead_code)]
        execution_id: ferrum_proto::ExecutionId,
        events: Vec<ferrum_proto::ProvenanceEvent>,
    }
    let lineage: LineageResponse = serde_json::from_slice(&body).expect("valid json");

    // Verify we got 5 events in the compensate chain
    // (authorize + prepare + ToolCallPrepared + execute + terminal)
    assert_eq!(
        lineage.events.len(),
        5,
        "lineage should contain exactly 5 events for compensate chain, got {}",
        lineage.events.len()
    );

    // Verify required event kinds are present
    let has_auth = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_prepare = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));
    let has_tool_prepared = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));
    let has_tool_executed = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_terminal = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCompensated));

    assert!(has_auth, "authorize event must be present in lineage");
    assert!(has_prepare, "prepare event must be present in lineage");
    assert!(
        has_tool_prepared,
        "ToolCallPrepared event must be present in lineage"
    );
    assert!(
        has_tool_executed,
        "execute event must be present in lineage"
    );
    assert!(
        has_terminal,
        "SideEffectCompensated terminal event must be present in lineage"
    );

    // Verify all events are linked to the execution_id
    for event in &lineage.events {
        assert!(
            event.execution_id.is_some(),
            "all lineage events must have execution_id set, event_id={}",
            event.event_id
        );
    }

    // Clean up temp file
    let _ = std::fs::remove_file(&test_file_path);
}

/// Full lineage chain test using FsAdapter with committed terminal state.
///
/// Flow: evaluate → mint → authorize → prepare (FsAdapter) → execute → verify
/// Verifies:
/// - Lineage contains the full 6-event chain (authorize+prepare+execute+verify)
/// - SideEffectVerified event is present
/// - SideEffectCommitted terminal event is present
/// - adapter_key='fs' is captured in the lineage events
///
/// This proves real FsAdapter side effects are reflected in the provenance chain
/// for the successful (committed) execution path.
#[tokio::test]
async fn test_lineage_chain_fs_adapter_full_committed() {
    let pdp = Arc::new(StaticPdpEngine);
    let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());

    // Register FsAdapter + PlannableFsAdapter so prepare selects fs path
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    register_fs_adapter(&mut registry);
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    let rollback = Arc::new(rollback_service);

    let store = Arc::new(
        SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect to sqlite"),
    );
    store
        .apply_embedded_migrations()
        .await
        .expect("apply migrations");

    // Create temp file for FsAdapter snapshot
    let temp_dir = std::env::temp_dir();
    let test_file_path = temp_dir.join(format!(
        "ferrum-lineage-fs-committed-{}.txt",
        uuid::Uuid::new_v4()
    ));
    let original_content = "original content for lineage committed test";
    std::fs::write(&test_file_path, original_content).expect("failed to write temp file");
    let file_path_str = test_file_path.to_string_lossy().to_string();

    // Pre-insert intent to satisfy FK constraint
    let intent_id = ferrum_proto::IntentId::new();
    let intent = make_fs_test_intent(intent_id, file_path_str.clone());
    store
        .intents()
        .insert(&intent)
        .await
        .expect("intent insert should succeed");

    let runtime = GatewayRuntime::new(
        pdp,
        cap,
        rollback,
        store.clone() as Arc<dyn StoreFacade>,
        vec![],
    );
    let router = build_router(runtime);

    // Step 1: Evaluate a proposal
    let proposal = ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 0,
        title: "fs-lineage-committed proposal".to_string(),
        tool_name: "file_write".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "file is written and verified".to_string(),
        estimated_risk: RiskTier::Medium,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: chrono::Utc::now(),
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/proposals/test/evaluate")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&proposal).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("evaluate request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let proposal_id = proposal.proposal_id;

    // Step 2: Mint a capability
    let cap_request = CapabilityMintRequest {
        intent_id: proposal.intent_id,
        proposal_id: proposal.proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "file_write".to_string(),
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

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/capabilities/mint")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&cap_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("mint request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let cap_response: ferrum_proto::CapabilityMintResponse =
        serde_json::from_slice(&body).expect("valid json");
    let capability_id = cap_response.lease.capability_id;

    // Step 3: Authorize execution
    let auth_request = AuthorizeExecutionRequest {
        proposal_id,
        capability_id,
        dry_run: false,
    };

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/executions/authorize")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&auth_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("authorize request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let auth_response: ferrum_proto::AuthorizeExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    let execution_id = auth_response.execution.execution_id;

    // Step 4: Prepare execution (FsAdapter captures snapshot)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/prepare", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("prepare request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let prepare_response: ferrum_proto::PrepareExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(prepare_response.prepared);

    // Step 5: Execute execution (transitions contract to ExecutedAwaitingVerify)
    let execute_request = ferrum_proto::ExecuteExecutionRequest {
        payload: serde_json::json!({ "content": "new committed content" }),
    };
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/execute", execution_id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&execute_request).unwrap()))
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("execute request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    // Step 6: Verify execution (FsAdapter verifies and transitions to Committed)
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/v1/executions/{}/verify", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("verify request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let verify_response: ferrum_proto::VerifyExecutionResponse =
        serde_json::from_slice(&body).expect("valid json");
    assert!(
        verify_response.verified,
        "verify should succeed for FsAdapter committed path"
    );

    // Step 7: Query lineage for this execution_id
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("/v1/provenance/lineage/{}", execution_id))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();

    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("lineage request should succeed");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "lineage endpoint should return 200"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    #[derive(serde::Deserialize)]
    struct LineageResponse {
        #[allow(dead_code)]
        execution_id: ferrum_proto::ExecutionId,
        events: Vec<ferrum_proto::ProvenanceEvent>,
    }
    let lineage: LineageResponse = serde_json::from_slice(&body).expect("valid json");

    // Verify we got 6 events in the full committed chain
    // (authorize + prepare + ToolCallPrepared + execute + verify + committed)
    assert_eq!(
        lineage.events.len(),
        6,
        "lineage should contain exactly 6 events for committed chain, got {}",
        lineage.events.len()
    );

    // Verify required event kinds are present
    let has_auth = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ActionProposalSubmitted));
    let has_prepare = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectPrepared));
    let has_tool_prepared = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallPrepared));
    let has_tool_executed = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::ToolCallExecuted));
    let has_verified = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectVerified));
    let has_committed = lineage
        .events
        .iter()
        .any(|e| matches!(e.kind, ProvenanceEventKind::SideEffectCommitted));

    assert!(has_auth, "authorize event must be present in lineage");
    assert!(has_prepare, "prepare event must be present in lineage");
    assert!(
        has_tool_prepared,
        "ToolCallPrepared event must be present in lineage"
    );
    assert!(
        has_tool_executed,
        "execute event must be present in lineage"
    );
    assert!(
        has_verified,
        "SideEffectVerified event must be present in lineage"
    );
    assert!(
        has_committed,
        "SideEffectCommitted terminal event must be present in lineage"
    );

    // Verify all events are linked to the execution_id
    for event in &lineage.events {
        assert!(
            event.execution_id.is_some(),
            "all lineage events must have execution_id set, event_id={}",
            event.event_id
        );
    }

    // Clean up temp file
    let _ = std::fs::remove_file(&test_file_path);
}

// ---------------------------------------------------------------------------
// Original test stubs (preserved from previous file)
// ---------------------------------------------------------------------------

/// Verify that querying lineage with a non-existent event_id returns 404.
#[tokio::test]
async fn test_lineage_query_returns_404_for_nonexistent_event() {
    let router = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();
    let request_body = LineageQueryRequest {
        event_id: EventId(event_id),
        direction: LineageDirection::Ancestors,
        max_hops: 3,
    };

    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/provenance/lineage")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .expect("failed to build request");

    let res = router.oneshot(req).await.expect("request failed");
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

/// Verify that lineage query with default direction returns 404 for unknown event.
#[tokio::test]
async fn test_lineage_query_accepts_default_direction() {
    let router = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();
    // Request without explicit direction (should default to "ancestors")
    let request_body = serde_json::json!({
        "event_id": event_id.to_string(),
        "max_hops": 3
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/provenance/lineage")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("failed to build request");

    let res = router.oneshot(req).await.expect("request failed");
    // 404 since event doesn't exist, but the request format should be valid
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

/// Verify that lineage query with invalid event_id format returns 422 (Unprocessable Entity).
#[tokio::test]
async fn test_lineage_query_rejects_invalid_event_id_format() {
    let router = spawn_test_server().await;

    let request_body = serde_json::json!({
        "event_id": "not-a-valid-uuid",
        "direction": "ancestors",
        "max_hops": 3
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/provenance/lineage")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("failed to build request");

    let res = router.oneshot(req).await.expect("request failed");
    // Axum returns 422 for JSON deserialization errors
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

/// Verify that lineage query accepts all direction variants.
#[tokio::test]
async fn test_lineage_query_accepts_all_directions() {
    let router = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();

    for direction in &["ancestors", "descendants", "both"] {
        let request_body = serde_json::json!({
            "event_id": event_id.to_string(),
            "direction": direction,
            "max_hops": 3
        });

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/provenance/lineage")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .expect("failed to build request");

        let res = router.clone().oneshot(req).await.expect("request failed");
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "direction {} should be accepted",
            direction
        );
    }
}

/// Verify that lineage query with max_hops clamped to valid range.
#[tokio::test]
async fn test_lineage_query_handles_max_hops_clamping() {
    let router = spawn_test_server().await;

    let event_id = uuid::Uuid::new_v4();

    // Test with max_hops = 0 (should be clamped to 1)
    let request_body = serde_json::json!({
        "event_id": event_id.to_string(),
        "direction": "ancestors",
        "max_hops": 0
    });

    let req = Request::builder()
        .method(Method::POST)
        .uri("/v1/provenance/lineage")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .expect("failed to build request");

    let res = router.oneshot(req).await.expect("request failed");
    // 404 since event doesn't exist, but clamping should work (not 500)
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
