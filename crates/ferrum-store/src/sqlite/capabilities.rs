use async_trait::async_trait;
use ferrum_proto::{CapabilityId, CapabilityLease, CapabilityStatus, IntentId};
use sqlx::SqlitePool;
use tokio::sync::oneshot;

use crate::sqlite::write_queue::WriteQueue;
use crate::{CapabilityRepo, Result, transitions};

use super::helpers::{enum_text, fetch_entities, fetch_entity_by_id, to_json};

#[derive(Clone)]
pub struct SqliteCapabilityRepo {
    pool: SqlitePool,
    write_queue: Option<WriteQueue>,
}

impl SqliteCapabilityRepo {
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
impl CapabilityRepo for SqliteCapabilityRepo {
    async fn insert(&self, capability: &CapabilityLease) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::InsertCapability {
                data: capability.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(capability)?;
        sqlx::query(
            "INSERT INTO capabilities (
                capability_id, intent_id, proposal_id, server_name, tool_name, status,
                issued_at, expires_at, revoked_at, raw_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(capability.capability_id.to_string())
        .bind(capability.intent_id.to_string())
        .bind(capability.proposal_id.to_string())
        .bind(&capability.tool_binding.server_name)
        .bind(&capability.tool_binding.tool_name)
        .bind(enum_text(&capability.status)?)
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .bind(capability.revoked_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, capability_id: CapabilityId) -> Result<Option<CapabilityLease>> {
        fetch_entity_by_id(
            &self.pool,
            "capabilities",
            "capability_id",
            &capability_id.to_string(),
        )
        .await
    }

    async fn update(&self, capability: &CapabilityLease) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateCapability {
                data: capability.clone(),
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let raw_json = to_json(capability)?;
        sqlx::query(
            "UPDATE capabilities
             SET status = ?2,
                 issued_at = ?3,
                 expires_at = ?4,
                 revoked_at = ?5,
                 raw_json = ?6
             WHERE capability_id = ?1",
        )
        .bind(capability.capability_id.to_string())
        .bind(enum_text(&capability.status)?)
        .bind(capability.issued_at)
        .bind(capability.expires_at)
        .bind(capability.revoked_at)
        .bind(raw_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_status(
        &self,
        capability_id: CapabilityId,
        status: CapabilityStatus,
    ) -> Result<()> {
        if let Some(ref queue) = self.write_queue {
            let (reply_tx, _) = oneshot::channel();
            let op = crate::sqlite::write_queue::WriteOp::UpdateCapabilityStatus {
                capability_id,
                status,
                reply: reply_tx,
            };
            return queue.send(op).await;
        }
        let Some(mut capability) = self.get(capability_id).await? else {
            return Ok(());
        };
        // Validate state transition
        if !transitions::is_valid_capability_transition(&capability.status, &status) {
            return Err(crate::StoreError::InvalidState(format!(
                "invalid capability transition from {:?} to {:?}",
                capability.status, status
            )));
        }
        capability.status = status;
        self.update(&capability).await
    }

    async fn update_status_if_active(
        &self,
        capability_id: CapabilityId,
        status: CapabilityStatus,
    ) -> Result<bool> {
        // Execute atomically directly on the pool; single-statement UPDATE is safe
        // to bypass the write queue because SQLite handles it atomically.
        let status_text = enum_text(&status)?;
        let active_text = enum_text(&CapabilityStatus::Active)?;
        let now = chrono::Utc::now();
        let result = sqlx::query(
            "UPDATE capabilities
             SET status = ?2,
                 raw_json = json_set(raw_json, '$.status', ?2)
             WHERE capability_id = ?1 AND status = ?3 AND expires_at > ?4",
        )
        .bind(capability_id.to_string())
        .bind(status_text)
        .bind(active_text)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn revoke_if_active(
        &self,
        capability_id: CapabilityId,
        revoked_at: ferrum_proto::Timestamp,
    ) -> Result<bool> {
        // Execute atomically directly on the pool; single-statement UPDATE is safe
        // to bypass the write queue because SQLite handles it atomically.
        let revoked_text = enum_text(&CapabilityStatus::Revoked)?;
        let active_text = enum_text(&CapabilityStatus::Active)?;
        let result = sqlx::query(
            "UPDATE capabilities
             SET status = ?2,
                 revoked_at = ?3,
                 raw_json = json_set(raw_json, '$.status', ?2, '$.revoked_at', ?4)
             WHERE capability_id = ?1 AND status = ?5 AND expires_at > ?3",
        )
        .bind(capability_id.to_string())
        .bind(revoked_text)
        .bind(revoked_at)
        .bind(revoked_at.to_rfc3339())
        .bind(active_text)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_by_intent(&self, intent_id: IntentId) -> Result<Vec<CapabilityLease>> {
        fetch_entities(
            &self.pool,
            "SELECT raw_json FROM capabilities WHERE intent_id = ?1 ORDER BY issued_at DESC",
            |query| query.bind(intent_id.to_string()),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityRepo, IntentRepo, ProposalRepo};
    use chrono::Utc;

    fn make_intent() -> ferrum_proto::IntentEnvelope {
        ferrum_proto::IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: ferrum_proto::PrincipalId::new(),
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
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(15),
        }
    }

    fn make_proposal(intent_id: ferrum_proto::IntentId) -> ferrum_proto::ActionProposal {
        ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
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
            created_at: Utc::now(),
        }
    }

    fn make_lease(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
        status: CapabilityStatus,
    ) -> CapabilityLease {
        let now = Utc::now();
        CapabilityLease {
            capability_id: CapabilityId::new(),
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
            status,
            issued_at: now,
            expires_at: now + chrono::Duration::seconds(300),
            revoked_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    async fn setup_store_with_lease(
        status: CapabilityStatus,
    ) -> (crate::SqliteStore, CapabilityLease) {
        let store = crate::SqliteStore::connect("sqlite::memory:")
            .await
            .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let intent = make_intent();
        let proposal = make_proposal(intent.intent_id);
        let lease = make_lease(intent.intent_id, proposal.proposal_id, status);

        store.intents().insert(&intent).await.unwrap();
        store.proposals().insert(&proposal).await.unwrap();
        store.capabilities().insert(&lease).await.unwrap();

        (store, lease)
    }

    #[tokio::test]
    async fn test_update_status_if_active_success() {
        let (store, lease) = setup_store_with_lease(CapabilityStatus::Active).await;
        let cap_id = lease.capability_id;

        let updated = store
            .capabilities()
            .update_status_if_active(cap_id, CapabilityStatus::Used)
            .await
            .unwrap();
        assert!(updated, "should return true when capability is Active");

        let fetched = store.capabilities().get(cap_id).await.unwrap().unwrap();
        assert!(matches!(fetched.status, CapabilityStatus::Used));
    }

    #[tokio::test]
    async fn test_update_status_if_active_fails_when_already_used() {
        let (store, lease) = setup_store_with_lease(CapabilityStatus::Active).await;
        let cap_id = lease.capability_id;

        // First transition to Used
        let updated1 = store
            .capabilities()
            .update_status_if_active(cap_id, CapabilityStatus::Used)
            .await
            .unwrap();
        assert!(updated1);

        // Second attempt should return false
        let updated2 = store
            .capabilities()
            .update_status_if_active(cap_id, CapabilityStatus::Used)
            .await
            .unwrap();
        assert!(
            !updated2,
            "should return false when capability is already Used"
        );
    }

    #[tokio::test]
    async fn test_update_status_if_active_fails_when_revoked() {
        let (store, lease) = setup_store_with_lease(CapabilityStatus::Revoked).await;
        let cap_id = lease.capability_id;

        let updated = store
            .capabilities()
            .update_status_if_active(cap_id, CapabilityStatus::Used)
            .await
            .unwrap();
        assert!(!updated, "should return false when capability is Revoked");
    }

    #[tokio::test]
    async fn test_update_status_if_active_fails_when_not_found() {
        let store = crate::SqliteStore::connect("sqlite::memory:")
            .await
            .unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let cap_id = CapabilityId::new();
        let updated = store
            .capabilities()
            .update_status_if_active(cap_id, CapabilityStatus::Used)
            .await
            .unwrap();
        assert!(
            !updated,
            "should return false when capability does not exist"
        );
    }
}
