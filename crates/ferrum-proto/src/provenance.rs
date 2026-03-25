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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
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
    /// Observed externally-derived event that has been ingested into the provenance lineage.
    /// Used by the external runtime event ingest boundary to anchor external observations
    /// into an existing execution lineage without granting the external system any agency.
    ExternalEventObserved,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceEdge {
    pub edge_type: ProvenanceEdgeType,
    pub from_event_id: EventId,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
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
    /// Indicates that this event was observed by an external runtime or system.
    /// Used by the external event ingest boundary to link external observations
    /// into the internal provenance lineage.
    ObservedBy,
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
    /// Maximum number of events to return (1-1000). Defaults to 100.
    #[serde(default = "default_query_limit")]
    pub limit: Option<u32>,
    /// Cursor for keyset pagination. Use the returned next_cursor to advance.
    /// The cursor encodes the (occurred_at, event_id) of the last item in the previous page.
    #[serde(default)]
    pub cursor: Option<String>,
}

impl Default for ProvenanceQueryRequest {
    fn default() -> Self {
        Self {
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            event_kind: None,
            terminal_only: None,
            since: None,
            until: None,
            limit: Some(100),
            cursor: None,
        }
    }
}

fn default_query_limit() -> Option<u32> {
    Some(100)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceQueryResponse {
    pub events: Vec<ProvenanceEvent>,
    /// Cursor for the next page. None if this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
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

/// Request to ingest an externally-observed runtime event into the provenance lineage.
/// Strict shape: #[serde(deny_unknown_fields)] prevents callers from injecting arbitrary fields.
/// The server derives internal lineage context from existing execution/event state
/// rather than trusting caller-supplied linkage intent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExternalEventIngestRequest {
    /// The execution_id that this external event is being anchored to.
    /// Must refer to an existing execution record.
    pub execution_id: ExecutionId,
    /// The parent event within the same execution that this external event observes.
    /// Must refer to an existing provenance event belonging to execution_id.
    pub parent_event_id: EventId,
    /// Identifier for the external system or runtime that observed this event.
    /// Vendor-neutral: use a descriptive string, not a vendor-specific enum.
    pub source_system: String,
    /// The event identifier assigned by the external source system.
    /// Allows correlation back to the external runtime's event log.
    pub source_event_id: String,
    /// Optional wall-clock time when the external system observed the event.
    /// If omitted, the server uses its own current time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<Timestamp>,
    /// Optional human-readable summary describing what was observed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Optional digest of the external event payload, for integrity verification.
    /// Format is opaque to the server; caller is responsible for consistent encoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_digest: Option<String>,
    /// Optional lightweight metadata bag. Small/simple values only; not raw payload blobs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
}

/// Response after successfully ingesting an external runtime event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExternalEventIngestResponse {
    /// The newly created provenance event that anchors the external observation.
    pub event: ProvenanceEvent,
}
