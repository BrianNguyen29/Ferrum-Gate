use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Utc;
use ferrum_proto::{
    ApprovalBinding, ApprovalState, ProposalId, ProvenanceEvent, ProvenanceEventKind,
    ProvenanceQueryRequest,
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
/// 7. When `approver_roles` is non-empty, an `ApprovalGranted` provenance event
///    for this approval carries authenticated resolver evidence
///    (`actor_authenticated=true` + matching `actor_role`). The durable
///    `resolver_evidence_version` marker (stamped atomically by
///    `ApprovalRepo::resolve`) decides strictness:
///    - Marker PRESENT (new-code resolution): only authenticated new-format
///      resolver evidence satisfies; absence of matching evidence fails closed
///      (403 PolicyDenied). A resolved-but-no-event grant can never fall back.
///    - Marker ABSENT (pre-hardening historical record): the legacy
///      `requested_by` fallback applies only when no new-format
///      resolver-evidence event exists for the approval.
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

        let approval_id_str = binding.approval_id.to_string();
        // Events relevant to this approval: new-format events carry
        // metadata.approval_id; both formats set object_id to the approval id.
        let relevant_events: Vec<&ProvenanceEvent> = grant_events
            .iter()
            .filter(|event| {
                event.proposal_id == Some(approval.proposal_id)
                    && (event
                        .metadata
                        .get("approval_id")
                        .and_then(|value| value.as_str())
                        == Some(approval_id_str.as_str())
                        || event.object.object_id == approval_id_str)
            })
            .collect();

        let role_matches = |event: &ProvenanceEvent| {
            event
                .metadata
                .get("actor_role")
                .and_then(|value| value.as_str())
                .is_some_and(|role| binding.approver_roles.iter().any(|allowed| allowed == role))
        };

        // New-format events carry the actor_authenticated marker. When any such
        // event exists for this approval, only authenticated resolver evidence
        // can satisfy the role binding: events marked actor_authenticated=false
        // (Bearer/Disabled request-body actor) never satisfy, and the
        // requested_by fallback does not apply.
        let has_new_format_event = relevant_events
            .iter()
            .any(|event| event.metadata.contains_key("actor_authenticated"));

        // The durable resolver-evidence marker is authoritative. When present,
        // the approval was resolved by the hardened resolver, so ONLY
        // authenticated new-format resolver evidence can satisfy the binding —
        // a resolved-but-no-event grant (provenance append failed after the
        // state committed) fails closed here instead of falling back. When the
        // marker is absent, the record predates the hardening, so the legacy
        // fallback is preserved for genuinely historical approvals.
        let requires_resolver_evidence = approval.resolver_evidence_version.is_some();

        let approved_by_allowed_role = if requires_resolver_evidence || has_new_format_event {
            relevant_events.iter().any(|event| {
                event
                    .metadata
                    .get("actor_authenticated")
                    .and_then(|value| value.as_bool())
                    == Some(true)
                    && role_matches(event)
            })
        } else {
            // Historical carve-out: approvals granted before resolver-evidence
            // metadata existed (or granted out-of-band with no provenance
            // event) keep the pre-hardening behavior: old-format actor_role
            // evidence, else the requested_by actor_type fallback.
            relevant_events.iter().any(|event| role_matches(event)) || {
                let requested_by_role =
                    format!("{:?}", approval.requested_by.actor_type).to_ascii_lowercase();
                binding
                    .approver_roles
                    .iter()
                    .any(|allowed| allowed == &requested_by_role)
            }
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

#[cfg(test)]
#[path = "approval_binding_tests.rs"]
mod tests;
