use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, ApprovalMode, Decision, EffectType, EvaluateOutcomeResponse,
    EvaluateProposalResponse, IntentEnvelope, OutcomeClause, OutcomeReport, RiskTier,
    RollbackClass, TrustContextSummary,
};
use std::mem;

#[async_trait]
pub trait PdpEngine: Send + Sync {
    async fn evaluate(
        &self,
        intent: &IntentEnvelope,
        proposal: &ActionProposal,
        trust: &TrustContextSummary,
    ) -> anyhow::Result<EvaluateProposalResponse>;

    /// Evaluate whether an execution's actual outcome aligns with the intent's
    /// outcome expectations (allowed_outcomes / forbidden_outcomes).
    async fn evaluate_outcome(
        &self,
        intent: &IntentEnvelope,
        report: &OutcomeReport,
    ) -> anyhow::Result<EvaluateOutcomeResponse>;
}

impl StaticPdpEngine {
    /// Infer the EffectType from an action proposal based on its characteristics.
    pub fn infer_effect_type(proposal: &ActionProposal) -> EffectType {
        let tool_name = proposal.tool_name.to_lowercase();
        let server_name = proposal.server_name.to_lowercase();
        let expected_effect = proposal.expected_effect.to_lowercase();

        // Check for database-related tools
        if tool_name.contains("sql") || tool_name.contains("db") || tool_name.contains("database") {
            return EffectType::DatabaseMutation;
        }

        // Check for git-related tools
        if tool_name.contains("git") || server_name.contains("git") {
            return EffectType::GitMutation;
        }

        // Check for file mutation keywords (careful about overlaps)
        let has_file_word = expected_effect.contains("file")
            || expected_effect.contains("directory")
            || expected_effect.contains("folder")
            || expected_effect.contains("path");
        let has_mutation_word = expected_effect.contains("delete")
            || expected_effect.contains("remove")
            || expected_effect.contains("rename")
            || expected_effect.contains("move")
            || expected_effect.contains("create")
            || expected_effect.contains("write")
            || expected_effect.contains("modify");

        // FileMutation requires both "file" concept AND mutation action
        if has_file_word && has_mutation_word {
            return EffectType::FileMutation;
        }

        // Check for external API calls
        if tool_name.contains("http") || tool_name.contains("api") || tool_name.contains("fetch") {
            return EffectType::ExternalApiCall;
        }

        // Check for external communication (email, slack, etc.)
        if tool_name.contains("email")
            || tool_name.contains("slack")
            || tool_name.contains("message")
            || tool_name.contains("send")
        {
            return EffectType::ExternalCommunication;
        }

        // Check for scheduling
        if tool_name.contains("schedule")
            || tool_name.contains("cron")
            || tool_name.contains("timer")
        {
            return EffectType::Scheduling;
        }

        // Check for administrative changes
        if tool_name.contains("admin")
            || tool_name.contains("user")
            || tool_name.contains("permission")
            || tool_name.contains("role")
        {
            return EffectType::AdministrativeChange;
        }

        // Check for draft creation
        if expected_effect.contains("draft") || expected_effect.contains("create") {
            return EffectType::DraftCreation;
        }

        // Default to ReadOnlyAnalysis for R0 with read-like effects
        if matches!(
            proposal.requested_rollback_class,
            RollbackClass::R0NativeReversible
        ) {
            return EffectType::ReadOnlyAnalysis;
        }

        // For R3 (IrreversibleHighConsequence), be conservative
        if matches!(
            proposal.requested_rollback_class,
            RollbackClass::R3IrreversibleHighConsequence
        ) {
            return EffectType::AdministrativeChange;
        }

        EffectType::ReadOnlyAnalysis
    }

    /// Check if the inferred effect matches any forbidden outcome. Returns Some(reason) if denied.
    pub fn check_forbidden_outcomes(
        inferred: &EffectType,
        forbidden_outcomes: &[OutcomeClause],
    ) -> Option<String> {
        for clause in forbidden_outcomes {
            if mem::discriminant(inferred) == mem::discriminant(&clause.effect_type) {
                return Some(format!(
                    "forbidden outcome detected: {} (effect_type={:?})",
                    clause.description, clause.effect_type
                ));
            }
        }
        None
    }

