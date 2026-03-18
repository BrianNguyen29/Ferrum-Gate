use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use ferrum_proto::{
    ActorRef, ActorType, ApiError, ApiErrorCode, ApprovalId, ApprovalRequest,
    ApprovalResolveRequest, ApprovalState, AuthorizeExecutionRequest, AuthorizeExecutionResponse,
    CapabilityId, CapabilityMintRequest, CapabilityMintResponse, CommitRequest, CommitResponse,
    CompensateRequest, CompensateResponse, Decision, EvaluateProposalResponse, ExecuteRequest,
    ExecuteResponse, ExecutionId, ExecutionRecord, ExecutionState, HashChainRef, HealthResponse,
    IntentCompileRequest, IntentCompileResponse, IntentEnvelope, IntentStatus, ObjectRef,
    ObjectType, OutcomeClause, ProvenanceEvent, ProvenanceEventKind, ResourceSelector, RiskTier,
    RollbackClass, RollbackRequest, RollbackResponse, RollbackState, TimeBudget,
    TrustContextSummary, VerifyRequest, VerifyResponse,
};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo,
    RollbackRepo,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::{GatewayConfig, GatewayRuntime};

fn create_provenance_event(
    kind: ProvenanceEventKind,
    occurred_at: chrono::DateTime<Utc>,
    intent_id: Option<ferrum_proto::IntentId>,
    proposal_id: Option<ferrum_proto::ProposalId>,
    execution_id: Option<ferrum_proto::ExecutionId>,
    capability_id: Option<CapabilityId>,
    rollback_contract_id: Option<ferrum_proto::RollbackContractId>,
    policy_bundle_id: Option<ferrum_proto::PolicyBundleId>,
) -> ProvenanceEvent {
    ProvenanceEvent {
        event_id: ferrum_proto::EventId::new(),
        kind,
        occurred_at,
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("Ferrum Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Unknown,
            object_id: String::new(),
            summary: None,
        },
        intent_id,
        proposal_id,
        execution_id,
        capability_id,
        rollback_contract_id,
        policy_bundle_id,
        trust_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: ferrum_proto::JsonMap::new(),
    }
}

pub async fn run_http_server(config: GatewayConfig, runtime: GatewayRuntime) -> anyhow::Result<()> {
    let app = build_router(runtime);
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("ferrumd listening on {}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_router(runtime: GatewayRuntime) -> Router {
    Router::new()
        .route("/v1/healthz", get(healthz))
        .route("/v1/readyz", get(readyz))
        .route("/v1/intents/compile", post(compile_intent))
        .route(
            "/v1/proposals/{proposal_id}/evaluate",
            post(evaluate_proposal),
        )
        .route("/v1/capabilities/mint", post(mint_capability))
        .route(
            "/v1/capabilities/{capability_id}/revoke",
            post(revoke_capability),
        )
        .route("/v1/executions/authorize", post(authorize_execution))
        .route(
            "/v1/executions/{execution_id}/prepare",
            post(prepare_execution),
        )
        .route(
            "/v1/executions/{execution_id}/execute",
            post(execute_execution),
        )
        .route(
            "/v1/executions/{execution_id}/verify",
            post(verify_execution),
        )
        .route(
            "/v1/executions/{execution_id}/commit",
            post(commit_execution),
        )
        .route(
            "/v1/executions/{execution_id}/compensate",
            post(compensate_execution),
        )
        .route(
            "/v1/executions/{execution_id}/rollback",
            post(rollback_execution),
        )
        .route("/v1/approvals", get(list_pending_approvals))
        .route("/v1/approvals/{approval_id}", get(get_approval))
        .route(
            "/v1/approvals/{approval_id}/resolve",
            post(resolve_approval),
        )
        .with_state(Arc::new(runtime))
        .layer(TraceLayer::new_for_http())
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn readyz() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ready".to_string(),
    })
}

