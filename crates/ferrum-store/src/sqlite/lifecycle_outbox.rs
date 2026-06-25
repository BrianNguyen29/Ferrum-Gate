use async_trait::async_trait;
use ferrum_proto::{
    CapabilityLease, CapabilityStatus, EventId, ExecutionRecord, JsonMap, LifecycleOutboxId,
    LifecycleOutboxRecord, LifecycleOutboxStatus, ProvenanceEventKind, RollbackContract,
};
use sqlx::{Row, SqlitePool};
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{
    LifecycleOutboxClaim, LifecycleOutboxLease, LifecycleOutboxLeaseStats, LifecycleOutboxRepo,
    ReconciliationFailureDisposition, Result,
};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, from_json, to_json};

#[derive(Clone)]
pub struct SqliteLifecycleOutboxRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteLifecycleOutboxRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            write_queue: None,
        }
    }

    pub fn with_write_queue(mut self, queue: WriteQueue) -> Self {
        self.write_queue = Some(queue);
        self
    }
}

#[async_trait]
impl LifecycleOutboxRepo for SqliteLifecycleOutboxRepo {
    async fn enqueue_lifecycle_transition(&self, record: &LifecycleOutboxRecord) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::EnqueueLifecycleOutbox {
                data: Box::new(record.clone()),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        insert_outbox_record(&self.pool, record).await
    }

    async fn record_lifecycle_transition(
        &self,
        execution: &ExecutionRecord,
        rollback_contract: Option<&RollbackContract>,
        outbox: &LifecycleOutboxRecord,
    ) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::RecordLifecycleTransition {
                execution: Box::new(execution.clone()),
                rollback_contract: Box::new(rollback_contract.cloned()),
                outbox: Box::new(outbox.clone()),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }

        let mut tx = self.pool.begin().await?;
        let expected_execution_state =
            outbox.previous_execution_state.as_ref().ok_or_else(|| {
                crate::StoreError::InvalidState(format!(
                    "lifecycle outbox {} missing previous execution state",
                    outbox.outbox_id
                ))
            })?;
        if !crate::transitions::is_valid_execution_transition(
            expected_execution_state,
            &execution.state,
        ) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid execution transition from {:?} to {:?}",
                expected_execution_state, execution.state
            )));
        }
        let execution_raw = to_json(execution)?;
        let execution_update = sqlx::query(
            "UPDATE executions
             SET rollback_contract_id = ?2,
                 decision = ?3,
                 state = ?4,
                 finished_at = ?5,
                 result_digest = ?6,
                 raw_json = ?7
             WHERE execution_id = ?1
               AND state = ?8",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(execution.finished_at)
        .bind(&execution.result_digest)
        .bind(execution_raw)
        .bind(enum_text(expected_execution_state)?)
        .execute(&mut *tx)
        .await?;
        if execution_update.rows_affected() != 1 {
            return Err(crate::StoreError::InvalidState(format!(
                "execution {} state changed before lifecycle transition",
                execution.execution_id
            )));
        }

        if let Some(contract) = rollback_contract {
            let contract_raw = to_json(contract)?;
            if let Some(expected_rollback_state) = outbox.previous_rollback_state.as_ref() {
                let contract_update = sqlx::query(
                    "UPDATE rollback_contracts
                     SET state = ?2,
                         auto_commit = ?3,
                         expires_at = ?4,
                         raw_json = ?5
                     WHERE contract_id = ?1
                       AND state = ?6",
                )
                .bind(contract.contract_id.to_string())
                .bind(enum_text(&contract.state)?)
                .bind(contract.auto_commit)
                .bind(contract.expires_at)
                .bind(contract_raw)
                .bind(enum_text(expected_rollback_state)?)
                .execute(&mut *tx)
                .await?;
                if contract_update.rows_affected() != 1 {
                    return Err(crate::StoreError::InvalidState(format!(
                        "rollback contract {} state changed before lifecycle transition",
                        contract.contract_id
                    )));
                }
            } else {
                sqlx::query(
                    "INSERT INTO rollback_contracts (
                    contract_id, intent_id, proposal_id, execution_id, adapter_key,
                    action_type, rollback_class, state, auto_commit, created_at, expires_at,
                    raw_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                )
                .bind(contract.contract_id.to_string())
                .bind(contract.intent_id.to_string())
                .bind(contract.proposal_id.to_string())
                .bind(contract.execution_id.to_string())
                .bind(&contract.adapter_key)
                .bind(enum_text(&contract.action_type)?)
                .bind(enum_text(&contract.rollback_class)?)
                .bind(enum_text(&contract.state)?)
                .bind(contract.auto_commit)
                .bind(contract.created_at)
                .bind(contract.expires_at)
                .bind(contract_raw)
                .execute(&mut *tx)
                .await?;
            }
        }

        insert_outbox_record_tx(&mut tx, outbox).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_authorization(
        &self,
        capability: &CapabilityLease,
        execution: &ExecutionRecord,
        outbox: &LifecycleOutboxRecord,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let active = enum_text(&CapabilityStatus::Active)?;
        let used = enum_text(&CapabilityStatus::Used)?;
        let updated = sqlx::query(
            "UPDATE capabilities
             SET status = ?2,
                 raw_json = json_set(raw_json, '$.status', ?2)
             WHERE capability_id = ?1 AND status = ?3",
        )
        .bind(capability.capability_id.to_string())
        .bind(&used)
        .bind(active)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(false);
        }

        let execution_raw = to_json(execution)?;
        sqlx::query(
            "INSERT INTO executions (
                execution_id, intent_id, proposal_id, capability_id, rollback_contract_id,
                decision, state, started_at, finished_at, result_digest, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.intent_id.to_string())
        .bind(execution.proposal_id.to_string())
        .bind(execution.capability_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(execution.started_at)
        .bind(execution.finished_at)
        .bind(&execution.result_digest)
        .bind(execution_raw)
        .execute(&mut *tx)
        .await?;

        insert_outbox_record_tx(&mut tx, outbox).await?;
        tx.commit().await?;
        Ok(true)
    }

    async fn mark_provenance_written(
        &self,
        outbox_id: LifecycleOutboxId,
        event_id: EventId,
    ) -> Result<()> {
        let Some(record) = self.get(outbox_id).await? else {
            return Err(crate::StoreError::not_found(
                "lifecycle_outbox",
                outbox_id.to_string(),
            ));
        };
        let updated = self
            .mark_provenance_obligation_written(
                outbox_id,
                record.intended_provenance_kind,
                event_id,
            )
            .await?;
        require_updated(updated)
    }

    async fn mark_provenance_obligation_written(
        &self,
        outbox_id: LifecycleOutboxId,
        event_kind: ProvenanceEventKind,
        event_id: EventId,
    ) -> Result<bool> {
        let Some(mut record) = self.get(outbox_id).await? else {
            return Err(crate::StoreError::not_found(
                "lifecycle_outbox",
                outbox_id.to_string(),
            ));
        };
        mark_obligation_written(&mut record, event_kind, event_id);
        update_outbox_record(&self.pool, &record, false, false).await
    }

    async fn mark_reconciled(&self, outbox_id: LifecycleOutboxId, result: JsonMap) -> Result<()> {
        let Some(mut record) = self.get(outbox_id).await? else {
            return Ok(());
        };
        record.status = LifecycleOutboxStatus::Reconciled;
        record.metadata.insert(
            "reconciliation_result".to_string(),
            serde_json::json!(result),
        );
        record.updated_at = chrono::Utc::now();
        require_all_obligations_satisfied(&record)?;
        require_updated(update_outbox_record(&self.pool, &record, true, false).await?)
    }

    async fn mark_needs_operator_review(
        &self,
        outbox_id: LifecycleOutboxId,
        reason: String,
    ) -> Result<()> {
        let Some(mut record) = self.get(outbox_id).await? else {
            return Ok(());
        };
        record.status = LifecycleOutboxStatus::NeedsOperatorReview;
        record.last_error = Some(reason);
        record.attempt_count = record.attempt_count.saturating_add(1);
        record.updated_at = chrono::Utc::now();
        require_updated(update_outbox_record(&self.pool, &record, true, false).await?)
    }

    async fn reset_for_retry(
        &self,
        outbox_id: LifecycleOutboxId,
        actor_id: String,
        reason: Option<String>,
    ) -> Result<Option<LifecycleOutboxRecord>> {
        let Some(mut record) = self.get(outbox_id).await? else {
            return Ok(None);
        };
        record.status = LifecycleOutboxStatus::PendingProvenance;
        record.last_error = None;
        record.updated_at = chrono::Utc::now();
        record.metadata.insert(
            "operator_retry".to_string(),
            serde_json::json!({
                "actor_id": actor_id,
                "reason": reason,
                "requested_at": record.updated_at,
            }),
        );
        require_updated(update_outbox_record(&self.pool, &record, true, true).await?)?;
        Ok(Some(record))
    }

    async fn mark_operator_resolved(
        &self,
        outbox_id: LifecycleOutboxId,
        actor_id: String,
        reason: String,
    ) -> Result<Option<LifecycleOutboxRecord>> {
        let Some(mut record) = self.get(outbox_id).await? else {
            return Ok(None);
        };
        record.status = LifecycleOutboxStatus::Reconciled;
        record.last_error = None;
        record.updated_at = chrono::Utc::now();
        record.metadata.insert(
            "operator_resolved".to_string(),
            serde_json::json!({
                "actor_id": actor_id,
                "reason": reason,
                "resolved_at": record.updated_at,
            }),
        );
        require_updated(update_outbox_record(&self.pool, &record, true, true).await?)?;
        Ok(Some(record))
    }

    async fn get(&self, outbox_id: LifecycleOutboxId) -> Result<Option<LifecycleOutboxRecord>> {
        fetch_entity_by_id(
            &self.pool,
            "lifecycle_outbox",
            "outbox_id",
            &outbox_id.to_string(),
        )
        .await
    }

    async fn list_by_status(
        &self,
        status: LifecycleOutboxStatus,
        limit: u32,
    ) -> Result<Vec<LifecycleOutboxRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json
             FROM lifecycle_outbox
             WHERE status = ?1
             ORDER BY updated_at ASC, outbox_id ASC
             LIMIT ?2",
            |query| {
                query
                    .bind(enum_text(&status).unwrap())
                    .bind(i64::from(limit))
            },
        )
        .await
    }

    async fn claim_pending_reconciliation(
        &self,
        limit: u32,
        lease_owner: &str,
        lease_ttl: chrono::Duration,
    ) -> Result<Vec<LifecycleOutboxClaim>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let now = chrono::Utc::now();
        let lease_expires_at = now + lease_ttl;
        let now = now.to_rfc3339();
        let lease_expires_at = lease_expires_at.to_rfc3339();
        let pending = enum_text(&LifecycleOutboxStatus::PendingProvenance)?;
        let written = enum_text(&LifecycleOutboxStatus::ProvenanceWritten)?;

        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT outbox_id
             FROM lifecycle_outbox
             WHERE status IN (?1, ?2)
               AND (
                    reconciliation_lease_expires_at IS NULL
                    OR reconciliation_lease_expires_at <= ?3
               )
             ORDER BY created_at ASC, outbox_id ASC
             LIMIT ?4",
        )
        .bind(&pending)
        .bind(&written)
        .bind(&now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        let outbox_ids = rows
            .into_iter()
            .map(|row| row.get::<String, _>("outbox_id"))
            .collect::<Vec<_>>();

        let mut claimed = Vec::with_capacity(outbox_ids.len());
        for outbox_id in outbox_ids {
            let generation = sqlx::query(
                "UPDATE lifecycle_outbox
                 SET reconciliation_lease_owner = ?2,
                     reconciliation_lease_expires_at = ?3,
                     reconciliation_lease_generation = reconciliation_lease_generation + 1
                 WHERE outbox_id = ?1
                   AND (
                        reconciliation_lease_expires_at IS NULL
                        OR reconciliation_lease_expires_at <= ?4
                   )
                 RETURNING reconciliation_lease_generation",
            )
            .bind(&outbox_id)
            .bind(lease_owner)
            .bind(&lease_expires_at)
            .bind(&now)
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| row.get::<i64, _>("reconciliation_lease_generation"));

            if let Some(generation) = generation
                && let Some(row) =
                    sqlx::query("SELECT raw_json FROM lifecycle_outbox WHERE outbox_id = ?1")
                        .bind(&outbox_id)
                        .fetch_optional(&mut *tx)
                        .await?
            {
                let record: LifecycleOutboxRecord = from_json(&row.get::<String, _>("raw_json"))?;
                claimed.push(LifecycleOutboxClaim {
                    lease: LifecycleOutboxLease {
                        outbox_id: record.outbox_id,
                        owner: lease_owner.to_string(),
                        generation,
                        expires_at: lease_expires_at.parse().map_err(|error| {
                            crate::StoreError::Other(format!(
                                "invalid lifecycle lease expiry: {error}"
                            ))
                        })?,
                    },
                    record,
                });
            }
        }

        tx.commit().await?;
        Ok(claimed)
    }

    async fn renew_reconciliation_lease(
        &self,
        lease: &LifecycleOutboxLease,
        lease_ttl: chrono::Duration,
    ) -> Result<bool> {
        let expires_at = (chrono::Utc::now() + lease_ttl).to_rfc3339();
        let result = sqlx::query(
            "UPDATE lifecycle_outbox
             SET reconciliation_lease_expires_at = ?4
             WHERE outbox_id = ?1
               AND reconciliation_lease_owner = ?2
               AND reconciliation_lease_generation = ?3
               AND reconciliation_lease_expires_at > ?7
               AND status IN (?5, ?6)",
        )
        .bind(lease.outbox_id.to_string())
        .bind(&lease.owner)
        .bind(lease.generation)
        .bind(expires_at)
        .bind(enum_text(&LifecycleOutboxStatus::PendingProvenance)?)
        .bind(enum_text(&LifecycleOutboxStatus::ProvenanceWritten)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn mark_provenance_written_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        event_id: EventId,
    ) -> Result<bool> {
        let Some(record) = self.get(lease.outbox_id).await? else {
            return Ok(false);
        };
        self.mark_provenance_obligation_written_claimed(
            lease,
            record.intended_provenance_kind,
            event_id,
        )
        .await
    }

    async fn mark_provenance_obligation_written_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        event_kind: ProvenanceEventKind,
        event_id: EventId,
    ) -> Result<bool> {
        let Some(mut record) = self.get(lease.outbox_id).await? else {
            return Ok(false);
        };
        mark_obligation_written(&mut record, event_kind, event_id);
        update_outbox_record_claimed(&self.pool, &record, lease, false).await
    }

    async fn mark_reconciled_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        result: JsonMap,
    ) -> Result<bool> {
        if !lease_is_current(&self.pool, lease).await? {
            return Ok(false);
        }
        let Some(mut record) = self.get(lease.outbox_id).await? else {
            return Ok(false);
        };
        record.status = LifecycleOutboxStatus::Reconciled;
        record.metadata.insert(
            "reconciliation_result".to_string(),
            serde_json::json!(result),
        );
        record.updated_at = chrono::Utc::now();
        require_all_obligations_satisfied(&record)?;
        update_outbox_record_claimed(&self.pool, &record, lease, true).await
    }

    async fn mark_needs_operator_review_claimed(
        &self,
        lease: &LifecycleOutboxLease,
        reason: String,
    ) -> Result<bool> {
        let Some(mut record) = self.get(lease.outbox_id).await? else {
            return Ok(false);
        };
        record.status = LifecycleOutboxStatus::NeedsOperatorReview;
        record.last_error = Some(reason);
        record.attempt_count = record.attempt_count.saturating_add(1);
        record.updated_at = chrono::Utc::now();
        update_outbox_record_claimed(&self.pool, &record, lease, true).await
    }

    async fn record_reconciliation_failure(
        &self,
        lease: &LifecycleOutboxLease,
        error: String,
        max_attempts: u32,
    ) -> Result<ReconciliationFailureDisposition> {
        let Some(mut record) = self.get(lease.outbox_id).await? else {
            return Ok(ReconciliationFailureDisposition::LeaseLost);
        };
        record.attempt_count = record.attempt_count.saturating_add(1);
        record.last_error = Some(error);
        record.updated_at = chrono::Utc::now();
        let disposition = if record.attempt_count >= max_attempts {
            record.status = LifecycleOutboxStatus::NeedsOperatorReview;
            ReconciliationFailureDisposition::NeedsOperatorReview
        } else {
            ReconciliationFailureDisposition::Retryable
        };
        if update_outbox_record_claimed(&self.pool, &record, lease, true).await? {
            Ok(disposition)
        } else {
            Ok(ReconciliationFailureDisposition::LeaseLost)
        }
    }

    async fn reconciliation_lease_stats(&self) -> Result<LifecycleOutboxLeaseStats> {
        let now = chrono::Utc::now().to_rfc3339();
        let row = sqlx::query(
            "SELECT
                COALESCE(SUM(CASE
                    WHEN reconciliation_lease_owner IS NOT NULL
                     AND reconciliation_lease_expires_at > ?1 THEN 1 ELSE 0 END), 0) AS active,
                COALESCE(SUM(CASE
                    WHEN reconciliation_lease_owner IS NOT NULL
                     AND reconciliation_lease_expires_at <= ?1 THEN 1 ELSE 0 END), 0) AS expired
             FROM lifecycle_outbox
             WHERE status IN (?2, ?3)",
        )
        .bind(now)
        .bind(enum_text(&LifecycleOutboxStatus::PendingProvenance)?)
        .bind(enum_text(&LifecycleOutboxStatus::ProvenanceWritten)?)
        .fetch_one(&self.pool)
        .await?;
        Ok(LifecycleOutboxLeaseStats {
            active: row.get::<i64, _>("active") as usize,
            expired: row.get::<i64, _>("expired") as usize,
        })
    }

    async fn list_pending_reconciliation(&self, limit: u32) -> Result<Vec<LifecycleOutboxRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json
             FROM lifecycle_outbox
             WHERE status IN (?1, ?2, ?3)
             ORDER BY created_at ASC, outbox_id ASC
             LIMIT ?4",
            |query| {
                query
                    .bind(enum_text(&LifecycleOutboxStatus::PendingProvenance).unwrap())
                    .bind(enum_text(&LifecycleOutboxStatus::ProvenanceWritten).unwrap())
                    .bind(enum_text(&LifecycleOutboxStatus::NeedsOperatorReview).unwrap())
                    .bind(i64::from(limit))
            },
        )
        .await
    }
}

