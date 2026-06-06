use std::{collections::HashSet, sync::Arc};

use ferrum_proto::{
    ActorRef, ActorType, ExecutionRecord, HashChainRef, JsonMap, LifecycleOutboxRecord,
    LifecycleOutboxStatus, ObjectRef, ObjectType, ProvenanceEdge, ProvenanceEdgeType,
    ProvenanceEvent, ProvenanceEventKind, ProvenanceObligation, ProvenanceQueryRequest,
    lineage_parent_spec,
};

use crate::{
    LifecycleOutboxClaim, LifecycleOutboxLease, ReconciliationFailureDisposition, Result,
    StoreError, StoreFacade,
};

const DEFAULT_RECONCILIATION_LEASE_TTL_SECS: i64 = 120;
const RECONCILIATION_RECORD_TIMEOUT_SECS: u64 = 30;
const RECONCILIATION_LEASE_RENEW_INTERVAL_SECS: u64 = 10;
const MAX_RECONCILIATION_ATTEMPTS: u32 = 3;
const MAX_CLAIMS_PER_LEASE_BATCH: u32 = 3;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LifecycleReconciliationReport {
    pub scanned: usize,
    pub already_reconciled: usize,
    pub repaired_missing_provenance: usize,
    pub needs_operator_review: usize,
    pub failures: usize,
    pub retryable_failures: usize,
    pub claim_conflicts: usize,
    pub timed_out: usize,
}

pub async fn reconcile_lifecycle_outbox(
    store: &Arc<dyn StoreFacade>,
    limit: u32,
) -> Result<LifecycleReconciliationReport> {
    let lease_owner = format!(
        "lifecycle-reconciler:{}:{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    );
    reconcile_lifecycle_outbox_with_lease(
        store,
        limit,
        &lease_owner,
        chrono::Duration::seconds(DEFAULT_RECONCILIATION_LEASE_TTL_SECS),
    )
    .await
}

pub async fn reconcile_lifecycle_outbox_with_lease(
    store: &Arc<dyn StoreFacade>,
    limit: u32,
    lease_owner: &str,
    lease_ttl: chrono::Duration,
) -> Result<LifecycleReconciliationReport> {
    let mut report = LifecycleReconciliationReport::default();
    let mut remaining = limit;
    let mut seen_outbox_ids = HashSet::new();
    while remaining > 0 {
        let batch_limit = remaining.min(MAX_CLAIMS_PER_LEASE_BATCH);
        let claims = store
            .lifecycle_outbox()
            .claim_pending_reconciliation(batch_limit, lease_owner, lease_ttl)
            .await?;
        if claims.is_empty() {
            break;
        }
        let mut saw_duplicate = false;
        let mut fresh_claims = Vec::with_capacity(claims.len());
        for claim in claims {
            if !seen_outbox_ids.insert(claim.lease.outbox_id) {
                saw_duplicate = true;
                continue;
            }
            fresh_claims.push(claim);
        }
        if fresh_claims.is_empty() {
            break;
        }
        report.scanned += fresh_claims.len();
        remaining = remaining.saturating_sub(fresh_claims.len() as u32);

        for claim in fresh_claims {
            let renewal_stop = spawn_lease_renewal(store.clone(), claim.lease.clone(), lease_ttl);
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(RECONCILIATION_RECORD_TIMEOUT_SECS),
                reconcile_record(store, &claim, lease_ttl),
            )
            .await;
            let _ = renewal_stop.send(());

            match result {
                Ok(Ok(ReconcileOutcome::AlreadyReconciled)) => report.already_reconciled += 1,
                Ok(Ok(ReconcileOutcome::RepairedMissingProvenance)) => {
                    report.repaired_missing_provenance += 1
                }
                Ok(Ok(ReconcileOutcome::NeedsOperatorReview)) => report.needs_operator_review += 1,
                Ok(Err(error)) => {
                    record_failure(store, &claim.lease, error.to_string(), &mut report).await;
                }
                Err(_) => {
                    report.timed_out += 1;
                    record_failure(
                        store,
                        &claim.lease,
                        format!(
                            "lifecycle reconciliation timed out after {} seconds",
                            RECONCILIATION_RECORD_TIMEOUT_SECS
                        ),
                        &mut report,
                    )
                    .await;
                }
            }
        }
        if saw_duplicate {
            break;
        }
    }

    Ok(report)
}

