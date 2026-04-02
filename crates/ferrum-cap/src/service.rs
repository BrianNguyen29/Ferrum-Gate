use async_trait::async_trait;
use chrono::{Duration, Utc};
use ferrum_proto::{
    CapabilityId, CapabilityLease, CapabilityMintRequest, CapabilityMintResponse, CapabilityStatus,
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
    #[error("capability persistence failure")]
    Internal,
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

        // U1-S9a: Use provided policy_bundle_id if given, otherwise generate a random one.
        // The provided ID is derived deterministically from the intent's outcome contracts.
        let policy_bundle_id = request.policy_bundle_id.unwrap_or_default();

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
