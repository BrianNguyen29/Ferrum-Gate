//! Object-guard tests (P1.4d).
//!
//! Covers the exact-owner access helper and representative handler integration
//! points for capability revoke and execution read/cancel. These tests use
//! direct handler calls with an injected `AuthActor`; the router/middleware
//! path is exercised separately by existing integration tests.

use std::sync::Arc;

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use chrono::Utc;

use crate::AuthActor;
use crate::auth_actor::enforce_object_owner_guard;
use crate::capabilities::revoke_capability;
use crate::execution::{
    authorize_execution, cancel_execution, evaluate_outcome, prepare_execution,
};
use crate::lineage::{get_execution, get_execution_lineage};
use crate::problem::ApiProblem;
use crate::state::{AppState, AuthMode, GatewayRuntime, ServerConfig};
use ferrum_proto::{
    ActionProposal, ActorRef, ActorType, ApprovalMode, CapabilityId, CapabilityLease,
    CapabilityStatus, Decision, EventId, ExecutionId, ExecutionRecord, ExecutionState,
    HashChainRef, IntentEnvelope, IntentId, IntentStatus, JsonMap, ObjectRef, ObjectType,
    OutcomeReport, PolicyBundleId, PrincipalId, ProposalId, ProvenanceEvent, ProvenanceEventKind,
    ResourceMode, ResourceSelector, RiskTier, RollbackClass, TaintBudget, TimeBudget, ToolBinding,
    TrustContextSummary,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{SqliteStore, StoreFacade};

fn actor(id: &str) -> AuthActor {
    AuthActor {
        actor_id: id.to_string(),
        source: "test",
        scopes: vec!["*".to_string()],
        role: None,
    }
}

fn actor_ext(id: &str) -> Option<Extension<AuthActor>> {
    Some(Extension(actor(id)))
}

fn some_owner(id: &str) -> Option<String> {
    Some(id.to_string())
}

/// Assert two 404 problem envelopes are identical in the fields visible to
/// callers (status, code, message, retriable, details). Correlation IDs are
/// intentionally excluded because they are per-response values.
fn assert_404_envelopes_equal(a: &ApiProblem, b: &ApiProblem) {
    assert_eq!(
        a.0.message, "object not found",
        "expected generic object not found"
    );
    assert_eq!(a.1, b.1, "status must match");
    assert_eq!(a.0.code, b.0.code, "error code must match");
    assert_eq!(a.0.message, b.0.message, "error message must match");
    assert_eq!(a.0.retriable, b.0.retriable, "retriable flag must match");
    assert_eq!(a.0.details, b.0.details, "details must match");
}

// ---------------------------------------------------------------------------
// Helper unit tests
// ---------------------------------------------------------------------------

#[test]
fn helper_disabled_allows_without_actor() {
    let result = enforce_object_owner_guard(
        None,
        some_owner("actor-a").as_ref(),
        AuthMode::Disabled,
        None,
        Utc::now(),
        "execution",
        "test",
    );
    assert!(result.is_ok());
}

#[test]
fn helper_bearer_allows_without_actor() {
    let result = enforce_object_owner_guard(
        None,
        some_owner("actor-a").as_ref(),
        AuthMode::Bearer,
        None,
        Utc::now(),
        "execution",
        "test",
    );
    assert!(result.is_ok());
}

#[test]
fn helper_scoped_matching_owner_allows() {
    let result = enforce_object_owner_guard(
        Some(&actor("actor-a")),
        some_owner("actor-a").as_ref(),
        AuthMode::Scoped,
        None,
        Utc::now(),
        "execution",
        "test",
    );
    assert!(result.is_ok());
}

#[test]
fn helper_scoped_mismatching_owner_denies_with_404() {
    let result = enforce_object_owner_guard(
        Some(&actor("actor-b")),
        some_owner("actor-a").as_ref(),
        AuthMode::Scoped,
        None,
        Utc::now(),
        "execution",
        "test",
    );
    let err = result.expect_err("expected deny");
    assert_eq!(err.1, StatusCode::NOT_FOUND);
}

#[test]
fn helper_scoped_missing_actor_denies() {
    let result = enforce_object_owner_guard(
        None,
        some_owner("actor-a").as_ref(),
        AuthMode::Scoped,
        None,
        Utc::now(),
        "execution",
        "test",
    );
    assert_eq!(result.unwrap_err().1, StatusCode::NOT_FOUND);
}

#[test]
fn helper_unbound_without_compat_denies() {
    let result = enforce_object_owner_guard(
        Some(&actor("actor-a")),
        None,
        AuthMode::Scoped,
        None,
        Utc::now(),
        "execution",
        "test",
    );
    assert_eq!(result.unwrap_err().1, StatusCode::NOT_FOUND);
}

#[test]
fn helper_unbound_with_future_compat_allows() {
    let allow_until = Utc::now() + chrono::Duration::hours(1);
    let result = enforce_object_owner_guard(
        Some(&actor("actor-a")),
        None,
        AuthMode::Scoped,
        Some(allow_until),
        Utc::now(),
        "execution",
        "test",
    );
    assert!(result.is_ok());
}

#[test]
fn helper_unbound_with_expired_compat_denies() {
    let allow_until = Utc::now() - chrono::Duration::hours(1);
    let result = enforce_object_owner_guard(
        Some(&actor("actor-a")),
        None,
        AuthMode::Scoped,
        Some(allow_until),
        Utc::now(),
        "execution",
        "test",
    );
    assert_eq!(result.unwrap_err().1, StatusCode::NOT_FOUND);
}

#[test]
fn helper_unbound_with_exact_deadline_denies() {
    let now = Utc::now();
    let result = enforce_object_owner_guard(
        Some(&actor("actor-a")),
        None,
        AuthMode::Scoped,
        Some(now),
        now,
        "execution",
        "test",
    );
    assert_eq!(result.unwrap_err().1, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Handler integration tests
// ---------------------------------------------------------------------------

async fn test_state(auth_mode: AuthMode, compat: Option<chrono::DateTime<Utc>>) -> Arc<AppState> {
    let pdp = Arc::new(ferrum_pdp::StaticPdpEngine);
    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();
    let cap = Arc::new(crate::capabilities::StoreCapabilityService::new(
        store.clone(),
    ));
    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));
    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
    let config = ServerConfig {
        auth_mode,
        legacy_object_compat_allow_until: compat,
        ..Default::default()
    };
    AppState::test_new(runtime, config)
}

fn make_intent(intent_id: IntentId, owner: Option<&str>) -> IntentEnvelope {
    IntentEnvelope {
        intent_id,
        principal_id: PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![],
        forbidden_outcomes: vec![],
        resource_scope: vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ResourceMode::Read,
            content_hash: None,
        }],
        risk_tier: RiskTier::Low,
        approval_mode: ApprovalMode::None,
        default_rollback_class: RollbackClass::R0NativeReversible,
        time_budget: TimeBudget {
            max_duration_ms: 30000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: TrustContextSummary {
            input_labels: vec![],
            sensitivity_labels: vec![],
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: vec![],
        tags: vec![],
        metadata: JsonMap::new(),
        status: IntentStatus::Active,
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        owner_actor_id: owner.map(|s| s.to_string()),
    }
}

fn make_proposal(intent_id: IntentId, proposal_id: ProposalId) -> ActionProposal {
    ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        taint_inputs: vec![],
        metadata: JsonMap::new(),
        created_at: Utc::now(),
        owner_actor_id: None,
    }
}

