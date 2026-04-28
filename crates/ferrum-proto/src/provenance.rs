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
    /// Identifier of the source runtime that produced this event.
    /// None for internally-generated events.
    #[serde(default)]
    pub source_runtime_id: Option<String>,
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
    ExternalEventReceived,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceEdge {
    pub edge_type: ProvenanceEdgeType,
    /// The parent event (source of the edge).
    pub from_event_id: EventId,
    /// The child event (target of the edge), used for descendant traversal.
    pub to_event_id: Option<EventId>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// Request for provenance query with optional edge-type filtering.
/// When edge_types is empty or None, no edge-type filtering is applied.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceQueryRequest {
    pub intent_id: Option<crate::IntentId>,
    pub execution_id: Option<ExecutionId>,
    pub capability_id: Option<CapabilityId>,
    pub event_kind: Option<ProvenanceEventKind>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    /// Filter events by edge types. Only events that have at least one parent
    /// edge matching one of the specified types will be returned.
    /// Empty or None means no edge-type filtering.
    #[serde(default)]
    pub edge_types: Vec<ProvenanceEdgeType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceQueryResponse {
    pub events: Vec<ProvenanceEvent>,
}

/// Direction for multi-hop lineage traversal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum LineageDirection {
    /// Traverse parent edges only (ancestors).
    #[default]
    Ancestors,
    /// Traverse child edges only (descendants).
    Descendants,
    /// Traverse both directions.
    Both,
}

/// Request for multi-hop lineage query from a seed event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LineageQueryRequest {
    /// The seed event_id to start traversal from.
    pub event_id: EventId,
    /// Direction to traverse: ancestors, descendants, or both.
    #[serde(default)]
    pub direction: LineageDirection,
    /// Maximum number of hops to traverse. Must be between 1 and 10.
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
}

fn default_max_hops() -> u8 {
    3
}

/// Response for multi-hop lineage query.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LineageQueryResponse {
    /// Events discovered during traversal.
    pub events: Vec<ProvenanceEvent>,
    /// Edges discovered during traversal.
    pub edges: Vec<ProvenanceEdge>,
}

/// Request to ingest an external provenance event into FerrumGate's lineage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceIngestRequest {
    pub source_runtime_id: String,
    pub kind: ProvenanceEventKind,
    pub description: String,
    pub execution_id: Option<ExecutionId>,
    pub intent_id: Option<crate::IntentId>,
    pub trust_labels: Vec<TrustLabel>,
    pub sensitivity_labels: Vec<SensitivityLabel>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProvenanceIngestResponse {
    pub event_id: EventId,
    pub linked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActorType, ObjectType};

    fn make_test_hash_chain() -> HashChainRef {
        HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        }
    }

    #[test]
    fn test_provenance_event_source_runtime_id_default_none() {
        let event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::UserGoalReceived,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test-actor".to_string(),
                display_name: Some("Test Actor".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: "test-object".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: make_test_hash_chain(),
            metadata: JsonMap::default(),
            source_runtime_id: None,
        };
        assert_eq!(event.source_runtime_id, None);
    }

    #[test]
    fn test_provenance_event_source_runtime_id_some() {
        let runtime_id = "mcp://test-runtime".to_string();
        let event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "test-actor".to_string(),
                display_name: Some("Test Actor".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: "test-object".to_string(),
                summary: None,
            },
            intent_id: None,
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: make_test_hash_chain(),
            metadata: JsonMap::default(),
            source_runtime_id: Some(runtime_id.clone()),
        };
        assert_eq!(event.source_runtime_id, Some(runtime_id));
    }

    #[test]
    fn test_provenance_ingest_request_roundtrip() {
        let request = ProvenanceIngestRequest {
            source_runtime_id: "mcp://external".to_string(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            description: "External event from MCP runtime".to_string(),
            execution_id: None,
            intent_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            metadata: JsonMap::default(),
        };
        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: ProvenanceIngestRequest = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.source_runtime_id, "mcp://external");
        assert!(matches!(
            deserialized.kind,
            ProvenanceEventKind::ExternalEventReceived
        ));
    }

    #[test]
    fn test_provenance_event_kind_external_variant() {
        let kind = ProvenanceEventKind::ExternalEventReceived;
        let serialized = serde_json::to_string(&kind).unwrap();
        let deserialized: ProvenanceEventKind = serde_json::from_str(&serialized).unwrap();
        assert!(matches!(
            deserialized,
            ProvenanceEventKind::ExternalEventReceived
        ));
    }
}