    /// Check if inferred effect is allowed (when allowed_outcomes is non-empty).
    /// Returns a warning message if the effect is not in the allowed list.
    pub fn check_allowed_outcomes(
        inferred: &EffectType,
        allowed_outcomes: &[OutcomeClause],
    ) -> Option<String> {
        // Empty allowed_outcomes means no restrictions
        if allowed_outcomes.is_empty() {
            return None;
        }

        for clause in allowed_outcomes {
            if mem::discriminant(inferred) == mem::discriminant(&clause.effect_type) {
                return None; // Found a match, no warning
            }
        }

        // Advisory mismatch - warn but don't deny
        Some(format!(
            "advisory mismatch: inferred effect {:?} is not in allowed outcomes",
            inferred
        ))
    }
}

#[derive(Debug, Default)]
pub struct StaticPdpEngine;

#[async_trait]
impl PdpEngine for StaticPdpEngine {
    async fn evaluate(
        &self,
        intent: &IntentEnvelope,
        proposal: &ActionProposal,
        trust: &TrustContextSummary,
    ) -> anyhow::Result<EvaluateProposalResponse> {
        let mut matched_rule_ids = Vec::new();
        let mut warnings = Vec::new();

        // P0: Scope mismatch deny - if intent has no resource scope and the proposal
        // requests a mutating rollback class (non-R0), deny explicitly.
        // This is a conservative default: mutations require explicit resource scope.
        let is_mutation = !matches!(
            proposal.requested_rollback_class,
            RollbackClass::R0NativeReversible
        );
        if intent.resource_scope.is_empty() && is_mutation {
            matched_rule_ids.push("scope.mismatch.empty.scope".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::Deny,
                reason: "scope mismatch: no resources authorized for mutation action".to_string(),
                matched_rule_ids,
                warnings,
            });
        }