async fn compile_intent(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(req): Json<IntentCompileRequest>,
) -> Result<Json<IntentCompileResponse>, ApiProblem> {
    let now = Utc::now();
    let requested_risk = req.requested_risk_tier.unwrap_or(RiskTier::Medium);
    let default_rollback_class = infer_rollback_class(&req.requested_resource_scope);

    // Use firewall to derive trust context from raw inputs
    let trust_context = runtime.firewall.derive_trust_context(&req.raw_inputs, &[]);

    // Collect warnings from inferred labels
    let mut warnings = Vec::new();
    if trust_context.contains_untrusted_text {
        warnings.push("Input contains potentially untrusted text".to_string());
    }
    if trust_context.contains_external_metadata {
        warnings.push("Input contains external metadata".to_string());
    }
    if trust_context.contains_tool_output {
        warnings.push("Input contains tool output".to_string());
    }
    if trust_context.taint_score > 50 {
        warnings.push(format!("High taint score: {}", trust_context.taint_score));
    }

    let envelope = IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: req.principal_id,
        session_id: req.session_id,
        channel_id: req.channel_id,
        title: req.title.clone(),
        goal: req.goal.clone(),
        normalized_goal: req.goal.trim().to_lowercase(),
        allowed_outcomes: vec![OutcomeClause {
            id: "primary".to_string(),
            description: req.agent_plan_summary.unwrap_or_else(|| req.goal.clone()),
            effect_type: req
                .effect_type
                .unwrap_or(ferrum_proto::EffectType::ReadOnlyAnalysis),
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: req.requested_resource_scope,
        risk_tier: requested_risk,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class,
        time_budget: TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context,
        derived_from_event_ids: req.raw_inputs.iter().filter_map(|r| r.event_id).collect(),
        tags: Vec::new(),
        metadata: req.metadata,
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + Duration::minutes(15),
    };

    let intent_id = envelope.intent_id;
    if let Err(e) = runtime.store.intents().insert(&envelope).await {
        tracing::warn!("failed to persist intent: {}", e);
    } else {
        let event = create_provenance_event(
            ProvenanceEventKind::IntentCompiled,
            now,
            Some(intent_id),
            None,
            None,
            None,
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(IntentCompileResponse { envelope, warnings }))
}

async fn evaluate_proposal(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(proposal_id_from_path): Path<String>,
    Json(proposal): Json<ferrum_proto::ActionProposal>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    let now = Utc::now();
    let intent_id = proposal.intent_id;

    // Validate path and body proposal_id match
    let proposal_id_from_body = proposal.proposal_id.to_string();
    if proposal_id_from_path != proposal_id_from_body {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            format!(
                "proposal_id mismatch: path has '{}', body has '{}'",
                proposal_id_from_path, proposal_id_from_body
            ),
        ));
    }

    // Load the real intent from store if it exists; fall back to minimal intent only if not found
    let intent = match runtime.store.intents().get(intent_id).await {
        Ok(Some(real_intent)) => real_intent,
        Ok(None) => {
            tracing::warn!("intent {} not found, using minimal intent", intent_id);
            minimal_intent_for(
                proposal.intent_id,
                proposal.requested_rollback_class.clone(),
            )
        }
        Err(e) => {
            tracing::warn!(
                "failed to load intent {}: {}, using minimal intent",
                intent_id,
                e
            );
            minimal_intent_for(
                proposal.intent_id,
                proposal.requested_rollback_class.clone(),
            )
        }
    };

    // Persist the incoming ActionProposal (requires valid intent_id due to FK constraint)
    // Only emit provenance if proposal was successfully persisted
    let proposal_persisted = match runtime.store.proposals().insert(&proposal).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("failed to persist proposal: {}", e);
            false
        }
    };

    // Emit provenance for proposal submission ONLY if proposal was persisted
    let submission_event_id = if proposal_persisted {
        let submission_event = create_provenance_event(
            ProvenanceEventKind::ActionProposalSubmitted,
            now,
            Some(intent_id),
            Some(proposal.proposal_id),
            None,
            None,
            None,
            None,
        );
        let event_id = submission_event.event_id;
        if let Err(e) = runtime
            .store
            .provenance()
            .append_event(&submission_event)
            .await
        {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
        Some(event_id)
    } else {
        None
    };

    // Run contradiction check using firewall
    let contradictions = runtime.firewall.contradiction_check(&intent, &proposal);

    // Severity-based decision mapping (fail-closed for high severity only)
    // High -> Deny immediately (unacceptable violation)
    // Medium/Low -> Add as warnings, let PDP decide
    use ferrum_firewall::Severity;
    let high_severity_contradictions: Vec<_> = contradictions
        .iter()
        .filter(|c| matches!(c.severity, Severity::High))
        .collect();

    if !high_severity_contradictions.is_empty() {
        // Fail-closed: High severity contradictions result in immediate Deny
        let rule_ids: Vec<String> = contradictions.iter().map(|c| c.rule_id.clone()).collect();
        let reason = contradictions
            .iter()
            .map(|c| format!("[{}] {}", c.rule_id, c.message))
            .collect::<Vec<_>>()
            .join("; ");
        let warnings = contradictions.into_iter().map(|c| c.message).collect();

        let out = EvaluateProposalResponse {
            decision: Decision::Deny,
            reason,
            matched_rule_ids: rule_ids,
            warnings,
        };

        // Store the denied decision in the proposal
        let mut proposal_with_decision = proposal.clone();
        proposal_with_decision.decision = Some(out.decision.clone());

        if let Err(e) = runtime
            .store
            .proposals()
            .update(&proposal_with_decision)
            .await
        {
            tracing::warn!("failed to update proposal with denial decision: {}", e);
        }

        // Emit provenance for policy evaluation that resulted in denial
        let mut eval_event = create_provenance_event(
            ProvenanceEventKind::PolicyEvaluated,
            now,
            Some(intent_id),
            Some(proposal.proposal_id),
            None,
            None,
            None,
            None,
        );
        if let Some(submission_event_id) = submission_event_id {
            eval_event.parent_edges = vec![ferrum_proto::ProvenanceEdge {
                edge_type: ferrum_proto::ProvenanceEdgeType::Caused,
                from_event_id: submission_event_id,
                summary: Some("policy evaluation follows proposal submission".to_string()),
            }];
        }

        if let Err(e) = runtime.store.provenance().append_event(&eval_event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }

        return Ok(Json(out));
    }

    // Collect medium/low severity contradictions as warnings for PDP
    let contradiction_warnings: Vec<String> = contradictions
        .into_iter()
        .filter(|c| matches!(c.severity, Severity::Medium | Severity::Low))
        .map(|c| c.message)
        .collect();

    // Derive trust context using firewall with intent labels and proposal taint inputs
    // Combine compile-time taint from intent with proposal-time taint inputs
    let mut combined_taint_inputs = proposal.taint_inputs.clone();

    // Add compile-time trust labels as taint sources
    for label in &intent.trust_context.input_labels {
        combined_taint_inputs.push(format!("{:?}", label).to_lowercase());
    }

    // Compute combined taint score (conservatively combines both sources)
    let combined_taint_score = runtime.firewall.compute_taint_score(&combined_taint_inputs);

    // Also compute proposal-only taint for comparison
    let proposal_taint = runtime.firewall.derive_trust_context(
        &[], // We already have labels from intent, no new raw inputs here
        &proposal.taint_inputs,
    );

    // Merge with intent's trust context - use MAX for boolean flags (conservative)
    // and combined taint score that includes both compile-time and proposal-time sources
    let combined_trust = TrustContextSummary {
        input_labels: intent.trust_context.input_labels.clone(),
        sensitivity_labels: intent.trust_context.sensitivity_labels.clone(),
        taint_score: combined_taint_score.min(100), // Hard cap at 100
        contains_external_metadata: proposal_taint.contains_external_metadata
            || intent.trust_context.contains_external_metadata,
        contains_tool_output: proposal_taint.contains_tool_output
            || intent.trust_context.contains_tool_output,
        contains_untrusted_text: proposal_taint.contains_untrusted_text
            || intent.trust_context.contains_untrusted_text,
    };

    let mut out = runtime
        .pdp
        .evaluate(&intent, &proposal, &combined_trust)
        .await
        .map_err(ApiProblem::internal)?;

    // Merge contradiction warnings into PDP output
    if !contradiction_warnings.is_empty() {
        out.warnings.extend(contradiction_warnings);
    }

    // Store the decision in the proposal after evaluation
    let mut proposal_with_decision = proposal.clone();
    proposal_with_decision.decision = Some(out.decision.clone());

    // Update the proposal with the decision
    if let Err(e) = runtime
        .store
        .proposals()
        .update(&proposal_with_decision)
        .await
    {
        tracing::warn!("failed to update proposal with decision: {}", e);
    }

    // Emit provenance for policy evaluation with linkage to submission event if proposal was persisted
    let parent_edge = if let Some(submission_event_id) = submission_event_id {
        vec![ferrum_proto::ProvenanceEdge {
            edge_type: ferrum_proto::ProvenanceEdgeType::Caused,
            from_event_id: submission_event_id,
            summary: Some("policy evaluation follows proposal submission".to_string()),
        }]
    } else {
        Vec::new()
    };

    let mut eval_event = create_provenance_event(
        ProvenanceEventKind::PolicyEvaluated,
        now,
        Some(intent_id),
        Some(proposal.proposal_id),
        None,
        None,
        None,
        None,
    );
    eval_event.parent_edges = parent_edge;

    if let Err(e) = runtime.store.provenance().append_event(&eval_event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(out))
}

