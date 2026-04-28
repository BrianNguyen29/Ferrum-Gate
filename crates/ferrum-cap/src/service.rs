use async_trait::async_trait;
use chrono::{Duration, Utc};
use ferrum_proto::{
    CapabilityId, CapabilityLease, CapabilityMintRequest, CapabilityMintResponse, CapabilityStatus,
    PolicyBundleId,
};
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum CapabilityError {
    #[error("capability not found")]
    NotFound,
    #[error("capability already used")]
    AlreadyUsed,
    #[error("capability revoked")]
    Revoked,
    #[error("capability expired")]
    Expired,
    #[error("requested ttl exceeds max allowed")]
    TtlTooLong,
}

#[async_trait]
pub trait CapabilityService: Send + Sync {
    async fn mint(
        &self,
        request: CapabilityMintRequest,
    ) -> Result<CapabilityMintResponse, CapabilityError>;
    async fn get(&self, capability_id: CapabilityId) -> Result<CapabilityLease, CapabilityError>;
    async fn mark_used(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityLease, CapabilityError>;
    async fn revoke(&self, capability_id: CapabilityId)
    -> Result<CapabilityLease, CapabilityError>;
}

#[derive(Default)]
pub struct InMemoryCapabilityService {
    inner: Arc<RwLock<HashMap<CapabilityId, CapabilityLease>>>,
}

#[async_trait]
impl CapabilityService for InMemoryCapabilityService {
    async fn mint(
        &self,
        request: CapabilityMintRequest,
    ) -> Result<CapabilityMintResponse, CapabilityError> {
        if request.requested_ttl_secs > 300 {
            return Err(CapabilityError::TtlTooLong);
        }

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
            policy_bundle_id: PolicyBundleId::new(),
            tool_manifest_id: None,
            manifest_hash: None,
            status: CapabilityStatus::Active,
            issued_at: now,
            expires_at: now + Duration::seconds(request.requested_ttl_secs as i64),
            revoked_at: None,
            metadata: request.metadata,
        };

        self.inner
            .write()
            .await
            .insert(lease.capability_id, lease.clone());

        Ok(CapabilityMintResponse {
            lease,
            warnings: Vec::new(),
        })
    }

    async fn get(&self, capability_id: CapabilityId) -> Result<CapabilityLease, CapabilityError> {
        let maybe = self.inner.read().await.get(&capability_id).cloned();
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
        let mut guard = self.inner.write().await;
        let lease = guard
            .get_mut(&capability_id)
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

        lease.status = CapabilityStatus::Used;
        Ok(lease.clone())
    }

    async fn revoke(
        &self,
        capability_id: CapabilityId,
    ) -> Result<CapabilityLease, CapabilityError> {
        let mut guard = self.inner.write().await;
        let lease = guard
            .get_mut(&capability_id)
            .ok_or(CapabilityError::NotFound)?;
        lease.status = CapabilityStatus::Revoked;
        lease.revoked_at = Some(Utc::now());
        Ok(lease.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mint_request(ttl_secs: u64) -> CapabilityMintRequest {
        CapabilityMintRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test-server".to_string(),
                tool_name: "test-tool".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: ttl_secs,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    #[tokio::test]
    async fn test_ttl_301_rejected() {
        let service = InMemoryCapabilityService::default();
        let request = make_mint_request(301);
        let result = service.mint(request).await;
        assert!(
            matches!(result, Err(CapabilityError::TtlTooLong)),
            "TTL=301 should be rejected with TtlTooLong, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_ttl_300_accepted() {
        let service = InMemoryCapabilityService::default();
        let request = make_mint_request(300);
        let result = service.mint(request).await;
        assert!(
            result.is_ok(),
            "TTL=300 should be accepted, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_ttl_0_accepted() {
        let service = InMemoryCapabilityService::default();
        let request = make_mint_request(0);
        let result = service.mint(request).await;
        assert!(
            result.is_ok(),
            "TTL=0 should be accepted (edge case), got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_ttl_302_rejected_boundary() {
        let service = InMemoryCapabilityService::default();
        let request = make_mint_request(302);
        let result = service.mint(request).await;
        assert!(
            matches!(result, Err(CapabilityError::TtlTooLong)),
            "TTL=302 should be rejected with TtlTooLong, got: {:?}",
            result
        );
    }
}
