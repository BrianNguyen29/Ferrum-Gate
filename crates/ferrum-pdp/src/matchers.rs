//! Pure policy bundle matcher/rule evaluation helpers.
//!
//! These helpers are stateless and operate only on the evaluation context
//! (intent, proposal, trust context). They are shared between the bundle PDP
//! engine and the gateway's active-bundle evaluation path.

use ferrum_proto::{
    ActionProposal, ApprovalMode, Decision, IntentEnvelope, Matcher, PolicyBundle, PolicyRule,
    RiskTier, RollbackClass, TrustContextSummary,
};

/// Total-order strictness rank for decisions, used when resolving same-priority
/// conflicts and selecting the stricter decision.
///
/// Order: Deny > Quarantine > RequireApproval > AllowDraftOnly > Allow.
pub fn decision_strictness_rank(decision: &Decision) -> u8 {
    match decision {
        Decision::Allow => 0,
        Decision::AllowDraftOnly => 1,
        Decision::RequireApproval => 2,
        Decision::Quarantine => 3,
        Decision::Deny => 4,
    }
}

/// Returns the stricter of two decisions according to the total order.
pub fn stricter_decision(a: &Decision, b: &Decision) -> Decision {
    if decision_strictness_rank(a) >= decision_strictness_rank(b) {
        a.clone()
    } else {
        b.clone()
    }
}

/// Evaluate all rules in a policy bundle, sorted by descending priority.
///
/// Returns `Some((matched_rule_id, decision, reason))` if a rule matches,
/// applying same-priority stricter-wins semantics. Returns `None` if no rule
/// matches, which callers should interpret as `Allow`.
pub fn evaluate_bundle_rules(
    bundle: &PolicyBundle,
    intent: &IntentEnvelope,
    proposal: &ActionProposal,
    trust: &TrustContextSummary,
) -> Option<(String, Decision, String)> {
    let mut rules: Vec<&PolicyRule> = bundle.rules.iter().collect();
    rules.sort_by_key(|rule| std::cmp::Reverse(rule.priority));

    let mut result: Option<(String, Decision, String)> = None;
    let mut current_priority: Option<i32> = None;

    for rule in rules {
        if current_priority.is_some_and(|p| p != rule.priority) && result.is_some() {
            // We have moved to a lower priority group and already have a match;
            // first-match-wins by priority group.
            break;
        }

        if evaluate_rule_matchers(rule, intent, proposal, trust) {
            let rule_id = format!("policy_bundle:{}:{}", bundle.bundle_id, rule.id);
            let reason = format!(
                "policy bundle {} matched rule {}: {}",
                bundle.bundle_id, rule.id, rule.description
            );

            if let Some((ref existing_id, ref existing_decision, _)) = result {
                let stricter = stricter_decision(existing_decision, &rule.decision);
                let combined_id = format!("{}+{}", existing_id, rule_id);
                result = Some((combined_id, stricter, reason));
            } else {
                result = Some((rule_id, rule.decision.clone(), reason));
            }
            current_priority = Some(rule.priority);
        }
    }

    result
}

/// Evaluate all matchers in a rule. All matchers must match for the rule to apply.
pub fn evaluate_rule_matchers(
    rule: &PolicyRule,
    intent: &IntentEnvelope,
    proposal: &ActionProposal,
    trust: &TrustContextSummary,
) -> bool {
    rule.matchers
        .iter()
        .all(|m| evaluate_matcher(m, intent, proposal, trust))
}

