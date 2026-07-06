//! Action proposal evaluation handler.
//!
//! Routes:
//! - `POST /v1/proposals/{proposal_id}/evaluate` -> `evaluate_proposal`
//!
//! `evaluate_proposal` loads the proposal's parent intent, builds the
//! firewall context, evaluates the proposal according to `server_config.pdp_mode`
//! (Static, Bundles-only, or Dual), persists the proposal to satisfy
//! foreign-key constraints, and emits a `PolicyEvaluated` provenance event
//! before returning the decision.

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::Utc;
use ferrum_proto::{
    ActorRef, ActorType, Decision, EvaluateProposalResponse, EventId, HashChainRef, IntentEnvelope,
    ObjectRef, ObjectType, ProvenanceEvent, TrustContextSummary,
};
use std::sync::Arc;

use crate::macros::{governance_err, governance_ok};
use crate::monitoring::GovernanceRoute;
use crate::policy_eval::{
    build_firewall_context, evaluate_active_policy_bundles, has_tool_output_label,
    has_untrusted_text_label, intent_has_external_label, minimal_intent_for,
    proposal_has_external_metadata,
};
use crate::problem::ApiProblem;
use crate::state::{AppState, PdpMode};

/// Evaluate a proposal according to the configured `pdp_mode`.
///
/// - `Static`: use the configured static PDP engine only.
/// - `Bundles`: use active policy bundles only; default Allow if no bundle matches.
/// - `Dual`: evaluate active bundles first, then fall back to the static PDP engine.
pub(crate) async fn evaluate_proposal_decision(
    state: &Arc<AppState>,
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    trust: &TrustContextSummary,
) -> Result<EvaluateProposalResponse, ApiProblem> {
    match state.server_config.pdp_mode {
        PdpMode::Static => state
            .runtime
            .pdp
            .evaluate(intent, proposal, trust)
            .await
            .map_err(ApiProblem::internal),
        PdpMode::Bundles => {
            if let Some(response) =
                evaluate_active_policy_bundles(&state.runtime.store, intent, proposal, trust).await
            {
                Ok(response)
            } else {
                Ok(EvaluateProposalResponse {
                    decision: Decision::Allow,
                    reason: "no active bundle matched; default allow".to_string(),
                    matched_rule_ids: vec!["allow.default".to_string()],
                    warnings: Vec::new(),
                })
            }
        }
        PdpMode::Dual => {
            if let Some(response) =
                evaluate_active_policy_bundles(&state.runtime.store, intent, proposal, trust).await
            {
                Ok(response)
            } else {
                state
                    .runtime
                    .pdp
                    .evaluate(intent, proposal, trust)
                    .await
                    .map_err(ApiProblem::internal)
            }
        }
    }
}

