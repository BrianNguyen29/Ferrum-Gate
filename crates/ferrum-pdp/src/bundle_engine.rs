//! PolicyBundle-based PDP engine (Phase 1).
//!
//! Implements the `PdpEngine` trait using rule bundles with the semantics
//! defined in ADR-012:
//! - Descending priority evaluation.
//! - First-match-wins by priority group.
//! - Same-priority conflict resolution by total-order strictness.
//! - Invariant rules prepended at `i32::MAX` and `i32::MAX - 1`.
//! - Empty bundle default-allow.
//!
//! Phase 1 does not propagate bundle identity into capabilities or evaluate
//! obligations; those are deferred to Phase 2.

use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, Decision, EvaluateOutcomeResponse, EvaluateProposalResponse, IntentEnvelope,
    OutcomeReport, PolicyBundle, PolicyRule, TrustContextSummary,
};

use crate::matchers::{evaluate_bundle_rules, invariant_reason};
use crate::{PdpEngine, StaticPdpEngine};

/// Source of policy bundles for the bundle PDP engine.
///
/// Phase 1 uses a simple in-memory source appropriate for tests and bootstrap
/// deployments. Store-backed loading is intentionally kept outside ferrum-pdp.
#[derive(Debug, Clone)]
pub struct BundleSource {
    bundles: Vec<PolicyBundle>,
}

impl BundleSource {
    /// Create a source from an owned list of bundles.
    pub fn new(bundles: Vec<PolicyBundle>) -> Self {
        Self { bundles }
    }

    /// Create a source containing a single bundle.
    pub fn single(bundle: PolicyBundle) -> Self {
        Self {
            bundles: vec![bundle],
        }
    }

    /// Return the bundles in evaluation order.
    pub fn bundles(&self) -> &[PolicyBundle] {
        &self.bundles
    }
}

impl From<Vec<PolicyBundle>> for BundleSource {
    fn from(bundles: Vec<PolicyBundle>) -> Self {
        Self::new(bundles)
    }
}

impl From<PolicyBundle> for BundleSource {
    fn from(bundle: PolicyBundle) -> Self {
        Self::single(bundle)
    }
}

/// Policy-bundle PDP engine.
#[derive(Debug, Clone)]
pub struct PolicyBundlePdpEngine {
    source: BundleSource,
    static_engine: StaticPdpEngine,
}

impl PolicyBundlePdpEngine {
    /// Create a new engine from a bundle source.
    pub fn new(source: impl Into<BundleSource>) -> Self {
        Self {
            source: source.into(),
            static_engine: StaticPdpEngine,
        }
    }

    /// Build the invariant rules that are conceptually prepended to every
    /// bundle and cannot be overridden by operator-authored rules.
    fn invariant_rules() -> Vec<PolicyRule> {
        vec![
            PolicyRule {
                id: "invariant.scope_mismatch".to_string(),
                description: "Deny mutating actions when no resource scope is declared".to_string(),
                decision: Decision::Deny,
                priority: i32::MAX,
                matchers: vec![ferrum_proto::Matcher::ScopeMismatch],
            },
            PolicyRule {
                id: "invariant.critical_risk_no_approval".to_string(),
                description: "Critical risk with no approval mode requires approval".to_string(),
                decision: Decision::RequireApproval,
                priority: i32::MAX - 1,
                matchers: vec![ferrum_proto::Matcher::CriticalRiskNoApproval],
            },
        ]
    }

    /// Evaluate a single bundle against the context, including invariant rules.
    fn evaluate_bundle(
        &self,
        bundle: &PolicyBundle,
        intent: &IntentEnvelope,
        proposal: &ActionProposal,
        trust: &TrustContextSummary,
    ) -> Option<EvaluateProposalResponse> {
        let mut augmented_bundle = bundle.clone();
        let mut invariants = Self::invariant_rules();
        augmented_bundle.rules.append(&mut invariants);

        evaluate_bundle_rules(&augmented_bundle, intent, proposal, trust).map(
            |(rule_id, decision, reason)| {
                let is_invariant = rule_id.contains(":invariant.");
                let final_reason = if is_invariant {
                    invariant_reason(&rule_id)
                } else {
                    reason
                };
                EvaluateProposalResponse {
                    decision,
                    reason: final_reason,
                    matched_rule_ids: vec![rule_id],
                    warnings: Vec::new(),
                }
            },
        )
    }
}

