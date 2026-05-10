use async_trait::async_trait;
use ferrum_proto::{ExecutionId, RollbackContract, RollbackContractId, RollbackState};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{Result, RollbackRepo};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteRollbackRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteRollbackRepo {
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
impl RollbackRepo for SqliteRollbackRepo {
    async fn insert(&self, contract: &RollbackContract) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertRollback {
                data: contract.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(contract)?;
        sqlx::query(
            "INSERT INTO rollback_contracts (
                contract_id, intent_id, proposal_id, execution_id, adapter_key,
                action_type, rollback_class, state, auto_commit, created_at, expires_at, raw_json
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
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, contract_id: RollbackContractId) -> Result<Option<RollbackContract>> {
        fetch_entity_by_id(
            &self.pool,
            "rollback_contracts",
            "contract_id",
            &contract_id.to_string(),
        )
        .await
    }

    async fn update(&self, contract: &RollbackContract) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateRollback {
                data: contract.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(contract)?;
        sqlx::query(
            "UPDATE rollback_contracts
             SET state = ?2,
                 auto_commit = ?3,
                 expires_at = ?4,
                 raw_json = ?5
             WHERE contract_id = ?1",
        )
        .bind(contract.contract_id.to_string())
        .bind(enum_text(&contract.state)?)
        .bind(contract.auto_commit)
        .bind(contract.expires_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_state(
        &self,
        contract_id: RollbackContractId,
        state: RollbackState,
    ) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateRollbackState {
                contract_id,
                state,
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let Some(mut contract) = self.get(contract_id).await? else {
            return Ok(());
        };
        contract.state = state;
        self.update(&contract).await
    }

    async fn list_by_execution(&self, execution_id: ExecutionId) -> Result<Vec<RollbackContract>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM rollback_contracts WHERE execution_id = ?1 ORDER BY created_at DESC",
            |query| query.bind(execution_id.to_string()),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{ActionType, ProposalId, RollbackClass, RollbackState, RollbackTarget};

    fn create_test_metadata() -> ferrum_proto::JsonMap {
        // Simulate fs adapter metadata that would come from prepare()
        let mut metadata = ferrum_proto::JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String("ferrum-adapter-fs".to_string()),
        );
        metadata.insert(
            "prepared_at".to_string(),
            serde_json::Value::String("2024-01-01T00:00:00Z".to_string()),
        );
        metadata.insert(
            "snapshot_path".to_string(),
            serde_json::Value::String("/tmp/ferrum-fs-snapshots/exec-123/path-hash".to_string()),
        );
        metadata.insert(
            "original_path".to_string(),
            serde_json::Value::String("/tmp/test.txt".to_string()),
        );
        metadata.insert(
            "bytes_written".to_string(),
            serde_json::Value::Number(13.into()),
        );
        metadata
    }

    fn create_test_contract(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
        execution_id: ExecutionId,
    ) -> RollbackContract {
        RollbackContract {
            contract_id: RollbackContractId::new(),
            intent_id,
            proposal_id,
            execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "fs".to_string(),
            target: RollbackTarget::FilePath {
                path: "/tmp/test.txt".to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: create_test_metadata(),
        }
    }

    /// Helper to insert required parent records for rollback_contract foreign keys
    /// Uses raw SQL to avoid proto struct mismatches in test code
    async fn insert_parent_records_via_sql(
        pool: &sqlx::SqlitePool,
        intent_id: &str,
        proposal_id: &str,
        execution_id: &str,
        capability_id: &str,
    ) -> crate::Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        // Insert intent
        sqlx::query(
            "INSERT INTO intents (intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode, default_rollback_class, created_at, expires_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        )
        .bind(intent_id)
        .bind("test-principal")
        .bind("test goal")
        .bind("Active")
        .bind("Low")
        .bind("Auto")
        .bind("R1SnapshotRecoverable")
        .bind(&now)
        .bind(&now)
        .bind(r#"{}"#)
        .execute(pool)
        .await?;

        // Insert proposal
        sqlx::query(
            "INSERT INTO proposals (proposal_id, intent_id, step_index, server_name, tool_name, estimated_risk, requested_rollback_class, created_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )
        .bind(proposal_id)
        .bind(intent_id)
        .bind(0)
        .bind("test-server")
        .bind("test-tool")
        .bind("Low")
        .bind("R1SnapshotRecoverable")
        .bind(&now)
        .bind(r#"{}"#)
        .execute(pool)
        .await?;

        // Insert capability
        sqlx::query(
            "INSERT INTO capabilities (capability_id, intent_id, proposal_id, server_name, tool_name, status, issued_at, expires_at, revoked_at, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        )
        .bind(capability_id)
        .bind(intent_id)
        .bind(proposal_id)
        .bind("test-server")
        .bind("test-tool")
        .bind("Granted")
        .bind(&now)
        .bind(&now)
        .bind(Option::<String>::None)
        .bind(r#"{}"#)
        .execute(pool)
        .await?;

        // Insert execution
        sqlx::query(
            "INSERT INTO executions (execution_id, intent_id, proposal_id, capability_id, rollback_contract_id, decision, state, started_at, finished_at, result_digest, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        )
        .bind(execution_id)
        .bind(intent_id)
        .bind(proposal_id)
        .bind(capability_id)
        .bind(Option::<String>::None)
        .bind("Approved")
        .bind("Completed")
        .bind(&now)
        .bind(&now)
        .bind(Option::<String>::None)
        .bind(r#"{}"#)
        .execute(pool)
        .await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_rollback_contract_metadata_round_trips_through_store() {
        // This test proves Q2.2/Q2.3 foundation: FileWrite (fs-prepare) metadata
        // round-trips through the SqliteRollbackRepo.
        use crate::sqlite::SqliteStore;

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ProposalId::new();
        let execution_id = ExecutionId::new();
        let capability_id = ferrum_proto::CapabilityId::new();

        insert_parent_records_via_sql(
            store.pool(),
            &intent_id.to_string(),
            &proposal_id.to_string(),
            &execution_id.to_string(),
            &capability_id.to_string(),
        )
        .await
        .unwrap();

        let repo = store.rollback_contracts();

        // Create a contract with fs adapter metadata
        let original = create_test_contract(intent_id, proposal_id, execution_id);
        let contract_id = original.contract_id;

        // Verify metadata fields are set before insert
        assert_eq!(
            original
                .metadata
                .get("adapter_kind")
                .and_then(|v| v.as_str()),
            Some("ferrum-adapter-fs")
        );
        assert!(
            original.metadata.get("snapshot_path").is_some(),
            "snapshot_path should be present"
        );
        assert!(
            original.metadata.get("original_path").is_some(),
            "original_path should be present"
        );

        // Insert the contract
        repo.insert(&original).await.unwrap();

        // Retrieve the contract
        let retrieved = repo.get(contract_id).await.unwrap();

        assert!(retrieved.is_some(), "retrieved contract should be present");
        let contract = retrieved.unwrap();

        // Verify metadata round-tripped correctly
        assert_eq!(
            contract
                .metadata
                .get("adapter_kind")
                .and_then(|v| v.as_str()),
            Some("ferrum-adapter-fs")
        );
        assert_eq!(
            contract
                .metadata
                .get("snapshot_path")
                .and_then(|v| v.as_str()),
            Some("/tmp/ferrum-fs-snapshots/exec-123/path-hash")
        );
        assert_eq!(
            contract
                .metadata
                .get("original_path")
                .and_then(|v| v.as_str()),
            Some("/tmp/test.txt")
        );
        assert_eq!(
            contract
                .metadata
                .get("bytes_written")
                .and_then(|v| v.as_i64()),
            Some(13)
        );
    }

    #[tokio::test]
    async fn test_rollback_contract_update_preserves_metadata() {
        use crate::sqlite::SqliteStore;

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ProposalId::new();
        let execution_id = ExecutionId::new();
        let capability_id = ferrum_proto::CapabilityId::new();

        insert_parent_records_via_sql(
            store.pool(),
            &intent_id.to_string(),
            &proposal_id.to_string(),
            &execution_id.to_string(),
            &capability_id.to_string(),
        )
        .await
        .unwrap();

        let repo = store.rollback_contracts();

        let mut contract = create_test_contract(intent_id, proposal_id, execution_id);
        let contract_id = contract.contract_id;

        // Insert
        repo.insert(&contract).await.unwrap();

        // Update the contract state via update() which also updates raw_json
        contract.state = RollbackState::ExecutedAwaitingVerify;
        repo.update(&contract).await.unwrap();

        // Retrieve and verify metadata still intact
        let retrieved = repo.get(contract_id).await.unwrap().unwrap();
        assert_eq!(retrieved.state, RollbackState::ExecutedAwaitingVerify);
        assert_eq!(
            retrieved
                .metadata
                .get("snapshot_path")
                .and_then(|v| v.as_str()),
            Some("/tmp/ferrum-fs-snapshots/exec-123/path-hash")
        );
    }

    #[tokio::test]
    async fn test_list_by_execution_returns_metadata_intact() {
        use crate::sqlite::SqliteStore;

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ProposalId::new();
        let execution_id = ExecutionId::new();
        let capability_id = ferrum_proto::CapabilityId::new();

        insert_parent_records_via_sql(
            store.pool(),
            &intent_id.to_string(),
            &proposal_id.to_string(),
            &execution_id.to_string(),
            &capability_id.to_string(),
        )
        .await
        .unwrap();

        let repo = store.rollback_contracts();

        let contract = create_test_contract(intent_id, proposal_id, execution_id);
        let execution_id_for_query = contract.execution_id;

        repo.insert(&contract).await.unwrap();

        let contracts = repo
            .list_by_execution(execution_id_for_query)
            .await
            .unwrap();
        assert_eq!(contracts.len(), 1);

        let retrieved = &contracts[0];
        assert_eq!(
            retrieved
                .metadata
                .get("snapshot_path")
                .and_then(|v| v.as_str()),
            Some("/tmp/ferrum-fs-snapshots/exec-123/path-hash")
        );
    }

    #[tokio::test]
    async fn test_update_state_keeps_raw_json_consistent() {
        use crate::sqlite::SqliteStore;

        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ProposalId::new();
        let execution_id = ExecutionId::new();
        let capability_id = ferrum_proto::CapabilityId::new();

        insert_parent_records_via_sql(
            store.pool(),
            &intent_id.to_string(),
            &proposal_id.to_string(),
            &execution_id.to_string(),
            &capability_id.to_string(),
        )
        .await
        .unwrap();

        let repo = store.rollback_contracts();

        let contract = create_test_contract(intent_id, proposal_id, execution_id);
        let contract_id = contract.contract_id;
        repo.insert(&contract).await.unwrap();

        // Update state via field-only update
        repo.update_state(contract_id, RollbackState::ExecutedAwaitingVerify)
            .await
            .unwrap();

        // get() deserializes from raw_json; if raw_json is stale, state will be wrong
        let retrieved = repo.get(contract_id).await.unwrap().unwrap();
        assert_eq!(retrieved.state, RollbackState::ExecutedAwaitingVerify);
    }
}
