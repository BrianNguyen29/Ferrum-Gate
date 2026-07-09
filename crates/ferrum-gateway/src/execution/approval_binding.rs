use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use ferrum_proto::{
    ApprovalBinding, ApprovalState, ProposalId, ProvenanceEventKind, ProvenanceQueryRequest,
};
use ferrum_store::StoreFacade;

use crate::problem::ApiProblem;

/// Validates the approval binding digest per I6 invariant.
///
/// Checks when `approval_binding=Some`:
/// 1. Approval exists (404 -> 403 IntegrityMismatch)
/// 2. Approval state is Granted (403 PolicyDenied)
/// 3. Binding not expired (403 PolicyDenied)
/// 4. Approval not expired (403 PolicyDenied)
/// 5. Binding digest matches approval digest (403 IntegrityMismatch)
/// 6. Computed proposal digest matches binding digest (403 IntegrityMismatch)
///
/// Skips all checks when `approval_binding=None` (backward compatible).
pub(crate) async fn validate_approval_binding_digest(
    store: &Arc<dyn StoreFacade>,
    binding: &ApprovalBinding,
    proposal_id: ProposalId,
) -> Result<(), ApiProblem> {
    // Step 1: Fetch the approval by ID
    let approval = store
        .approvals()
        .get(binding.approval_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ferrum_proto::ApiErrorCode::IntegrityMismatch,
                "approval not found for binding",
            )
        })?;

    // Step 2: Check approval state is Granted
    if !matches!(approval.state, ApprovalState::Granted) {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ferrum_proto::ApiErrorCode::PolicyDenied,
            format!("approval state is {:?}, expected Granted", approval.state),
        ));
    }

    // Step 3: Check binding not expired
    if binding.expires_at < Utc::now() {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ferrum_proto::ApiErrorCode::PolicyDenied,
            "approval binding has expired",
        ));
    }

    // Step 4: Check approval not expired
    if approval.expires_at < Utc::now() {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ferrum_proto::ApiErrorCode::PolicyDenied,
            "approval has expired",
        ));
    }

    // Step 5: Check binding digest matches approval digest
    if binding.approved_action_digest != approval.action_digest {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ferrum_proto::ApiErrorCode::IntegrityMismatch,
            "binding digest does not match approval digest",
        ));
    }

    // Step 6: Fetch proposal and verify computed digest matches binding digest
    let proposal = store
        .proposals()
        .get(proposal_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ferrum_proto::ApiErrorCode::IntegrityMismatch,
                "proposal not found",
            )
        })?;

    let computed_digest = proposal.canonical_action_digest();
    if computed_digest != binding.approved_action_digest {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ferrum_proto::ApiErrorCode::IntegrityMismatch,
            "computed proposal digest does not match binding digest",
        ));
    }

    if !binding.approver_roles.is_empty() {
        let grant_events = store
            .provenance()
            .query(&ProvenanceQueryRequest {
                intent_id: Some(approval.intent_id),
                execution_id: approval.execution_id,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::ApprovalGranted),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;
        let approved_by_allowed_role = grant_events.iter().any(|event| {
            event.proposal_id == Some(approval.proposal_id)
                && event
                    .metadata
                    .get("approval_id")
                    .and_then(|value| value.as_str())
                    == Some(binding.approval_id.to_string().as_str())
                && event
                    .metadata
                    .get("actor_role")
                    .and_then(|value| value.as_str())
                    .is_some_and(|role| {
                        binding.approver_roles.iter().any(|allowed| allowed == role)
                    })
        }) || {
            let requested_by_role =
                format!("{:?}", approval.requested_by.actor_type).to_ascii_lowercase();
            binding
                .approver_roles
                .iter()
                .any(|allowed| allowed == &requested_by_role)
        };
        if !approved_by_allowed_role {
            return Err(ApiProblem::new(
                StatusCode::FORBIDDEN,
                ferrum_proto::ApiErrorCode::PolicyDenied,
                "approval was not granted by an allowed approver role",
            ));
        }
    }

    Ok(())
}