#[async_trait]
impl PdpEngine for PolicyBundlePdpEngine {
    async fn evaluate(
        &self,
        intent: &IntentEnvelope,
        proposal: &ActionProposal,
        trust: &TrustContextSummary,
    ) -> anyhow::Result<EvaluateProposalResponse> {
        // Phase 1: evaluate bundles in source order, returning the first match.
        // If no bundle matches, default to Allow.
        let response = if let Some(response) = self
            .source
            .bundles()
            .iter()
            .find_map(|bundle| self.evaluate_bundle(bundle, intent, proposal, trust))
        {
            response
        } else {
            EvaluateProposalResponse {
                decision: Decision::Allow,
                reason: "no policy bundle rule matched; default allow".to_string(),
                matched_rule_ids: vec!["allow.default".to_string()],
                warnings: Vec::new(),
            }
        };

        // Outcome post-pass: mirror StaticPdpEngine semantics. Only Allow decisions
        // can be affected by outcome checks; stricter decisions (Deny, Quarantine,
        // RequireApproval, AllowDraftOnly) short-circuit earlier and are preserved.
        if response.decision == Decision::Allow {
            let inferred_effect = StaticPdpEngine::infer_effect_type(proposal);

            if let Some(reason) = StaticPdpEngine::check_forbidden_outcomes(
                &inferred_effect,
                &intent.forbidden_outcomes,
            ) {
                return Ok(EvaluateProposalResponse {
                    decision: Decision::Deny,
                    reason,
                    matched_rule_ids: vec!["outcome.forbidden".to_string()],
                    warnings: response.warnings,
                });
            }

            if let Some(warning) =
                StaticPdpEngine::check_allowed_outcomes(&inferred_effect, &intent.allowed_outcomes)
            {
                let mut warnings = response.warnings;
                warnings.push(warning);
                let mut matched_rule_ids = response.matched_rule_ids;
                matched_rule_ids.push("outcome.advisory.mismatch".to_string());
                return Ok(EvaluateProposalResponse {
                    decision: Decision::Allow,
                    reason: response.reason,
                    matched_rule_ids,
                    warnings,
                });
            }
        }

        Ok(response)
    }

    async fn evaluate_outcome(
        &self,
        intent: &IntentEnvelope,
        report: &OutcomeReport,
    ) -> anyhow::Result<EvaluateOutcomeResponse> {
        // Phase 1 keeps outcome evaluation static/hardcoded.
        self.static_engine.evaluate_outcome(intent, report).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActionProposal, ApprovalMode, EffectType, IntentEnvelope, IntentStatus, JsonMap,
        OutcomeClause, OutcomeReport, PrincipalId, ProposalId, ResourceMode, ResourceSelector,
        RiskTier, RollbackClass, TimeBudget, TrustContextSummary,
    };

    fn make_trust_context() -> TrustContextSummary {
        TrustContextSummary {
            input_labels: vec![],
            sensitivity_labels: vec![],
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        }
    }

