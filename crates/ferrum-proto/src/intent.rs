use crate::{
    ChannelId, EventId, JsonMap, PrincipalId, ResourceMode, RiskTier, RollbackClass, SessionId,
    TimeBudget, Timestamp, TrustContextSummary,
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

/// Optional selector block for higher-fidelity outcome matching (U1-S4, U1-S7a).
/// All fields are optional - when absent, coarse effect_type matching is used.
/// This enables more precise outcome contracts beyond the coarse effect_type.
///
/// U1-S7a: Each dimension supports both scalar (exact match) and list-based
/// (match any member) selectors. When both scalar and list are present for
/// the same dimension, the semantics are: scalar OR any list member matches.
/// This is an additive change that maintains backward compatibility with
/// existing scalar-only selectors.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct OutcomeSelectors {
    /// Adapter family filter (e.g., "fs", "git", "sqlite", "http", "noop", "maildraft").
    /// When present, the outcome only matches if the execution's adapter_key matches.
    pub adapter_family: Option<String>,
    /// Adapter family list filter (U1-S7a): match if adapter_key matches ANY member.
    /// When present alongside adapter_family, matches adapter_family OR any list member.
    pub adapter_family_in: Option<Vec<String>>,
    /// Target family filter (e.g., "file", "git", "sqlite", "http", "email", "generic").
    /// When present, the outcome only matches if the execution's rollback target family matches.
    pub target_family: Option<String>,
    /// Target family list filter (U1-S7a): match if target_family matches ANY member.
    /// When present alongside target_family, matches target_family OR any list member.
    pub target_family_in: Option<Vec<String>>,
    /// Request class filter (e.g., "mutation", "read_only", "draft").
    /// When present, the outcome only matches if the execution's request class matches.
    pub request_class: Option<String>,
    /// Request class list filter (U1-S7a): match if request_class matches ANY member.
    /// When present alongside request_class, matches request_class OR any list member.
    pub request_class_in: Option<Vec<String>>,
    /// Mutation family filter (e.g., "file_write", "file_delete", "git_commit", "http_mutation").
    /// When present, the outcome only matches if the execution's action_type matches.
    pub mutation_family: Option<String>,
    /// Mutation family list filter (U1-S7a): match if mutation_family matches ANY member.
    /// When present alongside mutation_family, matches mutation_family OR any list member.
    pub mutation_family_in: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutcomeClause {
    pub id: String,
    pub description: String,
    pub effect_type: EffectType,
    pub required: bool,
    /// Optional higher-fidelity selectors for more precise outcome matching (U1-S4).
    /// When present alongside effect_type, both must match for the clause to be considered aligned.
    /// When absent, only effect_type matching is used (backward compatible).
    pub selectors: Option<OutcomeSelectors>,
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
    /// Optional effect type for the intent. Defaults to ReadOnlyAnalysis if not specified.
    pub effect_type: Option<EffectType>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentCompileResponse {
    pub envelope: IntentEnvelope,
    pub warnings: Vec<String>,
}
