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
    ExecutionCancelled,
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
    /// Additive filter: match events from any of the given execution IDs.
    /// When both execution_id and execution_ids are set, events matching either are returned.
    #[serde(default)]
    pub execution_ids: Vec<ExecutionId>,
    pub capability_id: Option<CapabilityId>,
    pub event_kind: Option<ProvenanceEventKind>,
    pub terminal_only: Option<bool>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    /// Maximum number of events to return (1-10000). Defaults to 100.
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
            execution_ids: Vec::new(),
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

/// Request for multi-hop provenance lineage traversal.
/// Uses BFS to walk ancestry (backwards via parent edges) and/or descendants (forwards via child edges).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineageQueryRequest {
    /// The execution_id to fence traversed events against.
    /// All events with execution_id=Some(x) must match this value.
    pub execution_id: ExecutionId,
    /// The seed event_id to start traversal from.
    pub event_id: EventId,
    /// When true, walk ancestry backwards via parent edges.
    #[serde(default = "default_true")]
    pub ancestry: bool,
    /// When true, walk descendants forwards via child edges.
    #[serde(default)]
    pub descendants: bool,
    /// Maximum hops for BFS traversal. Hard-capped at 32 by the server.
    #[serde(default = "default_max_hops")]
    pub max_hops: Option<u32>,
    /// Optional filter to restrict traversal to specific edge types only.
    /// When None, all edge types are included.
    #[serde(default)]
    pub edge_types: Option<Vec<ProvenanceEdgeType>>,
}

fn default_true() -> bool {
    true
}

fn default_max_hops() -> Option<u32> {
    Some(8)
}

/// Response for multi-hop lineage query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LineageQueryResponse {
    /// All events discovered during traversal, including the seed event.
    pub events: Vec<ProvenanceEvent>,
    /// Edges discovered during traversal.
    pub edges: Vec<LineageEdge>,
}

/// A compact edge representation for lineage traversal results.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LineageEdge {
    pub edge_type: ProvenanceEdgeType,
    pub from_event_id: EventId,
    pub to_event_id: EventId,
    pub summary: Option<String>,
}

/// Request for replaying a read-only provenance reconstruction for a single execution.
/// Returns all events belonging to the execution, sorted topologically by parent_edges
/// for a deterministic, reproducible reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceReplayRequest {
    /// The execution_id to replay.
    pub execution_id: ExecutionId,
}

/// Response containing the replay reconstruction of an execution's provenance lineage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceReplayResponse {
    /// All events belonging to the execution, sorted topologically by parent_edges
    /// (roots first, leaves last) and then by occurred_at for stability.
    pub events: Vec<ProvenanceEvent>,
    /// The execution_id this replay covers.
    pub execution_id: ExecutionId,
}

/// Request for exporting provenance events as a deterministic audit payload.
/// Uses the same filter semantics as ProvenanceQueryRequest but returns
/// a self-contained export with metadata for auditability.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceExportRequest {
    /// Filter by intent_id.
    #[serde(default)]
    pub intent_id: Option<crate::IntentId>,
    /// Filter by proposal_id.
    #[serde(default)]
    pub proposal_id: Option<ProposalId>,
    /// Filter by execution_id.
    #[serde(default)]
    pub execution_id: Option<ExecutionId>,
    /// Filter by capability_id.
    #[serde(default)]
    pub capability_id: Option<CapabilityId>,
    /// Filter by event kind.
    #[serde(default)]
    pub event_kind: Option<ProvenanceEventKind>,
    /// Filter to terminal events only.
    #[serde(default)]
    pub terminal_only: Option<bool>,
    /// Filter events that occurred at or after this timestamp.
    #[serde(default)]
    pub since: Option<Timestamp>,
    /// Filter events that occurred at or before this timestamp.
    #[serde(default)]
    pub until: Option<Timestamp>,
    /// Maximum number of events to export (1-10000). Defaults to 1000.
    #[serde(default = "default_export_limit")]
    pub limit: Option<u32>,
    /// Cursor for keyset pagination. Use the returned next_cursor to advance.
    #[serde(default)]
    pub cursor: Option<String>,
}

