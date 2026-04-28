//! Repo-layer integration tests for invalid state transitions.
//!
//! These tests exercise the actual repo methods against an in-memory SQLite store,
//! asserting that `StoreError::InvalidState` is returned when invalid transitions
//! are attempted on capability, approval, and execution entities.

use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApprovalState, CapabilityId, CapabilityLease, CapabilityStatus, Decision,
    ExecutionId, ExecutionRecord, ExecutionState, IntentId, JsonMap, ProposalId, Timestamp,
};
use ferrum_store::{ApprovalRepo, CapabilityRepo, ExecutionRepo, StoreError, sqlite::SqliteStore};

/// Creates a Timestamp relative to now.
fn ts_offset(seconds: i64) -> Timestamp {
    Utc::now() + chrono::Duration::seconds(seconds)
}

/// Helper: creates and inserts a minimal Intent, returning its ID.
/// The intent table has no additional FK dependencies.
async fn insert_intent(store: &SqliteStore) -> IntentId {
    let intent_id = IntentId::new();
    let raw_json = serde_json::json!({
        "intent_id": intent_id.to_string(),
        "principal_id": "test-principal",
        "normalized_goal": "test goal",
        "status": "Active",
        "risk_tier": "Low",
        "approval_mode": "None",
        "default_rollback_class": "R0",
        "created_at": ts_offset(0).to_rfc3339(),
        "expires_at": ts_offset(3600).to_rfc3339(),
    });
    sqlx::query("INSERT INTO intents (intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode, default_rollback_class, created_at, expires_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")
        .bind(intent_id.to_string())
        .bind("test-principal")
        .bind("test goal")
        .bind("Active")
        .bind("Low")
        .bind("None")
        .bind("R0NativeReversible")
        .bind(ts_offset(0).to_rfc3339())
        .bind(ts_offset(3600).to_rfc3339())
        .bind(raw_json.to_string())
        .execute(store.pool())
        .await
        .unwrap();
    intent_id
}

/// Helper: creates and inserts a minimal Proposal for the given intent, returning its ID.
/// Capabilities, executions, and approvals all FK to proposals.
async fn insert_proposal(store: &SqliteStore, intent_id: IntentId) -> ProposalId {
    let proposal_id = ProposalId::new();
    let raw_json = serde_json::json!({
        "proposal_id": proposal_id.to_string(),
        "intent_id": intent_id.to_string(),
        "step_index": 0,
        "server_name": "test-server",
        "tool_name": "test-tool",
        "estimated_risk": "Low",
        "requested_rollback_class": "R0",
        "created_at": ts_offset(0).to_rfc3339(),
    });
    sqlx::query("INSERT INTO proposals (proposal_id, intent_id, step_index, server_name, tool_name, estimated_risk, requested_rollback_class, created_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")
        .bind(proposal_id.to_string())
        .bind(intent_id.to_string())
        .bind(0)
        .bind("test-server")
        .bind("test-tool")
        .bind("Low")
        .bind("R0NativeReversible")
        .bind(ts_offset(0).to_rfc3339())
        .bind(raw_json.to_string())
        .execute(store.pool())
        .await
        .unwrap();
    proposal_id
}

