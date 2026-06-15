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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutcomeClause {
    pub id: String,
    pub description: String,
    pub effect_type: EffectType,
    pub required: bool,
}

/// Report of an execution's actual outcome, submitted for post-execution evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OutcomeReport {
    /// The execution whose outcome is being reported.
    pub execution_id: crate::ExecutionId,
    /// The actual effect that was observed.
    pub actual_effect: EffectType,
    /// Human-readable description of what happened.
    pub description: String,
    /// Optional result digest for integrity verification.
    pub result_digest: Option<String>,
    /// Whether the execution completed successfully from the adapter's perspective.
    pub adapter_success: bool,
    /// Optional structured metadata from the adapter.
    pub adapter_metadata: JsonMap,
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
    S3Bucket {
        bucket: String,
        key_prefix_allowlist: Vec<String>,
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
    pub approval_mode: Option<crate::ApprovalMode>,
    pub metadata: JsonMap,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentCompileResponse {
    pub envelope: IntentEnvelope,
    pub warnings: Vec<String>,
}

impl IntentEnvelope {
    /// Returns the set of effect types that this intent authorizes as outcomes.
    pub fn allowed_effect_types(&self) -> Vec<EffectType> {
        self.allowed_outcomes
            .iter()
            .map(|o| o.effect_type.clone())
            .collect()
    }

    /// Validates this intent envelope for structural validity.
    ///
    /// Returns `Ok(())` if the envelope is valid:
    /// - `allowed_outcomes` is non-empty (I1: at least one outcome is required)
    /// - `expires_at > created_at` (I1: expiry must be in the future relative to creation)
    ///
    /// Returns `Err(String)` describing the first validation failure encountered.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrum_proto::{
    ///     IntentEnvelope, IntentStatus, OutcomeClause, EffectType,
    ///     RiskTier, RollbackClass, TimeBudget, TrustContextSummary,
    ///     ApprovalMode, JsonMap,
    /// };
    /// use chrono::Utc;
    ///
    /// let now = Utc::now();
    /// let intent = IntentEnvelope {
    ///     intent_id: ferrum_proto::IntentId::new(),
    ///     principal_id: ferrum_proto::PrincipalId::new(),
    ///     session_id: None,
    ///     channel_id: None,
    ///     title: "test".into(),
    ///     goal: "test goal".into(),
    ///     normalized_goal: "test goal".into(),
    ///     allowed_outcomes: vec![OutcomeClause {
    ///         id: "read".into(),
    ///         description: "read only".into(),
    ///         effect_type: EffectType::ReadOnlyAnalysis,
    ///         required: true,
    ///     }],
    ///     forbidden_outcomes: vec![],
    ///     resource_scope: vec![],
    ///     risk_tier: RiskTier::Low,
    ///     approval_mode: ApprovalMode::None,
    ///     default_rollback_class: RollbackClass::R0NativeReversible,
    ///     time_budget: TimeBudget {
    ///         max_duration_ms: 1000,
    ///         max_steps: 1,
    ///         max_retries_per_step: 0,
    ///     },
    ///     trust_context: TrustContextSummary {
    ///         input_labels: vec![],
    ///         sensitivity_labels: vec![],
    ///         taint_score: 0,
    ///         contains_external_metadata: false,
    ///         contains_tool_output: false,
    ///         contains_untrusted_text: false,
    ///     },
    ///     derived_from_event_ids: vec![],
    ///     tags: vec![],
    ///     metadata: JsonMap::new(),
    ///     status: IntentStatus::Active,
    ///     created_at: now,
    ///     expires_at: now + chrono::Duration::minutes(5),
    /// };
    /// assert!(intent.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), String> {
        if self.allowed_outcomes.is_empty() {
            return Err("intent envelope validation failed: allowed_outcomes is empty (at least one outcome is required)".to_string());
        }
        if self.expires_at <= self.created_at {
            return Err(
                "intent envelope validation failed: expires_at must be greater than created_at"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_intent() -> IntentEnvelope {
        let now = chrono::Utc::now();
        IntentEnvelope {
            intent_id: crate::IntentId::new(),
            principal_id: crate::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test-intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![OutcomeClause {
                id: "read".to_string(),
                description: "read only analysis".to_string(),
                effect_type: EffectType::ReadOnlyAnalysis,
                required: true,
            }],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: crate::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: Vec::new(),
                sensitivity_labels: Vec::new(),
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: Vec::new(),
            tags: Vec::new(),
            metadata: crate::JsonMap::new(),
            status: IntentStatus::Active,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(15),
        }
    }

    #[test]
    fn test_validate_valid_intent() {
        let intent = valid_intent();
        assert!(intent.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_allowed_outcomes() {
        let mut intent = valid_intent();
        intent.allowed_outcomes = vec![];
        let result = intent.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("allowed_outcomes is empty"));
    }

    #[test]
    fn test_validate_expires_at_equals_created_at() {
        let mut intent = valid_intent();
        intent.expires_at = intent.created_at;
        let result = intent.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expires_at must be greater than created_at")
        );
    }

    #[test]
    fn test_validate_expires_at_before_created_at() {
        let mut intent = valid_intent();
        intent.expires_at = intent.created_at - chrono::Duration::minutes(5);
        let result = intent.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("expires_at must be greater than created_at")
        );
    }
}