fn spawn_lease_renewal(
    store: Arc<dyn StoreFacade>,
    lease: LifecycleOutboxLease,
    lease_ttl: chrono::Duration,
) -> tokio::sync::oneshot::Sender<()> {
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            RECONCILIATION_LEASE_RENEW_INTERVAL_SECS,
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                _ = interval.tick() => {
                    match store
                        .lifecycle_outbox()
                        .renew_reconciliation_lease(&lease, lease_ttl)
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) => {
                            tracing::warn!(
                                outbox_id = %lease.outbox_id,
                                generation = lease.generation,
                                "lifecycle reconciliation lease renewal lost fencing ownership"
                            );
                            break;
                        }
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                outbox_id = %lease.outbox_id,
                                generation = lease.generation,
                                "lifecycle reconciliation lease renewal failed"
                            );
                        }
                    }
                }
            }
        }
    });
    stop_tx
}

async fn record_failure(
    store: &Arc<dyn StoreFacade>,
    lease: &LifecycleOutboxLease,
    error: String,
    report: &mut LifecycleReconciliationReport,
) {
    report.failures += 1;
    match store
        .lifecycle_outbox()
        .record_reconciliation_failure(lease, error, MAX_RECONCILIATION_ATTEMPTS)
        .await
    {
        Ok(ReconciliationFailureDisposition::Retryable) => report.retryable_failures += 1,
        Ok(ReconciliationFailureDisposition::NeedsOperatorReview) => {
            report.needs_operator_review += 1;
        }
        Ok(ReconciliationFailureDisposition::LeaseLost) => report.claim_conflicts += 1,
        Err(error) => {
            report.claim_conflicts += 1;
            tracing::warn!(
                %error,
                outbox_id = %lease.outbox_id,
                generation = lease.generation,
                "failed to persist lifecycle reconciliation failure"
            );
        }
    }
}

enum ReconcileOutcome {
    AlreadyReconciled,
    RepairedMissingProvenance,
    NeedsOperatorReview,
}

async fn reconcile_record(
    store: &Arc<dyn StoreFacade>,
    claim: &LifecycleOutboxClaim,
    lease_ttl: chrono::Duration,
) -> Result<ReconcileOutcome> {
    let record = &claim.record;
    let lease = &claim.lease;
    let outbox = store.lifecycle_outbox();
    if matches!(record.status, LifecycleOutboxStatus::Reconciled) {
        return Ok(ReconcileOutcome::AlreadyReconciled);
    }

    let Some(execution) = store.executions().get(record.execution_id).await? else {
        outbox
            .mark_needs_operator_review_claimed(
                lease,
                "execution missing during lifecycle reconciliation".to_string(),
            )
            .await
            .and_then(require_fence)?;
        return Ok(ReconcileOutcome::NeedsOperatorReview);
    };

    if execution.state != record.new_execution_state {
        outbox
            .mark_needs_operator_review_claimed(
                lease,
                format!(
                    "execution state drift: stored={:?}, outbox_expected={:?}",
                    execution.state, record.new_execution_state
                ),
            )
            .await
            .and_then(require_fence)?;
        return Ok(ReconcileOutcome::NeedsOperatorReview);
    }

    let rollback_contract = if let Some(contract_id) = record.rollback_contract_id {
        let Some(contract) = store.rollback_contracts().get(contract_id).await? else {
            outbox
                .mark_needs_operator_review_claimed(
                    lease,
                    "rollback contract missing during lifecycle reconciliation".to_string(),
                )
                .await
                .and_then(require_fence)?;
            return Ok(ReconcileOutcome::NeedsOperatorReview);
        };
        if Some(contract.state.clone()) != record.new_rollback_state {
            outbox
                .mark_needs_operator_review_claimed(
                    lease,
                    format!(
                        "rollback state drift: stored={:?}, outbox_expected={:?}",
                        contract.state, record.new_rollback_state
                    ),
                )
                .await
                .and_then(require_fence)?;
            return Ok(ReconcileOutcome::NeedsOperatorReview);
        }
        Some(contract)
    } else {
        None
    };

    let obligations = obligations_for_record(record);
    let mut repaired = false;
    let mut discovered = false;
    for obligation in obligations {
        if obligation.is_satisfied()
            && let Some(event_id) = obligation.event_id
            && store.provenance().get_event(event_id).await?.is_some()
        {
            if !ensure_required_parent_edge(
                store,
                claim,
                &execution,
                &obligation,
                event_id,
                lease_ttl,
            )
            .await?
            {
                mark_ambiguous_parent(&outbox, lease).await?;
                return Ok(ReconcileOutcome::NeedsOperatorReview);
            }
            continue;
        }

        if let Some(event) = discover_matching_event(store, record, &obligation).await? {
            if !ensure_required_parent_edge(
                store,
                claim,
                &execution,
                &obligation,
                event.event_id,
                lease_ttl,
            )
            .await?
            {
                mark_ambiguous_parent(&outbox, lease).await?;
                return Ok(ReconcileOutcome::NeedsOperatorReview);
            }
            outbox
                .mark_provenance_obligation_written_claimed(
                    lease,
                    obligation.event_kind.clone(),
                    event.event_id,
                )
                .await
                .and_then(require_fence)?;
            discovered = true;
            continue;
        }

        let event = repaired_event(
            record,
            &execution,
            rollback_contract.as_ref(),
            obligation.event_kind.clone(),
        );
        let event_id = event.event_id;
        let Some(parent_edge) =
            required_parent_edge(store, record, &execution, &obligation, event_id).await?
        else {
            mark_ambiguous_parent(&outbox, lease).await?;
            return Ok(ReconcileOutcome::NeedsOperatorReview);
        };
        renew_or_fence(store, lease, lease_ttl).await?;
        store
            .provenance()
            .append_event_with_edges(&event, &[parent_edge])
            .await?;
        outbox
            .mark_provenance_obligation_written_claimed(
                lease,
                obligation.event_kind.clone(),
                event_id,
            )
            .await
            .and_then(require_fence)?;
        repaired = true;
    }

    outbox
        .mark_reconciled_claimed(
            lease,
            reconciliation_result(if repaired {
                "missing_provenance_repaired"
            } else if discovered {
                "event_discovered"
            } else {
                "event_present"
            }),
        )
        .await
        .and_then(require_fence)?;

    if repaired {
        Ok(ReconcileOutcome::RepairedMissingProvenance)
    } else {
        Ok(ReconcileOutcome::AlreadyReconciled)
    }
}