fn make_capability_lease(
    intent_id: IntentId,
    proposal_id: ProposalId,
    capability_id: CapabilityId,
    owner: Option<&str>,
) -> CapabilityLease {
    let now = Utc::now();
    CapabilityLease {
        capability_id,
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "test-server".to_string(),
            tool_name: "test-tool".to_string(),
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
        issued_by: "test".to_string(),
        policy_bundle_id: PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status: CapabilityStatus::Active,
        issued_at: now,
        expires_at: now + chrono::Duration::hours(1),
        revoked_at: None,
        metadata: JsonMap::new(),
        owner_actor_id: owner.map(|s| s.to_string()),
    }
}

async fn insert_execution_with_parents(
    store: &Arc<dyn StoreFacade>,
    owner: Option<&str>,
) -> ExecutionRecord {
    let now = Utc::now();
    let execution_id = ExecutionId::new();
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let capability_id = CapabilityId::new();

    store
        .intents()
        .insert(&make_intent(intent_id, owner))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_proposal(intent_id, proposal_id))
        .await
        .unwrap();
    store
        .capabilities()
        .insert(&make_capability_lease(
            intent_id,
            proposal_id,
            capability_id,
            owner,
        ))
        .await
        .unwrap();

    let record = ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Authorized,
        started_at: now,
        finished_at: None,
        result_digest: None,
        metadata: JsonMap::new(),
        owner_actor_id: owner.map(|s| s.to_string()),
    };
    store.executions().insert(&record).await.unwrap();
    record
}

async fn insert_capability_with_parents(
    store: &Arc<dyn StoreFacade>,
    owner: Option<&str>,
) -> CapabilityLease {
    let intent_id = IntentId::new();
    let proposal_id = ProposalId::new();
    let capability_id = CapabilityId::new();
    store
        .intents()
        .insert(&make_intent(intent_id, owner))
        .await
        .unwrap();
    store
        .proposals()
        .insert(&make_proposal(intent_id, proposal_id))
        .await
        .unwrap();
    let lease = make_capability_lease(intent_id, proposal_id, capability_id, owner);
    store.capabilities().insert(&lease).await.unwrap();
    lease
}

