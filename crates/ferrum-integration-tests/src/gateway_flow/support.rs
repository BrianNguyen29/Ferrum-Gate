#![allow(dead_code)]

use async_trait::async_trait;
use ferrum_pdp::{PdpEngine, StaticPdpEngine};
use ferrum_proto::{
    Decision, EvaluateProposalResponse, IntentEnvelope, ProvenanceEventKind, RollbackClass,
    TrustContextSummary,
};
use ferrum_store::{ProvenanceRepo, SqliteStore};

/// Test-only PDP engine that allows injecting a trust context.
pub struct InjectablePdpEngine {
    _inner: StaticPdpEngine,
    trust: TrustContextSummary,
}

impl InjectablePdpEngine {
    pub fn new(trust: TrustContextSummary) -> Self {
        Self {
            _inner: StaticPdpEngine,
            trust,
        }
    }
}

#[async_trait]
impl PdpEngine for InjectablePdpEngine {
    async fn evaluate(
        &self,
        _intent: &IntentEnvelope,
        proposal: &ferrum_proto::ActionProposal,
        _trust: &TrustContextSummary,
    ) -> anyhow::Result<EvaluateProposalResponse> {
        let mut matched_rule_ids = Vec::new();

        if self.trust.taint_score >= 70
            && !matches!(
                proposal.requested_rollback_class,
                RollbackClass::R0NativeReversible
            )
        {
            matched_rule_ids.push("quarantine.high.taint.mutation".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::Quarantine,
                reason: "taint score is too high for mutating or impactful action".to_string(),
                matched_rule_ids,
                warnings: Vec::new(),
            });
        }

        if matches!(
            proposal.requested_rollback_class,
            RollbackClass::R3IrreversibleHighConsequence
        ) {
            matched_rule_ids.push("approval.r3.required".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::RequireApproval,
                reason: "R3 action requires approval or draft-only handling".to_string(),
                matched_rule_ids,
                warnings: Vec::new(),
            });
        }

        matched_rule_ids.push("allow.default".to_string());
        Ok(EvaluateProposalResponse {
            decision: Decision::Allow,
            reason: "proposal passed default scaffold policy".to_string(),
            matched_rule_ids,
            warnings: Vec::new(),
        })
    }

    async fn evaluate_outcome(
        &self,
        _intent: &IntentEnvelope,
        report: &ferrum_proto::OutcomeReport,
    ) -> anyhow::Result<ferrum_proto::EvaluateOutcomeResponse> {
        // Delegate to StaticPdpEngine's implementation for outcome evaluation
        self._inner.evaluate_outcome(_intent, report).await
    }
}

// ---------------------------------------------------------------------------
// Single-use capability test
// ---------------------------------------------------------------------------

/// Helper to create a minimal intent envelope for testing (satisfies foreign key constraints).
pub fn make_test_intent(intent_id: ferrum_proto::IntentId) -> ferrum_proto::IntentEnvelope {
    let now = chrono::Utc::now();
    ferrum_proto::IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "test-intent".to_string(),
        goal: "test goal".to_string(),
        normalized_goal: "test goal".to_string(),
        allowed_outcomes: vec![ferrum_proto::OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: ferrum_proto::RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: Vec::new(),
            sensitivity_labels: Vec::new(),
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: Vec::new(),
        tags: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        status: ferrum_proto::IntentStatus::Active,
        created_at: now,
        expires_at: now + chrono::Duration::hours(1),
        owner_actor_id: None,
    }
}

/// Helper to create a minimal action proposal for testing (satisfies foreign key constraints).
pub fn make_test_proposal(
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
) -> ferrum_proto::ActionProposal {
    make_test_proposal_with_class(
        intent_id,
        proposal_id,
        ferrum_proto::RollbackClass::R0NativeReversible,
    )
}

pub fn noop_binding_metadata() -> ferrum_proto::JsonMap {
    ferrum_proto::JsonMap::from([
        (
            "action_type".to_string(),
            serde_json::json!("McpToolMutation"),
        ),
        ("adapter_key".to_string(), serde_json::json!("noop")),
    ])
}

pub async fn seed_policy_evaluated(store: &SqliteStore, proposal: &ferrum_proto::ActionProposal) {
    let event = ferrum_proto::ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind: ProvenanceEventKind::PolicyEvaluated,
        occurred_at: chrono::Utc::now(),
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Gateway,
            actor_id: "integration-test".to_string(),
            display_name: None,
        },
        object: ferrum_proto::ObjectRef {
            object_type: ferrum_proto::ObjectType::Proposal,
            object_id: proposal.proposal_id.to_string(),
            summary: Some("seeded policy evaluation".to_string()),
        },
        intent_id: Some(proposal.intent_id),
        proposal_id: Some(proposal.proposal_id),
        execution_id: None,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: ferrum_proto::HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::from([(
            "decision".to_string(),
            serde_json::json!("Allow"),
        )]),
        source_runtime_id: None,
    };
    store
        .provenance()
        .append_event(&event)
        .await
        .expect("seed PolicyEvaluated event");
}

/// Helper to create a minimal action proposal with a specific rollback class.
pub fn make_test_proposal_with_class(
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
    rollback_class: ferrum_proto::RollbackClass,
) -> ferrum_proto::ActionProposal {
    let now = chrono::Utc::now();
    ferrum_proto::ActionProposal {
        proposal_id,
        intent_id,
        step_index: 0,
        title: "test proposal".to_string(),
        tool_name: "test-tool".to_string(),
        server_name: "test-server".to_string(),
        raw_arguments: serde_json::json!({}),
        expected_effect: "test effect".to_string(),
        estimated_risk: ferrum_proto::RiskTier::Medium,
        requested_rollback_class: rollback_class,
        taint_inputs: Vec::new(),
        metadata: noop_binding_metadata(),
        created_at: now,
        owner_actor_id: None,
    }
}

/// Helper to create a pending approval request for testing.
pub fn make_test_approval(
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
    created_at: chrono::DateTime<chrono::Utc>,
) -> ferrum_proto::ApprovalRequest {
    ferrum_proto::ApprovalRequest {
        approval_id: ferrum_proto::ApprovalId::new(),
        intent_id,
        proposal_id,
        execution_id: None,
        requested_by: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Operator,
            actor_id: "test-actor".to_string(),
            display_name: Some("Test Operator".to_string()),
        },
        reason: "test approval".to_string(),
        action_digest: "test-digest".to_string(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        state: ferrum_proto::ApprovalState::Pending,
        created_at,
        resolver_evidence_version: None,
        owner_actor_id: None,
    }
}

/// Helper to create a policy bundle for testing.
pub fn make_test_policy_bundle(
    bundle_id: &str,
    rules: Vec<ferrum_proto::PolicyRule>,
    active: bool,
) -> ferrum_proto::PolicyBundle {
    let now = chrono::Utc::now();
    let mut bundle = ferrum_proto::PolicyBundle {
        bundle_id: bundle_id.to_string(),
        version: "1.0.0".to_string(),
        rules,
        active,
        content_hash: None,
        created_at: now,
        updated_at: now,
    };
    let hash = bundle.compute_content_hash();
    bundle.content_hash = Some(hash);
    bundle
}