async fn ensure_required_parent_edge(
    store: &Arc<dyn StoreFacade>,
    claim: &LifecycleOutboxClaim,
    execution: &ExecutionRecord,
    obligation: &ProvenanceObligation,
    event_id: ferrum_proto::EventId,
    lease_ttl: chrono::Duration,
) -> Result<bool> {
    let existing_edges = store.provenance().get_edges_to(event_id).await?;
    let Some(expected_edge_type) = obligation.edge_type.as_ref() else {
        return Ok(true);
    };
    if existing_edges
        .iter()
        .any(|edge| &edge.edge_type == expected_edge_type)
    {
        return Ok(true);
    }

    let Some(edge) =
        required_parent_edge(store, &claim.record, execution, obligation, event_id).await?
    else {
        return Ok(false);
    };
    renew_or_fence(store, &claim.lease, lease_ttl).await?;
    store.provenance().append_edges(event_id, &[edge]).await?;
    Ok(true)
}

fn obligations_for_record(record: &LifecycleOutboxRecord) -> Vec<ProvenanceObligation> {
    if !record.provenance_obligations.is_empty() {
        return record.provenance_obligations.clone();
    }
    let (parent_kind, edge_type) = lineage_parent_spec(&record.intended_provenance_kind)
        .map(|(parent, edge)| (Some(parent), Some(edge)))
        .unwrap_or((None, None));
    vec![ProvenanceObligation {
        event_kind: record.intended_provenance_kind.clone(),
        parent_kind,
        edge_type,
        event_id: record.provenance_event_id,
    }]
}

async fn discover_matching_event(
    store: &Arc<dyn StoreFacade>,
    record: &LifecycleOutboxRecord,
    obligation: &ProvenanceObligation,
) -> Result<Option<ProvenanceEvent>> {
    let events = store
        .provenance()
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: Some(record.execution_id),
            capability_id: None,
            event_kind: Some(obligation.event_kind.clone()),
            since: None,
            until: None,
            edge_types: Vec::new(),
        })
        .await?;
    let mut matches = events
        .into_iter()
        .filter(|event| {
            (record.rollback_contract_id.is_none()
                || event.rollback_contract_id == record.rollback_contract_id)
                && event
                    .metadata
                    .get("lifecycle_outbox_id")
                    .and_then(|value| value.as_str())
                    == Some(record.outbox_id.to_string().as_str())
                && event
                    .metadata
                    .get("idempotency_key")
                    .and_then(|value| value.as_str())
                    == Some(record.idempotency_key.as_str())
        })
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        Ok(matches.pop())
    } else {
        Ok(None)
    }
}

