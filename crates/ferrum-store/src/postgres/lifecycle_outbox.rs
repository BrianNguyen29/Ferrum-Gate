use async_trait::async_trait;
use ferrum_proto::{
    CapabilityLease, CapabilityStatus, EventId, ExecutionRecord, JsonMap, LifecycleOutboxId,
    LifecycleOutboxRecord, LifecycleOutboxStatus, ProvenanceEventKind, RollbackContract,
};
use sqlx::{PgPool, Row};

use crate::{
    LifecycleOutboxClaim, LifecycleOutboxLease, LifecycleOutboxLeaseStats, LifecycleOutboxRepo,
    ReconciliationFailureDisposition, Result,
};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, from_json, to_json};

#[derive(Clone)]
pub struct PostgresLifecycleOutboxRepo {
    pool: PgPool,
}

impl PostgresLifecycleOutboxRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LifecycleOutboxRepo for PostgresLifecycleOutboxRepo {
    async fn enqueue_lifecycle_transition(&self, record: &LifecycleOutboxRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        insert_outbox_record_tx(&mut tx, record).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_lifecycle_transition(
        &self,
        execution: &ExecutionRecord,
        rollback_contract: Option<&RollbackContract>,
        outbox: &LifecycleOutboxRecord,
    ) -> Result<()> {
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
             SET rollback_contract_id = $2,
                 decision = $3,
                 state = $4,
                 finished_at = $5,
                 result_digest = $6,
                 raw_json = $7
             WHERE execution_id = $1
               AND state = $8",
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
                     SET state = $2,
                         auto_commit = $3,
                         expires_at = $4,
                         raw_json = $5
                     WHERE contract_id = $1
                       AND state = $6",
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
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
             SET status = $2,
                 raw_json = jsonb_set(raw_json, '{status}', to_jsonb($2::text))
             WHERE capability_id = $1 AND status = $3",
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
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
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
             WHERE status = $1
             ORDER BY updated_at ASC, outbox_id ASC
             LIMIT $2",
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
        let statuses = vec![
            enum_text(&LifecycleOutboxStatus::PendingProvenance)?,
            enum_text(&LifecycleOutboxStatus::ProvenanceWritten)?,
        ];

        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            "WITH claimed AS (
                SELECT outbox_id
                FROM lifecycle_outbox
                WHERE status = ANY($1)
                  AND (
                        reconciliation_lease_expires_at IS NULL
                        OR reconciliation_lease_expires_at <= $2
                  )
                ORDER BY created_at ASC, outbox_id ASC
                LIMIT $3
                FOR UPDATE SKIP LOCKED
             )
             UPDATE lifecycle_outbox AS outbox
             SET reconciliation_lease_owner = $4,
                 reconciliation_lease_expires_at = $5,
                 reconciliation_lease_generation =
                    outbox.reconciliation_lease_generation + 1
             FROM claimed
             WHERE outbox.outbox_id = claimed.outbox_id
             RETURNING outbox.raw_json,
                       outbox.outbox_id,
                       outbox.reconciliation_lease_generation,
                       outbox.reconciliation_lease_expires_at",
        )
        .bind(statuses)
        .bind(now)
        .bind(i64::from(limit))
        .bind(lease_owner)
        .bind(lease_expires_at)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        rows.into_iter()
            .map(|row| {
                let record: LifecycleOutboxRecord = from_json(&row.get::<String, _>("raw_json"))?;
                Ok(LifecycleOutboxClaim {
                    lease: LifecycleOutboxLease {
                        outbox_id: record.outbox_id,
                        owner: lease_owner.to_string(),
                        generation: row.get("reconciliation_lease_generation"),
                        expires_at: row.get("reconciliation_lease_expires_at"),
                    },
                    record,
                })
            })
            .collect()
    }

    async fn renew_reconciliation_lease(
        &self,
        lease: &LifecycleOutboxLease,
        lease_ttl: chrono::Duration,
    ) -> Result<bool> {
        let expires_at = chrono::Utc::now() + lease_ttl;
        let result = sqlx::query(
            "UPDATE lifecycle_outbox
             SET reconciliation_lease_expires_at = $4
             WHERE outbox_id = $1
               AND reconciliation_lease_owner = $2
               AND reconciliation_lease_generation = $3
               AND reconciliation_lease_expires_at > NOW()
               AND status = ANY($5)",
        )
        .bind(lease.outbox_id.to_string())
        .bind(&lease.owner)
        .bind(lease.generation)
        .bind(expires_at)
        .bind(vec![
            enum_text(&LifecycleOutboxStatus::PendingProvenance)?,
            enum_text(&LifecycleOutboxStatus::ProvenanceWritten)?,
        ])
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
        let row = sqlx::query(
            "SELECT
                COUNT(*) FILTER (
                    WHERE reconciliation_lease_owner IS NOT NULL
                      AND reconciliation_lease_expires_at > NOW()
                ) AS active,
                COUNT(*) FILTER (
                    WHERE reconciliation_lease_owner IS NOT NULL
                      AND reconciliation_lease_expires_at <= NOW()
                ) AS expired
             FROM lifecycle_outbox
             WHERE status = ANY($1)",
        )
        .bind(vec![
            enum_text(&LifecycleOutboxStatus::PendingProvenance)?,
            enum_text(&LifecycleOutboxStatus::ProvenanceWritten)?,
        ])
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
             WHERE status = ANY($1)
             ORDER BY created_at ASC, outbox_id ASC
             LIMIT $2",
            |query| {
                query
                    .bind(vec![
                        enum_text(&LifecycleOutboxStatus::PendingProvenance).unwrap(),
                        enum_text(&LifecycleOutboxStatus::ProvenanceWritten).unwrap(),
                        enum_text(&LifecycleOutboxStatus::NeedsOperatorReview).unwrap(),
                    ])
                    .bind(i64::from(limit))
            },
        )
        .await
    }
}

