use crate::{
    ChannelId, EventId, JsonMap, PrincipalId, ResourceMode, RiskTier, RollbackClass,
    SessionId, TimeBudget, Timestamp, TrustContextSummary,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentEnvelope {
    pub intent_id: crate::IntentId,
    pub principal_id: PrincipalId,
    pub session_id: Option<SessionId>,
    pub channel_id: Option<ChannelId>,
    pub title: String,
    pub goal: String,
    pub normalized_goal: String,
    pub allowed_outcomes: Vec<OutcomeClause>,
    pub forbidden_outcomes: Vec<OutcomeClause>,
    pub resource_scope: Vec<ResourceSelector>,
    pub risk_tier: RiskTier,
    pub approval_mode: crate::ApprovalMode,
    pub default_rollback_class: RollbackClass,
    pub time_budget: TimeBudget,
    pub trust_context: TrustContextSummary,
    pub derived_from_event_ids: Vec<EventId>,
    pub tags: Vec<String>,
    pub metadata: JsonMap,
    pub status: IntentStatus,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum IntentStatus {
    Active,
    Expired,
    Closed,
    Quarantined,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutcomeClause {
    pub id: String,
    pub description: String,
    pub effect_type: EffectType,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum EffectType {
    ReadOnlyAnalysis,
    DraftCreation,
    FileMutation,
    GitMutation,
    DatabaseMutation,
    ExternalApiCall,
    ExternalCommunication,
    Scheduling,
    AdministrativeChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum ResourceSelector {
    FilesystemPath {
        path: String,
        mode: ResourceMode,
        content_hash: Option<String>,
    },
    GitRepository {
        repo_path: String,
        allowed_refs: Vec<String>,
        mode: ResourceMode,
    },
    SqliteDatabase {
        db_path: String,
        tables: Vec<String>,
        mode: ResourceMode,
    },
    HttpEndpoint {
        method: crate::HttpMethod,
        base_url: String,
        path_prefix: String,
        mode: ResourceMode,
    },
    EmailDraft {
        recipient_allowlist: Vec<String>,
        subject_prefix_allowlist: Vec<String>,
        mode: ResourceMode,
    },
    McpTool {
        server_name: String,
        tool_name: String,
        mode: ResourceMode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentInputRef {
    pub source_id: String,
    pub source_type: String,
    pub trust_labels: Vec<crate::TrustLabel>,
    pub sensitivity_labels: Vec<crate::SensitivityLabel>,
    pub summary: String,
    pub event_id: Option<EventId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentCompileRequest {
    pub principal_id: PrincipalId,
    pub session_id: Option<SessionId>,
    pub channel_id: Option<ChannelId>,
    pub title: String,
    pub goal: String,
    pub agent_plan_summary: Option<String>,
    pub trusted_context: JsonMap,
    pub raw_inputs: Vec<IntentInputRef>,
    pub requested_resource_scope: Vec<ResourceSelector>,
    pub requested_risk_tier: Option<RiskTier>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentCompileResponse {
    pub envelope: IntentEnvelope,
    pub warnings: Vec<String>,
}
