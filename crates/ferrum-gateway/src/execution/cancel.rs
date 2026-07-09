use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, ApiErrorCode, AuditAction, AuditResourceType, CancelExecutionResponse,
    EventId, ExecutionState, ObjectRef, ObjectType, ProvenanceEvent, ProvenanceEventKind,
};

use crate::audit;
use crate::execution::{
    execution_is_cancelable_pre_side_effect, mark_lifecycle_transition_reconciled,
    parse_execution_id, record_lifecycle_transition_outbox,
};
use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::problem::ApiProblem;
use crate::provenance::append_governance_event;
use crate::state::AppState;

/// `POST /v1/executions/{execution_id}/cancel`
///
/// Cancels a pre-side-effect execution by transitioning it to `Canceled`,
/// recording an audit entry, and emitting a `SideEffectRolledBack`
/// provenance event so the lineage reflects the cancel as a rollback-like
/// terminal effect.
pub(crate) async fn cancel_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<CancelExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsCancel, e)
    })?;

    // Look up the execution record
    let execution = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                ),
            )
        })?;

    let previous_state = execution.state.clone();

    // ------------------------------------------------------------------
    // Cancel guard: only pre-side-effect states can be canceled. Once the
    // adapter call has started or awaits verification, callers must use the
    // compensate/rollback path so recovery semantics are explicit.
    // ------------------------------------------------------------------
    if !execution_is_cancelable_pre_side_effect(&previous_state) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCancel,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "cancel not allowed: execution is not in a cancelable pre-side-effect state",
            )
        );
    }

    // Update execution state to Canceled
    let previous_execution = execution.clone();
    let mut updated_execution = execution;
    updated_execution.state = ExecutionState::Canceled;
    updated_execution.finished_at = Some(Utc::now());
    let outbox = record_lifecycle_transition_outbox(
        &state.runtime.store,
        "cancel",
        &previous_execution,
        &updated_execution,
        None,
        None,
        ProvenanceEventKind::SideEffectRolledBack,
    )
    .await
    .map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ExecutionsCancel,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;

    // Audit log: execution canceled
    if let Err(problem) = audit::append_audit_checked(
        &state,
        "gateway",
        AuditAction::ExecutionCancel,
        AuditResourceType::Execution,
        &execution_id.to_string(),
        "success",
        Some(serde_json::json!({
            "previous_state": format!("{:?}", previous_state),
        })),
        Some(GovernanceRoute::ExecutionsCancel),
    )
    .await
    {
        return governance_err!(state, GovernanceRoute::ExecutionsCancel, problem);
    }

    // Emit SideEffectRolledBack provenance event for cancel operation.
    // Cancel triggers a rollback-like effect even if no contract exists.
    let cancel_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::SideEffectRolledBack,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Execution canceled".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: updated_execution.rollback_contract_id,
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
        metadata: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert(
                "previous_state".to_string(),
                serde_json::json!(format!("{:?}", previous_state)),
            );
            m.insert(
                "lineage_parent_optional".to_string(),
                serde_json::json!(true),
            );
            m
        },
        source_runtime_id: None,
    };
    let cancel_event_id = cancel_event.event_id;
    append_governance_event(&state.runtime.store, cancel_event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    mark_lifecycle_transition_reconciled(&state.runtime.store, &outbox, cancel_event_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCancel,
        Ok(Json(CancelExecutionResponse {
            execution_id,
            previous_state,
            current_state: ExecutionState::Canceled,
            canceled_at: Utc::now(),
        }))
    )
}