async fn mint_capability(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(request): Json<CapabilityMintRequest>,
) -> Result<Json<CapabilityMintResponse>, ApiProblem> {
    let response = runtime
        .cap
        .mint(request)
        .await
        .map_err(ApiProblem::from_capability)?;

    let now = Utc::now();
    let capability_id = response.lease.capability_id;
    let intent_id = response.lease.intent_id;
    let proposal_id = response.lease.proposal_id;
    let policy_bundle_id = response.lease.policy_bundle_id;

    if let Err(e) = runtime.store.capabilities().insert(&response.lease).await {
        tracing::warn!("failed to persist capability: {}", e);
    } else {
        let event = create_provenance_event(
            ProvenanceEventKind::CapabilityMinted,
            now,
            Some(intent_id),
            Some(proposal_id),
            None,
            Some(capability_id),
            None,
            Some(policy_bundle_id),
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(response))
}

async fn revoke_capability(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(capability_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    let id = parse_capability_id(&capability_id)?;
    let lease = runtime
        .cap
        .revoke(id)
        .await
        .map_err(ApiProblem::from_capability)?;

    let now = Utc::now();
    if let Err(e) = runtime.store.capabilities().update(&lease).await {
        tracing::warn!("failed to update capability: {}", e);
    } else {
        let event = create_provenance_event(
            ProvenanceEventKind::CapabilityRevoked,
            now,
            Some(lease.intent_id),
            Some(lease.proposal_id),
            None,
            Some(lease.capability_id),
            None,
            Some(lease.policy_bundle_id),
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "capability_id": lease.capability_id.to_string()
    })))
}

