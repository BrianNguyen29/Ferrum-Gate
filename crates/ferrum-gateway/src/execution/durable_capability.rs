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