fn default_export_limit() -> Option<u32> {
    Some(1000)
}

/// Response containing the exported provenance events as a deterministic audit payload.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceExportResponse {
    /// Exported provenance events, sorted by occurred_at ascending, then event_id ascending.
    pub events: Vec<ProvenanceEvent>,
    /// Number of events in this export page.
    pub total_matched: u64,
    /// Number of events included in this export page (same as total_matched for single-page exports).
    pub exported_count: u64,
    /// Cursor for fetching the next page of exports. None if this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Export metadata for auditability.
    pub export_info: ProvenanceExportInfo,
}

/// Metadata about the export itself, for audit trail purposes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceExportInfo {
    /// RFC3339 timestamp when this export was generated.
    pub exported_at: Timestamp,
    /// The filter criteria used for this export (mirrors the request).
    pub filters: ProvenanceExportFilters,
}

/// Subset of request filters echoed back in the response for auditability.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceExportFilters {
    pub intent_id: Option<bool>,
    pub proposal_id: Option<bool>,
    pub execution_id: Option<bool>,
    pub capability_id: Option<bool>,
    pub event_kind: Option<bool>,
    pub terminal_only: Option<bool>,
    pub since: Option<bool>,
    pub until: Option<bool>,
}

// =============================================================================
// Server-side provenance stats types
// =============================================================================

/// Request for server-side provenance statistics aggregation.
/// Uses the same filter semantics as ProvenanceQueryRequest but returns
/// aggregated statistics instead of individual events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceStatsRequest {
    /// Filter by intent_id.
    #[serde(default)]
    pub intent_id: Option<crate::IntentId>,
    /// Filter by proposal_id.
    #[serde(default)]
    pub proposal_id: Option<ProposalId>,
    /// Filter by execution_id.
    #[serde(default)]
    pub execution_id: Option<ExecutionId>,
    /// Filter by capability_id.
    #[serde(default)]
    pub capability_id: Option<CapabilityId>,
    /// Filter by event kind.
    #[serde(default)]
    pub event_kind: Option<ProvenanceEventKind>,
    /// Filter events that occurred at or after this timestamp.
    #[serde(default)]
    pub since: Option<Timestamp>,
    /// Filter events that occurred at or before this timestamp.
    #[serde(default)]
    pub until: Option<Timestamp>,
    /// Maximum events to process for stats computation (1-100000).
    /// Defaults to 10000. Events beyond this limit are not reflected in stats.
    #[serde(default = "default_stats_max_events")]
    pub max_events: Option<u32>,
}

fn default_stats_max_events() -> Option<u32> {
    Some(10_000)
}

/// Response containing aggregated provenance statistics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceStatsResponse {
    /// Total number of events matching the filter.
    pub total_events: u64,
    /// Count of events by kind.
    pub kinds: std::collections::HashMap<String, u64>,
    /// Count of terminal events (SideEffectCommitted, SideEffectCompensated,
    /// SideEffectRolledBack, ApprovalDenied, Quarantined, ErrorRaised).
    pub terminal_count: u64,
    /// Count of events that indicate a problem condition (ErrorRaised, Quarantined,
    /// ApprovalDenied, SideEffectRolledBack).
    pub issue_count: u64,
    /// Count of events missing an execution_id.
    pub events_without_execution_id: u64,
    /// Number of unique intents with matching events.
    pub unique_intents: u64,
    /// Number of unique proposals with matching events.
    pub unique_proposals: u64,
    /// Number of unique executions with matching events.
    pub unique_executions: u64,
    /// Events flagged by consistency checks.
    pub flagged_events: Vec<FlaggedEvent>,
}

/// A single event flagged by a consistency check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FlaggedEvent {
    /// The event ID of the flagged event.
    pub event_id: EventId,
    /// The kind of the flagged event.
    pub kind: ProvenanceEventKind,
    /// Human-readable reason why the event was flagged.
    pub reason: String,
}
