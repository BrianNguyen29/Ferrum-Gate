use crate::{
    ActorRef, CapabilityId, EventId, ExecutionId, HashChainRef, JsonMap, ObjectRef, PolicyBundleId,
    ProposalId, RollbackContractId, SensitivityLabel, Timestamp, TrustLabel,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceEvent {
    pub event_id: EventId,
    pub kind: ProvenanceEventKind,
    pub occurred_at: Timestamp,
    pub actor: ActorRef,
    pub object: ObjectRef,
    pub intent_id: Option<crate::IntentId>,
    pub proposal_id: Option<ProposalId>,
    pub execution_id: Option<ExecutionId>,
    pub capability_id: Option<CapabilityId>,
    pub rollback_contract_id: Option<RollbackContractId>,
    pub policy_bundle_id: Option<PolicyBundleId>,
    pub trust_labels: Vec<TrustLabel>,
    pub sensitivity_labels: Vec<SensitivityLabel>,
    pub parent_edges: Vec<ProvenanceEdge>,
    pub hash_chain: HashChainRef,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ProvenanceEventKind {
    UserGoalReceived,
    IntentCompiled,
    IntentRevoked,
    ActionProposalSubmitted,
    PolicyEvaluated,
    CapabilityMinted,
    CapabilityRevoked,
    ApprovalRequested,
    ApprovalGranted,
    ApprovalDenied,
    ToolCallPrepared,
    ToolCallIntercepted,
    ToolCallExecuted,
    ToolOutputReceived,
    ToolOutputSanitized,
    DlpBlocked,
    SideEffectPrepared,
    SideEffectVerified,
    SideEffectCommitted,
    SideEffectCompensated,
    SideEffectRolledBack,
    Quarantined,
    ErrorRaised,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceEdge {
    pub edge_type: ProvenanceEdgeType,
    pub from_event_id: EventId,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ProvenanceEdgeType {
    DerivedFrom,
    AuthorizedBy,
    ApprovedBy,
    TaintedBy,
    UsesManifest,
    EvaluatedByPolicy,
    Caused,
    Compensates,
    Verifies,
    References,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceQueryRequest {
    pub intent_id: Option<crate::IntentId>,
    pub proposal_id: Option<ProposalId>,
    pub execution_id: Option<ExecutionId>,
    pub capability_id: Option<CapabilityId>,
    pub event_kind: Option<ProvenanceEventKind>,
    pub terminal_only: Option<bool>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceQueryResponse {
    pub events: Vec<ProvenanceEvent>,
}

/// Response for a single provenance event lookup.
/// Contains the event and optional ancestry/descendants when requested.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceEventResponse {
    pub event: ProvenanceEvent,
    /// Ancestor events reachable by walking backwards via parent_edges (when ?ancestry=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestry: Option<Vec<ProvenanceEvent>>,
    /// Descendant events reachable by walking forwards via child_edges (when ?descendants=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descendants: Option<Vec<ProvenanceEvent>>,
}
