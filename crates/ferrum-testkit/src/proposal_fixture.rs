use crate::test_now;

/// Builder for a minimal, valid `ActionProposal` in tests.
pub struct ProposalFixture {
    proposal_id: Option<ferrum_proto::ProposalId>,
    intent_id: Option<ferrum_proto::IntentId>,
    title: String,
    tool_name: String,
    server_name: String,
    raw_arguments: serde_json::Value,
    expected_effect: String,
    estimated_risk: ferrum_proto::RiskTier,
    requested_rollback_class: ferrum_proto::RollbackClass,
    metadata: ferrum_proto::JsonMap,
    now: chrono::DateTime<chrono::Utc>,
}

fn noop_binding_metadata() -> ferrum_proto::JsonMap {
    ferrum_proto::JsonMap::from([
        (
            "action_type".to_string(),
            serde_json::json!("McpToolMutation"),
        ),
        ("adapter_key".to_string(), serde_json::json!("noop")),
    ])
}

impl Default for ProposalFixture {
    fn default() -> Self {
        Self {
            proposal_id: None,
            intent_id: None,
            title: "test proposal".to_string(),
            tool_name: "test-tool".to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test effect".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Medium,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            metadata: noop_binding_metadata(),
            now: test_now(),
        }
    }
}

impl ProposalFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_id(mut self, proposal_id: ferrum_proto::ProposalId) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    pub fn with_intent_id(mut self, intent_id: ferrum_proto::IntentId) -> Self {
        self.intent_id = Some(intent_id);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_tool_name(mut self, tool_name: impl Into<String>) -> Self {
        self.tool_name = tool_name.into();
        self
    }

    pub fn with_server_name(mut self, server_name: impl Into<String>) -> Self {
        self.server_name = server_name.into();
        self
    }

    pub fn with_raw_arguments(mut self, raw_arguments: serde_json::Value) -> Self {
        self.raw_arguments = raw_arguments;
        self
    }

    pub fn with_expected_effect(mut self, expected_effect: impl Into<String>) -> Self {
        self.expected_effect = expected_effect.into();
        self
    }

    pub fn with_risk_tier(mut self, risk_tier: ferrum_proto::RiskTier) -> Self {
        self.estimated_risk = risk_tier;
        self
    }

    pub fn with_rollback_class(mut self, rollback_class: ferrum_proto::RollbackClass) -> Self {
        self.requested_rollback_class = rollback_class;
        self
    }

    pub fn with_metadata(mut self, metadata: ferrum_proto::JsonMap) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_now(mut self, now: chrono::DateTime<chrono::Utc>) -> Self {
        self.now = now;
        self
    }

    pub fn build(self) -> ferrum_proto::ActionProposal {
        ferrum_proto::ActionProposal {
            proposal_id: self.proposal_id.unwrap_or_default(),
            intent_id: self.intent_id.unwrap_or_default(),
            step_index: 0,
            title: self.title,
            tool_name: self.tool_name,
            server_name: self.server_name,
            raw_arguments: self.raw_arguments,
            expected_effect: self.expected_effect,
            estimated_risk: self.estimated_risk,
            requested_rollback_class: self.requested_rollback_class,
            taint_inputs: Vec::new(),
            metadata: self.metadata,
            created_at: self.now,
            owner_actor_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_fixture_defaults() {
        let proposal = ProposalFixture::new().build();
        assert_eq!(proposal.title, "test proposal");
        assert_eq!(proposal.tool_name, "test-tool");
        assert_eq!(proposal.server_name, "test-server");
        assert_eq!(proposal.estimated_risk, ferrum_proto::RiskTier::Medium);
        assert_eq!(
            proposal.requested_rollback_class,
            ferrum_proto::RollbackClass::R0NativeReversible
        );
        assert_eq!(proposal.created_at, test_now());
        assert!(proposal.owner_actor_id.is_none());
        assert!(proposal.metadata.contains_key("action_type"));
        assert!(proposal.metadata.contains_key("adapter_key"));
    }

    #[test]
    fn test_proposal_fixture_overrides() {
        let proposal_id = ferrum_proto::ProposalId::new();
        let intent_id = ferrum_proto::IntentId::new();
        let proposal = ProposalFixture::new()
            .with_id(proposal_id)
            .with_intent_id(intent_id)
            .with_tool_name("custom-tool")
            .with_rollback_class(ferrum_proto::RollbackClass::R2Compensatable)
            .build();
        assert_eq!(proposal.proposal_id, proposal_id);
        assert_eq!(proposal.intent_id, intent_id);
        assert_eq!(proposal.tool_name, "custom-tool");
        assert_eq!(
            proposal.requested_rollback_class,
            ferrum_proto::RollbackClass::R2Compensatable
        );
    }
}
