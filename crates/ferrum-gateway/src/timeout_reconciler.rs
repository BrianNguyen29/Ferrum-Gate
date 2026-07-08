use std::sync::Arc;
use std::sync::atomic::Ordering;

use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, EventId, HashChainRef, ObjectRef, ObjectType, ProvenanceEvent,
    ProvenanceEventKind,
};
use tokio::time::{MissedTickBehavior, interval};

use crate::state::AppState;

const APPROVAL_TIMEOUT_BATCH_SIZE: u32 = 100;
const QUARANTINE_TIMEOUT_BATCH_SIZE: u32 = 100;

/// Emit a provenance event recording that an approval timed out.
pub(crate) async fn emit_approval_timed_out_provenance(
    state: &AppState,
    approval: &ferrum_proto::ApprovalRequest,
) {
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "approval_id".to_string(),
        serde_json::json!(approval.approval_id.to_string()),
    );
    metadata.insert(
        "previous_state".to_string(),
        serde_json::json!(format!("{:?}", approval.state)),
    );

    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::ApprovalTimedOut,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Approval,
            object_id: approval.approval_id.to_string(),
            summary: Some("Approval timed out and was transitioned to Expired".to_string()),
        },
        intent_id: Some(approval.intent_id),
        proposal_id: Some(approval.proposal_id),
        execution_id: approval.execution_id,
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
        metadata,
        source_runtime_id: None,
    };

    if let Err(e) = state.runtime.store.provenance().append_event(&event).await {
        tracing::warn!(
            error = %e,
            approval_id = %approval.approval_id,
            "failed to append ApprovalTimedOut provenance event"
        );
    }
}

/// Background task that periodically reconciles stale pending approvals.
pub(crate) async fn approval_timeout_reconciler(
    state: Arc<AppState>,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let interval_secs = state.server_config.approval_reconciliation_interval_secs;
    let timeout_seconds = state.server_config.approval_timeout_seconds;
    let mut interval = interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Utc::now();
                match state
                    .runtime
                    .store
                    .approvals()
                    .expire_stale_pending(now, timeout_seconds, APPROVAL_TIMEOUT_BATCH_SIZE)
                    .await
                {
                    Ok(expired) => {
                        for approval in &expired {
                            state
                                .metrics
                                .approval_timeouts_total
                                .fetch_add(1, Ordering::Relaxed);
                            emit_approval_timed_out_provenance(&state, approval).await;
                        }
                        if !expired.is_empty() {
                            tracing::info!(
                                count = expired.len(),
                                "approval timeout reconciliation expired stale pending approvals"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "approval timeout reconciliation failed");
                    }
                }
            }
            _ = shutdown.notified() => {
                tracing::info!("approval timeout reconciler shutting down");
                break;
            }
        }
    }
}

/// Emit a provenance event recording that a quarantine hold timed out.
pub(crate) async fn emit_quarantine_timed_out_provenance(
    state: &AppState,
    hold: &ferrum_proto::QuarantineHold,
) {
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "hold_id".to_string(),
        serde_json::json!(hold.hold_id.to_string()),
    );
    metadata.insert(
        "previous_state".to_string(),
        serde_json::json!(format!("{:?}", hold.state)),
    );

    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ProvenanceEventKind::QuarantineTimedOut,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::QuarantineHold,
            object_id: hold.hold_id.to_string(),
            summary: Some("Quarantine hold timed out and was transitioned to Expired".to_string()),
        },
        intent_id: Some(hold.intent_id),
        proposal_id: Some(hold.proposal_id),
        execution_id: None,
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
        metadata,
        source_runtime_id: None,
    };

    if let Err(e) = crate::provenance::append_governance_event(&state.runtime.store, event).await {
        tracing::warn!(
            error = %e,
            hold_id = %hold.hold_id,
            "failed to append QuarantineTimedOut provenance event"
        );
    }
}

/// Background task that periodically reconciles stale pending quarantine holds.
pub(crate) async fn quarantine_timeout_reconciler(
    state: Arc<AppState>,
    shutdown: Arc<tokio::sync::Notify>,
) {
    let interval_secs = state.server_config.quarantine_reconciliation_interval_secs;
    let timeout_seconds = state.server_config.quarantine_timeout_seconds;
    let mut interval = interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = Utc::now();
                match state
                    .runtime
                    .store
                    .quarantine_holds()
                    .expire_stale_pending(now, timeout_seconds, QUARANTINE_TIMEOUT_BATCH_SIZE)
                    .await
                {
                    Ok(expired) => {
                        for hold in &expired {
                            state
                                .metrics
                                .quarantine_timeouts_total
                                .fetch_add(1, Ordering::Relaxed);
                            emit_quarantine_timed_out_provenance(&state, hold).await;
                        }
                        if !expired.is_empty() {
                            tracing::info!(
                                count = expired.len(),
                                "quarantine timeout reconciliation expired stale pending holds"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "quarantine timeout reconciliation failed");
                    }
                }
            }
            _ = shutdown.notified() => {
                tracing::info!("quarantine timeout reconciler shutting down");
                break;
            }
        }
    }
}
