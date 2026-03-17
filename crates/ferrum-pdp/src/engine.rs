use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, Decision, EvaluateProposalResponse, IntentEnvelope, RollbackClass,
    TrustContextSummary,
};

#[async_trait]
pub trait PdpEngine: Send + Sync {
    async fn evaluate(
        &self,
        intent: &IntentEnvelope,
        proposal: &ActionProposal,
        trust: &TrustContextSummary,
    ) -> anyhow::Result<EvaluateProposalResponse>;
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

        if trust.taint_score >= 70
            && !matches!(proposal.requested_rollback_class, RollbackClass::R0NativeReversible)
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

        matched_rule_ids.push("allow.default".to_string());
        Ok(EvaluateProposalResponse {
            decision: Decision::Allow,
            reason: "proposal passed default scaffold policy".to_string(),
            matched_rule_ids,
            warnings,
        })
    }
}