pub(crate) async fn evaluate_proposal(
    State(state): State<Arc<AppState>>,
    Path(_proposal_id): Path<String>,
    Json(proposal): Json<ferrum_proto::ActionProposal>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    let intent = match state.runtime.store.intents().get(proposal.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => minimal_intent_for(
            proposal.intent_id,
            proposal.requested_rollback_class.clone(),
        ),
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ProposalsEvaluate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Determine if proposal is external based on intent trust labels and proposal attributes.
    let is_external = intent_has_external_label(&intent)
        || !proposal.taint_inputs.is_empty()
        || proposal_has_external_metadata(&proposal);

    // Build firewall context from proposal and intent.
    let firewall_ctx = build_firewall_context(&intent, &proposal, is_external);

    // Compute taint score via firewall.
    let firewall_taint = state.runtime.firewall.compute_taint_score(&firewall_ctx);

    // Preserve intent's trust labels and sensitivity labels; override taint_score with firewall-derived value.
    let trust = TrustContextSummary {
        input_labels: intent.trust_context.input_labels.clone(),
        sensitivity_labels: intent.trust_context.sensitivity_labels.clone(),
        taint_score: firewall_taint,
        contains_external_metadata: intent.trust_context.contains_external_metadata
            || proposal_has_external_metadata(&proposal),
        contains_tool_output: intent.trust_context.contains_tool_output
            || has_tool_output_label(&intent),
        contains_untrusted_text: intent.trust_context.contains_untrusted_text
            || has_untrusted_text_label(&intent),
    };

    // Evaluate according to the configured PDP mode.
    let out = match evaluate_proposal_decision(&state, &intent, &proposal, &trust).await {
        Ok(out) => out,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ProposalsEvaluate, e);
        }
    };

    // Persist the proposal so foreign-key constraints in executions table are satisfied.
    // Synchronous write: must complete before response to guarantee FK constraints.
    if let Err(e) = state.runtime.store.proposals().insert(&proposal).await {
        tracing::warn!(error = %e, "failed to persist proposal to DB");
        return governance_err!(
            state,
            GovernanceRoute::ProposalsEvaluate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit PolicyEvaluated provenance event after evaluation succeeds.
    let decision_str = format!("{:?}", out.decision);
    let mut policy_metadata = ferrum_proto::JsonMap::new();
    policy_metadata.insert("decision".to_string(), serde_json::json!(decision_str));
    policy_metadata.insert("reason".to_string(), serde_json::json!("policy_evaluation"));
    let policy_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::PolicyEvaluated,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::PolicyBundle,
            object_id: proposal.proposal_id.to_string(),
            summary: Some("Policy evaluated for proposal".to_string()),
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
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: policy_metadata,
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&policy_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ProposalsEvaluate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(state, GovernanceRoute::ProposalsEvaluate, Ok(Json(out)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GatewayRuntime, ServerConfig};
    use ferrum_proto::{
        ActionProposal, ApprovalMode, IntentStatus, JsonMap, Matcher, PolicyBundle, PolicyRule,
        PrincipalId, ProposalId, ResourceMode, ResourceSelector, RiskTier, RollbackClass,
        TimeBudget, TrustContextSummary,
    };
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::{SqliteStore, StoreFacade};

    async fn test_runtime() -> GatewayRuntime {
        let pdp = Arc::new(ferrum_pdp::StaticPdpEngine);
        let cap = Arc::new(ferrum_cap::InMemoryCapabilityService::default());
        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));
        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();
        GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![])
    }

    fn make_proposal(tool_name: &str, rollback_class: RollbackClass) -> ActionProposal {
        ActionProposal {
            proposal_id: ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: tool_name.to_string(),
            server_name: "test-server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test effect".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: rollback_class,
            taint_inputs: vec![],
            metadata: JsonMap::new(),
            created_at: chrono::Utc::now(),
        }
    }

    fn make_intent_with_scope(intent_id: ferrum_proto::IntentId) -> IntentEnvelope {
        IntentEnvelope {
            intent_id,
            principal_id: PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![ResourceSelector::FilesystemPath {
                path: "/tmp".to_string(),
                mode: ResourceMode::Write,
                content_hash: None,
            }],
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
        }
    }

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

    fn allow_all_bundle() -> PolicyBundle {
        PolicyBundle {
            bundle_id: "allow-all".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![PolicyRule {
                id: "allow.everything".to_string(),
                description: "Allow all proposals".to_string(),
                decision: Decision::Allow,
                priority: 100,
                matchers: vec![Matcher::ActionIsMutation],
            }],
            active: true,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn deny_all_bundle() -> PolicyBundle {
        PolicyBundle {
            bundle_id: "deny-all".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![PolicyRule {
                id: "deny.everything".to_string(),
                description: "Deny all proposals".to_string(),
                decision: Decision::Deny,
                priority: 100,
                matchers: vec![Matcher::ActionIsMutation],
            }],
            active: true,
            content_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_pdp_mode_static_skips_active_bundle() {
        let runtime = test_runtime().await;
        let bundle = allow_all_bundle();
        runtime
            .store
            .policy_bundles()
            .insert(&bundle)
            .await
            .unwrap();

        let config = ServerConfig {
            pdp_mode: PdpMode::Static,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        // Proposal that the active bundle would Allow, but static PDP denies
        // because resource_scope is empty and rollback class is non-R0.
        let proposal = make_proposal("git.commit", RollbackClass::R1SnapshotRecoverable);
        let intent = make_intent_with_scope(proposal.intent_id);
        // Replace resource scope with empty to trigger static scope-deny.
        let mut intent = intent;
        intent.resource_scope = vec![];
        let trust = make_trust_context();

        let result = evaluate_proposal_decision(&state, &intent, &proposal, &trust)
            .await
            .unwrap();
        assert_eq!(result.decision, Decision::Deny);
        assert!(result.reason.contains("scope mismatch"));
    }

    #[tokio::test]
    async fn test_pdp_mode_bundles_uses_active_bundle() {
        let runtime = test_runtime().await;
        let bundle = deny_all_bundle();
        runtime
            .store
            .policy_bundles()
            .insert(&bundle)
            .await
            .unwrap();

        let config = ServerConfig {
            pdp_mode: PdpMode::Bundles,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        let proposal = make_proposal("git.commit", RollbackClass::R1SnapshotRecoverable);
        let intent = make_intent_with_scope(proposal.intent_id);
        let trust = make_trust_context();

        let result = evaluate_proposal_decision(&state, &intent, &proposal, &trust)
            .await
            .unwrap();
        assert_eq!(result.decision, Decision::Deny);
    }

    #[tokio::test]
    async fn test_pdp_mode_bundles_default_allow_skips_static_fallback() {
        let runtime = test_runtime().await;
        // No active bundles.

        let config = ServerConfig {
            pdp_mode: PdpMode::Bundles,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        // Proposal that static PDP would deny due to empty resource scope.
        let proposal = make_proposal("git.commit", RollbackClass::R1SnapshotRecoverable);
        let intent = make_intent_with_scope(proposal.intent_id);
        // Replace resource scope with empty to trigger static scope-deny.
        let mut intent = intent;
        intent.resource_scope = vec![];
        let trust = make_trust_context();

        let result = evaluate_proposal_decision(&state, &intent, &proposal, &trust)
            .await
            .unwrap();
        assert_eq!(result.decision, Decision::Allow);
        assert!(result.reason.contains("no active bundle matched"));
    }

    #[tokio::test]
    async fn test_pdp_mode_dual_uses_static_fallback_when_no_bundle_matches() {
        let runtime = test_runtime().await;
        // No active bundles.

        let config = ServerConfig {
            pdp_mode: PdpMode::Dual,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        // Proposal that static PDP would deny due to empty resource scope.
        let proposal = make_proposal("git.commit", RollbackClass::R1SnapshotRecoverable);
        let intent = make_intent_with_scope(proposal.intent_id);
        // Replace resource scope with empty to trigger static scope-deny.
        let mut intent = intent;
        intent.resource_scope = vec![];
        let trust = make_trust_context();

        let result = evaluate_proposal_decision(&state, &intent, &proposal, &trust)
            .await
            .unwrap();
        assert_eq!(result.decision, Decision::Deny);
        assert!(result.reason.contains("scope mismatch"));
    }
}
