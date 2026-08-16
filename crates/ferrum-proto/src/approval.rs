use crate::{ActorRef, ApprovalId, ExecutionId, MfaFactor, ProposalId, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Resolution-evidence format version stamped by `ApprovalRepo::resolve` when
/// an approval is resolved by the hardened resolver (P0.5). Persisted on the
/// approval record so I6 role-bound binding can distinguish new-code
/// resolutions (which must carry authenticated resolver evidence) from
/// pre-hardening historical records.
pub const CURRENT_RESOLVER_EVIDENCE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRequest {
    pub approval_id: ApprovalId,
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub execution_id: Option<ExecutionId>,
    pub requested_by: ActorRef,
    pub reason: String,
    pub action_digest: String,
    pub expires_at: Timestamp,
    pub state: ApprovalState,
    pub created_at: Timestamp,
    /// Durable resolution-evidence version stamped atomically by
    /// `ApprovalRepo::resolve` when the approval is granted/denied by the
    /// hardened resolver. `None` — including absence on deserialize — marks a
    /// pre-hardening historical record that may use the legacy `requested_by`
    /// role fallback during I6 role-bound binding. `Some(_)` marks a new-code
    /// resolution that requires authenticated resolver evidence; absence of
    /// matching evidence fails closed. Backward compatible: existing rows lack
    /// the key and deserialize to `None`, and the marker is omitted from
    /// serialized JSON when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolver_evidence_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_actor_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ApprovalState {
    Pending,
    Granted,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalResolveRequest {
    pub actor: ActorRef,
    pub approve: bool,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa_factor: Option<MfaFactor>,
}