async fn insert_outbox_record(pool: &SqlitePool, record: &LifecycleOutboxRecord) -> Result<()> {
    let mut tx = pool.begin().await?;
    insert_outbox_record_tx(&mut tx, record).await?;
    tx.commit().await?;
    Ok(())
}

async fn insert_outbox_record_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record: &LifecycleOutboxRecord,
) -> Result<()> {
    let raw_json = to_json(record)?;
    let result = sqlx::query(
        "INSERT INTO lifecycle_outbox (
            outbox_id, execution_id, rollback_contract_id, previous_execution_state,
            new_execution_state, previous_rollback_state, new_rollback_state,
            intended_provenance_kind, idempotency_key, status, provenance_event_id,
            attempt_count, last_error, created_at, updated_at, raw_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
        ON CONFLICT(idempotency_key) DO NOTHING",
    )
    .bind(record.outbox_id.to_string())
    .bind(record.execution_id.to_string())
    .bind(record.rollback_contract_id.map(|id| id.to_string()))
    .bind(
        record
            .previous_execution_state
            .as_ref()
            .map(enum_text)
            .transpose()?,
    )
    .bind(enum_text(&record.new_execution_state)?)
    .bind(
        record
            .previous_rollback_state
            .as_ref()
            .map(enum_text)
            .transpose()?,
    )
    .bind(
        record
            .new_rollback_state
            .as_ref()
            .map(enum_text)
            .transpose()?,
    )
    .bind(enum_text(&record.intended_provenance_kind)?)
    .bind(&record.idempotency_key)
    .bind(enum_text(&record.status)?)
    .bind(record.provenance_event_id.map(|id| id.to_string()))
    .bind(i64::from(record.attempt_count))
    .bind(&record.last_error)
    .bind(record.created_at)
    .bind(record.updated_at)
    .bind(raw_json)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(crate::StoreError::Other(format!(
            "lifecycle outbox idempotency conflict for key {}",
            record.idempotency_key
        )));
    }
    Ok(())
}

