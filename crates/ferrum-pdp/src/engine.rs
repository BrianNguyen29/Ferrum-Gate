use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, Decision, EffectType, EvaluateProposalResponse, IntentEnvelope, RollbackClass,
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

impl StaticPdpEngine {
    /// Infer the EffectType from a proposal's expected_effect description.
    /// This uses keyword matching to classify the effect.
    pub fn infer_effect_type(effect_description: &str) -> EffectType {
        let lower = effect_description.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
            .collect();

        // Helper to check if any word exactly matches a read-only keyword
        let has_read_word = words.iter().any(|w| {
            *w == "read"
                || *w == "inspect"
                || *w == "view"
                || *w == "get"
                || *w == "fetch"
                || *w == "list"
                || *w == "search"
                || *w == "query"
                || *w == "analyze"
                || *w == "check"
                || *w == "query"
        });

        // Helper to check if any word exactly matches a mutating keyword
        let has_mutate_word = words.iter().any(|w| {
            *w == "write"
                || *w == "create"
                || *w == "delete"
                || *w == "remove"
                || *w == "modify"
                || *w == "update"
                || *w == "insert"
                || *w == "drop"
                || *w == "alter"
                || *w == "mutate"
                || *w == "commit"
                || *w == "push"
                || *w == "send"
        });

        let has_git_word = words
            .iter()
            .any(|w| *w == "git" || *w == "commit" || *w == "push" || *w == "merge");
        // Database-specific keywords only (not "delete"/"insert"/"update" which are handled by mutate_word)
        let has_db_word = words.iter().any(|w| {
            *w == "sql"
                || *w == "database"
                || *w == "db"
                || *w == "table"
                || *w == "row"
                || *w == "column"
        });
        let has_api_word = words
            .iter()
            .any(|w| *w == "api" || *w == "http" || *w == "request" || *w == "post");
        let has_comm_word = words
            .iter()
            .any(|w| *w == "email" || *w == "send" || *w == "message" || *w == "notify");
        let has_schedule_word = words
            .iter()
            .any(|w| *w == "schedule" || *w == "cron" || *w == "timer" || *w == "delay");
        let has_admin_word = words
            .iter()
            .any(|w| *w == "admin" || *w == "config" || *w == "setting" || *w == "permission");

        // Priority: mutating > read-only > unknown (treat unknown as mutating for fail-closed)
        if has_git_word {
            EffectType::GitMutation
        } else if has_db_word {
            EffectType::DatabaseMutation
        } else if has_api_word {
            EffectType::ExternalApiCall
        } else if has_comm_word {
            EffectType::ExternalCommunication
        } else if has_schedule_word {
            EffectType::Scheduling
        } else if has_admin_word {
            EffectType::AdministrativeChange
        } else if has_mutate_word {
            EffectType::FileMutation
        } else if has_read_word {
            EffectType::ReadOnlyAnalysis
        } else {
            // Unknown effect - bias toward mutating (fail-closed)
            EffectType::FileMutation
        }
    }

    /// Check if the proposal's effect matches any forbidden outcome in the intent.
    /// Returns Some(reason) if a forbidden outcome is matched (should deny).
    fn check_forbidden_outcomes(
        &self,
        intent: &IntentEnvelope,
        proposal_effect: &EffectType,
    ) -> Option<String> {
        for forbidden in &intent.forbidden_outcomes {
            if std::mem::discriminant(&forbidden.effect_type)
                == std::mem::discriminant(proposal_effect)
            {
                return Some(format!(
                    "proposal effect '{:?}' matches forbidden outcome '{}': {}",
                    proposal_effect, forbidden.id, forbidden.description
                ));
            }
        }
        None
    }

    /// Check if the proposal's effect aligns with any allowed outcome in the intent.
    /// Returns (is_aligned, warnings) where warnings contain advisory messages if not aligned.
    fn check_allowed_outcomes(
        &self,
        intent: &IntentEnvelope,
        proposal_effect: &EffectType,
    ) -> (bool, Vec<String>) {
        // If intent has no allowed_outcomes specified, any effect is acceptable
        if intent.allowed_outcomes.is_empty() {
            return (true, Vec::new());
        }

        let mut aligned = false;
        let mut warnings = Vec::new();

        for allowed in &intent.allowed_outcomes {
            if std::mem::discriminant(&allowed.effect_type)
                == std::mem::discriminant(proposal_effect)
            {
                aligned = true;
                break;
            }
        }

        if !aligned {
            // Collect all allowed effect types for the warning message
            let allowed_effects: Vec<String> = intent
                .allowed_outcomes
                .iter()
                .map(|a| format!("{:?}", a.effect_type))
                .collect();
            warnings.push(format!(
                "proposal effect '{:?}' does not match any allowed outcome; allowed effects: {}",
                proposal_effect,
                allowed_effects.join(", ")
            ));
        }

        (aligned, warnings)
    }

    /// Assess outcome alignment for a proposal against an intent's outcome clauses.
    /// Returns (deny_reason, warnings) where:
    /// - deny_reason is Some if the proposal should be denied (forbidden match)
    /// - warnings contains advisory messages for misalignment
    pub fn assess_outcome_alignment(
        &self,
        intent: &IntentEnvelope,
        proposal: &ActionProposal,
    ) -> (Option<String>, Vec<String>) {
        let proposal_effect = Self::infer_effect_type(&proposal.expected_effect);

        // First check: explicit forbidden outcome match → deny
        if let Some(reason) = self.check_forbidden_outcomes(intent, &proposal_effect) {
            return (Some(reason), Vec::new());
        }

        // Second check: allowed outcome alignment → advisory warning if not aligned
        let (is_aligned, warnings) = self.check_allowed_outcomes(intent, &proposal_effect);

        // If not aligned with allowed outcomes, it's an advisory warning only (unless there's already a deny reason)
        if !is_aligned {
            // Return the warnings for advisory case
            (None, warnings)
        } else {
            (None, Vec::new())
        }
    }
}

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

        // U1: Outcome-aware governance check
        // Check forbidden outcomes first (explicit deny), then allowed outcomes (advisory)
        let (deny_reason, outcome_warnings) = self.assess_outcome_alignment(intent, proposal);
        warnings.extend(outcome_warnings);

        if let Some(reason) = deny_reason {
            matched_rule_ids.push("forbidden.outcome.match".to_string());
            return Ok(EvaluateProposalResponse {
                decision: Decision::Deny,
                reason,
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

        matched_rule_ids.push("allow.default".to_string());
        Ok(EvaluateProposalResponse {
            decision: Decision::Allow,
            reason: "proposal passed default scaffold policy".to_string(),
            matched_rule_ids,
            warnings,
        })
    }
}