async fn authorize_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(request): Json<AuthorizeExecutionRequest>,
) -> Result<Json<AuthorizeExecutionResponse>, ApiProblem> {
    let lease = runtime
        .cap
        .get(request.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

    // Validate proposal/capability binding: request proposal_id must match capability lease
    if lease.proposal_id != request.proposal_id {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            format!(
                "proposal_id mismatch: capability is bound to proposal '{}', but request specifies proposal '{}'",
                lease.proposal_id, request.proposal_id
            ),
        ));
    }

    // Load the proposal to check its decision
    let proposal = runtime
        .store
        .proposals()
        .get(request.proposal_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "proposal not found",
            )
        })?;

    // Check proposal decision - fail-safe: block execution for non-Allow decisions
    let decision = proposal.decision.as_ref();
    let is_quarantined = decision
        .map(|d| *d == Decision::Quarantine)
        .unwrap_or(false);
    let is_require_approval = decision
        .map(|d| *d == Decision::RequireApproval)
        .unwrap_or(false);
    let is_deny = decision.map(|d| *d == Decision::Deny).unwrap_or(false);
    let is_draft_only = decision
        .map(|d| *d == Decision::AllowDraftOnly)
        .unwrap_or(false);

    let now = Utc::now();

    // Handle DraftOnly: allow only for dry_run, otherwise fail-closed
    if is_draft_only && !request.dry_run {
        let record = ExecutionRecord {
            execution_id: ExecutionId::new(),
            proposal_id: request.proposal_id,
            intent_id: lease.intent_id,
            capability_id: lease.capability_id,
            rollback_contract_id: None,
            decision: Decision::Deny,
            state: ExecutionState::Denied,
            started_at: now,
            finished_at: Some(now),
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        };

        let execution_id = record.execution_id;
        let intent_id = record.intent_id;
        let capability_id = record.capability_id;
        let proposal_id = record.proposal_id;

        if let Err(e) = runtime.store.executions().insert(&record).await {
            tracing::warn!("failed to persist execution: {}", e);
        } else {
            let event = create_provenance_event(
                ProvenanceEventKind::ToolCallPrepared,
                now,
                Some(intent_id),
                Some(proposal_id),
                Some(execution_id),
                Some(capability_id),
                None,
                None,
            );
            if let Err(e) = runtime.store.provenance().append_event(&event).await {
                tracing::warn!("failed to persist provenance event: {}", e);
            }
        }

        return Ok(Json(AuthorizeExecutionResponse {
            execution: record,
            warnings: vec!["draft-only proposal denied for non-dry-run execution".to_string()],
        }));
    }

    // Non-allow decisions should not progress to executable states
    let is_blocked = is_quarantined || is_require_approval || is_deny;

    // Determine execution state and decision based on proposal decision
    let (execution_state, execution_decision) = if is_blocked {
        // Blocked decisions get terminal error states:
        // - Quarantine -> Quarantined (already terminal)
        // - RequireApproval -> AwaitingApproval (requires external approval before execution)
        // - Deny -> Denied (terminal, rejected)
        if is_quarantined {
            (ExecutionState::Quarantined, Decision::Quarantine)
        } else if is_require_approval {
            (ExecutionState::AwaitingApproval, Decision::RequireApproval)
        } else {
            (ExecutionState::Denied, Decision::Deny)
        }
    } else if request.dry_run {
        let decision = if is_draft_only {
            Decision::AllowDraftOnly
        } else {
            Decision::Allow
        };
        (ExecutionState::Authorized, decision)
    } else {
        (ExecutionState::Prepared, Decision::Allow)
    };

    let record = ExecutionRecord {
        execution_id: ExecutionId::new(),
        proposal_id: request.proposal_id,
        intent_id: lease.intent_id,
        capability_id: lease.capability_id,
        rollback_contract_id: None,
        decision: execution_decision,
        state: execution_state,
        started_at: now,
        finished_at: if is_blocked { Some(now) } else { None },
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    let execution_id = record.execution_id;
    let intent_id = record.intent_id;
    let capability_id = record.capability_id;
    let proposal_id = record.proposal_id;

    if let Err(e) = runtime.store.executions().insert(&record).await {
        tracing::warn!("failed to persist execution: {}", e);
    } else {
        // Emit appropriate provenance event based on decision
        let event_kind = if is_quarantined {
            ProvenanceEventKind::Quarantined
        } else {
            ProvenanceEventKind::ToolCallPrepared
        };
        let event = create_provenance_event(
            event_kind,
            now,
            Some(intent_id),
            Some(proposal_id),
            Some(execution_id),
            Some(capability_id),
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    // For RequireApproval, create and persist an approval request
    if is_require_approval {
        let approval = ApprovalRequest {
            approval_id: ApprovalId::new(),
            intent_id,
            proposal_id,
            execution_id: Some(execution_id),
            requested_by: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("Ferrum Gateway".to_string()),
            },
            reason: proposal.expected_effect.clone(),
            action_digest: format!("{}/{}", proposal.server_name, proposal.tool_name),
            expires_at: now + Duration::hours(24),
            state: ApprovalState::Pending,
            created_at: now,
        };

        if let Err(e) = runtime.store.approvals().insert(&approval).await {
            tracing::warn!("failed to persist approval request: {}", e);
        } else {
            tracing::info!(
                "approval request created: {} for execution: {}",
                approval.approval_id,
                execution_id
            );

            // Emit ApprovalRequested provenance event
            let event = create_provenance_event(
                ProvenanceEventKind::ApprovalRequested,
                now,
                Some(intent_id),
                Some(proposal_id),
                Some(execution_id),
                Some(capability_id),
                None,
                None,
            );
            if let Err(e) = runtime.store.provenance().append_event(&event).await {
                tracing::warn!(
                    "failed to persist approval requested provenance event: {}",
                    e
                );
            }
        }
    }

    Ok(Json(AuthorizeExecutionResponse {
        execution: record,
        warnings: Vec::new(),
    }))
}

async fn prepare_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::PrepareExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id)?;

    let Some(existing) = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
    else {
        return Err(ApiProblem::new(
            StatusCode::NOT_FOUND,
            ApiErrorCode::NotFound,
            "execution record not found",
        ));
    };

    if matches!(existing.decision, Decision::AllowDraftOnly) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::PolicyDenied,
            "draft-only execution cannot proceed to prepare",
        ));
    }

    // State guard: block terminal/error states from proceeding to prepare
    // Terminal states that should NOT proceed: Quarantined, Denied, Failed, Compensated, RolledBack, Committed
    // Also block: AwaitingApproval (requires external approval before proceeding)
    match existing.state {
        ExecutionState::Quarantined => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "execution is quarantined and cannot proceed",
            ));
        }
        ExecutionState::AwaitingApproval => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::ApprovalRequired,
                "execution is awaiting approval and cannot proceed",
            ));
        }
        ExecutionState::Denied => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::PolicyDenied,
                "execution was denied and cannot proceed",
            ));
        }
        ExecutionState::Failed => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "execution has failed and cannot proceed",
            ));
        }
        ExecutionState::Compensated | ExecutionState::RolledBack | ExecutionState::Committed => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "execution is already in terminal state: {:?}",
                    existing.state
                ),
            ));
        }
        _ => {} // Proceed for non-terminal states: Proposed, Authorized, Prepared, Running, AwaitingVerification
    }

    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Load proposal to get the correct rollback class - FAIL CLOSED if not found
    let requested_rollback_class = match runtime.store.proposals().get(proposal_id).await {
        Ok(Some(proposal)) => proposal.requested_rollback_class,
        Ok(None) => {
            return Err(ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                format!(
                    "proposal {} not found for execution preparation",
                    proposal_id
                ),
            ));
        }
        Err(e) => {
            return Err(ApiProblem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Internal,
                format!("failed to load proposal {}: {}", proposal_id, e),
            ));
        }
    };

    let request = runtime.rollback.default_prepare_request(
        intent_id,
        proposal_id,
        execution_id,
        requested_rollback_class,
    );

    let response = runtime
        .rollback
        .prepare(request)
        .await
        .map_err(ApiProblem::internal)?;

    let contract = response.contract.clone();
    let now = Utc::now();

    if let Err(e) = runtime.store.rollback_contracts().insert(&contract).await {
        tracing::warn!("failed to persist rollback contract: {}", e);
    } else {
        let mut updated_execution = existing;
        updated_execution.rollback_contract_id = Some(contract.contract_id);
        updated_execution.state = ExecutionState::Prepared;

        if let Err(e) = runtime.store.executions().update(&updated_execution).await {
            tracing::warn!("failed to update execution with rollback contract: {}", e);
        }

        let event = create_provenance_event(
            ProvenanceEventKind::SideEffectPrepared,
            now,
            Some(intent_id),
            Some(proposal_id),
            Some(execution_id),
            None,
            Some(contract.contract_id),
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }
    }

    Ok(Json(ferrum_proto::PrepareExecutionResponse {
        execution_id,
        prepared: response.accepted,
        rollback_contract: Some(contract),
        warnings: response.warnings,
    }))
}