async fn mark_ambiguous_parent(
    outbox: &Arc<dyn crate::LifecycleOutboxRepo>,
    lease: &LifecycleOutboxLease,
) -> Result<()> {
    outbox
        .mark_needs_operator_review_claimed(
            lease,
            "required parent edge is ambiguous or missing".to_string(),
        )
        .await
        .and_then(require_fence)
}

async fn renew_or_fence(
    store: &Arc<dyn StoreFacade>,
    lease: &LifecycleOutboxLease,
    lease_ttl: chrono::Duration,
) -> Result<()> {
    require_fence(
        store
            .lifecycle_outbox()
            .renew_reconciliation_lease(lease, lease_ttl)
            .await?,
    )
}

fn require_fence(updated: bool) -> Result<()> {
    if updated {
        Ok(())
    } else {
        Err(StoreError::Other(
            "lifecycle reconciliation lease fencing token is stale".to_string(),
        ))
    }
}

async fn required_parent_edge(
    store: &Arc<dyn StoreFacade>,
    record: &LifecycleOutboxRecord,
    execution: &ExecutionRecord,
    obligation: &ProvenanceObligation,
    event_id: ferrum_proto::EventId,
) -> Result<Option<ProvenanceEdge>> {
    let Some(parent_kind) = obligation.parent_kind.clone() else {
        return Ok(None);
    };
    let edge_type = obligation
        .edge_type
        .clone()
        .unwrap_or(ProvenanceEdgeType::Caused);

    let mut candidates = store
        .provenance()
        .query(&ProvenanceQueryRequest {
            intent_id: None,
            execution_id: Some(record.execution_id),
            capability_id: None,
            event_kind: Some(parent_kind.clone()),
            since: None,
            until: None,
            edge_types: Vec::new(),
        })
        .await?;

    if candidates.is_empty() && matches!(parent_kind, ProvenanceEventKind::CapabilityMinted) {
        candidates = store
            .provenance()
            .query(&ProvenanceQueryRequest {
                intent_id: None,
                execution_id: None,
                capability_id: Some(execution.capability_id),
                event_kind: Some(parent_kind),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await?;
    }

    if candidates.len() != 1 {
        return Ok(None);
    }
    Ok(Some(ProvenanceEdge {
        edge_type,
        from_event_id: candidates[0].event_id,
        to_event_id: Some(event_id),
        summary: Some("reconciled lifecycle causality edge".to_string()),
    }))
}

fn repaired_event(
    record: &LifecycleOutboxRecord,
    execution: &ferrum_proto::ExecutionRecord,
    rollback_contract: Option<&ferrum_proto::RollbackContract>,
    event_kind: ProvenanceEventKind,
) -> ProvenanceEvent {
    let mut metadata = JsonMap::new();
    metadata.insert("reconciled".to_string(), serde_json::json!(true));
    metadata.insert(
        "lifecycle_outbox_id".to_string(),
        serde_json::json!(record.outbox_id.to_string()),
    );
    metadata.insert(
        "idempotency_key".to_string(),
        serde_json::json!(record.idempotency_key),
    );

    ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind: event_kind,
        occurred_at: chrono::Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-store-reconciler".to_string(),
            display_name: Some("FerrumGate Store Reconciler".to_string()),
        },
        object: ObjectRef {
            object_type: if rollback_contract.is_some() {
                ObjectType::RollbackContract
            } else {
                ObjectType::SideEffect
            },
            object_id: rollback_contract
                .map(|contract| contract.contract_id.to_string())
                .unwrap_or_else(|| execution.execution_id.to_string()),
            summary: Some("Lifecycle provenance repaired from outbox".to_string()),
        },
        intent_id: Some(execution.intent_id),
        proposal_id: Some(execution.proposal_id),
        execution_id: Some(execution.execution_id),
        capability_id: Some(execution.capability_id),
        rollback_contract_id: rollback_contract.map(|contract| contract.contract_id),
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
    }
}

fn reconciliation_result(status: &str) -> JsonMap {
    let mut result = JsonMap::new();
    result.insert("status".to_string(), serde_json::json!(status));
    result
}