    fn make_intent() -> IntentEnvelope {
        IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Low,
            approval_mode: ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: TimeBudget {
                max_duration_ms: 60000,
                max_steps: 100,
                max_retries_per_step: 3,
            },
            trust_context: make_trust_context(),
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: JsonMap::new(),
            status: IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn make_intent_with_outcomes(
        allowed: Vec<OutcomeClause>,
        forbidden: Vec<OutcomeClause>,
    ) -> IntentEnvelope {
        let mut intent = make_intent();
        intent.allowed_outcomes = allowed;
        intent.forbidden_outcomes = forbidden;
        intent
    }

    fn make_proposal(
        tool_name: &str,
        expected_effect: &str,
        rollback_class: RollbackClass,
    ) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: tool_name.to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: expected_effect.to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: rollback_class,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn make_proposal_with_risk(
        tool_name: &str,
        expected_effect: &str,
        rollback_class: RollbackClass,
        risk: RiskTier,
    ) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: tool_name.to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: expected_effect.to_string(),
            estimated_risk: risk,
            requested_rollback_class: rollback_class,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn static_default_bundle() -> PolicyBundle {
        ferrum_proto::parse_policy_bundle_yaml(include_str!(
            "../../../configs/policy-bundles/static-default.yaml"
        ))
        .expect("static-default bundle parses")
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Parity tests: PolicyBundlePdpEngine with static-default bundle vs
    // StaticPdpEngine for existing proposal-evaluation cases.
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_parity_scope_deny_empty_scope() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let intent = make_intent();
        let proposal = make_proposal(
            "git.commit",
            "create a commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::Deny);
    }

    #[tokio::test]
    async fn test_parity_taint_quarantine() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let mut intent = make_intent();
        intent.resource_scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ResourceMode::Write,
            content_hash: None,
        }];
        let proposal = make_proposal(
            "git.commit",
            "create a commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let mut trust = make_trust_context();
        trust.taint_score = 85;

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::Quarantine);
    }

    #[tokio::test]
    async fn test_parity_r3_require_approval() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let mut intent = make_intent();
        intent.resource_scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ResourceMode::Write,
            content_hash: None,
        }];
        let proposal = make_proposal(
            "git.push",
            "push to remote",
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::RequireApproval);
    }

    #[tokio::test]
    async fn test_parity_draft_only() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::DraftOnly;
        let proposal = make_proposal(
            "file.write",
            "write a file",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::AllowDraftOnly);
    }

    #[tokio::test]
    async fn test_parity_forbidden_outcome() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let intent = make_intent_with_outcomes(
            vec![],
            vec![OutcomeClause {
                id: "no-git".to_string(),
                effect_type: EffectType::GitMutation,
                description: "no git mutations allowed".to_string(),
                required: true,
            }],
        );
        let proposal = make_proposal(
            "git.commit",
            "commit changes",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::Deny);
    }

    #[tokio::test]
    async fn test_parity_forbidden_outcome_negative_empty() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        // Empty forbidden_outcomes: git mutation should not be denied.
        let intent = make_intent_with_outcomes(vec![], vec![]);
        let proposal = make_proposal(
            "git.commit",
            "commit changes",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, Decision::Allow);
        assert_eq!(bundle_result.decision, Decision::Allow);
        assert_eq!(static_result.decision, bundle_result.decision);
    }

    #[tokio::test]
    async fn test_parity_forbidden_outcome_negative_other_effect() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        // Forbidden FileMutation, but the proposal is a GitMutation.
        let intent = make_intent_with_outcomes(
            vec![],
            vec![OutcomeClause {
                id: "no-file".to_string(),
                effect_type: EffectType::FileMutation,
                description: "no file mutations allowed".to_string(),
                required: true,
            }],
        );
        let proposal = make_proposal(
            "git.commit",
            "commit changes",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, Decision::Allow);
        assert_eq!(bundle_result.decision, Decision::Allow);
        assert_eq!(static_result.decision, bundle_result.decision);
    }

    #[tokio::test]
    async fn test_parity_advisory_mismatch() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let intent = make_intent_with_outcomes(
            vec![OutcomeClause {
                id: "file-only".to_string(),
                effect_type: EffectType::FileMutation,
                description: "file changes only".to_string(),
                required: false,
            }],
            vec![],
        );
        let proposal = make_proposal(
            "fetch.http",
            "call external API",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::Allow);
    }

    #[tokio::test]
    async fn test_parity_default_allow() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let intent = make_intent();
        let proposal = make_proposal(
            "read.file",
            "read a file",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::Allow);
    }

    #[tokio::test]
    async fn test_parity_invariant2_critical_risk_none_approval() {
        let static_engine = StaticPdpEngine;
        let bundle_engine = PolicyBundlePdpEngine::new(static_default_bundle());

        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::None;
        intent.resource_scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ResourceMode::Write,
            content_hash: None,
        }];
        let proposal = make_proposal_with_risk(
            "db.execute",
            "execute database query",
            RollbackClass::R1SnapshotRecoverable,
            RiskTier::Critical,
        );
        let trust = make_trust_context();

        let static_result = static_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();
        let bundle_result = bundle_engine
            .evaluate(&intent, &proposal, &trust)
            .await
            .unwrap();

        assert_eq!(static_result.decision, bundle_result.decision);
        assert_eq!(static_result.decision, Decision::RequireApproval);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Engine-specific tests
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_empty_bundle_default_allow() {
        let bundle = PolicyBundle {
            bundle_id: "empty".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![],
            active: false,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let engine = PolicyBundlePdpEngine::new(bundle);
        let intent = make_intent();
        // Use an R0 read-only proposal so invariant rules do not match.
        let proposal = make_proposal(
            "read.file",
            "read a file",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();

        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert_eq!(result.decision, Decision::Allow);
        assert!(
            result
                .matched_rule_ids
                .contains(&"allow.default".to_string())
        );
    }

    #[tokio::test]
    async fn test_invariant_scope_cannot_be_overridden() {
        let bundle = PolicyBundle {
            bundle_id: "override-attempt".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![PolicyRule {
                id: "allow.everything".to_string(),
                description: "Attempt to allow all".to_string(),
                decision: Decision::Allow,
                priority: i32::MAX,
                matchers: vec![ferrum_proto::Matcher::ActionIsMutation],
            }],
            active: false,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let engine = PolicyBundlePdpEngine::new(bundle);
        let intent = make_intent();
        let proposal = make_proposal(
            "git.commit",
            "create a commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let trust = make_trust_context();

        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert_eq!(result.decision, Decision::Deny);
        assert!(
            result
                .matched_rule_ids
                .iter()
                .any(|id| id.contains("invariant.scope_mismatch"))
        );
    }

    #[tokio::test]
    async fn test_invariant_critical_risk_cannot_be_overridden() {
        let bundle = PolicyBundle {
            bundle_id: "override-attempt".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![PolicyRule {
                id: "allow.critical".to_string(),
                description: "Attempt to allow critical risk".to_string(),
                decision: Decision::Allow,
                priority: i32::MAX - 1,
                matchers: vec![ferrum_proto::Matcher::CriticalRiskNoApproval],
            }],
            active: false,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let engine = PolicyBundlePdpEngine::new(bundle);
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::None;
        intent.resource_scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ResourceMode::Write,
            content_hash: None,
        }];
        let proposal = make_proposal_with_risk(
            "db.execute",
            "execute database query",
            RollbackClass::R1SnapshotRecoverable,
            RiskTier::Critical,
        );
        let trust = make_trust_context();

        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert_eq!(result.decision, Decision::RequireApproval);
        assert!(
            result
                .matched_rule_ids
                .iter()
                .any(|id| id.contains("invariant.critical_risk_no_approval"))
        );
    }

    #[tokio::test]
    async fn test_same_priority_stricter_wins() {
        let bundle = PolicyBundle {
            bundle_id: "conflict".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![
                PolicyRule {
                    id: "allow.mutation".to_string(),
                    description: "Allow mutation".to_string(),
                    decision: Decision::Allow,
                    priority: 50,
                    matchers: vec![ferrum_proto::Matcher::ActionIsMutation],
                },
                PolicyRule {
                    id: "deny.mutation".to_string(),
                    description: "Deny mutation".to_string(),
                    decision: Decision::Deny,
                    priority: 50,
                    matchers: vec![ferrum_proto::Matcher::ActionIsMutation],
                },
            ],
            active: false,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let engine = PolicyBundlePdpEngine::new(bundle);
        let intent = make_intent();
        let proposal = make_proposal(
            "git.commit",
            "create a commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let trust = make_trust_context();

        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert_eq!(result.decision, Decision::Deny);
    }

    #[tokio::test]
    async fn test_evaluate_outcome_delegates_to_static() {
        let engine = PolicyBundlePdpEngine::new(static_default_bundle());
        let intent = make_intent_with_outcomes(
            vec![OutcomeClause {
                id: "clause-1".to_string(),
                effect_type: EffectType::FileMutation,
                description: "file changes".to_string(),
                required: true,
            }],
            vec![],
        );
        let report = OutcomeReport {
            execution_id: ferrum_proto::ExecutionId::new(),
            actual_effect: EffectType::FileMutation,
            description: "file was modified".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: JsonMap::new(),
        };

        let result = engine.evaluate_outcome(&intent, &report).await.unwrap();
        assert!(result.aligned);
    }
}
