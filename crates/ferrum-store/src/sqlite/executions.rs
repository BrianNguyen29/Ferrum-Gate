use async_trait::async_trait;
use ferrum_proto::{CapabilityId, ExecutionId, ExecutionRecord, ExecutionState, IntentId};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{ExecutionRepo, Result, transitions};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteExecutionRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteExecutionRepo {
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
impl ExecutionRepo for SqliteExecutionRepo {
    async fn insert(&self, execution: &ExecutionRecord) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertExecution {
                data: execution.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(execution)?;
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
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, execution_id: ExecutionId) -> Result<Option<ExecutionRecord>> {
        fetch_entity_by_id(
            &self.pool,
            "executions",
            "execution_id",
            &execution_id.to_string(),
        )
        .await
    }

    async fn update(&self, execution: &ExecutionRecord) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateExecution {
                data: execution.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(execution)?;
        sqlx::query(
            "UPDATE executions
             SET rollback_contract_id = ?2,
                 decision = ?3,
                 state = ?4,
                 finished_at = ?5,
                 result_digest = ?6,
                 raw_json = ?7
             WHERE execution_id = ?1",
        )
        .bind(execution.execution_id.to_string())
        .bind(execution.rollback_contract_id.map(|id| id.to_string()))
        .bind(enum_text(&execution.decision)?)
        .bind(enum_text(&execution.state)?)
        .bind(execution.finished_at)
        .bind(&execution.result_digest)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_state(&self, execution_id: ExecutionId, state: ExecutionState) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateExecutionState {
                execution_id,
                state,
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let Some(mut execution) = self.get(execution_id).await? else {
            return Ok(());
        };
        // Validate state transition - block transitions out of terminal states
        if !transitions::is_valid_execution_transition(&execution.state, &state) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid execution transition from {:?} to {:?}",
                execution.state, state
            )));
        }
        execution.state = state;
        self.update(&execution).await
    }

    async fn compare_and_set_state(
        &self,
        execution_id: ExecutionId,
        expected_states: &[ExecutionState],
        new_state: ExecutionState,
    ) -> Result<bool> {
        if expected_states.is_empty() {
            return Ok(false);
        }
        let new_state_text = enum_text(&new_state)?;
        let expected = expected_states
            .iter()
            .map(enum_text)
            .collect::<Result<Vec<_>>>()?;
        let placeholders = std::iter::repeat_n("?", expected.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE executions
             SET state = ?2,
                 raw_json = json_set(raw_json, '$.state', ?2)
             WHERE execution_id = ?1 AND state IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql)
            .bind(execution_id.to_string())
            .bind(new_state_text);
        for state in expected {
            query = query.bind(state);
        }
        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected() == 1)
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<ExecutionRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM executions WHERE intent_id = ?1 ORDER BY started_at DESC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }

    async fn list_by_capability(
        &self,
        capability_id: CapabilityId,
    ) -> Result<Vec<ExecutionRecord>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM executions WHERE capability_id = ?1 ORDER BY started_at DESC",
            |query| query.bind(capability_id.to_string()),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo};
    use ferrum_proto::{
        ActionProposal, CapabilityLease, CapabilityStatus, Decision, ExecutionId, ExecutionRecord,
        ExecutionState, IntentEnvelope, PrincipalId, ProposalId,
    };

    fn create_test_intent() -> IntentEnvelope {
        IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30000,
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
            status: ferrum_proto::IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        }
    }

    fn create_test_proposal(intent_id: ferrum_proto::IntentId) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id,
            step_index: 0,
            title: "test".to_string(),
            tool_name: "test-tool".to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        }
    }

    fn create_test_capability(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
    ) -> CapabilityLease {
        CapabilityLease {
            capability_id: ferrum_proto::CapabilityId::new(),
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
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
            issued_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            revoked_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    fn create_test_execution(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
        capability_id: ferrum_proto::CapabilityId,
    ) -> ExecutionRecord {
        ExecutionRecord {
            execution_id: ExecutionId::new(),
            proposal_id,
            intent_id,
            capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Proposed,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    #[tokio::test]
    async fn test_update_state_keeps_raw_json_consistent() {
        use crate::sqlite::SqliteStore;

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let intent = create_test_intent();
        let intent_id = intent.intent_id;
        store.intents().insert(&intent).await.unwrap();

        let proposal = create_test_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store.proposals().insert(&proposal).await.unwrap();

        let capability = create_test_capability(intent_id, proposal_id);
        let capability_id = capability.capability_id;
        store.capabilities().insert(&capability).await.unwrap();

        let execution = create_test_execution(intent_id, proposal_id, capability_id);
        let execution_id = execution.execution_id;
        store.executions().insert(&execution).await.unwrap();

        // Update state via field-only update
        store
            .executions()
            .update_state(execution_id, ExecutionState::Running)
            .await
            .unwrap();

        // get() deserializes from raw_json; if raw_json is stale, state will be wrong
        let retrieved = store.executions().get(execution_id).await.unwrap().unwrap();
        assert_eq!(retrieved.state, ExecutionState::Running);
    }
}