/// Evaluate a single matcher against the given context.
pub fn evaluate_matcher(
    matcher: &Matcher,
    intent: &IntentEnvelope,
    proposal: &ActionProposal,
    trust: &TrustContextSummary,
) -> bool {
    match matcher {
        Matcher::ScopeMismatch => {
            // True if intent has no resource scope and proposal is a mutation (non-R0)
            intent.resource_scope.is_empty()
                && !matches!(
                    proposal.requested_rollback_class,
                    RollbackClass::R0NativeReversible
                )
        }
        Matcher::TaintAtLeast { value } => trust.taint_score >= *value,
        Matcher::ActionIsMutation => !matches!(
            proposal.requested_rollback_class,
            RollbackClass::R0NativeReversible
        ),
        Matcher::RollbackClassEquals { value } => {
            let class_debug = format!("{:?}", proposal.requested_rollback_class);
            class_debug == *value
        }
        Matcher::ActionTypeEquals { value } => {
            let inferred_effect = crate::StaticPdpEngine::infer_effect_type(proposal);
            let effect_debug = format!("{:?}", inferred_effect);
            effect_debug == *value
        }
        Matcher::CriticalRiskNoApproval => {
            proposal.estimated_risk == RiskTier::Critical
                && matches!(intent.approval_mode, ApprovalMode::None)
        }
        Matcher::ApprovalModeEquals { value } => {
            let mode_debug = format!("{:?}", intent.approval_mode);
            mode_debug == *value
        }
        Matcher::Unknown { .. } => false,
    }
}