/// Helper to build a minimal CapabilityLease in the given status.
fn make_capability(
    capability_id: CapabilityId,
    intent_id: IntentId,
    proposal_id: ProposalId,
    status: CapabilityStatus,
) -> CapabilityLease {
    CapabilityLease {
        capability_id,
        intent_id,
        proposal_id,
        tool_binding: ferrum_proto::ToolBinding {
            server_name: "test-server".into(),
            tool_name: "test-tool".into(),
            tool_version: None,
        },
        resource_bindings: vec![],
        argument_constraints: vec![],
        taint_budget: ferrum_proto::TaintBudget {
            max_taint_score: 0,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        issued_by: "test".into(),
        policy_bundle_id: ferrum_proto::PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status,
        issued_at: ts_offset(0),
        expires_at: ts_offset(3600),
        revoked_at: None,
        metadata: JsonMap::new(),
    }
}

/// Helper to build a minimal ExecutionRecord in the given state.
fn make_execution(
    execution_id: ExecutionId,
    intent_id: IntentId,
    proposal_id: ProposalId,
    capability_id: CapabilityId,
    state: ExecutionState,
) -> ExecutionRecord {
    ExecutionRecord {
        execution_id,
        proposal_id,
        intent_id,
        capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state,
        started_at: ts_offset(0),
        finished_at: None,
        result_digest: None,
        metadata: JsonMap::new(),
    }
}

/// Helper to build a minimal ApprovalRequest in the given state.
fn make_approval(
    approval_id: ferrum_proto::ApprovalId,
    intent_id: IntentId,
    proposal_id: ProposalId,
    state: ApprovalState,
) -> ferrum_proto::ApprovalRequest {
    ferrum_proto::ApprovalRequest {
        approval_id,
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ActorRef {
            actor_type: ActorType::User,
            actor_id: "test-actor".into(),
            display_name: Some("test".into()),
        },
        reason: "test".into(),
        action_digest: "test-digest".into(),
        expires_at: ts_offset(3600),
        state,
        created_at: ts_offset(0),
    }
}

// =============================================================================
// Capability transition tests
// =============================================================================

#[tokio::test]
async fn capability_update_status_used_to_active_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.capabilities();
    let cap_id = CapabilityId::new();

    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Used);
    repo.insert(&cap).await.unwrap();

    // Attempt invalid transition: Used -> Active
    let result = repo.update_status(cap_id, CapabilityStatus::Active).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Used"), "error should mention 'Used': {}", msg);
    assert!(
        msg.contains("Active"),
        "error should mention 'Active': {}",
        msg
    );
}

#[tokio::test]
async fn capability_update_status_expired_to_active_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.capabilities();
    let cap_id = CapabilityId::new();

    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Expired);
    repo.insert(&cap).await.unwrap();

    let result = repo.update_status(cap_id, CapabilityStatus::Active).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn capability_update_status_revoked_to_active_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.capabilities();
    let cap_id = CapabilityId::new();

    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Revoked);
    repo.insert(&cap).await.unwrap();

    let result = repo.update_status(cap_id, CapabilityStatus::Active).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn capability_update_status_quarantined_to_active_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.capabilities();
    let cap_id = CapabilityId::new();

    let cap = make_capability(
        cap_id,
        intent_id,
        proposal_id,
        CapabilityStatus::Quarantined,
    );
    repo.insert(&cap).await.unwrap();

    let result = repo.update_status(cap_id, CapabilityStatus::Active).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn capability_update_status_used_to_expired_returns_invalid_state() {
    // Cannot transition between terminal states
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.capabilities();
    let cap_id = CapabilityId::new();

    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Used);
    repo.insert(&cap).await.unwrap();

    let result = repo.update_status(cap_id, CapabilityStatus::Expired).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn capability_update_status_active_to_used_is_valid() {
    // Valid transition: Active -> Used should succeed
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.capabilities();
    let cap_id = CapabilityId::new();

    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    repo.insert(&cap).await.unwrap();

    let result = repo.update_status(cap_id, CapabilityStatus::Used).await;
    assert!(
        result.is_ok(),
        "Active->Used should succeed, got: {:?}",
        result
    );
}

// =============================================================================
// Approval transition tests
// =============================================================================

#[tokio::test]
async fn approval_resolve_granted_to_pending_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.approvals();
    let approval_id = ferrum_proto::ApprovalId::new();

    let approval = make_approval(approval_id, intent_id, proposal_id, ApprovalState::Granted);
    repo.insert(&approval).await.unwrap();

    // Attempt invalid transition: Granted -> Pending
    let result = repo.resolve(approval_id, ApprovalState::Pending).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Granted"),
        "error should mention 'Granted': {}",
        msg
    );
    assert!(
        msg.contains("Pending"),
        "error should mention 'Pending': {}",
        msg
    );
}

