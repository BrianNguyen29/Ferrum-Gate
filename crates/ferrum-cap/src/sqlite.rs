use async_trait::async_trait;
use chrono::{Duration, Utc};
use ferrum_proto::{
    CapabilityId, CapabilityLease, CapabilityMintRequest, CapabilityMintResponse, CapabilityStatus,
    PolicyBundleId,
};
use std::sync::Arc;

use crate::CapabilityService;
use crate::service::CapabilityError;
use ferrum_store::CapabilityRepo;

#[derive(Clone)]
pub struct SqliteCapabilityService {
    repo: Arc<dyn CapabilityRepo>,
}

impl SqliteCapabilityService {
    pub fn new(repo: Arc<dyn CapabilityRepo>) -> Self {
        Self { repo }
    }

    fn map_store_err(err: ferrum_store::StoreError) -> CapabilityError {
        let _ = err;
        CapabilityError::Internal
    }
}

#[async_trait]
impl CapabilityService for SqliteCapabilityService {
    async fn mint(
        &self,
        request: CapabilityMintRequest,
    ) -> Result<CapabilityMintResponse, CapabilityError> {
        if request.requested_ttl_secs > 300 {
            return Err(CapabilityError::TtlTooLong);
        }

        // U1-S9a: Use provided policy_bundle_id if given, otherwise generate a random one.
        // The provided ID is derived deterministically from the intent's outcome contracts.
        let policy_bundle_id = request.policy_bundle_id.unwrap_or_else(PolicyBundleId::new);

        let now = Utc::now();
        let lease = CapabilityLease {
            capability_id: CapabilityId::new(),
            intent_id: request.intent_id,
            proposal_id: request.proposal_id,
            tool_binding: request.tool_binding,
            resource_bindings: request.resource_bindings,
            argument_constraints: request.argument_constraints,
            taint_budget: request.taint_budget,
            approval_binding: request.approval_binding,
            issued_by: "ferrum-cap".to_string(),
            policy_bundle_id,
            tool_manifest_id: None,
            manifest_hash: None,
            status: CapabilityStatus::Active,
            issued_at: now,
            expires_at: now + Duration::seconds(request.requested_ttl_secs as i64),
            revoked_at: None,
            metadata: request.metadata,
        };

        self.repo
            .insert(&lease)
            .await
            .map_err(Self::map_store_err)?;

        Ok(CapabilityMintResponse {
            lease,
            warnings: Vec::new(),
        })
    }

    async fn get(&self, capability_id: CapabilityId) -> Result<CapabilityLease, CapabilityError> {
        let maybe = self
            .repo
            .get(capability_id)
            .await
            .map_err(Self::map_store_err)?;
        let lease = maybe.ok_or(CapabilityError::NotFound)?;

        if matches!(lease.status, CapabilityStatus::Revoked) {
            return Err(CapabilityError::Revoked);
        }
        if lease.expires_at < Utc::now() {
            return Err(CapabilityError::Expired);
        }

        Ok(lease)
    }

