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
    ActorRef, ActorType, AuditAction, AuditResourceType, Decision, EvaluateProposalResponse,
    EventId, HashChainRef, IntentEnvelope, ObjectRef, ObjectType, ProvenanceEvent,
    TrustContextSummary,
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

    // Advisory behavioral anomaly detection after persistence, before provenance.
    // The profiler uses the principal from the intent; it never records raw
    // arguments or copies actor identifiers into the response.
    let anomaly = state.profiler.inspect_proposal(
        &intent.principal_id.to_string(),
        proposal.estimated_risk.clone(),
        proposal.requested_rollback_class.clone(),
    );

    // Emit PolicyEvaluated provenance event after evaluation succeeds.
    let decision_str = format!("{:?}", out.decision);
    let mut policy_metadata = ferrum_proto::JsonMap::new();
    policy_metadata.insert("decision".to_string(), serde_json::json!(decision_str));
    policy_metadata.insert("reason".to_string(), serde_json::json!("policy_evaluation"));
    if let Some(ref finding) = anomaly {
        state.metrics.record_behavioral_anomaly(finding.severity);
        let anomaly_meta = serde_json::json!({
            "severity": format!("{:?}", finding.severity).to_lowercase(),
            "window_count": finding.window_count,
            "window_secs": finding.window_secs,
            "threshold": finding.threshold,
        });
        policy_metadata.insert("behavioral_anomaly".to_string(), anomaly_meta.clone());
        if let Err(problem) = crate::audit::append_audit_checked(
            &state,
            "ferrum-gateway",
            AuditAction::BehavioralAnomaly,
            AuditResourceType::Proposal,
            &proposal.proposal_id.to_string(),
            "advisory",
            Some(anomaly_meta),
            Some(GovernanceRoute::ProposalsEvaluate),
        )
        .await
        {
            return governance_err!(state, GovernanceRoute::ProposalsEvaluate, problem);
        }
    }
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
    if let Err(e) =
        crate::provenance::append_governance_event(&state.runtime.store, policy_event).await
    {
        return governance_err!(
            state,
            GovernanceRoute::ProposalsEvaluate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // If the policy decision is Quarantine, create a pending hold so the
    // proposal cannot be executed until an operator resolves it.
    if out.decision == Decision::Quarantine {
        let hold = create_quarantine_hold(&state, &intent, &proposal, &out).await;
        if let Err(problem) = hold {
            return governance_err!(state, GovernanceRoute::ProposalsEvaluate, problem);
        }
    }

    governance_ok!(state, GovernanceRoute::ProposalsEvaluate, Ok(Json(out)))
}

async fn create_quarantine_hold(
    state: &Arc<AppState>,
    intent: &ferrum_proto::IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    evaluation: &ferrum_proto::EvaluateProposalResponse,
) -> Result<(), ApiProblem> {
    let now = chrono::Utc::now();
    let expires_at =
        now + chrono::Duration::seconds(state.server_config.quarantine_timeout_seconds as i64);
    let hold = ferrum_proto::QuarantineHold {
        hold_id: ferrum_proto::QuarantineHoldId::new(),
        intent_id: intent.intent_id,
        proposal_id: proposal.proposal_id,
        reason: evaluation.reason.clone(),
        matched_rule_ids: evaluation.matched_rule_ids.clone(),
        policy_bundle_id: None,
        state: ferrum_proto::QuarantineHoldState::Pending,
        expires_at,
        created_at: now,
        resolved_at: None,
        resolved_by: None,
        resolution_reason: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    if let Err(e) = state.runtime.store.quarantine_holds().insert(&hold).await {
        return Err(ApiProblem::internal(anyhow::Error::from(e)));
    }

    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "hold_id".to_string(),
        serde_json::json!(hold.hold_id.to_string()),
    );
    metadata.insert(
        "matched_rule_ids".to_string(),
        serde_json::json!(hold.matched_rule_ids.clone()),
    );

    let event = ferrum_proto::ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::QuarantineHoldCreated,
        occurred_at: now,
        actor: ferrum_proto::ActorRef {
            actor_type: ferrum_proto::ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ferrum_proto::ObjectRef {
            object_type: ferrum_proto::ObjectType::QuarantineHold,
            object_id: hold.hold_id.to_string(),
            summary: Some("Quarantine hold created for proposal".to_string()),
        },
        intent_id: Some(intent.intent_id),
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
        metadata,
        source_runtime_id: None,
    };

    if let Err(e) = crate::provenance::append_governance_event(&state.runtime.store, event).await {
        return Err(ApiProblem::internal(anyhow::Error::from(e)));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{GatewayRuntime, ServerConfig};
    use ferrum_proto::{
        ActionProposal, ApprovalMode, AuditAction, AuditResourceType, Decision, IntentStatus,
        JsonMap, Matcher, PolicyBundle, PolicyRule, PrincipalId, ProposalId, ResourceMode,
        ResourceSelector, RiskTier, RollbackClass, TimeBudget, TrustContextSummary,
    };
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::{SqliteStore, StoreFacade};
    use std::sync::atomic::Ordering;

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

    fn quarantine_all_bundle() -> PolicyBundle {
        PolicyBundle {
            bundle_id: "quarantine-all".to_string(),
            version: "0.1.0".to_string(),
            rules: vec![PolicyRule {
                id: "quarantine.everything".to_string(),
                description: "Quarantine all proposals".to_string(),
                decision: Decision::Quarantine,
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

    #[tokio::test]
    async fn test_quarantine_hold_created_on_quarantine_decision() {
        let runtime = test_runtime().await;
        let bundle = quarantine_all_bundle();
        runtime
            .store
            .policy_bundles()
            .insert(&bundle)
            .await
            .unwrap();

        let proposal = make_proposal("git.commit", RollbackClass::R1SnapshotRecoverable);
        let intent = make_intent_with_scope(proposal.intent_id);
        runtime.store.intents().insert(&intent).await.unwrap();

        let config = ServerConfig {
            pdp_mode: PdpMode::Bundles,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        let result = evaluate_proposal(
            State(Arc::clone(&state)),
            Path(proposal.proposal_id.to_string()),
            Json(proposal.clone()),
        )
        .await;

        assert!(result.is_ok(), "evaluate_proposal failed: {:?}", result);
        let out = result.unwrap().0;
        assert_eq!(out.decision, Decision::Quarantine);

        let hold = state
            .runtime
            .store
            .quarantine_holds()
            .get_by_proposal(proposal.proposal_id)
            .await
            .unwrap();
        assert!(hold.is_some(), "expected quarantine hold to be created");
        let hold = hold.unwrap();
        assert_eq!(hold.state, ferrum_proto::QuarantineHoldState::Pending);
        assert_eq!(hold.intent_id, proposal.intent_id);

        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(proposal.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ferrum_proto::ProvenanceEventKind::QuarantineHoldCreated),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_behavioral_anomaly_audit_fail_closed_blocks_request() {
        let runtime = test_runtime().await;
        let failing_store = Arc::new(crate::audit::FailingAuditStoreFacade::new(
            runtime.store.clone(),
        )) as Arc<dyn StoreFacade>;
        let mut runtime = runtime;
        runtime.store = failing_store;

        let mut proposal = make_proposal("git.commit", RollbackClass::R0NativeReversible);
        proposal.estimated_risk = RiskTier::High;

        let intent = make_intent_with_scope(proposal.intent_id);
        runtime.store.intents().insert(&intent).await.unwrap();

        let config = ServerConfig {
            pdp_mode: PdpMode::Bundles,
            behavioral_anomaly_enabled: true,
            behavioral_anomaly_window_secs: 60,
            behavioral_anomaly_warning_threshold: 2,
            behavioral_anomaly_critical_threshold: 3,
            behavioral_anomaly_max_actors: 10,
            audit_fail_closed: true,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        // First high-risk proposal is below the warning threshold; no anomaly audit.
        let result = evaluate_proposal(
            State(Arc::clone(&state)),
            Path(proposal.proposal_id.to_string()),
            Json(proposal.clone()),
        )
        .await;
        assert!(
            result.is_ok(),
            "first proposal should succeed: {:?}",
            result
        );

        // Second high-risk proposal triggers an anomaly. With a failing audit
        // store and audit_fail_closed=true, the request must be rejected.
        let mut proposal2 = make_proposal("git.commit", RollbackClass::R0NativeReversible);
        proposal2.estimated_risk = RiskTier::High;
        proposal2.intent_id = intent.intent_id;
        let result2 = evaluate_proposal(
            State(Arc::clone(&state)),
            Path(proposal2.proposal_id.to_string()),
            Json(proposal2.clone()),
        )
        .await;
        assert!(
            result2.is_err(),
            "expected fail-closed rejection when anomaly audit fails"
        );
        let problem = result2.unwrap_err();
        assert_eq!(problem.1, axum::http::StatusCode::SERVICE_UNAVAILABLE);

        // Fail-closed metric should be incremented.
        assert!(
            state
                .metrics
                .audit_fail_closed_rejections
                .load(Ordering::Relaxed)
                > 0,
            "expected audit_fail_closed_rejections metric to increment"
        );
    }

    #[tokio::test]
    async fn test_behavioral_anomaly_emits_audit_metadata_and_metrics() {
        let runtime = test_runtime().await;

        let mut proposal = make_proposal("git.commit", RollbackClass::R0NativeReversible);
        proposal.estimated_risk = RiskTier::High;

        let intent = make_intent_with_scope(proposal.intent_id);
        runtime.store.intents().insert(&intent).await.unwrap();

        let config = ServerConfig {
            behavioral_anomaly_enabled: true,
            behavioral_anomaly_window_secs: 60,
            behavioral_anomaly_warning_threshold: 2,
            behavioral_anomaly_critical_threshold: 3,
            behavioral_anomaly_max_actors: 10,
            ..ServerConfig::default()
        };
        let state = AppState::test_new(runtime, config);

        // First high-risk proposal is below the warning threshold.
        let result = evaluate_proposal(
            State(Arc::clone(&state)),
            Path(proposal.proposal_id.to_string()),
            Json(proposal.clone()),
        )
        .await;
        assert!(result.is_ok(), "evaluate_proposal failed: {:?}", result);
        assert_eq!(result.unwrap().0.decision, Decision::Allow);

        // Second high-risk proposal from the same principal triggers a warning.
        let mut proposal2 = make_proposal("git.commit", RollbackClass::R0NativeReversible);
        proposal2.estimated_risk = RiskTier::High;
        proposal2.intent_id = intent.intent_id;
        let result2 = evaluate_proposal(
            State(Arc::clone(&state)),
            Path(proposal2.proposal_id.to_string()),
            Json(proposal2.clone()),
        )
        .await;
        assert!(result2.is_ok(), "evaluate_proposal failed: {:?}", result2);
        assert_eq!(result2.unwrap().0.decision, Decision::Allow);

        // PolicyEvaluated provenance metadata contains sanitized anomaly info.
        let events = state
            .runtime
            .store
            .provenance()
            .query(&ferrum_proto::ProvenanceQueryRequest {
                intent_id: Some(intent.intent_id),
                execution_id: None,
                capability_id: None,
                event_kind: Some(ferrum_proto::ProvenanceEventKind::PolicyEvaluated),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
        let latest = events
            .iter()
            .max_by_key(|e| e.occurred_at)
            .expect("at least one PolicyEvaluated event");
        assert!(
            latest.metadata.contains_key("behavioral_anomaly"),
            "expected behavioral_anomaly metadata in PolicyEvaluated event"
        );
        let anomaly_meta = latest
            .metadata
            .get("behavioral_anomaly")
            .expect("behavioral_anomaly metadata");
        assert_eq!(anomaly_meta["severity"], "warning");
        assert_eq!(anomaly_meta["window_count"], 2);
        assert_eq!(anomaly_meta["threshold"], 2);

        // Audit log entry was appended for the second proposal.
        let (audit_entries, _) = state
            .runtime
            .store
            .audit_log()
            .list(
                Some(AuditAction::BehavioralAnomaly),
                Some(AuditResourceType::Proposal),
                Some(&proposal2.proposal_id.to_string()),
                None,
                10,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(audit_entries.len(), 1);
        assert_eq!(audit_entries[0].action, AuditAction::BehavioralAnomaly);
        assert_eq!(audit_entries[0].result, "advisory");

        // Metrics counter incremented with a bounded severity label.
        assert_eq!(
            state
                .metrics
                .behavioral_anomaly_warnings_total
                .load(Ordering::Relaxed),
            1
        );
        assert_eq!(
            state
                .metrics
                .behavioral_anomaly_critical_total
                .load(Ordering::Relaxed),
            0
        );
    }
}