async fn execute_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: execute requires Prepared
    if !matches!(existing.state, ExecutionState::Prepared) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::ValidationError,
            format!(
                "execution must be in Prepared state to execute, current state: {:?}",
                existing.state
            ),
        ));
    }

    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract, must prepare first",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Load capability lease for enforcement check
    let lease = runtime
        .cap
        .get(existing.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

    // Enforce execution-time HTTP egress policy
    // Non-HTTP flows pass through unchanged; HTTP flows are validated against bindings
    if let Err(enforcement_err) = runtime
        .firewall
        .enforce_execution_payload(&lease.resource_bindings, &req.payload)
    {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            format!("execution denied: {}", enforcement_err),
        ));
    }

    // Execute via adapter
    let receipt = runtime
        .rollback
        .execute(&contract, &req.payload)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to Running (executing)
    let mut updated_execution = existing;
    updated_execution.state = ExecutionState::Running;
    updated_execution.result_digest = receipt.result_digest.clone();

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to ExecutedAwaitingVerify
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::ExecutedAwaitingVerify)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit ToolCallExecuted provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::ToolCallExecuted,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(ExecuteResponse {
        execution_id,
        executed: true,
        result_digest: receipt.result_digest,
        external_id: receipt.external_id,
    }))
}

async fn verify_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: verify requires Running
    if !matches!(existing.state, ExecutionState::Running) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::ValidationError,
            format!(
                "execution must be in Running state to verify, current state: {:?}",
                existing.state
            ),
        ));
    }

    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Verify via adapter
    let verified = runtime
        .rollback
        .verify(&contract)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();

    // Update execution state to AwaitingVerification
    let mut updated_execution = existing.clone();
    updated_execution.state = if verified {
        ExecutionState::AwaitingVerification
    } else {
        ExecutionState::Failed
    };

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to Verified (or Failed)
    if verified {
        if let Err(e) = runtime
            .store
            .rollback_contracts()
            .update_state(contract_id, RollbackState::Verified)
            .await
        {
            tracing::warn!("failed to update rollback contract state: {}", e);
        }
    }

    // Emit SideEffectVerified provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectVerified,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    // Auto-commit for non-R3 contracts if verified
    if verified && contract.auto_commit {
        let commit_response = perform_commit(&runtime, &existing, &contract, now).await?;
        return Ok(Json(VerifyResponse {
            execution_id,
            verified: true,
            verified_at: Some(commit_response.committed_at.unwrap_or(now)),
        }));
    }

    Ok(Json(VerifyResponse {
        execution_id,
        verified,
        verified_at: if verified { Some(now) } else { None },
    }))
}

