use std::sync::Arc;

use chrono::Utc;
use ferrum_cap::{CapabilityError, CapabilityService};
use ferrum_proto::{CapabilityId, CapabilityLease, CapabilityStatus};
use ferrum_store::StoreFacade;

/// Load capability from in-memory service, falling back to persisted store.
/// Returns NotFound if not found in either.
pub(crate) async fn get_capability_for_authorize(
    cap: &Arc<dyn CapabilityService>,
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> Result<CapabilityLease, CapabilityError> {
    // Try in-memory first
    match cap.get(capability_id).await {
        Ok(lease) => return Ok(lease),
        Err(CapabilityError::NotFound) => {}
        Err(e) => return Err(e),
    }

    // Fall back to persisted store
    let Some(lease) = store
        .capabilities()
        .get(capability_id)
        .await
        .map_err(|_e| CapabilityError::NotFound)?
    // Treat store errors as NotFound for authorize
    else {
        return Err(CapabilityError::NotFound);
    };

    // Validate persisted capability status
    if matches!(lease.status, CapabilityStatus::Used) {
        return Err(CapabilityError::AlreadyUsed);
    }
    if matches!(lease.status, CapabilityStatus::Revoked) {
        return Err(CapabilityError::Revoked);
    }
    if lease.expires_at < Utc::now() {
        return Err(CapabilityError::Expired);
    }

    Ok(lease)
}

/// Classify the capability state observed after a durable single-use CAS loss
/// during authorization, mapping it to the accurate [`CapabilityError`].
///
/// `now` is supplied by the caller so the gateway can use a single consistent
/// timestamp and tests can pin the boundary. A capability that still reads
/// `Active` and unexpired after a lost CAS was consumed by a concurrent
/// transition, so it falls back to `AlreadyUsed` (single-use denial).
pub(crate) fn classify_capability_after_cas_loss(
    lease: Option<&CapabilityLease>,
    now: chrono::DateTime<Utc>,
) -> CapabilityError {
    let Some(lease) = lease else {
        return CapabilityError::NotFound;
    };
    if matches!(lease.status, CapabilityStatus::Used) {
        return CapabilityError::AlreadyUsed;
    }
    if matches!(lease.status, CapabilityStatus::Revoked) {
        return CapabilityError::Revoked;
    }
    if lease.expires_at <= now {
        return CapabilityError::Expired;
    }
    CapabilityError::AlreadyUsed
}

/// Reload the capability after a failed `record_authorization` CAS and return
/// the accurate capability error for the gateway to surface. Store read errors
/// are treated as `NotFound`, consistent with [`get_capability_for_authorize`].
pub(crate) async fn classify_authorization_cas_failure(
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> CapabilityError {
    let lease = store.capabilities().get(capability_id).await.ok().flatten();
    classify_capability_after_cas_loss(lease.as_ref(), Utc::now())
}

/// Mark capability as used by winning an atomic durable transition first.
/// In-memory state is a cache and is synchronized only after the store accepts
/// the single-use Active -> Used transition.
#[allow(dead_code)]
pub(crate) async fn mark_capability_used_durable(
    cap: &Arc<dyn CapabilityService>,
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> Result<CapabilityLease, CapabilityError> {
    let Some(mut lease) = store.capabilities().get(capability_id).await.map_err(|e| {
        tracing::error!(error = %e, "failed to load capability from store for mark_used");
        CapabilityError::NotFound
    })?
    else {
        return Err(CapabilityError::NotFound);
    };

    if matches!(lease.status, CapabilityStatus::Used) {
        return Err(CapabilityError::AlreadyUsed);
    }
    if matches!(lease.status, CapabilityStatus::Revoked) {
        return Err(CapabilityError::Revoked);
    }
    if lease.expires_at < Utc::now() {
        return Err(CapabilityError::Expired);
    }

    let updated = store
        .capabilities()
        .update_status_if_active(capability_id, CapabilityStatus::Used)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to atomically update capability status");
            CapabilityError::NotFound
        })?;

    if !updated {
        let status = store
            .capabilities()
            .get(capability_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to reload capability after lost active update");
                CapabilityError::NotFound
            })?
            .map(|lease| lease.status);
        return match status {
            Some(CapabilityStatus::Used) => Err(CapabilityError::AlreadyUsed),
            Some(CapabilityStatus::Revoked) => Err(CapabilityError::Revoked),
            Some(_) | None => Err(CapabilityError::NotFound),
        };
    }

    if let Err(error) = cap.mark_used(capability_id).await {
        match error {
            CapabilityError::NotFound | CapabilityError::AlreadyUsed => {
                tracing::debug!(
                    ?error,
                    "capability store transition won; in-memory cache was absent or already used"
                );
            }
            other => {
                tracing::warn!(
                    error = ?other,
                    "capability store transition won but in-memory cache sync failed"
                );
            }
        }
    }

    lease.status = CapabilityStatus::Used;
    Ok(lease)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{CapabilityId, IntentId, PolicyBundleId, ProposalId};

    fn lease(status: CapabilityStatus, expires_in_secs: i64) -> CapabilityLease {
        let now = Utc::now();
        CapabilityLease {
            capability_id: CapabilityId::new(),
            intent_id: IntentId::new(),
            proposal_id: ProposalId::new(),
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "s".to_string(),
                tool_name: "t".to_string(),
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
            policy_bundle_id: PolicyBundleId::new(),
            tool_manifest_id: None,
            manifest_hash: None,
            status,
            issued_at: now,
            expires_at: now + chrono::Duration::seconds(expires_in_secs),
            revoked_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    #[test]
    fn cas_loss_maps_missing_to_not_found() {
        let err = classify_capability_after_cas_loss(None, Utc::now());
        assert!(matches!(err, CapabilityError::NotFound));
    }

    #[test]
    fn cas_loss_maps_used_to_already_used() {
        let cap = lease(CapabilityStatus::Used, 300);
        let err = classify_capability_after_cas_loss(Some(&cap), Utc::now());
        assert!(matches!(err, CapabilityError::AlreadyUsed));
    }

    #[test]
    fn cas_loss_maps_revoked_to_revoked() {
        let cap = lease(CapabilityStatus::Revoked, 300);
        let err = classify_capability_after_cas_loss(Some(&cap), Utc::now());
        assert!(matches!(err, CapabilityError::Revoked));
    }

    #[test]
    fn cas_loss_maps_active_expired_to_expired() {
        let cap = lease(CapabilityStatus::Active, -1);
        let err = classify_capability_after_cas_loss(Some(&cap), Utc::now());
        assert!(matches!(err, CapabilityError::Expired));
    }

    #[test]
    fn cas_loss_maps_active_unexpired_to_already_used() {
        // Concurrent transition won the row while the lease still reads Active.
        let cap = lease(CapabilityStatus::Active, 300);
        let err = classify_capability_after_cas_loss(Some(&cap), Utc::now());
        assert!(matches!(err, CapabilityError::AlreadyUsed));
    }
}
