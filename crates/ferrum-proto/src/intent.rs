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
    /// U1-S9a: Deterministic policy bundle fingerprint derived from authored
    /// outcome contracts (allowed_outcomes/forbidden_outcomes and selectors).
    /// Used to derive PolicyBundleId for capability/provenance traceability.
    pub policy_bundle_fingerprint: Option<String>,
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

/// H1.2a: Temporal constraints for outcome clause validity windows.
/// When temporal constraints are present, the clause only applies within
/// the specified time window (valid_from <= current_time < valid_until).
/// Both fields are optional - when absent, the clause is always valid
/// within the intent's overall expiration window.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutcomeTemporalConstraints {
    /// When present, the clause is not valid before this timestamp.
    /// If None, the clause has no lower bound (valid from intent creation).
    pub valid_from: Option<Timestamp>,
    /// When present, the clause is not valid after this timestamp.
    /// If None, the clause has no upper bound (valid until intent expiration).
    pub valid_until: Option<Timestamp>,
}

impl Default for OutcomeTemporalConstraints {
    fn default() -> Self {
        Self {
            valid_from: None,
            valid_until: None,
        }
    }
}

impl OutcomeTemporalConstraints {
    /// Check if this temporal constraint is active at the given timestamp.
    /// Returns true if the timestamp falls within the validity window.
    pub fn is_active_at(&self, timestamp: &Timestamp) -> bool {
        if let Some(valid_from) = &self.valid_from {
            if timestamp < valid_from {
                return false;
            }
        }
        if let Some(valid_until) = &self.valid_until {
            if timestamp >= valid_until {
                return false;
            }
        }
        true
    }

    /// Validate the temporal constraints for internal consistency.
    /// Returns None if valid, Some(error_message) if invalid.
    pub fn validate(&self) -> Option<String> {
        if let (Some(valid_from), Some(valid_until)) = (&self.valid_from, &self.valid_until) {
            if valid_from >= valid_until {
                return Some(format!(
                    "valid_from ({}) must be before valid_until ({})",
                    valid_from, valid_until
                ));
            }
        }
        None
    }
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
    /// H1.2a: Optional temporal constraints for time-windowed outcome validity.
    /// When present, the clause only applies within the specified time window.
    /// When absent, the clause is always valid (within intent expiration).
    #[serde(default)]
    pub temporal: Option<OutcomeTemporalConstraints>,
}

impl Default for OutcomeClause {
    fn default() -> Self {
        Self {
            id: String::new(),
            description: String::new(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: false,
            selectors: None,
            temporal: None,
        }
    }
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
    /// Optional explicit allowed outcomes. When omitted, a default single coarse
    /// allowed outcome is inferred from effect_type (backward-compatible behavior).
    #[serde(default)]
    pub allowed_outcomes: Option<Vec<OutcomeClause>>,
    /// Optional explicit forbidden outcomes.
    #[serde(default)]
    pub forbidden_outcomes: Option<Vec<OutcomeClause>>,
    pub metadata: JsonMap,
}

impl Default for IntentCompileRequest {
    fn default() -> Self {
        Self {
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: String::new(),
            goal: String::new(),
            agent_plan_summary: None,
            trusted_context: JsonMap::new(),
            raw_inputs: Vec::new(),
            requested_resource_scope: Vec::new(),
            requested_risk_tier: None,
            effect_type: None,
            allowed_outcomes: None,
            forbidden_outcomes: None,
            metadata: JsonMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentCompileResponse {
    pub envelope: IntentEnvelope,
    pub warnings: Vec<String>,
}