async fn commit_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<CommitRequest>,
) -> Result<Json<CommitResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: commit requires AwaitingVerification
    if !matches!(existing.state, ExecutionState::AwaitingVerification) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::ValidationError,
            format!(
                "execution must be in AwaitingVerification state to commit, current state: {:?}",
                existing.state
            ),
        ));
    }

    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    let now = Utc::now();
    perform_commit(&runtime, &existing, &contract, now).await
}

async fn perform_commit(
    runtime: &Arc<GatewayRuntime>,
    existing: &ExecutionRecord,
    contract: &ferrum_proto::RollbackContract,
    now: chrono::DateTime<Utc>,
) -> Result<Json<CommitResponse>, ApiProblem> {
    let execution_id = existing.execution_id;
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;
    let contract_id = contract.contract_id;

    // Update execution state to Committed
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::Committed;
    updated_execution.finished_at = Some(now);

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to Committed
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::Committed)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit SideEffectCommitted provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectCommitted,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(CommitResponse {
        execution_id,
        committed: true,
        committed_at: Some(now),
    }))
}

async fn compensate_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<CompensateRequest>,
) -> Result<Json<CompensateResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: reject if already in a terminal state that cannot be compensated
    // Note: Committed is allowed (can undo after commit), but Compensated/RolledBack/Denied/Failed/Quarantined are terminal
    use ferrum_proto::ExecutionState::*;
    match existing.state {
        Compensated | RolledBack | Denied | Failed | Quarantined => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!("execution already in terminal state: {:?}", existing.state),
            ));
        }
        _ => {}
    }

    // State guard: compensate requires execution to have a rollback contract
    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Call rollback service to compensate
    runtime
        .rollback
        .compensate(&contract)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to Compensated
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::Compensated;
    updated_execution.finished_at = Some(now);

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to Compensated
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::Compensated)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit SideEffectCompensated provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectCompensated,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(CompensateResponse {
        execution_id,
        compensated: true,
        compensated_at: Some(now),
    }))
}