async fn seed_capability_minted(store: &Arc<dyn StoreFacade>, lease: &CapabilityLease) {
    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::CapabilityMinted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "gateway".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::Capability,
            object_id: lease.capability_id.to_string(),
            summary: None,
        },
        intent_id: Some(lease.intent_id),
        proposal_id: Some(lease.proposal_id),
        execution_id: None,
        capability_id: Some(lease.capability_id),
        rollback_contract_id: None,
        policy_bundle_id: Some(lease.policy_bundle_id),
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
        source_runtime_id: None,
    };
    store
        .provenance()
        .append_event_with_edges(&event, &[])
        .await
        .unwrap();
}

#[tokio::test]
async fn execution_detail_owner_match_succeeds() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let execution_id = record.execution_id;
    let result = get_execution(
        State(state),
        Path(execution_id.to_string()),
        actor_ext("actor-a"),
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn execution_detail_owner_mismatch_returns_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let execution_id = record.execution_id;
    let result = get_execution(
        State(state),
        Path(execution_id.to_string()),
        actor_ext("actor-b"),
    )
    .await;
    let err = result.expect_err("expected 404");
    assert_eq!(err.1, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn execution_detail_bearer_ignores_owner() {
    let state = test_state(AuthMode::Bearer, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let execution_id = record.execution_id;
    let result = get_execution(
        State(state),
        Path(execution_id.to_string()),
        None, // no auth actor in bearer mode
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn execution_cancel_owner_mismatch_leaves_state_unchanged() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let execution_id = record.execution_id;
    let result = cancel_execution(
        State(state.clone()),
        Path(execution_id.to_string()),
        actor_ext("actor-b"),
    )
    .await;
    let err = result.expect_err("expected 404");
    assert_eq!(err.1, StatusCode::NOT_FOUND);

    let after = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.state, ExecutionState::Authorized);
}

#[tokio::test]
async fn capability_revoke_owner_mismatch_leaves_capability_active() {
    let state = test_state(AuthMode::Scoped, None).await;
    let lease = insert_capability_with_parents(&state.runtime.store, Some("actor-a")).await;
    let cap_id = lease.capability_id;
    let result = revoke_capability(
        State(state.clone()),
        Path(cap_id.to_string()),
        actor_ext("actor-b"),
    )
    .await;
    let err = result.expect_err("expected 404");
    assert_eq!(err.1, StatusCode::NOT_FOUND);

    let after = state
        .runtime
        .store
        .capabilities()
        .get(cap_id)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(after.status, CapabilityStatus::Active));
}

#[tokio::test]
async fn capability_revoke_owner_match_succeeds() {
    let state = test_state(AuthMode::Scoped, None).await;
    let lease = insert_capability_with_parents(&state.runtime.store, Some("actor-a")).await;
    let cap_id = lease.capability_id;
    seed_capability_minted(&state.runtime.store, &lease).await;
    let result = revoke_capability(
        State(state.clone()),
        Path(cap_id.to_string()),
        actor_ext("actor-a"),
    )
    .await;
    assert!(result.is_ok());

    let after = state
        .runtime
        .store
        .capabilities()
        .get(cap_id)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(after.status, CapabilityStatus::Revoked));
}

#[tokio::test]
async fn unbound_execution_denied_by_default() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, None).await;
    let execution_id = record.execution_id;
    let result = get_execution(
        State(state),
        Path(execution_id.to_string()),
        actor_ext("actor-a"),
    )
    .await;
    assert_eq!(result.unwrap_err().1, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unbound_execution_allowed_until_future_compat() {
    let allow_until = Utc::now() + chrono::Duration::hours(1);
    let state = test_state(AuthMode::Scoped, Some(allow_until)).await;
    let record = insert_execution_with_parents(&state.runtime.store, None).await;
    let execution_id = record.execution_id;
    let result = get_execution(
        State(state),
        Path(execution_id.to_string()),
        actor_ext("actor-a"),
    )
    .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn evaluate_outcome_owner_mismatch_returns_404_before_pdp() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let execution_id = record.execution_id;
    let report = OutcomeReport {
        execution_id,
        actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
        description: "test".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: JsonMap::new(),
    };
    let result = evaluate_outcome(
        State(state),
        Path(execution_id.to_string()),
        actor_ext("actor-b"),
        axum::Json(report),
    )
    .await;
    assert_eq!(result.unwrap_err().1, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn disabled_mode_missing_actor_allows_execution_detail() {
    let state = test_state(AuthMode::Disabled, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let execution_id = record.execution_id;
    let result = get_execution(State(state), Path(execution_id.to_string()), None).await;
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Missing-vs-denied 404 envelope normalization tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execution_detail_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = record.execution_id;
    let missing_id = ExecutionId::new();

    let denied = get_execution(
        State(state.clone()),
        Path(existing_id.to_string()),
        actor_ext("actor-b"),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing = get_execution(
        State(state),
        Path(missing_id.to_string()),
        actor_ext("actor-a"),
    )
    .await
    .expect_err("expected 404 for missing execution");

    assert_404_envelopes_equal(&denied, &missing);
}

#[tokio::test]
async fn execution_lineage_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = record.execution_id;
    let missing_id = ExecutionId::new();

    let denied = get_execution_lineage(
        State(state.clone()),
        Path(existing_id.to_string()),
        actor_ext("actor-b"),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing = get_execution_lineage(
        State(state),
        Path(missing_id.to_string()),
        actor_ext("actor-a"),
    )
    .await
    .expect_err("expected 404 for missing execution");

    assert_404_envelopes_equal(&denied, &missing);
}

#[tokio::test]
async fn execution_cancel_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = record.execution_id;
    let missing_id = ExecutionId::new();

    let denied = cancel_execution(
        State(state.clone()),
        Path(existing_id.to_string()),
        actor_ext("actor-b"),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing = cancel_execution(
        State(state),
        Path(missing_id.to_string()),
        actor_ext("actor-a"),
    )
    .await
    .expect_err("expected 404 for missing execution");

    assert_404_envelopes_equal(&denied, &missing);
}

#[tokio::test]
async fn capability_revoke_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let lease = insert_capability_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = lease.capability_id;
    let missing_id = CapabilityId::new();

    let denied = revoke_capability(
        State(state.clone()),
        Path(existing_id.to_string()),
        actor_ext("actor-b"),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing = revoke_capability(
        State(state),
        Path(missing_id.to_string()),
        actor_ext("actor-a"),
    )
    .await
    .expect_err("expected 404 for missing capability");

    assert_404_envelopes_equal(&denied, &missing);
}

#[tokio::test]
async fn evaluate_outcome_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = record.execution_id;
    let missing_id = ExecutionId::new();

    let denied_report = OutcomeReport {
        execution_id: existing_id,
        actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
        description: "test".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: JsonMap::new(),
    };
    let denied = evaluate_outcome(
        State(state.clone()),
        Path(existing_id.to_string()),
        actor_ext("actor-b"),
        axum::Json(denied_report),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing_report = OutcomeReport {
        execution_id: missing_id,
        actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
        description: "test".to_string(),
        result_digest: None,
        adapter_success: true,
        adapter_metadata: JsonMap::new(),
    };
    let missing = evaluate_outcome(
        State(state),
        Path(missing_id.to_string()),
        actor_ext("actor-a"),
        axum::Json(missing_report),
    )
    .await
    .expect_err("expected 404 for missing execution");

    assert_404_envelopes_equal(&denied, &missing);
}

#[tokio::test]
async fn prepare_execution_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let record = insert_execution_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = record.execution_id;
    let missing_id = ExecutionId::new();

    let denied = prepare_execution(
        State(state.clone()),
        Path(existing_id.to_string()),
        actor_ext("actor-b"),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing = prepare_execution(
        State(state),
        Path(missing_id.to_string()),
        actor_ext("actor-a"),
    )
    .await
    .expect_err("expected 404 for missing execution");

    assert_404_envelopes_equal(&denied, &missing);
}

#[tokio::test]
async fn authorize_execution_missing_and_denied_return_identical_404() {
    let state = test_state(AuthMode::Scoped, None).await;
    let lease = insert_capability_with_parents(&state.runtime.store, Some("actor-a")).await;
    let existing_id = lease.capability_id;
    let missing_id = CapabilityId::new();

    let denied_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: lease.proposal_id,
        capability_id: existing_id,
        dry_run: true,
    };
    let denied = authorize_execution(
        State(state.clone()),
        actor_ext("actor-b"),
        axum::Json(denied_request),
    )
    .await
    .expect_err("expected 404 for owner mismatch");

    let missing_request = ferrum_proto::AuthorizeExecutionRequest {
        proposal_id: ProposalId::new(),
        capability_id: missing_id,
        dry_run: true,
    };
    let missing = authorize_execution(
        State(state),
        actor_ext("actor-a"),
        axum::Json(missing_request),
    )
    .await
    .expect_err("expected 404 for missing capability");

    assert_404_envelopes_equal(&denied, &missing);
}
