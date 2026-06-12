//! Policy bundle evaluation and firewall taint-derivation helpers.
//!
//! These helpers are pure/shared logic used by both the per-request
//! governance handlers (`proposals::evaluate_proposal`, `policy::simulate_*`)
//! and the bundle evaluation flow. They were previously defined inline in
//! `server.rs`; this module moves them into a dedicated file so `server.rs`
//! can shrink and so `policy.rs` no longer needs to import private helpers
//! from `crate::server`.
//!
//! Helpers grouped here:
//! - Policy bundle evaluation: [`evaluate_active_policy_bundles`],
//!   [`evaluate_bundle_rules`], [`evaluate_rule_matchers`], [`evaluate_matcher`].
//! - Firewall context derivation: [`build_firewall_context`],
//!   [`intent_has_external_label`], [`proposal_has_external_metadata`],
//!   [`has_tool_output_label`], [`has_untrusted_text_label`].
//! - Intent scaffold: [`minimal_intent_for`].

use chrono::{Duration, Utc};
use ferrum_firewall::FirewallContext;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    EvaluateProposalResponse, IntentEnvelope, Matcher, OutcomeClause, PolicyBundle, PolicyRule,
    RiskTier, RollbackClass, TimeBudget, TrustContextSummary, TrustLabel as ProtoTrustLabel,
};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Policy bundle evaluation helpers
// ---------------------------------------------------------------------------

/// Load active policy bundles and evaluate their rules against the given context.
/// Returns `Some(EvaluateProposalResponse)` if a bundle rule matches, `None` otherwise.
pub(crate) async fn evaluate_active_policy_bundles(
    store: &Arc<dyn ferrum_store::StoreFacade>,
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    trust: &TrustContextSummary,
) -> Option<EvaluateProposalResponse> {
    let active_bundles = match store.policy_bundles().list_active().await {
        Ok(bundles) => bundles,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load active policy bundles");
            return None;
        }
    };

    for bundle in active_bundles {
        if let Some(response) = evaluate_bundle_rules(&bundle, intent, proposal, trust) {
            return Some(response);
        }
    }

    None
}

/// Evaluate all rules in a policy bundle, sorted by descending priority.
/// Returns `Some(EvaluateProposalResponse)` if a rule matches, `None` otherwise.
pub(crate) fn evaluate_bundle_rules(
    bundle: &PolicyBundle,
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    trust: &TrustContextSummary,
) -> Option<EvaluateProposalResponse> {
    // Sort rules by descending priority
    let mut rules = bundle.rules.clone();
    rules.sort_by_key(|rule| std::cmp::Reverse(rule.priority));

    for rule in rules {
        if evaluate_rule_matchers(&rule, intent, proposal, trust) {
            let matched_rule_id = format!("policy_bundle:{}:{}", bundle.bundle_id, rule.id);
            return Some(EvaluateProposalResponse {
                decision: rule.decision.clone(),
                reason: format!(
                    "policy bundle {} matched rule {}: {}",
                    bundle.bundle_id, rule.id, rule.description
                ),
                matched_rule_ids: vec![matched_rule_id],
                warnings: Vec::new(),
            });
        }
    }

    None
}

/// Evaluate all matchers in a rule. All matchers must match for the rule to apply.
pub(crate) fn evaluate_rule_matchers(
    rule: &PolicyRule,
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    trust: &TrustContextSummary,
) -> bool {
    rule.matchers
        .iter()
        .all(|m| evaluate_matcher(m, intent, proposal, trust))
}