async fn rollback_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id_str): Path<String>,
    Json(req): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id_str)?;

    // Validate that the request execution_id matches the path
    if req.execution_id != execution_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "execution_id in body does not match path",
        ));
    }

    let existing = runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "execution record not found",
            )
        })?;

    // State guard: reject if already in a terminal state that cannot be rolled back
    // Note: Committed is allowed (can undo after commit), but Compensated/RolledBack/Denied/Failed/Quarantined are terminal
    use ferrum_proto::ExecutionState::*;
    match existing.state {
        Compensated | RolledBack | Denied | Failed | Quarantined => {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!("execution already in terminal state: {:?}", existing.state),
            ));
        }
        _ => {}
    }

    // State guard: rollback requires execution to have a rollback contract
    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::PRECONDITION_FAILED,
            ApiErrorCode::ValidationError,
            "execution has no rollback contract",
        )
    })?;

    let contract = runtime
        .store
        .rollback_contracts()
        .get(contract_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "rollback contract not found",
            )
        })?;

    // Call rollback service to rollback
    runtime
        .rollback
        .rollback(&contract)
        .await
        .map_err(ApiProblem::internal)?;

    let now = Utc::now();
    let intent_id = existing.intent_id;
    let proposal_id = existing.proposal_id;

    // Update execution state to RolledBack
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::RolledBack;
    updated_execution.finished_at = Some(now);

    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::warn!("failed to update execution state: {}", e);
    }

    // Advance rollback contract state to RolledBack
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update_state(contract_id, RollbackState::RolledBack)
        .await
    {
        tracing::warn!("failed to update rollback contract state: {}", e);
    }

    // Emit SideEffectRolledBack provenance event
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectRolledBack,
        now,
        Some(intent_id),
        Some(proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist provenance event: {}", e);
    }

    Ok(Json(RollbackResponse {
        execution_id,
        rolled_back: true,
        rolled_back_at: Some(now),
    }))
}

// Approval handlers

async fn get_approval(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalRequest>, ApiProblem> {
    let approval_id = parse_approval_id(&approval_id)?;

    let approval = runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "approval not found",
            )
        })?;

    Ok(Json(approval))
}

async fn list_pending_approvals(
    State(runtime): State<Arc<GatewayRuntime>>,
) -> Result<Json<Vec<ApprovalRequest>>, ApiProblem> {
    let pending = runtime
        .store
        .approvals()
        .list_pending()
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?;

    Ok(Json(pending))
}