async fn update_outbox_record(
    pool: &SqlitePool,
    record: &LifecycleOutboxRecord,
    clear_lease: bool,
    allow_lease_override: bool,
) -> Result<bool> {
    let raw_json = to_json(record)?;
    let result = sqlx::query(
        "UPDATE lifecycle_outbox
         SET status = ?2,
             provenance_event_id = ?3,
             attempt_count = ?4,
             last_error = ?5,
             updated_at = ?6,
             raw_json = ?7,
             reconciliation_lease_owner = CASE WHEN ?8 THEN NULL ELSE reconciliation_lease_owner END,
             reconciliation_lease_expires_at = CASE WHEN ?8 THEN NULL ELSE reconciliation_lease_expires_at END
         WHERE outbox_id = ?1
           AND (?9 OR reconciliation_lease_owner IS NULL)",
    )
    .bind(record.outbox_id.to_string())
    .bind(enum_text(&record.status)?)
    .bind(record.provenance_event_id.map(|id| id.to_string()))
    .bind(i64::from(record.attempt_count))
    .bind(&record.last_error)
    .bind(record.updated_at)
    .bind(raw_json)
    .bind(clear_lease)
    .bind(allow_lease_override)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

fn mark_obligation_written(
    record: &mut LifecycleOutboxRecord,
    event_kind: ProvenanceEventKind,
    event_id: EventId,
) {
    if record.provenance_obligations.is_empty() {
        record
            .provenance_obligations
            .push(ferrum_proto::ProvenanceObligation::pending(
                record.intended_provenance_kind.clone(),
            ));
    }
    let mut matched = false;
    for obligation in &mut record.provenance_obligations {
        if obligation.event_kind == event_kind {
            obligation.event_id = Some(event_id);
            matched = true;
            break;
        }
    }
    if !matched {
        let mut obligation = ferrum_proto::ProvenanceObligation::pending(event_kind);
        obligation.event_id = Some(event_id);
        record.provenance_obligations.push(obligation);
    }
    record.provenance_event_id = Some(event_id);
    record.status = LifecycleOutboxStatus::ProvenanceWritten;
    record.updated_at = chrono::Utc::now();
}

fn require_all_obligations_satisfied(record: &LifecycleOutboxRecord) -> Result<()> {
    let obligations_satisfied = if record.provenance_obligations.is_empty() {
        record.provenance_event_id.is_some()
    } else {
        record
            .provenance_obligations
            .iter()
            .all(ferrum_proto::ProvenanceObligation::is_satisfied)
    };
    if obligations_satisfied {
        Ok(())
    } else {
        Err(crate::StoreError::Other(format!(
            "lifecycle outbox {} has unsatisfied provenance obligations",
            record.outbox_id
        )))
    }
}

fn require_updated(updated: bool) -> Result<()> {
    if updated {
        Ok(())
    } else {
        Err(crate::StoreError::Other(
            "lifecycle outbox update did not affect any row".to_string(),
        ))
    }
}

async fn update_outbox_record_claimed(
    pool: &SqlitePool,
    record: &LifecycleOutboxRecord,
    lease: &LifecycleOutboxLease,
    clear_lease: bool,
) -> Result<bool> {
    let raw_json = to_json(record)?;
    let result = sqlx::query(
        "UPDATE lifecycle_outbox
         SET status = ?4,
             provenance_event_id = ?5,
             attempt_count = ?6,
             last_error = ?7,
             updated_at = ?8,
             raw_json = ?9,
             reconciliation_lease_owner = CASE WHEN ?10 THEN NULL ELSE reconciliation_lease_owner END,
             reconciliation_lease_expires_at = CASE WHEN ?10 THEN NULL ELSE reconciliation_lease_expires_at END
         WHERE outbox_id = ?1
           AND reconciliation_lease_owner = ?2
           AND reconciliation_lease_generation = ?3
           AND reconciliation_lease_expires_at > ?11",
    )
    .bind(lease.outbox_id.to_string())
    .bind(&lease.owner)
    .bind(lease.generation)
    .bind(enum_text(&record.status)?)
    .bind(record.provenance_event_id.map(|id| id.to_string()))
    .bind(i64::from(record.attempt_count))
    .bind(&record.last_error)
    .bind(record.updated_at)
    .bind(raw_json)
    .bind(clear_lease)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

async fn lease_is_current(pool: &SqlitePool, lease: &LifecycleOutboxLease) -> Result<bool> {
    let row = sqlx::query(
        "SELECT 1
         FROM lifecycle_outbox
         WHERE outbox_id = ?1
           AND reconciliation_lease_owner = ?2
           AND reconciliation_lease_generation = ?3
           AND reconciliation_lease_expires_at > ?4
         LIMIT 1",
    )
    .bind(lease.outbox_id.to_string())
    .bind(&lease.owner)
    .bind(lease.generation)
    .bind(chrono::Utc::now().to_rfc3339())
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo, RollbackRepo,
        StoreFacade, sqlite::SqliteStore,
    };
    use ferrum_proto::{
        ActionProposal, ActionType, ActorRef, ActorType, CapabilityLease, CapabilityStatus,
        Decision, EventId, ExecutionId, HashChainRef, IntentEnvelope, IntentStatus, ObjectRef,
        ObjectType, PrincipalId, ProposalId, ProvenanceEvent, ProvenanceEventKind, RiskTier,
        RollbackClass, RollbackContract, RollbackContractId, RollbackState, RollbackTarget,
        ToolBinding,
    };
    use std::sync::Arc;

    fn test_intent() -> IntentEnvelope {
        IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: ferrum_proto::TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        }
    }

    fn test_proposal(intent_id: ferrum_proto::IntentId) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        }
    }

    fn test_capability(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
    ) -> CapabilityLease {
        let now = chrono::Utc::now();
        CapabilityLease {
            capability_id: ferrum_proto::CapabilityId::new(),
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "test_server".to_string(),
                tool_name: "test_tool".to_string(),
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
            issued_by: "test".to_string(),
            policy_bundle_id: ferrum_proto::PolicyBundleId::new(),
            tool_manifest_id: None,
            manifest_hash: None,
            status: CapabilityStatus::Active,
            issued_at: now,
            expires_at: now + chrono::Duration::minutes(5),
            revoked_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    fn test_execution(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
        capability_id: ferrum_proto::CapabilityId,
    ) -> ExecutionRecord {
        ExecutionRecord {
            execution_id: ExecutionId::new(),
            intent_id,
            proposal_id,
            capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ferrum_proto::ExecutionState::Authorized,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    fn test_contract(execution: &ExecutionRecord) -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id: execution.intent_id,
            proposal_id: execution.proposal_id,
            execution_id: execution.execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "noop".to_string(),
            target: RollbackTarget::Generic {
                namespace: "test".to_string(),
                identifier: "target".to_string(),
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::PendingPrepare,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    async fn seed_execution(store: &SqliteStore) -> (ExecutionRecord, RollbackContract) {
        let intent = test_intent();
        store.intents().insert(&intent).await.unwrap();
        let proposal = test_proposal(intent.intent_id);
        store.proposals().insert(&proposal).await.unwrap();
        let capability = test_capability(intent.intent_id, proposal.proposal_id);
        store.capabilities().insert(&capability).await.unwrap();
        let mut execution = test_execution(
            intent.intent_id,
            proposal.proposal_id,
            capability.capability_id,
        );
        store.executions().insert(&execution).await.unwrap();
        let contract = test_contract(&execution);
        store.rollback_contracts().insert(&contract).await.unwrap();
        execution.rollback_contract_id = Some(contract.contract_id);
        store.executions().update(&execution).await.unwrap();
        (execution, contract)
    }

    fn outbox_for(
        execution: &ExecutionRecord,
        contract: &RollbackContract,
    ) -> LifecycleOutboxRecord {
        LifecycleOutboxRecord::pending(
            execution.execution_id,
            Some(contract.contract_id),
            Some(ferrum_proto::ExecutionState::Authorized),
            ferrum_proto::ExecutionState::Running,
            Some(RollbackState::PendingPrepare),
            Some(RollbackState::Prepared),
            ProvenanceEventKind::SideEffectPrepared,
            format!("prepare:{}", execution.execution_id),
        )
    }

    fn provenance_event(
        execution: &ExecutionRecord,
        contract: &RollbackContract,
    ) -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::SideEffectPrepared,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::RollbackContract,
                object_id: contract.contract_id.to_string(),
                summary: None,
            },
            intent_id: Some(execution.intent_id),
            proposal_id: Some(execution.proposal_id),
            execution_id: Some(execution.execution_id),
            capability_id: Some(execution.capability_id),
            rollback_contract_id: Some(contract.contract_id),
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
            source_runtime_id: None,
        }
    }

    fn action_proposal_submitted_event(execution: &ExecutionRecord) -> ProvenanceEvent {
        ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::ActionProposalSubmitted,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::Proposal,
                object_id: execution.proposal_id.to_string(),
                summary: None,
            },
            intent_id: Some(execution.intent_id),
            proposal_id: Some(execution.proposal_id),
            execution_id: Some(execution.execution_id),
            capability_id: Some(execution.capability_id),
            rollback_contract_id: execution.rollback_contract_id,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
            source_runtime_id: None,
        }
    }

    #[tokio::test]
    async fn lifecycle_outbox_marks_provenance_and_reconciles() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();

        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();
        let pending = repo.list_pending_reconciliation(10).await.unwrap();
        assert_eq!(pending.len(), 1);

        let event = provenance_event(&execution, &contract);
        store.provenance().append_event(&event).await.unwrap();
        repo.mark_provenance_written(outbox.outbox_id, event.event_id)
            .await
            .unwrap();
        let written = repo.get(outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(written.status, LifecycleOutboxStatus::ProvenanceWritten);
        assert_eq!(written.provenance_event_id, Some(event.event_id));

        repo.mark_reconciled(outbox.outbox_id, ferrum_proto::JsonMap::new())
            .await
            .unwrap();
        let reconciled = repo.get(outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(reconciled.status, LifecycleOutboxStatus::Reconciled);
        assert!(
            repo.list_pending_reconciliation(10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn lifecycle_outbox_operator_retry_and_resolve_update_status_and_metadata() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();

        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();
        repo.mark_needs_operator_review(outbox.outbox_id, "ambiguous parent".to_string())
            .await
            .unwrap();

        let review = repo
            .list_by_status(LifecycleOutboxStatus::NeedsOperatorReview, 10)
            .await
            .unwrap();
        assert_eq!(review.len(), 1);
        assert_eq!(review[0].last_error.as_deref(), Some("ambiguous parent"));

        let retried = repo
            .reset_for_retry(
                outbox.outbox_id,
                "operator-1".to_string(),
                Some("fixed parent event".to_string()),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retried.status, LifecycleOutboxStatus::PendingProvenance);
        assert!(retried.last_error.is_none());
        assert!(retried.metadata.contains_key("operator_retry"));

        let resolved = repo
            .mark_operator_resolved(
                outbox.outbox_id,
                "operator-1".to_string(),
                "accepted manual review".to_string(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.status, LifecycleOutboxStatus::Reconciled);
        assert!(resolved.last_error.is_none());
        assert!(resolved.metadata.contains_key("operator_resolved"));
    }

    #[tokio::test]
    async fn lifecycle_outbox_claim_prevents_duplicate_claim_until_lease_expires() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();

        let first = repo
            .claim_pending_reconciliation(10, "node-a", chrono::Duration::minutes(5))
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].record.outbox_id, outbox.outbox_id);

        let second = repo
            .claim_pending_reconciliation(10, "node-b", chrono::Duration::minutes(5))
            .await
            .unwrap();
        assert!(second.is_empty());

        sqlx::query(
            "UPDATE lifecycle_outbox
             SET reconciliation_lease_expires_at = ?2
             WHERE outbox_id = ?1",
        )
        .bind(outbox.outbox_id.to_string())
        .bind((chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339())
        .execute(store.pool())
        .await
        .unwrap();

        let reclaimed = repo
            .claim_pending_reconciliation(10, "node-b", chrono::Duration::minutes(5))
            .await
            .unwrap();
        assert_eq!(reclaimed.len(), 1);
        assert_eq!(reclaimed[0].record.outbox_id, outbox.outbox_id);
    }

    #[tokio::test]
    async fn lifecycle_outbox_claim_does_not_claim_operator_review_records() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();
        repo.mark_needs_operator_review(outbox.outbox_id, "missing parent".to_string())
            .await
            .unwrap();

        let claimed = repo
            .claim_pending_reconciliation(10, "node-a", chrono::Duration::minutes(5))
            .await
            .unwrap();
        assert!(claimed.is_empty());
    }

    #[tokio::test]
    async fn claimed_reconcile_requires_all_provenance_obligations() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();

        let claim = repo
            .claim_pending_reconciliation(1, "node-a", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);
        let err = repo
            .mark_reconciled_claimed(&claim.lease, ferrum_proto::JsonMap::new())
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("unsatisfied provenance obligations")
        );

        let stored = repo.get(outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(stored.status, LifecycleOutboxStatus::PendingProvenance);
    }

    #[tokio::test]
    async fn lifecycle_outbox_fencing_rejects_stale_worker_after_reclaim() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();

        let first = repo
            .claim_pending_reconciliation(1, "node-a", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);
        let event = provenance_event(&execution, &contract);
        store.provenance().append_event(&event).await.unwrap();
        repo.mark_provenance_obligation_written_claimed(
            &first.lease,
            ProvenanceEventKind::SideEffectPrepared,
            event.event_id,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE lifecycle_outbox
             SET reconciliation_lease_expires_at = ?2
             WHERE outbox_id = ?1",
        )
        .bind(outbox.outbox_id.to_string())
        .bind((chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339())
        .execute(store.pool())
        .await
        .unwrap();
        let second = repo
            .claim_pending_reconciliation(1, "node-b", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);

        assert!(second.lease.generation > first.lease.generation);
        assert!(
            !repo
                .mark_reconciled_claimed(&first.lease, ferrum_proto::JsonMap::new())
                .await
                .unwrap()
        );
        assert!(
            repo.mark_reconciled_claimed(&second.lease, ferrum_proto::JsonMap::new())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn lifecycle_outbox_renewal_and_lease_stats_track_current_fence() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();

        let claim = repo
            .claim_pending_reconciliation(1, "node-a", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);
        assert!(
            repo.renew_reconciliation_lease(&claim.lease, chrono::Duration::minutes(10))
                .await
                .unwrap()
        );
        assert_eq!(
            repo.reconciliation_lease_stats().await.unwrap(),
            LifecycleOutboxLeaseStats {
                active: 1,
                expired: 0
            }
        );

        sqlx::query(
            "UPDATE lifecycle_outbox
             SET reconciliation_lease_expires_at = ?2
             WHERE outbox_id = ?1",
        )
        .bind(outbox.outbox_id.to_string())
        .bind((chrono::Utc::now() - chrono::Duration::seconds(1)).to_rfc3339())
        .execute(store.pool())
        .await
        .unwrap();
        assert_eq!(
            repo.reconciliation_lease_stats().await.unwrap(),
            LifecycleOutboxLeaseStats {
                active: 0,
                expired: 1
            }
        );

        let replacement = repo
            .claim_pending_reconciliation(1, "node-b", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);
        assert!(
            !repo
                .renew_reconciliation_lease(&claim.lease, chrono::Duration::minutes(10))
                .await
                .unwrap()
        );
        assert!(
            repo.renew_reconciliation_lease(&replacement.lease, chrono::Duration::minutes(10))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn lifecycle_outbox_failures_release_lease_and_escalate_at_threshold() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let outbox = outbox_for(&execution, &contract);
        let repo = store.lifecycle_outbox();
        repo.enqueue_lifecycle_transition(&outbox).await.unwrap();

        let first = repo
            .claim_pending_reconciliation(1, "node-a", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            repo.record_reconciliation_failure(&first.lease, "transient failure".to_string(), 2)
                .await
                .unwrap(),
            ReconciliationFailureDisposition::Retryable
        );

        let second = repo
            .claim_pending_reconciliation(1, "node-b", chrono::Duration::minutes(5))
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            repo.record_reconciliation_failure(&second.lease, "second failure".to_string(), 2)
                .await
                .unwrap(),
            ReconciliationFailureDisposition::NeedsOperatorReview
        );
        let stored = repo.get(outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(stored.attempt_count, 2);
        assert_eq!(stored.status, LifecycleOutboxStatus::NeedsOperatorReview);
        assert_eq!(stored.last_error.as_deref(), Some("second failure"));
    }

    #[tokio::test]
    async fn record_lifecycle_transition_rolls_back_when_outbox_insert_fails() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (mut execution, mut contract) = seed_execution(&store).await;
        let repo = store.lifecycle_outbox();
        let outbox = outbox_for(&execution, &contract);
        let parent_event = action_proposal_submitted_event(&execution);
        store
            .provenance()
            .append_event(&parent_event)
            .await
            .unwrap();

        sqlx::query(
            "CREATE TRIGGER fail_lifecycle_outbox_insert
             BEFORE INSERT ON lifecycle_outbox
             BEGIN
                 SELECT RAISE(FAIL, 'simulated outbox insert failure');
             END;",
        )
        .execute(store.pool())
        .await
        .unwrap();

        execution.state = ferrum_proto::ExecutionState::Running;
        contract.state = RollbackState::Prepared;
        let err = repo
            .record_lifecycle_transition(&execution, Some(&contract), &outbox)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("simulated outbox insert failure"));

        let stored_execution = store
            .executions()
            .get(execution.execution_id)
            .await
            .unwrap()
            .unwrap();
        let stored_contract = store
            .rollback_contracts()
            .get(contract.contract_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored_execution.state,
            ferrum_proto::ExecutionState::Authorized
        );
        assert_eq!(stored_contract.state, RollbackState::PendingPrepare);
        assert!(
            repo.list_pending_reconciliation(10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn record_lifecycle_transition_rejects_stale_execution_state() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (mut execution, mut contract) = seed_execution(&store).await;
        let repo = store.lifecycle_outbox();
        let outbox = outbox_for(&execution, &contract);

        store
            .executions()
            .update_state(
                execution.execution_id,
                ferrum_proto::ExecutionState::Running,
            )
            .await
            .unwrap();

        execution.state = ferrum_proto::ExecutionState::Running;
        contract.state = RollbackState::Prepared;
        let err = repo
            .record_lifecycle_transition(&execution, Some(&contract), &outbox)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("state changed"));

        let stored_execution = store
            .executions()
            .get(execution.execution_id)
            .await
            .unwrap()
            .unwrap();
        let stored_contract = store
            .rollback_contracts()
            .get(contract.contract_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored_execution.state,
            ferrum_proto::ExecutionState::Running
        );
        assert_eq!(stored_contract.state, RollbackState::PendingPrepare);
        assert!(
            repo.list_pending_reconciliation(10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn record_lifecycle_transition_rejects_stale_rollback_state() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();
        let (mut execution, mut contract) = seed_execution(&store).await;
        let repo = store.lifecycle_outbox();
        let outbox = outbox_for(&execution, &contract);

        store
            .rollback_contracts()
            .update_state(contract.contract_id, RollbackState::Verified)
            .await
            .unwrap();

        execution.state = ferrum_proto::ExecutionState::Running;
        contract.state = RollbackState::Prepared;
        let err = repo
            .record_lifecycle_transition(&execution, Some(&contract), &outbox)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("state changed"));

        let stored_execution = store
            .executions()
            .get(execution.execution_id)
            .await
            .unwrap()
            .unwrap();
        let stored_contract = store
            .rollback_contracts()
            .get(contract.contract_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored_execution.state,
            ferrum_proto::ExecutionState::Authorized
        );
        assert_eq!(stored_contract.state, RollbackState::Verified);
        assert!(
            repo.list_pending_reconciliation(10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn reconciler_repairs_missing_provenance_after_state_transition() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let (mut execution, mut contract) = seed_execution(&store).await;
        let repo = store.lifecycle_outbox();
        let outbox = outbox_for(&execution, &contract);
        let parent_event = action_proposal_submitted_event(&execution);
        store
            .provenance()
            .append_event(&parent_event)
            .await
            .unwrap();

        execution.state = ferrum_proto::ExecutionState::Running;
        contract.state = RollbackState::Prepared;
        repo.record_lifecycle_transition(&execution, Some(&contract), &outbox)
            .await
            .unwrap();

        let facade: Arc<dyn StoreFacade> = store.clone();
        let report = crate::reconcile_lifecycle_outbox(&facade, 10)
            .await
            .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.repaired_missing_provenance, 1);
        let reconciled = repo.get(outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(reconciled.status, LifecycleOutboxStatus::Reconciled);
        assert!(reconciled.provenance_event_id.is_some());

        let event = store
            .provenance()
            .get_event(reconciled.provenance_event_id.unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event.kind,
            ProvenanceEventKind::SideEffectPrepared
        ));
        assert_eq!(
            event.metadata.get("reconciled"),
            Some(&serde_json::json!(true))
        );
        let edges = store
            .provenance()
            .get_edges_to(event.event_id)
            .await
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_event_id, parent_event.event_id);

        let second_report = crate::reconcile_lifecycle_outbox(&facade, 10)
            .await
            .unwrap();
        assert_eq!(
            second_report,
            crate::LifecycleReconciliationReport::default()
        );
    }

    #[tokio::test]
    async fn reconciler_isolates_record_failure_and_continues_batch() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let (mut first_execution, mut first_contract) = seed_execution(&store).await;
        let (mut second_execution, mut second_contract) = seed_execution(&store).await;
        let first_outbox = outbox_for(&first_execution, &first_contract);
        let second_outbox = outbox_for(&second_execution, &second_contract);

        store
            .provenance()
            .append_event(&action_proposal_submitted_event(&first_execution))
            .await
            .unwrap();
        store
            .provenance()
            .append_event(&action_proposal_submitted_event(&second_execution))
            .await
            .unwrap();

        first_execution.state = ferrum_proto::ExecutionState::Running;
        first_contract.state = RollbackState::Prepared;
        second_execution.state = ferrum_proto::ExecutionState::Running;
        second_contract.state = RollbackState::Prepared;
        store
            .lifecycle_outbox()
            .record_lifecycle_transition(&first_execution, Some(&first_contract), &first_outbox)
            .await
            .unwrap();
        store
            .lifecycle_outbox()
            .record_lifecycle_transition(&second_execution, Some(&second_contract), &second_outbox)
            .await
            .unwrap();

        sqlx::query(&format!(
            "CREATE TRIGGER fail_first_reconciled_event
             BEFORE INSERT ON provenance_events
             WHEN NEW.execution_id = '{}'
             BEGIN
                 SELECT RAISE(FAIL, 'simulated per-record provenance failure');
             END;",
            first_execution.execution_id
        ))
        .execute(store.pool())
        .await
        .unwrap();

        let facade: Arc<dyn StoreFacade> = store.clone();
        let report = crate::reconcile_lifecycle_outbox(&facade, 10)
            .await
            .unwrap();

        assert_eq!(report.scanned, 2);
        assert_eq!(report.failures, 1);
        assert_eq!(report.retryable_failures, 1);
        assert_eq!(report.repaired_missing_provenance, 1);

        let first = store
            .lifecycle_outbox()
            .get(first_outbox.outbox_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.status, LifecycleOutboxStatus::PendingProvenance);
        assert_eq!(first.attempt_count, 1);
        assert!(
            first
                .last_error
                .as_deref()
                .unwrap()
                .contains("simulated per-record provenance failure")
        );

        let second = store
            .lifecycle_outbox()
            .get(second_outbox.outbox_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.status, LifecycleOutboxStatus::Reconciled);
    }

    #[tokio::test]
    async fn reconciler_repairs_missing_parent_edge_for_existing_event() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let (mut execution, mut contract) = seed_execution(&store).await;
        let repo = store.lifecycle_outbox();
        let outbox = outbox_for(&execution, &contract);
        let parent_event = action_proposal_submitted_event(&execution);
        let prepared_event = provenance_event(&execution, &contract);

        store
            .provenance()
            .append_event(&parent_event)
            .await
            .unwrap();
        store
            .provenance()
            .append_event(&prepared_event)
            .await
            .unwrap();

        execution.state = ferrum_proto::ExecutionState::Running;
        contract.state = RollbackState::Prepared;
        repo.record_lifecycle_transition(&execution, Some(&contract), &outbox)
            .await
            .unwrap();
        repo.mark_provenance_written(outbox.outbox_id, prepared_event.event_id)
            .await
            .unwrap();

        let facade: Arc<dyn StoreFacade> = store.clone();
        let report = crate::reconcile_lifecycle_outbox(&facade, 10)
            .await
            .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.already_reconciled, 1);
        let reconciled = repo.get(outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(reconciled.status, LifecycleOutboxStatus::Reconciled);
        let edges = store
            .provenance()
            .get_edges_to(prepared_event.event_id)
            .await
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_event_id, parent_event.event_id);
    }

    #[tokio::test]
    async fn reconciler_repairs_missing_terminal_provenance_after_state_transition() {
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        let (execution, contract) = seed_execution(&store).await;
        let repo = store.lifecycle_outbox();

        // Move execution to Running directly so we can test the terminal transition
        store
            .executions()
            .update_state(
                execution.execution_id,
                ferrum_proto::ExecutionState::Running,
            )
            .await
            .unwrap();

        // Create the parent event for terminal provenance (SideEffectVerified -> SideEffectCommitted)
        let verified_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::SideEffectVerified,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::SideEffect,
                object_id: execution.execution_id.to_string(),
                summary: None,
            },
            intent_id: Some(execution.intent_id),
            proposal_id: Some(execution.proposal_id),
            execution_id: Some(execution.execution_id),
            capability_id: Some(execution.capability_id),
            rollback_contract_id: Some(contract.contract_id),
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
            source_runtime_id: None,
        };
        store
            .provenance()
            .append_event(&verified_event)
            .await
            .unwrap();

        // Transition to Committed terminal state
        let committed_execution = ExecutionRecord {
            state: ferrum_proto::ExecutionState::Committed,
            ..execution.clone()
        };
        let committed_outbox = LifecycleOutboxRecord::pending(
            execution.execution_id,
            Some(contract.contract_id),
            Some(ferrum_proto::ExecutionState::Running),
            ferrum_proto::ExecutionState::Committed,
            Some(RollbackState::PendingPrepare),
            Some(RollbackState::PendingPrepare),
            ProvenanceEventKind::SideEffectCommitted,
            format!("commit:{}", execution.execution_id),
        );
        repo.record_lifecycle_transition(&committed_execution, Some(&contract), &committed_outbox)
            .await
            .unwrap();

        let facade: Arc<dyn StoreFacade> = store.clone();
        let report = crate::reconcile_lifecycle_outbox(&facade, 10)
            .await
            .unwrap();

        assert_eq!(report.scanned, 1);
        assert_eq!(report.repaired_missing_provenance, 1);
        let reconciled = repo.get(committed_outbox.outbox_id).await.unwrap().unwrap();
        assert_eq!(reconciled.status, LifecycleOutboxStatus::Reconciled);
        assert!(reconciled.provenance_event_id.is_some());

        let event = store
            .provenance()
            .get_event(reconciled.provenance_event_id.unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            event.kind,
            ProvenanceEventKind::SideEffectCommitted
        ));
        assert_eq!(
            event.metadata.get("reconciled"),
            Some(&serde_json::json!(true))
        );
        let edges = store
            .provenance()
            .get_edges_to(event.event_id)
            .await
            .unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_event_id, verified_event.event_id);
    }
}
