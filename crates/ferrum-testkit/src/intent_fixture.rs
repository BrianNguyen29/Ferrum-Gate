use crate::test_now;

/// Builder for a minimal, valid `IntentEnvelope` in tests.
pub struct IntentFixture {
    intent_id: Option<ferrum_proto::IntentId>,
    principal_id: Option<ferrum_proto::PrincipalId>,
    title: String,
    goal: String,
    normalized_goal: String,
    allowed_outcomes: Vec<ferrum_proto::OutcomeClause>,
    risk_tier: ferrum_proto::RiskTier,
    approval_mode: ferrum_proto::ApprovalMode,
    default_rollback_class: ferrum_proto::RollbackClass,
    resource_scope: Vec<ferrum_proto::ResourceSelector>,
    trust_context: ferrum_proto::TrustContextSummary,
    now: chrono::DateTime<chrono::Utc>,
}

impl Default for IntentFixture {
    fn default() -> Self {
        Self {
            intent_id: None,
            principal_id: None,
            title: "test-intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![ferrum_proto::OutcomeClause {
                id: "read".to_string(),
                description: "read only analysis".to_string(),
                effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
                required: true,
            }],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            resource_scope: Vec::new(),
            trust_context: ferrum_proto::TrustContextSummary {
                input_labels: Vec::new(),
                sensitivity_labels: Vec::new(),
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            now: test_now(),
        }
    }
}

impl IntentFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_id(mut self, intent_id: ferrum_proto::IntentId) -> Self {
        self.intent_id = Some(intent_id);
        self
    }

    pub fn with_principal_id(mut self, principal_id: ferrum_proto::PrincipalId) -> Self {
        self.principal_id = Some(principal_id);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_goal(mut self, goal: impl Into<String>) -> Self {
        let goal = goal.into();
        self.normalized_goal = goal.clone();
        self.goal = goal;
        self
    }

    pub fn with_normalized_goal(mut self, normalized_goal: impl Into<String>) -> Self {
        self.normalized_goal = normalized_goal.into();
        self
    }

    pub fn with_allowed_outcomes(
        mut self,
        allowed_outcomes: Vec<ferrum_proto::OutcomeClause>,
    ) -> Self {
        self.allowed_outcomes = allowed_outcomes;
        self
    }

    pub fn with_risk_tier(mut self, risk_tier: ferrum_proto::RiskTier) -> Self {
        self.risk_tier = risk_tier;
        self
    }

    pub fn with_approval_mode(mut self, approval_mode: ferrum_proto::ApprovalMode) -> Self {
        self.approval_mode = approval_mode;
        self
    }

    pub fn with_rollback_class(mut self, rollback_class: ferrum_proto::RollbackClass) -> Self {
        self.default_rollback_class = rollback_class;
        self
    }

    pub fn with_resource_scope(
        mut self,
        resource_scope: Vec<ferrum_proto::ResourceSelector>,
    ) -> Self {
        self.resource_scope = resource_scope;
        self
    }

    pub fn with_trust_context(mut self, trust_context: ferrum_proto::TrustContextSummary) -> Self {
        self.trust_context = trust_context;
        self
    }

    pub fn with_now(mut self, now: chrono::DateTime<chrono::Utc>) -> Self {
        self.now = now;
        self
    }

    pub fn build(self) -> ferrum_proto::IntentEnvelope {
        ferrum_proto::IntentEnvelope {
            intent_id: self.intent_id.unwrap_or_default(),
            principal_id: self.principal_id.unwrap_or_default(),
            session_id: None,
            channel_id: None,
            title: self.title,
            goal: self.goal,
            normalized_goal: self.normalized_goal,
            allowed_outcomes: self.allowed_outcomes,
            forbidden_outcomes: Vec::new(),
            resource_scope: self.resource_scope,
            risk_tier: self.risk_tier,
            approval_mode: self.approval_mode,
            default_rollback_class: self.default_rollback_class,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: self.trust_context,
            derived_from_event_ids: Vec::new(),
            tags: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            created_at: self.now,
            expires_at: self.now + chrono::Duration::hours(1),
            owner_actor_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_fixture_defaults() {
        let intent = IntentFixture::new().build();
        assert_eq!(intent.title, "test-intent");
        assert_eq!(intent.goal, "test goal");
        assert_eq!(intent.risk_tier, ferrum_proto::RiskTier::Low);
        assert_eq!(intent.approval_mode, ferrum_proto::ApprovalMode::None);
        assert_eq!(
            intent.default_rollback_class,
            ferrum_proto::RollbackClass::R0NativeReversible
        );
        assert_eq!(intent.created_at, test_now());
        assert_eq!(intent.expires_at, test_now() + chrono::Duration::hours(1));
        assert!(intent.owner_actor_id.is_none());
    }

    #[test]
    fn test_intent_fixture_overrides() {
        let intent_id = ferrum_proto::IntentId::new();
        let principal_id = ferrum_proto::PrincipalId::new();
        let intent = IntentFixture::new()
            .with_id(intent_id)
            .with_principal_id(principal_id)
            .with_title("custom")
            .with_risk_tier(ferrum_proto::RiskTier::High)
            .with_rollback_class(ferrum_proto::RollbackClass::R3IrreversibleHighConsequence)
            .build();
        assert_eq!(intent.intent_id, intent_id);
        assert_eq!(intent.principal_id, principal_id);
        assert_eq!(intent.title, "custom");
        assert_eq!(intent.risk_tier, ferrum_proto::RiskTier::High);
        assert_eq!(
            intent.default_rollback_class,
            ferrum_proto::RollbackClass::R3IrreversibleHighConsequence
        );
    }
}