/// Evaluate a single matcher against the given context.
pub(crate) fn evaluate_matcher(
    matcher: &Matcher,
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
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
            // Compare against debug format (e.g., "R3IrreversibleHighConsequence")
            let class_debug = format!("{:?}", proposal.requested_rollback_class);
            class_debug == *value
        }
        Matcher::ActionTypeEquals { value } => {
            // Infer effect type and compare against the provided value
            let inferred_effect = StaticPdpEngine::infer_effect_type(proposal);
            let effect_debug = format!("{:?}", inferred_effect);
            effect_debug == *value
        }
        Matcher::Unknown { .. } => {
            // Unknown matchers should not match; add warning only if needed
            tracing::warn!("encountered unknown matcher type");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Firewall taint derivation helpers
// ---------------------------------------------------------------------------

/// Returns true if the intent's trust context contains any external/trusted-label.
pub(crate) fn intent_has_external_label(intent: &IntentEnvelope) -> bool {
    intent.trust_context.input_labels.iter().any(|l| {
        matches!(
            l,
            ProtoTrustLabel::ExternalWeb
                | ProtoTrustLabel::ExternalEmail
                | ProtoTrustLabel::ExternalRepoText
                | ProtoTrustLabel::ExternalToolMetadata
                | ProtoTrustLabel::ExternalToolOutput
                | ProtoTrustLabel::OCRExtracted
                | ProtoTrustLabel::Untrusted
        )
    })
}

/// Returns true if proposal metadata contains external-like hints.
pub(crate) fn proposal_has_external_metadata(proposal: &ferrum_proto::ActionProposal) -> bool {
    // Check for common external source indicators in metadata.
    let external_indicators = [
        "source",
        "external",
        "untrusted",
        "tool_output",
        "web_content",
        "email_content",
    ];
    proposal.metadata.keys().any(|k| {
        let k_lower = k.to_lowercase();
        external_indicators.iter().any(|ind| k_lower.contains(ind))
    })
}

/// Returns true if intent trust context has tool output labels.
pub(crate) fn has_tool_output_label(intent: &IntentEnvelope) -> bool {
    intent
        .trust_context
        .input_labels
        .contains(&ProtoTrustLabel::ExternalToolOutput)
}

/// Returns true if intent trust context has untrusted text labels.
pub(crate) fn has_untrusted_text_label(intent: &IntentEnvelope) -> bool {
    intent
        .trust_context
        .input_labels
        .contains(&ProtoTrustLabel::Untrusted)
}

/// Builds a FirewallContext from intent and proposal for taint scoring.
pub(crate) fn build_firewall_context(
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    is_external: bool,
) -> FirewallContext {
    let mut attributes: HashMap<String, String> = HashMap::new();

    // Add action attribute: "write" for non-R0, "read" for R0.
    let action = if matches!(
        proposal.requested_rollback_class,
        RollbackClass::R0NativeReversible
    ) {
        "read"
    } else {
        "write"
    };
    attributes.insert("action".to_string(), action.to_string());

    // Add rollback_class attribute.
    let rc_debug = format!("{:?}", proposal.requested_rollback_class);
    attributes.insert("rollback_class".to_string(), rc_debug);

    // Add tool_name and server_name.
    attributes.insert("tool_name".to_string(), proposal.tool_name.clone());
    attributes.insert("server_name".to_string(), proposal.server_name.clone());

    // Add proposal metadata as string attributes (bool/string values only).
    for (key, value) in &proposal.metadata {
        if let Some(s) = value.as_str() {
            attributes.insert(key.clone(), s.to_string());
        } else if let Some(b) = value.as_bool() {
            attributes.insert(key.clone(), b.to_string());
        }
    }

    // Determine trust_score: 30 if external/untrusted, else 80.
    let trust_score = if is_external { 30 } else { 80 };

    FirewallContext {
        source: if proposal.server_name.is_empty() {
            proposal.tool_name.clone()
        } else {
            proposal.server_name.clone()
        },
        intent: Some(intent.normalized_goal.clone()).filter(|g| !g.is_empty()),
        trust_score,
        is_external,
        attributes,
    }
}

/// Build a minimal intent envelope for policy simulation paths that need
/// to evaluate against the PDP or active bundles without a real intent.
pub(crate) fn minimal_intent_for(
    intent_id: ferrum_proto::IntentId,
    rollback: RollbackClass,
) -> IntentEnvelope {
    let now = Utc::now();
    IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "scaffold-intent".to_string(),
        goal: "scaffold evaluation".to_string(),
        normalized_goal: "scaffold evaluation".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Medium,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: rollback,
        time_budget: TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: TrustContextSummary {
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
        expires_at: now + Duration::minutes(15),
    }
}