async fn resolve_approval(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(approval_id): Path<String>,
    Json(request): Json<ApprovalResolveRequest>,
) -> Result<Json<ApprovalRequest>, ApiProblem> {
    let approval_id = parse_approval_id(&approval_id)?;

    // Get the approval
    let mut approval = runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "approval not found",
            )
        })?;

    // Validate current state - only Pending approvals can be resolved
    if !matches!(approval.state, ApprovalState::Pending) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::Conflict,
            format!(
                "approval is not in Pending state, current state: {:?}",
                approval.state
            ),
        ));
    }

    let now = Utc::now();

    // Update approval state based on resolution
    let new_state = if request.approve {
        ApprovalState::Granted
    } else {
        ApprovalState::Denied
    };

    approval.state = new_state.clone();

    // Persist the updated approval
    if let Err(e) = runtime.store.approvals().update(&approval).await {
        return Err(ApiProblem::internal(e.into()));
    }

    // Update linked execution if present
    if let Some(execution_id) = approval.execution_id {
        let Some(mut execution) = runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .map_err(|err| ApiProblem::internal(err.into()))?
        else {
            return Err(ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "linked execution not found",
            ));
        };

        // Validate execution is in AwaitingApproval state
        if !matches!(execution.state, ExecutionState::AwaitingApproval) {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "execution is not in AwaitingApproval state, current state: {:?}",
                    execution.state
                ),
            ));
        }

        // Transition execution based on approval decision
        if request.approve {
            execution.state = ExecutionState::Authorized;
            execution.decision = Decision::Allow;
        } else {
            execution.state = ExecutionState::Denied;
            execution.decision = Decision::Deny;
            execution.finished_at = Some(now);
        }

        if let Err(e) = runtime.store.executions().update(&execution).await {
            tracing::warn!(
                "failed to update execution after approval resolution: {}",
                e
            );
        } else {
            // Emit provenance event for approval resolution
            let event_kind = if request.approve {
                ProvenanceEventKind::ApprovalGranted
            } else {
                ProvenanceEventKind::ApprovalDenied
            };
            let event = create_provenance_event(
                event_kind,
                now,
                Some(execution.intent_id),
                Some(execution.proposal_id),
                Some(execution_id),
                Some(execution.capability_id),
                None,
                None,
            );
            if let Err(e) = runtime.store.provenance().append_event(&event).await {
                tracing::warn!(
                    "failed to persist approval resolution provenance event: {}",
                    e
                );
            }
        }
    }

    Ok(Json(approval))
}

fn parse_approval_id(value: &str) -> Result<ApprovalId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid approval uuid",
        )
    })?;
    Ok(ApprovalId(parsed))
}

fn infer_rollback_class(scope: &[ResourceSelector]) -> RollbackClass {
    if scope
        .iter()
        .any(|selector| matches!(selector, ResourceSelector::EmailDraft { .. }))
    {
        RollbackClass::R2Compensatable
    } else {
        RollbackClass::R0NativeReversible
    }
}

fn minimal_intent_for(
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
            id: "default".to_string(),
            description: "scaffold outcome".to_string(),
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
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + Duration::minutes(15),
    }
}

fn parse_capability_id(value: &str) -> Result<CapabilityId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid capability uuid",
        )
    })?;
    Ok(CapabilityId(parsed))
}

fn parse_execution_id(value: &str) -> Result<ExecutionId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "path id is not a valid execution uuid",
        )
    })?;
    Ok(ExecutionId(parsed))
}

#[derive(Debug)]
struct ApiProblem(ApiError, StatusCode);

impl ApiProblem {
    fn new(status: StatusCode, code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self(
            ApiError {
                code,
                message: message.into(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            },
            status,
        )
    }

    fn internal(err: anyhow::Error) -> Self {
        Self(
            ApiError {
                code: ApiErrorCode::Internal,
                message: err.to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
                retriable: false,
                details: serde_json::json!({}),
            },
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    }

    fn from_capability(err: ferrum_cap::CapabilityError) -> Self {
        let code = match err {
            ferrum_cap::CapabilityError::NotFound => ApiErrorCode::NotFound,
            ferrum_cap::CapabilityError::AlreadyUsed => ApiErrorCode::Conflict,
            ferrum_cap::CapabilityError::Revoked => ApiErrorCode::CapabilityRevoked,
            ferrum_cap::CapabilityError::Expired => ApiErrorCode::CapabilityExpired,
            ferrum_cap::CapabilityError::TtlTooLong => ApiErrorCode::ValidationError,
        };
        Self::new(StatusCode::BAD_REQUEST, code, err.to_string())
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        (self.1, Json(self.0)).into_response()
    }
}
