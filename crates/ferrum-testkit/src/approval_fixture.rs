use crate::test_now;

/// Builder for a minimal `ApprovalRequest` in tests.
pub struct ApprovalFixture {
    approval_id: Option<ferrum_proto::ApprovalId>,
    intent_id: Option<ferrum_proto::IntentId>,
    proposal_id: Option<ferrum_proto::ProposalId>,
    requested_by: ferrum_proto::ActorRef,
    reason: String,
    action_digest: String,
    state: ferrum_proto::ApprovalState,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
}

impl Default for ApprovalFixture {
    fn default() -> Self {
        Self {
            approval_id: None,
            intent_id: None,
            proposal_id: None,
            requested_by: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::Operator,
                actor_id: "test-actor".to_string(),
                display_name: Some("Test Operator".to_string()),
            },
            reason: "test approval".to_string(),
            action_digest: "test-digest".to_string(),
            state: ferrum_proto::ApprovalState::Pending,
            created_at: None,
            expires_at: None,
            now: test_now(),
        }
    }
}

impl ApprovalFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_id(mut self, approval_id: ferrum_proto::ApprovalId) -> Self {
        self.approval_id = Some(approval_id);
        self
    }

    pub fn with_intent_id(mut self, intent_id: ferrum_proto::IntentId) -> Self {
        self.intent_id = Some(intent_id);
        self
    }

    pub fn with_proposal_id(mut self, proposal_id: ferrum_proto::ProposalId) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    pub fn with_requested_by(mut self, requested_by: ferrum_proto::ActorRef) -> Self {
        self.requested_by = requested_by;
        self
    }

    pub fn with_action_digest(mut self, action_digest: impl Into<String>) -> Self {
        self.action_digest = action_digest.into();
        self
    }

    pub fn with_state(mut self, state: ferrum_proto::ApprovalState) -> Self {
        self.state = state;
        self
    }

    pub fn with_created_at(mut self, created_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    pub fn with_expires_at(mut self, expires_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn with_now(mut self, now: chrono::DateTime<chrono::Utc>) -> Self {
        self.now = now;
        self
    }

    pub fn build(self) -> ferrum_proto::ApprovalRequest {
        let created_at = self.created_at.unwrap_or(self.now);
        let expires_at = self
            .expires_at
            .unwrap_or(self.now + chrono::Duration::hours(1));
        ferrum_proto::ApprovalRequest {
            approval_id: self.approval_id.unwrap_or_default(),
            intent_id: self.intent_id.unwrap_or_default(),
            proposal_id: self.proposal_id.unwrap_or_default(),
            execution_id: None,
            requested_by: self.requested_by,
            reason: self.reason,
            action_digest: self.action_digest,
            expires_at,
            state: self.state,
            created_at,
            resolver_evidence_version: None,
            owner_actor_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_fixture_defaults() {
        let approval = ApprovalFixture::new().build();
        assert_eq!(approval.reason, "test approval");
        assert_eq!(approval.action_digest, "test-digest");
        assert!(matches!(
            approval.state,
            ferrum_proto::ApprovalState::Pending
        ));
        assert_eq!(approval.created_at, test_now());
        assert_eq!(approval.expires_at, test_now() + chrono::Duration::hours(1));
        assert!(approval.owner_actor_id.is_none());
        assert_eq!(approval.requested_by.actor_id, "test-actor");
    }

    #[test]
    fn test_approval_fixture_overrides() {
        let approval_id = ferrum_proto::ApprovalId::new();
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        let now = chrono::Utc::now();
        let approval = ApprovalFixture::new()
            .with_id(approval_id)
            .with_intent_id(intent_id)
            .with_proposal_id(proposal_id)
            .with_action_digest("digest")
            .with_state(ferrum_proto::ApprovalState::Granted)
            .with_now(now)
            .build();
        assert_eq!(approval.approval_id, approval_id);
        assert_eq!(approval.intent_id, intent_id);
        assert_eq!(approval.proposal_id, proposal_id);
        assert_eq!(approval.action_digest, "digest");
        assert!(matches!(
            approval.state,
            ferrum_proto::ApprovalState::Granted
        ));
        assert_eq!(approval.created_at, now);
        assert_eq!(approval.expires_at, now + chrono::Duration::hours(1));
    }

    #[test]
    fn test_approval_fixture_custom_timestamps() {
        let created_at = test_now() - chrono::Duration::hours(2);
        let expires_at = test_now() - chrono::Duration::minutes(1);
        let approval = ApprovalFixture::new()
            .with_created_at(created_at)
            .with_expires_at(expires_at)
            .build();
        assert_eq!(approval.created_at, created_at);
        assert_eq!(approval.expires_at, expires_at);
    }
}