/// Build the default reason text for a matched invariant rule.
pub fn invariant_reason(rule_id: &str) -> String {
    match rule_id {
        "invariant.scope_mismatch" => {
            "scope mismatch: no resources authorized for mutation action".to_string()
        }
        "invariant.critical_risk_no_approval" => {
            "critical risk tier requires explicit approval mode".to_string()
        }
        other => format!("invariant rule matched: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActionProposal, IntentEnvelope, IntentStatus, JsonMap, PrincipalId, ProposalId, RiskTier,
        TimeBudget, TrustContextSummary,
    };

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
            metadata: JsonMap::new(),
            status: IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    fn make_proposal(rollback_class: RollbackClass) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "read.file".to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "read a file".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: rollback_class,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
            owner_actor_id: None,
        }
    }

    #[test]
    fn test_decision_strictness_ordering() {
        assert!(
            decision_strictness_rank(&Decision::Deny)
                > decision_strictness_rank(&Decision::Quarantine)
        );
        assert!(
            decision_strictness_rank(&Decision::Quarantine)
                > decision_strictness_rank(&Decision::RequireApproval)
        );
        assert!(
            decision_strictness_rank(&Decision::RequireApproval)
                > decision_strictness_rank(&Decision::AllowDraftOnly)
        );
        assert!(
            decision_strictness_rank(&Decision::AllowDraftOnly)
                > decision_strictness_rank(&Decision::Allow)
        );
    }

    #[test]
    fn test_stricter_decision() {
        assert_eq!(
            stricter_decision(&Decision::Allow, &Decision::Deny),
            Decision::Deny
        );
        assert_eq!(
            stricter_decision(&Decision::RequireApproval, &Decision::AllowDraftOnly),
            Decision::RequireApproval
        );
        assert_eq!(
            stricter_decision(&Decision::Quarantine, &Decision::Quarantine),
            Decision::Quarantine
        );
    }

    #[test]
    fn test_matcher_scope_mismatch() {
        let mut intent = make_intent();
        intent.resource_scope = vec![];
        let proposal = make_proposal(RollbackClass::R1SnapshotRecoverable);
        let trust = TrustContextSummary {
            taint_score: 0,
            ..intent.trust_context.clone()
        };
        assert!(evaluate_matcher(
            &Matcher::ScopeMismatch,
            &intent,
            &proposal,
            &trust
        ));

        intent.resource_scope = vec![ferrum_proto::ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        assert!(!evaluate_matcher(
            &Matcher::ScopeMismatch,
            &intent,
            &proposal,
            &trust
        ));

        // R0 read-only proposal does not trigger scope mismatch
        intent.resource_scope = vec![];
        let r0_proposal = make_proposal(RollbackClass::R0NativeReversible);
        assert!(!evaluate_matcher(
            &Matcher::ScopeMismatch,
            &intent,
            &r0_proposal,
            &trust
        ));
    }

    #[test]
    fn test_matcher_taint_at_least() {
        let intent = make_intent();
        let proposal = make_proposal(RollbackClass::R0NativeReversible);
        let mut trust = intent.trust_context.clone();
        trust.taint_score = 70;
        assert!(evaluate_matcher(
            &Matcher::TaintAtLeast { value: 70 },
            &intent,
            &proposal,
            &trust
        ));
        assert!(!evaluate_matcher(
            &Matcher::TaintAtLeast { value: 71 },
            &intent,
            &proposal,
            &trust
        ));
    }

    #[test]
    fn test_matcher_critical_risk_no_approval() {
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::None;
        let mut proposal = make_proposal(RollbackClass::R0NativeReversible);
        proposal.estimated_risk = RiskTier::Critical;
        let trust = intent.trust_context.clone();
        assert!(evaluate_matcher(
            &Matcher::CriticalRiskNoApproval,
            &intent,
            &proposal,
            &trust
        ));

        intent.approval_mode = ApprovalMode::Required;
        assert!(!evaluate_matcher(
            &Matcher::CriticalRiskNoApproval,
            &intent,
            &proposal,
            &trust
        ));

        intent.approval_mode = ApprovalMode::None;
        proposal.estimated_risk = RiskTier::High;
        assert!(!evaluate_matcher(
            &Matcher::CriticalRiskNoApproval,
            &intent,
            &proposal,
            &trust
        ));
    }

    #[test]
    fn test_matcher_approval_mode_equals() {
        let mut intent = make_intent();
        intent.approval_mode = ApprovalMode::DraftOnly;
        let proposal = make_proposal(RollbackClass::R0NativeReversible);
        let trust = intent.trust_context.clone();
        assert!(evaluate_matcher(
            &Matcher::ApprovalModeEquals {
                value: "DraftOnly".to_string()
            },
            &intent,
            &proposal,
            &trust
        ));
        assert!(!evaluate_matcher(
            &Matcher::ApprovalModeEquals {
                value: "Required".to_string()
            },
            &intent,
            &proposal,
            &trust
        ));
    }

    #[test]
    fn test_evaluate_bundle_rules_same_priority_stricter_wins() {
        let bundle = PolicyBundle {
            bundle_id: "test".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![
                PolicyRule {
                    id: "allow.read".to_string(),
                    description: "Allow read".to_string(),
                    decision: Decision::Allow,
                    priority: 50,
                    matchers: vec![Matcher::ActionIsMutation],
                },
                PolicyRule {
                    id: "deny.mutation".to_string(),
                    description: "Deny mutation".to_string(),
                    decision: Decision::Deny,
                    priority: 50,
                    matchers: vec![Matcher::ActionIsMutation],
                },
            ],
            active: false,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let intent = make_intent();
        let proposal = make_proposal(RollbackClass::R1SnapshotRecoverable);
        let trust = intent.trust_context.clone();
        let result = evaluate_bundle_rules(&bundle, &intent, &proposal, &trust);
        assert!(result.is_some());
        let (_, decision, _) = result.unwrap();
        assert_eq!(decision, Decision::Deny);
    }

    #[test]
    fn test_evaluate_bundle_rules_first_priority_group_wins() {
        let bundle = PolicyBundle {
            bundle_id: "test".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![
                PolicyRule {
                    id: "lower.deny".to_string(),
                    description: "Deny at lower priority".to_string(),
                    decision: Decision::Deny,
                    priority: 10,
                    matchers: vec![Matcher::ActionIsMutation],
                },
                PolicyRule {
                    id: "higher.allow".to_string(),
                    description: "Allow at higher priority".to_string(),
                    decision: Decision::Allow,
                    priority: 100,
                    matchers: vec![Matcher::ActionIsMutation],
                },
            ],
            active: false,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let intent = make_intent();
        let proposal = make_proposal(RollbackClass::R1SnapshotRecoverable);
        let trust = intent.trust_context.clone();
        let result = evaluate_bundle_rules(&bundle, &intent, &proposal, &trust);
        assert_eq!(result.map(|(_, d, _)| d), Some(Decision::Allow));
    }
}
