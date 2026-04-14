use async_trait::async_trait;
use ferrum_proto::{
    ActionProposal, Decision, EffectType, EvaluateProposalResponse, IntentEnvelope, OutcomeClause,
    RollbackClass, Timestamp, TrustContextSummary,
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

    /// Check if a clause is temporally active at the given timestamp.
    /// Returns true if no temporal constraints are present, or if the
    /// timestamp falls within the validity window.
    fn is_temporally_valid(clause: &OutcomeClause, timestamp: &Timestamp) -> bool {
        if let Some(ref temporal) = clause.temporal {
            temporal.is_active_at(timestamp)
        } else {
            true
        }
    }

    /// Check if the proposal's effect matches any forbidden outcome in the intent.
    /// Returns Some(reason) if a forbidden outcome is matched (should deny).
    fn check_forbidden_outcomes(
        &self,
        intent: &IntentEnvelope,
        proposal_effect: &EffectType,
        timestamp: &Timestamp,
    ) -> Option<String> {
        for forbidden in &intent.forbidden_outcomes {
            // H1.2a: Skip temporally inactive forbidden outcomes
            if !Self::is_temporally_valid(forbidden, timestamp) {
                continue;
            }
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
        timestamp: &Timestamp,
    ) -> (bool, Vec<String>) {
        // If intent has no allowed_outcomes specified, any effect is acceptable
        if intent.allowed_outcomes.is_empty() {
            return (true, Vec::new());
        }

        let mut aligned = false;
        let mut warnings = Vec::new();

        for allowed in &intent.allowed_outcomes {
            // H1.2a: Skip temporally inactive allowed outcomes when checking alignment
            if !Self::is_temporally_valid(allowed, timestamp) {
                continue;
            }
            if std::mem::discriminant(&allowed.effect_type)
                == std::mem::discriminant(proposal_effect)
            {
                aligned = true;
                break;
            }
        }

        if !aligned {
            // Collect all temporally active allowed effect types for the warning message
            let allowed_effects: Vec<String> = intent
                .allowed_outcomes
                .iter()
                .filter(|a| Self::is_temporally_valid(a, timestamp))
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
        let timestamp = &proposal.created_at;

        // First check: explicit forbidden outcome match → deny
        if let Some(reason) = self.check_forbidden_outcomes(intent, &proposal_effect, timestamp) {
            return (Some(reason), Vec::new());
        }

        // Second check: allowed outcome alignment → advisory warning if not aligned
        let (is_aligned, warnings) =
            self.check_allowed_outcomes(intent, &proposal_effect, timestamp);

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

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        IntentId, OutcomeTemporalConstraints, PrincipalId, ProposalId, RiskTier, RollbackClass,
    };

    /// Helper to create a proposal at a specific timestamp
    fn make_proposal_at(intent_id: IntentId, effect: &str, timestamp: Timestamp) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id,
            step_index: 1,
            title: "test proposal".to_string(),
            tool_name: "test.tool".to_string(),
            server_name: "test".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: effect.to_string(),
            estimated_risk: RiskTier::Medium,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            decision: None,
            taint_inputs: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            created_at: timestamp,
        }
    }

    /// Helper to create an intent with forbidden FileMutation
    fn make_intent_with_forbidden(intent_id: IntentId) -> IntentEnvelope {
        IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![OutcomeClause {
                id: "forbid-mutation".to_string(),
                description: "forbid file mutations".to_string(),
                effect_type: EffectType::FileMutation,
                required: true,
                selectors: None,
                temporal: None,
            }],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
        }
    }

    /// Helper to create a temporal constraint that is valid from start to end
    fn make_temporal(
        start: Option<Timestamp>,
        end: Option<Timestamp>,
    ) -> Option<OutcomeTemporalConstraints> {
        Some(OutcomeTemporalConstraints {
            valid_from: start,
            valid_until: end,
        })
    }

    // =============================================================================
    // H1.2a: Temporal constraint tests
    // =============================================================================

    #[test]
    fn test_temporal_constraints_no_temporal_is_always_valid() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();
        let intent = make_intent_with_forbidden(intent_id);

        // Proposal at any time should match the forbidden outcome (no temporal constraints)
        let timestamps = vec![
            chrono::Utc::now() - chrono::Duration::days(10),
            chrono::Utc::now(),
            chrono::Utc::now() + chrono::Duration::days(10),
        ];

        for ts in timestamps {
            let proposal = make_proposal_at(intent_id, "write a file", ts);
            let (deny_reason, _) = engine.assess_outcome_alignment(&intent, &proposal);
            assert!(
                deny_reason.is_some(),
                "proposal at {:?} should match forbidden (no temporal constraints)",
                ts
            );
        }
    }

    #[test]
    fn test_temporal_constraints_valid_within_window() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();

        let now = chrono::Utc::now();
        let valid_from = now - chrono::Duration::hours(1);
        let valid_until = now + chrono::Duration::hours(1);

        let intent = IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![OutcomeClause {
                id: "forbid-mutation".to_string(),
                description: "forbid file mutations".to_string(),
                effect_type: EffectType::FileMutation,
                required: true,
                selectors: None,
                temporal: make_temporal(Some(valid_from), Some(valid_until)),
            }],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: now - chrono::Duration::days(1),
            expires_at: now + chrono::Duration::days(1),
        };

        // Proposal within the window should match forbidden
        let proposal_in_window = make_proposal_at(intent_id, "write a file", now);
        let (deny_reason, _) = engine.assess_outcome_alignment(&intent, &proposal_in_window);
        assert!(
            deny_reason.is_some(),
            "proposal within temporal window should match forbidden"
        );
    }

    #[test]
    fn test_temporal_constraints_not_yet_valid() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();

        let now = chrono::Utc::now();
        let valid_from = now + chrono::Duration::hours(1); // starts in the future
        let valid_until = now + chrono::Duration::hours(2);

        let intent = IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![OutcomeClause {
                id: "forbid-mutation".to_string(),
                description: "forbid file mutations".to_string(),
                effect_type: EffectType::FileMutation,
                required: true,
                selectors: None,
                temporal: make_temporal(Some(valid_from), Some(valid_until)),
            }],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: now - chrono::Duration::days(1),
            expires_at: now + chrono::Duration::days(1),
        };

        // Proposal before the window should NOT match forbidden (temporal skip)
        let proposal_before = make_proposal_at(intent_id, "write a file", now);
        let (deny_reason, warnings) = engine.assess_outcome_alignment(&intent, &proposal_before);
        assert!(
            deny_reason.is_none(),
            "proposal before temporal window should NOT match forbidden"
        );
        // Note: When allowed_outcomes is empty, PDP returns "aligned" (backward compatible).
        // This means no warning is generated even though no explicit allowed outcome exists.
        // The temporal skip of forbidden is the key behavior being tested.
        assert!(
            warnings.is_empty(),
            "with empty allowed_outcomes, no warning is generated (backward compatible)"
        );
    }

    #[test]
    fn test_temporal_constraints_expired() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();

        let now = chrono::Utc::now();
        let valid_from = now - chrono::Duration::hours(2);
        let valid_until = now - chrono::Duration::hours(1); // already expired

        let intent = IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![OutcomeClause {
                id: "forbid-mutation".to_string(),
                description: "forbid file mutations".to_string(),
                effect_type: EffectType::FileMutation,
                required: true,
                selectors: None,
                temporal: make_temporal(Some(valid_from), Some(valid_until)),
            }],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: now - chrono::Duration::days(1),
            expires_at: now + chrono::Duration::days(1),
        };

        // Proposal after expiry should NOT match forbidden (temporal skip)
        let proposal_after = make_proposal_at(intent_id, "write a file", now);
        let (deny_reason, warnings) = engine.assess_outcome_alignment(&intent, &proposal_after);
        assert!(
            deny_reason.is_none(),
            "proposal after expiration should NOT match forbidden"
        );
        // Note: When allowed_outcomes is empty, PDP returns "aligned" (backward compatible).
        // This means no warning is generated even though no explicit allowed outcome exists.
        // The temporal skip of forbidden is the key behavior being tested.
        assert!(
            warnings.is_empty(),
            "with empty allowed_outcomes, no warning is generated (backward compatible)"
        );
    }

    #[test]
    fn test_temporal_constraints_valid_from_only() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();

        let now = chrono::Utc::now();
        let valid_from = now - chrono::Duration::hours(1); // started 1 hour ago, no end

        let intent = IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![OutcomeClause {
                id: "forbid-mutation".to_string(),
                description: "forbid file mutations".to_string(),
                effect_type: EffectType::FileMutation,
                required: true,
                selectors: None,
                temporal: make_temporal(Some(valid_from), None), // valid from start, no end
            }],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: now - chrono::Duration::days(1),
            expires_at: now + chrono::Duration::days(1),
        };

        // Proposal now (after valid_from) should match forbidden
        let proposal_now = make_proposal_at(intent_id, "write a file", now);
        let (deny_reason, _) = engine.assess_outcome_alignment(&intent, &proposal_now);
        assert!(
            deny_reason.is_some(),
            "proposal after valid_from should match forbidden"
        );

        // Proposal 2 hours ago (before valid_from) should NOT match
        let proposal_before =
            make_proposal_at(intent_id, "write a file", now - chrono::Duration::hours(2));
        let (deny_reason_before, _) = engine.assess_outcome_alignment(&intent, &proposal_before);
        assert!(
            deny_reason_before.is_none(),
            "proposal before valid_from should NOT match forbidden"
        );
    }

    #[test]
    fn test_temporal_constraints_valid_until_only() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();

        let now = chrono::Utc::now();
        let valid_until = now + chrono::Duration::hours(1); // expires in 1 hour, no start

        let intent = IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![OutcomeClause {
                id: "forbid-mutation".to_string(),
                description: "forbid file mutations".to_string(),
                effect_type: EffectType::FileMutation,
                required: true,
                selectors: None,
                temporal: make_temporal(None, Some(valid_until)), // no start, valid until end
            }],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: now - chrono::Duration::days(1),
            expires_at: now + chrono::Duration::days(1),
        };

        // Proposal now (before valid_until) should match forbidden
        let proposal_now = make_proposal_at(intent_id, "write a file", now);
        let (deny_reason, _) = engine.assess_outcome_alignment(&intent, &proposal_now);
        assert!(
            deny_reason.is_some(),
            "proposal before valid_until should match forbidden"
        );

        // Proposal in 2 hours (after valid_until) should NOT match
        let proposal_after =
            make_proposal_at(intent_id, "write a file", now + chrono::Duration::hours(2));
        let (deny_reason_after, _) = engine.assess_outcome_alignment(&intent, &proposal_after);
        assert!(
            deny_reason_after.is_none(),
            "proposal after valid_until should NOT match forbidden"
        );
    }

    #[test]
    fn test_temporal_constraints_allowed_outcome_temporal() {
        let engine = StaticPdpEngine;
        let intent_id = IntentId::new();

        let now = chrono::Utc::now();
        let valid_from = now + chrono::Duration::hours(1); // starts in the future
        let valid_until = now + chrono::Duration::hours(2);

        // Intent with ALLOWED FileMutation that starts in the future
        let intent = IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test".to_string(),
            normalized_goal: "test".to_string(),
            allowed_outcomes: vec![OutcomeClause {
                id: "allow-mutation".to_string(),
                description: "allow file mutations after 1 hour".to_string(),
                effect_type: EffectType::FileMutation,
                required: false,
                selectors: None,
                temporal: make_temporal(Some(valid_from), Some(valid_until)),
            }],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Medium,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30_000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            policy_bundle_fingerprint: None,
            created_at: now - chrono::Duration::days(1),
            expires_at: now + chrono::Duration::days(1),
        };

        // Proposal now (before allowed outcome is valid) should NOT be aligned
        let proposal_now = make_proposal_at(intent_id, "write a file", now);
        let (_, warnings) = engine.assess_outcome_alignment(&intent, &proposal_now);
        assert!(
            !warnings.is_empty(),
            "proposal before allowed temporal window should get warning"
        );

        // Proposal within window should be aligned (no warning)
        let proposal_in_window = make_proposal_at(
            intent_id,
            "write a file",
            now + chrono::Duration::minutes(90),
        );
        let (_, warnings_in_window) = engine.assess_outcome_alignment(&intent, &proposal_in_window);
        assert!(
            warnings_in_window.is_empty(),
            "proposal within allowed temporal window should be aligned"
        );
    }

    #[test]
    fn test_temporal_constraints_invalid_range_rejected() {
        // valid_from >= valid_until should be rejected by validation
        let now = chrono::Utc::now();
        let temporal = OutcomeTemporalConstraints {
            valid_from: Some(now),
            valid_until: Some(now - chrono::Duration::hours(1)), // valid_from > valid_until
        };

        let err = temporal.validate();
        assert!(err.is_some(), "invalid temporal range should be rejected");
        assert!(
            err.unwrap().contains("valid_from"),
            "error should mention valid_from"
        );
    }

    #[test]
    fn test_temporal_constraints_valid_range_accepted() {
        let now = chrono::Utc::now();
        let temporal = OutcomeTemporalConstraints {
            valid_from: Some(now - chrono::Duration::hours(1)),
            valid_until: Some(now + chrono::Duration::hours(1)),
        };

        let err = temporal.validate();
        assert!(err.is_none(), "valid temporal range should be accepted");
    }
}