#[tokio::test]
async fn approval_resolve_denied_to_pending_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.approvals();
    let approval_id = ferrum_proto::ApprovalId::new();

    let approval = make_approval(approval_id, intent_id, proposal_id, ApprovalState::Denied);
    repo.insert(&approval).await.unwrap();

    let result = repo.resolve(approval_id, ApprovalState::Pending).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn approval_resolve_expired_to_pending_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.approvals();
    let approval_id = ferrum_proto::ApprovalId::new();

    let approval = make_approval(approval_id, intent_id, proposal_id, ApprovalState::Expired);
    repo.insert(&approval).await.unwrap();

    let result = repo.resolve(approval_id, ApprovalState::Pending).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn approval_resolve_granted_to_denied_returns_invalid_state() {
    // Cannot transition between terminal states
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.approvals();
    let approval_id = ferrum_proto::ApprovalId::new();

    let approval = make_approval(approval_id, intent_id, proposal_id, ApprovalState::Granted);
    repo.insert(&approval).await.unwrap();

    let result = repo.resolve(approval_id, ApprovalState::Denied).await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn approval_resolve_pending_to_granted_is_valid() {
    // Valid transition: Pending -> Granted should succeed
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;

    let repo = store.approvals();
    let approval_id = ferrum_proto::ApprovalId::new();

    let approval = make_approval(approval_id, intent_id, proposal_id, ApprovalState::Pending);
    repo.insert(&approval).await.unwrap();

    let result = repo.resolve(approval_id, ApprovalState::Granted).await;
    assert!(
        result.is_ok(),
        "Pending->Granted should succeed, got: {:?}",
        result
    );
}

// =============================================================================
// Execution transition tests
// =============================================================================

#[tokio::test]
async fn execution_update_state_committed_to_running_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    // Execution also FK to capabilities, so create a capability too
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Committed,
    );
    repo.insert(&exec).await.unwrap();

    // Attempt invalid transition: Committed -> Running
    let result = repo
        .update_state(execution_id, ExecutionState::Running)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Committed"),
        "error should mention 'Committed': {}",
        msg
    );
    assert!(
        msg.contains("Running"),
        "error should mention 'Running': {}",
        msg
    );
}

#[tokio::test]
async fn execution_update_state_compensated_to_running_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Compensated,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Running)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn execution_update_state_rolled_back_to_running_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::RolledBack,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Running)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn execution_update_state_denied_to_running_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Denied,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Running)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn execution_update_state_quarantined_to_running_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Quarantined,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Running)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn execution_update_state_failed_to_running_returns_invalid_state() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Failed,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Running)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn execution_update_state_committed_to_failed_returns_invalid_state() {
    // Cannot transition between terminal states
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Committed,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Failed)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidState(_))));
}

#[tokio::test]
async fn execution_update_state_proposed_to_authorized_is_valid() {
    // Valid non-terminal transition: Proposed -> Authorized
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();

    let intent_id = insert_intent(&store).await;
    let proposal_id = insert_proposal(&store, intent_id).await;
    let cap_id = CapabilityId::new();
    let cap = make_capability(cap_id, intent_id, proposal_id, CapabilityStatus::Active);
    store.capabilities().insert(&cap).await.unwrap();

    let repo = store.executions();
    let execution_id = ExecutionId::new();

    let exec = make_execution(
        execution_id,
        intent_id,
        proposal_id,
        cap_id,
        ExecutionState::Proposed,
    );
    repo.insert(&exec).await.unwrap();

    let result = repo
        .update_state(execution_id, ExecutionState::Authorized)
        .await;
    assert!(
        result.is_ok(),
        "Proposed->Authorized should succeed, got: {:?}",
        result
    );
}

// =============================================================================
// Edge cases: non-existent entities
// =============================================================================

#[tokio::test]
async fn capability_update_status_nonexistent_returns_ok() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let repo = store.capabilities();

    // No intent/proposal/capability needed; repo.update_status returns Ok if not found
    let result = repo
        .update_status(CapabilityId::new(), CapabilityStatus::Active)
        .await;
    assert!(
        result.is_ok(),
        "updating nonexistent capability should be a no-op"
    );
}

#[tokio::test]
async fn approval_resolve_nonexistent_returns_ok() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let repo = store.approvals();

    // No intent/proposal/approval needed; repo.resolve returns Ok if not found
    let result = repo
        .resolve(ferrum_proto::ApprovalId::new(), ApprovalState::Granted)
        .await;
    assert!(
        result.is_ok(),
        "resolving nonexistent approval should be a no-op"
    );
}

#[tokio::test]
async fn execution_update_state_nonexistent_returns_ok() {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.apply_embedded_migrations().await.unwrap();
    let repo = store.executions();

    // No intent/proposal/capability/execution needed; repo.update_state returns Ok if not found
    let result = repo
        .update_state(ExecutionId::new(), ExecutionState::Running)
        .await;
    assert!(
        result.is_ok(),
        "updating nonexistent execution should be a no-op"
    );
}
