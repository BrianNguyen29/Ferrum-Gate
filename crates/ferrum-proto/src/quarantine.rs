use crate::{ActorRef, JsonMap, ProposalId, QuarantineHoldId, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuarantineHold {
    pub hold_id: QuarantineHoldId,
    pub intent_id: crate::IntentId,
    pub proposal_id: ProposalId,
    pub reason: String,
    pub matched_rule_ids: Vec<String>,
    pub policy_bundle_id: Option<String>,
    pub state: QuarantineHoldState,
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
    pub resolved_at: Option<Timestamp>,
    pub resolved_by: Option<ActorRef>,
    pub resolution_reason: Option<String>,
    pub metadata: JsonMap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_actor_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum QuarantineHoldState {
    Pending,
    Allowed,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuarantineResolveRequest {
    pub actor: ActorRef,
    pub allow: bool,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa_factor: Option<crate::MfaFactor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuarantineListEnvelope {
    pub items: Vec<QuarantineHold>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