    async fn mark_used(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityLease, CapabilityError> {
        let updated = self
            .repo
            .mark_used_if_active(capability_id)
            .await
            .map_err(Self::map_store_err)?;

        if updated {
            let lease = self
                .repo
                .get(capability_id)
                .await
                .map_err(Self::map_store_err)?
                .ok_or(CapabilityError::NotFound)?;
            return Ok(lease);
        }

        // fail-closed: determine which error applies
        let lease = self
            .repo
            .get(capability_id)
            .await
            .map_err(Self::map_store_err)?
            .ok_or(CapabilityError::NotFound)?;

        if matches!(lease.status, CapabilityStatus::Used) {
            return Err(CapabilityError::AlreadyUsed);
        }
        if matches!(lease.status, CapabilityStatus::Revoked) {
            return Err(CapabilityError::Revoked);
        }
        if lease.expires_at < Utc::now() {
            return Err(CapabilityError::Expired);
        }

        Err(CapabilityError::AlreadyUsed)
    }

    async fn revoke(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityLease, CapabilityError> {
        self.repo
            .revoke(capability_id)
            .await
            .map_err(Self::map_store_err)?;

        let lease = self
            .repo
            .get(capability_id)
            .await
            .map_err(Self::map_store_err)?
            .ok_or(CapabilityError::NotFound)?;

        Ok(lease)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use ferrum_proto::{CapabilityMintRequest, CapabilityStatus, TaintBudget, ToolBinding};
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::CapabilityService;
    use crate::sqlite::SqliteCapabilityService;
    use ferrum_store::{CapabilityRepo, IntentRepo, ProposalRepo, SqliteStore};

    async fn create_test_store() -> (TempDir, SqliteStore) {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let db_path = temp_dir.path().join("store.sqlite");
        std::fs::File::create(&db_path).expect("failed to create sqlite file");
        let database_url = format!("sqlite://{}", db_path.display());
        let store = SqliteStore::connect(&database_url)
            .await
            .expect("failed to connect to sqlite");
        store
            .apply_embedded_migrations()
            .await
            .expect("failed to apply migrations");
        (temp_dir, store)
    }

    fn make_mint_request(
        intent_id: ferrum_proto::IntentId,
        proposal_id: ferrum_proto::ProposalId,
        ttl_secs: u64,
    ) -> CapabilityMintRequest {
        CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ToolBinding {
                server_name: "workspace".to_string(),
                tool_name: "fs.read".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: TaintBudget {
                max_taint_score: 20,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: ttl_secs,
            policy_bundle_id: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    fn sample_intent() -> ferrum_proto::IntentEnvelope {
        ferrum_proto::IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "Test Intent".to_string(),
            goal: "Test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![ferrum_proto::OutcomeClause {
                id: "primary".to_string(),
                description: "test outcome".to_string(),
                effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
                required: true,
                selectors: None,
            }],
            forbidden_outcomes: Vec::new(),
            resource_scope: Vec::new(),
            risk_tier: ferrum_proto::RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: ferrum_proto::TrustContextSummary {
                input_labels: Vec::new(),
                sensitivity_labels: Vec::new(),
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: Vec::new(),
            tags: vec!["test".to_string()],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::minutes(15),
        }
    }

    fn sample_proposal(intent_id: ferrum_proto::IntentId) -> ferrum_proto::ActionProposal {
        ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id,
            step_index: 1,
            title: "Inspect state".to_string(),
            tool_name: "fs.read".to_string(),
            server_name: "workspace".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "read a file".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            decision: None,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn mint_and_get_durable() {
        let (_temp_dir, store) = create_test_store().await;

        // Insert prerequisite intent and proposal
        let intent = sample_intent();
        let intent_id = intent.intent_id;
        store
            .intents()
            .insert(&intent)
            .await
            .expect("insert intent");

        let proposal = sample_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store
            .proposals()
            .insert(&proposal)
            .await
            .expect("insert proposal");

        let repo = store.capabilities();
        let service = SqliteCapabilityService::new(Arc::new(repo));

        let request = make_mint_request(intent_id, proposal_id, 60);
        let response = service.mint(request).await.expect("mint should succeed");
        let capability_id = response.lease.capability_id;

        // get should succeed
        let loaded = service
            .get(capability_id)
            .await
            .expect("get should succeed");
        assert_eq!(loaded.capability_id, capability_id);
        assert!(matches!(loaded.status, CapabilityStatus::Active));
    }

    #[tokio::test]
    async fn used_capability_still_readable() {
        let (_temp_dir, store) = create_test_store().await;

        let intent = sample_intent();
        let intent_id = intent.intent_id;
        store
            .intents()
            .insert(&intent)
            .await
            .expect("insert intent");

        let proposal = sample_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store
            .proposals()
            .insert(&proposal)
            .await
            .expect("insert proposal");

        let repo = store.capabilities();
        let service = SqliteCapabilityService::new(Arc::new(repo));

        let request = make_mint_request(intent_id, proposal_id, 60);
        let response = service.mint(request).await.expect("mint should succeed");
        let capability_id = response.lease.capability_id;

        // mark_used
        service
            .mark_used(capability_id)
            .await
            .expect("mark_used should succeed");

        // get should still succeed (Used is readable, not fail-closed)
        let loaded = service
            .get(capability_id)
            .await
            .expect("get should still succeed for Used capability");
        assert!(matches!(loaded.status, CapabilityStatus::Used));
    }

    #[tokio::test]
    async fn double_mark_used_returns_already_used() {
        let (_temp_dir, store) = create_test_store().await;

        let intent = sample_intent();
        let intent_id = intent.intent_id;
        store
            .intents()
            .insert(&intent)
            .await
            .expect("insert intent");

        let proposal = sample_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store
            .proposals()
            .insert(&proposal)
            .await
            .expect("insert proposal");

        let repo = store.capabilities();
        let service = SqliteCapabilityService::new(Arc::new(repo));

        let request = make_mint_request(intent_id, proposal_id, 60);
        let response = service.mint(request).await.expect("mint should succeed");
        let capability_id = response.lease.capability_id;

        // first mark_used succeeds
        service
            .mark_used(capability_id)
            .await
            .expect("first mark_used should succeed");

        // second mark_used returns AlreadyUsed
        let err = service
            .mark_used(capability_id)
            .await
            .expect_err("second mark_used should fail");
        assert!(matches!(err, crate::CapabilityError::AlreadyUsed));
    }

    #[tokio::test]
    async fn revoke_returns_revoked() {
        let (_temp_dir, store) = create_test_store().await;

        let intent = sample_intent();
        let intent_id = intent.intent_id;
        store
            .intents()
            .insert(&intent)
            .await
            .expect("insert intent");

        let proposal = sample_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store
            .proposals()
            .insert(&proposal)
            .await
            .expect("insert proposal");

        let repo = store.capabilities();
        let service = SqliteCapabilityService::new(Arc::new(repo));

        let request = make_mint_request(intent_id, proposal_id, 60);
        let response = service.mint(request).await.expect("mint should succeed");
        let capability_id = response.lease.capability_id;

        // revoke
        let revoked = service
            .revoke(capability_id)
            .await
            .expect("revoke should succeed");
        assert!(matches!(revoked.status, CapabilityStatus::Revoked));
        assert!(revoked.revoked_at.is_some());

        // get should now return Revoked error
        let err = service
            .get(capability_id)
            .await
            .expect_err("get after revoke should fail");
        assert!(matches!(err, crate::CapabilityError::Revoked));
    }

    #[tokio::test]
    async fn expired_capability_returns_expired() {
        let (_temp_dir, store) = create_test_store().await;

        let intent = sample_intent();
        let intent_id = intent.intent_id;
        store
            .intents()
            .insert(&intent)
            .await
            .expect("insert intent");

        let proposal = sample_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store
            .proposals()
            .insert(&proposal)
            .await
            .expect("insert proposal");

        let repo = store.capabilities();
        let service = SqliteCapabilityService::new(Arc::new(repo));

        let request = make_mint_request(intent_id, proposal_id, 60);
        let response = service.mint(request).await.expect("mint should succeed");
        let capability_id = response.lease.capability_id;

        let mut expired_lease = store
            .capabilities()
            .get(capability_id)
            .await
            .expect("load capability from store")
            .expect("capability present");
        expired_lease.expires_at = Utc::now() - Duration::seconds(1);
        store
            .capabilities()
            .update(&expired_lease)
            .await
            .expect("persist expired capability");

        // get should return Expired
        let err = service
            .get(capability_id)
            .await
            .expect_err("get expired capability should fail");
        assert!(matches!(err, crate::CapabilityError::Expired));
    }

    #[tokio::test]
    async fn mint_rejects_ttl_over_300() {
        let (_temp_dir, store) = create_test_store().await;

        let intent = sample_intent();
        let intent_id = intent.intent_id;
        store
            .intents()
            .insert(&intent)
            .await
            .expect("insert intent");

        let proposal = sample_proposal(intent_id);
        let proposal_id = proposal.proposal_id;
        store
            .proposals()
            .insert(&proposal)
            .await
            .expect("insert proposal");

        let repo = store.capabilities();
        let service = SqliteCapabilityService::new(Arc::new(repo));

        let request = make_mint_request(intent_id, proposal_id, 301);
        let err = service
            .mint(request)
            .await
            .expect_err("mint with TTL > 300 should fail");
        assert!(matches!(err, crate::CapabilityError::TtlTooLong));
    }
}