async fn insert_outbox_record_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    record: &LifecycleOutboxRecord,
) -> Result<()> {
    let raw_json = to_json(record)?;
    let result = sqlx::query(
        "INSERT INTO lifecycle_outbox (
            outbox_id, execution_id, rollback_contract_id, previous_execution_state,
            new_execution_state, previous_rollback_state, new_rollback_state,
            intended_provenance_kind, idempotency_key, status, provenance_event_id,
            attempt_count, last_error, created_at, updated_at, raw_json
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
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
    .bind(record.attempt_count as i32)
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
    pool: &PgPool,
    record: &LifecycleOutboxRecord,
    clear_lease: bool,
    allow_lease_override: bool,
) -> Result<bool> {
    let raw_json = to_json(record)?;
    let result = sqlx::query(
        "UPDATE lifecycle_outbox
         SET status = $2,
             provenance_event_id = $3,
             attempt_count = $4,
             last_error = $5,
             updated_at = $6,
             raw_json = $7,
             reconciliation_lease_owner = CASE WHEN $8 THEN NULL ELSE reconciliation_lease_owner END,
             reconciliation_lease_expires_at = CASE WHEN $8 THEN NULL ELSE reconciliation_lease_expires_at END
         WHERE outbox_id = $1
           AND ($9 OR reconciliation_lease_owner IS NULL)",
    )
    .bind(record.outbox_id.to_string())
    .bind(enum_text(&record.status)?)
    .bind(record.provenance_event_id.map(|id| id.to_string()))
    .bind(record.attempt_count as i32)
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
    pool: &PgPool,
    record: &LifecycleOutboxRecord,
    lease: &LifecycleOutboxLease,
    clear_lease: bool,
) -> Result<bool> {
    let raw_json = to_json(record)?;
    let result = sqlx::query(
        "UPDATE lifecycle_outbox
         SET status = $4,
             provenance_event_id = $5,
             attempt_count = $6,
             last_error = $7,
             updated_at = $8,
             raw_json = $9,
             reconciliation_lease_owner = CASE WHEN $10 THEN NULL ELSE reconciliation_lease_owner END,
             reconciliation_lease_expires_at = CASE WHEN $10 THEN NULL ELSE reconciliation_lease_expires_at END
         WHERE outbox_id = $1
           AND reconciliation_lease_owner = $2
           AND reconciliation_lease_generation = $3
           AND reconciliation_lease_expires_at > NOW()",
    )
    .bind(lease.outbox_id.to_string())
    .bind(&lease.owner)
    .bind(lease.generation)
    .bind(enum_text(&record.status)?)
    .bind(record.provenance_event_id.map(|id| id.to_string()))
    .bind(record.attempt_count as i32)
    .bind(&record.last_error)
    .bind(record.updated_at)
    .bind(raw_json)
    .bind(clear_lease)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}