        // Invariant 2: Critical risk with no approval mode requires approval
        if proposal.estimated_risk == RiskTier::Critical
            && matches!(intent.approval_mode, ApprovalMode::None)
        {
            matched_rule_ids.push("approval.critical.risk.required".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::RequireApproval,
                reason: "critical risk tier requires explicit approval mode".to_string(),
                matched_rule_ids,
                warnings,
            });
        }

        if trust.taint_score >= 70
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
                warnings,
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
                warnings,
            });
        }

        if matches!(intent.approval_mode, ferrum_proto::ApprovalMode::DraftOnly) {
            matched_rule_ids.push("draft.only.intent".to_string());
            warnings.push("intent enforces draft-only mode".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::AllowDraftOnly,
                reason: "intent requires draft-only execution".to_string(),
                matched_rule_ids,
                warnings,
            });
        }

        // U1: Outcome-aware governance - infer effect type and check against outcomes
        let inferred_effect = Self::infer_effect_type(proposal);

        // Check for explicit forbidden outcome match - this is a DENY
        if let Some(reason) =
            Self::check_forbidden_outcomes(&inferred_effect, &intent.forbidden_outcomes)
        {
            matched_rule_ids.push("outcome.forbidden".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::Deny,
                reason,
                matched_rule_ids,
                warnings,
            });
        }

        // Check for advisory allowed outcome mismatch - this is a WARNING
        if let Some(warning) =
            Self::check_allowed_outcomes(&inferred_effect, &intent.allowed_outcomes)
        {
            matched_rule_ids.push("outcome.advisory.mismatch".to_string());
            warnings.push(warning);
        }

        matched_rule_ids.push("allow.default".to_string());
        Ok(EvaluateProposalResponse {
            decision: Decision::Allow,
            reason: "proposal passed default scaffold policy".to_string(),
            matched_rule_ids,
            warnings,
        })
    }

    async fn evaluate_outcome(
        &self,
        intent: &IntentEnvelope,
        report: &OutcomeReport,
    ) -> anyhow::Result<EvaluateOutcomeResponse> {
        let mut matched_rule_ids = Vec::new();
        let mut warnings = Vec::new();

        // Check forbidden outcomes — DENY alignment if matched
        if let Some(reason) =
            Self::check_forbidden_outcomes(&report.actual_effect, &intent.forbidden_outcomes)
        {
            matched_rule_ids.push("outcome.forbidden".to_string());
            return Ok(EvaluateOutcomeResponse {
                aligned: false,
                reason,
                matched_rule_ids,
                warnings,
            });
        }

        // Check allowed outcomes — advisory mismatch
        if let Some(warning) =
            Self::check_allowed_outcomes(&report.actual_effect, &intent.allowed_outcomes)
        {
            matched_rule_ids.push("outcome.advisory.mismatch".to_string());
            warnings.push(warning);
        }

        // Adapter failure = not aligned
        if !report.adapter_success {
            matched_rule_ids.push("outcome.adapter.failure".to_string());
            return Ok(EvaluateOutcomeResponse {
                aligned: false,
                reason: "adapter reported execution failure".to_string(),
                matched_rule_ids,
                warnings,
            });
        }

        matched_rule_ids.push("outcome.aligned".to_string());
        Ok(EvaluateOutcomeResponse {
            aligned: true,
            reason: "outcome matches intent expectations".to_string(),
            matched_rule_ids,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ApprovalMode, EffectType, IntentEnvelope, IntentStatus, JsonMap, OutcomeClause,
        OutcomeReport, RiskTier, TimeBudget, TrustContextSummary,
    };
    use ferrum_proto::{ExecutionId, IntentId, PrincipalId, ProposalId};

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
            intent_id: IntentId::new(),
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
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
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
        }
    }

    async fn make_intent_with_outcomes(
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
            intent_id: IntentId::new(),
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
            intent_id: IntentId::new(),
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
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 1: scope deny — empty resource_scope + non-R0 mutation
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_scope_deny_empty_scope() {
        let engine = StaticPdpEngine;
        let intent = make_intent(); // resource_scope is empty
        let proposal = make_proposal(
            "git.commit",
            "create a commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::Deny));
        assert!(result.reason.contains("scope mismatch"));
        assert!(
            result
                .matched_rule_ids
                .contains(&"scope.mismatch.empty.scope".to_string())
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 2: taint quarantine — taint_score >= 70 + non-R0 mutation
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_taint_quarantine() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        // non-empty resource_scope bypasses scope-deny guard
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let proposal = make_proposal(
            "git.commit",
            "create a commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let mut trust = make_trust_context();
        trust.taint_score = 85;
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::Quarantine));
        assert!(result.reason.contains("taint score"));
        assert!(
            result
                .matched_rule_ids
                .contains(&"quarantine.high.taint.mutation".to_string())
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 3: R3 require approval
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_r3_require_approval() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let proposal = make_proposal(
            "git.push",
            "push to remote",
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::RequireApproval));
        assert!(result.reason.contains("R3"));
        assert!(
            result
                .matched_rule_ids
                .contains(&"approval.r3.required".to_string())
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 4: draft-only intent
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_draft_only() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::DraftOnly;
        let proposal = make_proposal(
            "file.write",
            "write a file",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::AllowDraftOnly));
        assert!(result.reason.contains("draft-only"));
        assert!(
            result
                .matched_rule_ids
                .contains(&"draft.only.intent".to_string())
        );
        assert!(!result.warnings.is_empty()); // "intent enforces draft-only mode"
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 5: forbidden outcome
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_forbidden_outcome() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(
            vec![],
            vec![OutcomeClause {
                id: "no-git".to_string(),
                effect_type: EffectType::GitMutation,
                description: "no git mutations allowed".to_string(),
                required: true,
            }],
        )
        .await;
        // tool/inference will produce GitMutation
        let proposal = make_proposal(
            "git.commit",
            "commit changes",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::Deny));
        assert!(result.reason.contains("forbidden outcome"));
        assert!(
            result
                .matched_rule_ids
                .contains(&"outcome.forbidden".to_string())
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 6: advisory mismatch (allowed_outcomes non-empty, no match)
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_advisory_mismatch() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(
            vec![OutcomeClause {
                id: "file-only".to_string(),
                effect_type: EffectType::FileMutation,
                description: "file changes only".to_string(),
                required: false,
            }],
            vec![],
        )
        .await;
        // inferred effect will be ExternalApiCall (no file/http keywords)
        let proposal = make_proposal(
            "fetch.http",
            "call external API",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::Allow)); // warn but allow
        assert!(
            result
                .matched_rule_ids
                .contains(&"outcome.advisory.mismatch".to_string())
        );
        assert!(!result.warnings.is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("advisory mismatch"))
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Branch 7: default allow
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_default_allow() {
        let engine = StaticPdpEngine;
        let intent = make_intent(); // empty allowed/forbidden outcomes
        let proposal = make_proposal(
            "read.file",
            "read a file",
            RollbackClass::R0NativeReversible,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.reason.contains("default scaffold"));
        assert!(
            result
                .matched_rule_ids
                .contains(&"allow.default".to_string())
        );
        assert!(result.warnings.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Ordering test 1: R3 supersedes DraftOnly
    // (R3 check fires before DraftOnly check in evaluate())
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_ordering_r3_before_draft_only() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::DraftOnly;
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        // R3 + DraftOnly intent — R3 check should fire first → RequireApproval
        let proposal = make_proposal(
            "git.push",
            "push to remote",
            RollbackClass::R3IrreversibleHighConsequence,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::RequireApproval));
        assert!(
            result
                .matched_rule_ids
                .contains(&"approval.r3.required".to_string())
        );
        // DraftOnly rule must NOT appear (R3 took priority)
        assert!(
            !result
                .matched_rule_ids
                .contains(&"draft.only.intent".to_string())
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Ordering test 2: scope deny supersedes DraftOnly
    // (scope deny check fires before DraftOnly check in evaluate())
    // ─────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_evaluate_ordering_scope_deny_before_draft_only() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::DraftOnly;
        intent.resource_scope = vec![]; // empty scope → scope deny fires
        // Non-R0 mutation with empty scope + DraftOnly intent
        let proposal = make_proposal(
            "git.commit",
            "create commit",
            RollbackClass::R1SnapshotRecoverable,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::Deny));
        assert!(
            result
                .matched_rule_ids
                .contains(&"scope.mismatch.empty.scope".to_string())
        );
        // DraftOnly rule must NOT appear (scope deny took priority)
        assert!(
            !result
                .matched_rule_ids
                .contains(&"draft.only.intent".to_string())
        );
    }

    #[tokio::test]
    async fn test_evaluate_outcome_aligned() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(
            vec![OutcomeClause {
                id: "clause-1".to_string(),
                effect_type: EffectType::FileMutation,
                description: "file changes".to_string(),
                required: true,
            }],
            vec![],
        )
        .await;
        let report = OutcomeReport {
            execution_id: ExecutionId::new(),
            actual_effect: EffectType::FileMutation,
            description: "file was modified".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };
        let result = engine.evaluate_outcome(&intent, &report).await.unwrap();
        assert!(result.aligned);
        assert!(result.warnings.is_empty());
        assert!(
            result
                .matched_rule_ids
                .contains(&"outcome.aligned".to_string())
        );
    }

    #[tokio::test]
    async fn test_evaluate_outcome_forbidden() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(
            vec![],
            vec![OutcomeClause {
                id: "clause-1".to_string(),
                effect_type: EffectType::GitMutation,
                description: "no git changes".to_string(),
                required: true,
            }],
        )
        .await;
        let report = OutcomeReport {
            execution_id: ExecutionId::new(),
            actual_effect: EffectType::GitMutation,
            description: "git was modified".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };
        let result = engine.evaluate_outcome(&intent, &report).await.unwrap();
        assert!(!result.aligned);
        assert!(
            result
                .matched_rule_ids
                .contains(&"outcome.forbidden".to_string())
        );
    }

    #[tokio::test]
    async fn test_evaluate_outcome_adapter_failure() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(vec![], vec![]).await;
        let report = OutcomeReport {
            execution_id: ExecutionId::new(),
            actual_effect: EffectType::ReadOnlyAnalysis,
            description: "failed".to_string(),
            result_digest: None,
            adapter_success: false,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };
        let result = engine.evaluate_outcome(&intent, &report).await.unwrap();
        assert!(!result.aligned);
        assert!(
            result
                .matched_rule_ids
                .contains(&"outcome.adapter.failure".to_string())
        );
    }

    #[tokio::test]
    async fn test_evaluate_outcome_advisory_mismatch() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(
            vec![OutcomeClause {
                id: "clause-1".to_string(),
                effect_type: EffectType::FileMutation,
                description: "expected file changes".to_string(),
                required: true,
            }],
            vec![],
        )
        .await;
        let report = OutcomeReport {
            execution_id: ExecutionId::new(),
            actual_effect: EffectType::ReadOnlyAnalysis,
            description: "read-only analysis happened".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };
        let result = engine.evaluate_outcome(&intent, &report).await.unwrap();
        assert!(result.aligned); // still aligned — advisory only
        assert!(!result.warnings.is_empty());
        assert!(
            result
                .matched_rule_ids
                .contains(&"outcome.advisory.mismatch".to_string())
        );
    }

    #[tokio::test]
    async fn test_evaluate_outcome_empty_allowed() {
        let engine = StaticPdpEngine;
        let intent = make_intent_with_outcomes(vec![], vec![]).await;
        let report = OutcomeReport {
            execution_id: ExecutionId::new(),
            actual_effect: EffectType::ExternalApiCall,
            description: "api call happened".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };
        let result = engine.evaluate_outcome(&intent, &report).await.unwrap();
        assert!(result.aligned);
        assert!(result.warnings.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Invariant 2: Critical risk + None approval => RequireApproval
    // ─────────────────────────────────────────────────────────────────────────

    /// Invariant 2: Critical risk with no approval mode requires approval.
    #[tokio::test]
    async fn test_evaluate_invariant2_critical_risk_none_approval() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::None;
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
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
        assert!(matches!(result.decision, Decision::RequireApproval));
        assert!(
            result
                .matched_rule_ids
                .contains(&"approval.critical.risk.required".to_string())
        );
    }

    /// Invariant 2: Critical risk with explicit approval mode should pass through
    /// to other rules (not blocked by Invariant 2).
    #[tokio::test]
    async fn test_evaluate_invariant2_critical_risk_with_approval() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::Required; // explicit approval
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
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
        // Should NOT trigger approval.critical.risk.required since approval_mode != None
        assert!(
            !result
                .matched_rule_ids
                .contains(&"approval.critical.risk.required".to_string())
        );
        // And should allow (other rules pass)
        assert!(matches!(result.decision, Decision::Allow));
    }

    /// Invariant 2 + R3 ordering: Critical+None should be checked before R3.
    /// Critical+None should trigger RequireApproval via Invariant 2, not R3.
    #[tokio::test]
    async fn test_evaluate_invariant2_ordering_before_r3() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::None;
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        // R3 + Critical + None - Invariant 2 should fire first
        let proposal = make_proposal_with_risk(
            "git.push",
            "push to remote",
            RollbackClass::R3IrreversibleHighConsequence,
            RiskTier::Critical,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        assert!(matches!(result.decision, Decision::RequireApproval));
        // Invariant 2 fires before R3 rule
        assert!(
            result
                .matched_rule_ids
                .contains(&"approval.critical.risk.required".to_string())
        );
        // R3 rule should NOT also fire (Invariant 2 took priority)
        assert!(
            !result
                .matched_rule_ids
                .contains(&"approval.r3.required".to_string())
        );
    }

    /// Invariant 2 + Draft ordering: Critical+None should be checked before DraftOnly.
    /// When approval_mode == DraftOnly (not None), Invariant 2 does NOT fire;
    /// the DraftOnly rule fires instead.
    #[tokio::test]
    async fn test_evaluate_invariant2_draft_only_mode_not_blocked() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::DraftOnly;
        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        // Critical + DraftOnly - Invariant 2 does NOT fire because approval_mode != None
        // Instead DraftOnly rule fires
        let proposal = make_proposal_with_risk(
            "file.write",
            "write a file",
            RollbackClass::R0NativeReversible,
            RiskTier::Critical,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        // DraftOnly rule should fire since approval_mode == DraftOnly
        assert!(matches!(result.decision, Decision::AllowDraftOnly));
        assert!(
            result
                .matched_rule_ids
                .contains(&"draft.only.intent".to_string())
        );
        // Invariant 2 should NOT fire (approval_mode is DraftOnly, not None)
        assert!(
            !result
                .matched_rule_ids
                .contains(&"approval.critical.risk.required".to_string())
        );
    }

    /// Invariant 2 + scope-deny ordering: scope-deny fires before Invariant 2.
    #[tokio::test]
    async fn test_evaluate_invariant2_ordering_after_scope_deny() {
        let engine = StaticPdpEngine;
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::None;
        // Empty resource_scope triggers scope-deny
        let proposal = make_proposal_with_risk(
            "db.execute",
            "execute database query",
            RollbackClass::R1SnapshotRecoverable,
            RiskTier::Critical,
        );
        let trust = make_trust_context();
        let result = engine.evaluate(&intent, &proposal, &trust).await.unwrap();
        // Scope-deny fires first
        assert!(matches!(result.decision, Decision::Deny));
        assert!(
            result
                .matched_rule_ids
                .contains(&"scope.mismatch.empty.scope".to_string())
        );
        // Invariant 2 should NOT fire since scope-deny already denied
        assert!(
            !result
                .matched_rule_ids
                .contains(&"approval.critical.risk.required".to_string())
        );
    }
}
