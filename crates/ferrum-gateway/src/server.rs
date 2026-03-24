use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Duration, Utc};
use ferrum_graph::LineageGraph;
use ferrum_proto::{
    ActorRef, ActorType, ApiError, ApiErrorCode, ApprovalId, ApprovalRequest,
    ApprovalResolveRequest, ApprovalState, AuthorizeExecutionRequest, AuthorizeExecutionResponse,
    CapabilityId, CapabilityMintRequest, CapabilityMintResponse, CapabilityStatus,
    CompensateExecutionResponse, Decision, EvaluateProposalResponse, ExecutionId, ExecutionRecord,
    ExecutionState, HashChainRef, HealthResponse, IntentCompileRequest, IntentCompileResponse,
    IntentEnvelope, IntentStatus, ListApprovalsResponse, ObjectRef, ObjectType, OutcomeClause,
    ProposalId, ProvenanceEvent, ProvenanceEventKind, ResourceSelector, RiskTier, RollbackClass,
    RollbackExecutionResponse, RollbackState, TimeBudget, TrustContextSummary,
};
use ferrum_store::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, ProvenanceRepo,
    RollbackRepo,
};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::{AuthMode, GatewayConfig, GatewayRuntime, ServerConfig};

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

/// HTTP server entry point. Chooses auth-aware or auth-disabled router based on config.
/// Auth-aware router enforces bearer token on all non-health endpoints.
pub async fn run_http_server(
    config: GatewayConfig,
    runtime: GatewayRuntime,
    server_config: ServerConfig,
) -> anyhow::Result<()> {
    let app = if server_config.auth_mode == AuthMode::Bearer {
        build_authenticated_router(runtime, server_config)
    } else {
        build_router(runtime)
    };
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("ferrumd listening on {}", config.bind_addr);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Builds the auth-disabled router used by integration tests and local development.
/// All routes are accessible without authentication.
pub fn build_router(runtime: GatewayRuntime) -> Router {
    build_router_inner(runtime, None)
}

/// Builds an auth-aware router that enforces bearer token on all non-health endpoints.
/// Health endpoints (/v1/healthz, /v1/readyz) remain unauthenticated.
pub fn build_authenticated_router(runtime: GatewayRuntime, server_config: ServerConfig) -> Router {
    build_router_inner(runtime, Some(server_config))
}

fn build_router_inner(runtime: GatewayRuntime, auth_config: Option<ServerConfig>) -> Router {
    let router = Router::new()
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
        .route("/v1/approvals", get(list_approvals))
        .route("/v1/approvals/{approval_id}", get(get_approval))
        .route(
            "/v1/approvals/{approval_id}/resolve",
            post(resolve_approval),
        )
        .route(
            "/v1/executions/{execution_id}/rollback",
            post(rollback_execution),
        )
        .route(
            "/v1/executions/{execution_id}/compensate",
            post(compensate_execution),
        )
        .with_state(Arc::new(runtime))
        .layer(TraceLayer::new_for_http());

    if let Some(server_config) = auth_config {
        let bearer_token = server_config.bearer_token.clone();
        router.layer(axum::middleware::from_fn_with_state(
            bearer_token,
            bearer_auth_middleware,
        ))
    } else {
        router
    }
}

/// Bearer authentication middleware.
/// Returns 401 with ApiErrorCode::PolicyDenied when auth is missing or invalid.
/// Health endpoints pass through without authentication.
async fn bearer_auth_middleware(
    State(token): State<Option<String>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Health endpoints are always accessible without auth
    if path == "/v1/healthz" || path == "/v1/readyz" {
        return next.run(request).await;
    }

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return ApiProblem::auth_error("authentication required");
        }
    };

    // Extract Bearer token from Authorization header
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let provided = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return ApiProblem::auth_error("missing or invalid Authorization header");
        }
    };

    if provided != token {
        return ApiProblem::auth_error("invalid bearer token");
    }

    next.run(request).await
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
        tracing::error!(
            intent_id = %intent_id,
            error = %e,
            "compile_intent: failed to persist intent; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

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

    Ok(Json(IntentCompileResponse { envelope, warnings }))
}

async fn evaluate_proposal(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(proposal_id_from_path): Path<String>,
    Json(proposal): Json<ferrum_proto::ActionProposal>,
) -> Result<Json<EvaluateProposalResponse>, ApiProblem> {
    let intent = minimal_intent_for(
        proposal.intent_id,
        proposal.requested_rollback_class.clone(),
    );
    // Derive taint score from proposal taint inputs: 10 points per taint input.
    // 7+ taint inputs (>= 70) with non-R0 rollback triggers PDP quarantine.
    let taint_score = (proposal.taint_inputs.len() as u8).saturating_mul(10);
    let trust = TrustContextSummary {
        input_labels: Vec::new(),
        sensitivity_labels: Vec::new(),
        taint_score,
        contains_external_metadata: false,
        contains_tool_output: false,
        contains_untrusted_text: false,
    };

    // Persist the intent first (proposals have FK to intents).
    // Fail closed if we cannot persist critical intent data.
    if let Err(e) = runtime.store.intents().insert(&intent).await {
        tracing::error!(
            intent_id = %intent.intent_id,
            error = %e,
            "evaluate_proposal: failed to persist scaffold intent; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    // Persist the proposal so it can be looked up by authorize_execution.
    // Fail closed if we cannot persist critical proposal data.
    if let Err(e) = runtime.store.proposals().insert(&proposal).await {
        tracing::error!(
            proposal_id = %proposal.proposal_id,
            error = %e,
            "authorize_execution: failed to persist proposal"
        );
        return Err(ApiProblem::internal(anyhow::Error::from(e)));
    }

    let out = runtime
        .pdp
        .evaluate(&intent, &proposal_for_eval, &combined_trust)
        .await
        .map_err(ApiProblem::internal)?;

    // Merge contradiction warnings into PDP output
    if !contradiction_warnings.is_empty() {
        out.warnings.extend(contradiction_warnings);
    }

    // Store the decision in the proposal after evaluation (already has effective rollback class)
    let mut proposal_with_decision = proposal_for_eval.clone();
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

/// Check if a resource binding is within the allowed scope (subset check)
/// Fail-closed: any mismatch or permission widening results in denial
fn is_binding_within_scope(binding: &ResourceBinding, scope: &[ResourceSelector]) -> bool {
    // Fail-closed: if no scope defined, deny any non-empty binding
    if scope.is_empty() {
        return false;
    }

    match binding {
        ResourceBinding::File { path, mode, .. } => {
            scope.iter().any(|selector| match selector {
                ResourceSelector::FilesystemPath {
                    path: scope_path,
                    mode: scope_mode,
                    ..
                } => {
                    // Path must be within scope (prefix match)
                    let path_ok = path.starts_with(scope_path);
                    // Mode must not exceed scope - conservative subset check
                    let mode_ok = is_mode_subset_of(mode, scope_mode);
                    path_ok && mode_ok
                }
                _ => false,
            })
        }
        ResourceBinding::Http {
            base_url,
            path_prefix,
            method: _,
            mode,
            ..
        } => scope.iter().any(|selector| match selector {
            ResourceSelector::HttpEndpoint {
                base_url: scope_url,
                path_prefix: scope_prefix,
                mode: scope_mode,
                ..
            } => {
                let url_ok = base_url == scope_url;
                let prefix_ok = path_prefix.starts_with(scope_prefix);
                // Mode must not exceed scope - conservative subset check
                let mode_ok = is_mode_subset_of(mode, scope_mode);
                url_ok && prefix_ok && mode_ok
            }
            _ => false,
        }),
        ResourceBinding::Sqlite {
            db_path,
            tables,
            mode,
        } => {
            scope.iter().any(|selector| match selector {
                ResourceSelector::SqliteDatabase {
                    db_path: scope_db,
                    tables: scope_tables,
                    mode: scope_mode,
                } => {
                    let db_ok = db_path == scope_db;
                    // Tables must be subset of scope tables (or scope allows all)
                    let tables_ok =
                        scope_tables.is_empty() || tables.iter().all(|t| scope_tables.contains(t));
                    // Mode must not exceed scope - conservative subset check
                    let mode_ok = is_mode_subset_of(mode, scope_mode);
                    db_ok && tables_ok && mode_ok
                }
                _ => false,
            })
        }
        ResourceBinding::Git {
            repo_path,
            allowed_refs,
            mode,
        } => {
            scope.iter().any(|selector| match selector {
                ResourceSelector::GitRepository {
                    repo_path: scope_repo,
                    allowed_refs: scope_refs,
                    mode: scope_mode,
                } => {
                    let repo_ok = repo_path == scope_repo;
                    // Refs must be subset of scope refs (or scope allows all)
                    let refs_ok = scope_refs.is_empty()
                        || allowed_refs.iter().all(|r| scope_refs.contains(r));
                    // Mode must not exceed scope - conservative subset check
                    let mode_ok = is_mode_subset_of(mode, scope_mode);
                    repo_ok && refs_ok && mode_ok
                }
                _ => false,
            })
        }
        ResourceBinding::EmailDraft {
            recipients,
            allow_send,
            mode,
        } => {
            scope.iter().any(|selector| match selector {
                ResourceSelector::EmailDraft {
                    recipient_allowlist,
                    mode: scope_mode,
                    ..
                } => {
                    // Recipients must be in allowlist
                    let recipients_ok = recipient_allowlist.is_empty()
                        || recipients.iter().all(|r| recipient_allowlist.contains(r));
                    // If scope is read-only, cannot send
                    let send_ok = !matches!((scope_mode, allow_send), (ResourceMode::Read, true));
                    // Mode must not exceed scope - conservative subset check
                    let mode_ok = is_mode_subset_of(mode, scope_mode);
                    recipients_ok && send_ok && mode_ok
                }
                _ => false,
            })
        }
    }
}

/// Check if a requested mode is a subset of (does not exceed) the scope mode.
/// Conservative permission model: scope_mode must encompass all permissions in requested mode.
fn is_mode_subset_of(requested: &ResourceMode, scope: &ResourceMode) -> bool {
    match scope {
        // Admin scope allows any mode
        ResourceMode::Admin => true,
        // ReadWrite scope allows Read, Write, ReadWrite, but NOT Execute/Admin
        ResourceMode::ReadWrite => matches!(
            requested,
            ResourceMode::Read | ResourceMode::Write | ResourceMode::ReadWrite
        ),
        // Write scope allows Write and Read (write access typically implies read access to written data)
        ResourceMode::Write => matches!(requested, ResourceMode::Write | ResourceMode::Read),
        // Read scope allows only Read (most restrictive)
        ResourceMode::Read => matches!(requested, ResourceMode::Read),
        // Draft scope allows only Draft (special purpose mode)
        ResourceMode::Draft => matches!(requested, ResourceMode::Draft),
        // Execute scope allows only Execute (special purpose mode)
        ResourceMode::Execute => matches!(requested, ResourceMode::Execute),
    }
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

    // Fail-closed: return an error if capability lease cannot be persisted.
    // A minted capability that is not durable is a security risk because the
    // capability can be re-issued on restart, bypassing single-use enforcement.
    if let Err(e) = runtime.store.capabilities().insert(&response.lease).await {
        tracing::error!(
            capability_id = %capability_id,
            error = %e,
            "mint_capability: failed to persist capability lease; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    // Provenance is best-effort only; capability persistence is the critical path.
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
        tracing::error!(
            capability_id = %lease.capability_id,
            error = %e,
            "revoke_capability: failed to persist revoked capability lease; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

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

    Ok(Json(serde_json::json!({
        "ok": true,
        "capability_id": lease.capability_id.to_string()
    })))
}

async fn authorize_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Json(request): Json<AuthorizeExecutionRequest>,
) -> Result<Json<AuthorizeExecutionResponse>, ApiProblem> {
    // Step 1: Inspect the lease to validate proposal_id scope before consuming.
    let lease = runtime
        .cap
        .get(request.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

    // Fail-closed: reject execution if proposal_id does not match the capability's authorized scope.
    if request.proposal_id != lease.proposal_id {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::PolicyDenied,
            format!(
                "proposal mismatch: capability is authorized for proposal {}, but request targets proposal {}",
                lease.proposal_id, request.proposal_id
            ),
        ));
    }

    // Fail-closed: reject if the capability has already been used (single-use enforcement).
    // This check is placed after proposal_id validation but before any state-changing operation.
    if matches!(lease.status, CapabilityStatus::Used) {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::Conflict,
            "capability already used",
        ));
    }

    // Step 2: Load the proposal to inspect its rollback class.
    // We do this before consuming the capability so that R3 proposals can be
    // gated without burning the single-use token.
    //
    // Fail-closed: if the proposal is missing or lookup errors, reject execution
    // rather than silently proceeding without R3 safety gating.
    let is_r3 = match runtime.store.proposals().get(request.proposal_id).await {
        Ok(Some(proposal)) => {
            proposal.requested_rollback_class == RollbackClass::R3IrreversibleHighConsequence
        }
        Ok(None) => {
            tracing::error!(
                proposal_id = %request.proposal_id,
                "authorize_execution: proposal not found in store; rejecting (fail-closed)"
            );
            return Err(ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                format!("proposal {} not found", request.proposal_id),
            ));
        }
        Err(e) => {
            tracing::error!(
                proposal_id = %request.proposal_id,
                error = %e,
                "authorize_execution: proposal lookup error; rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }
    };

    // Step 3: For R3 (IrreversibleHighConsequence), gate with approval instead of auto-committing.
    if is_r3 {
        let now = Utc::now();
        let record = ExecutionRecord {
            execution_id: ExecutionId::new(),
            proposal_id: request.proposal_id,
            intent_id: lease.intent_id,
            capability_id: lease.capability_id,
            rollback_contract_id: None,
            decision: Decision::RequireApproval,
            state: ExecutionState::AwaitingApproval,
            started_at: now,
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        };

        // Persist the execution record (capability is NOT consumed yet — approval must be granted first)
        // Fail-closed: return an error if execution record cannot be persisted.
        if let Err(e) = runtime.store.executions().insert(&record).await {
            tracing::error!(
                execution_id = %record.execution_id,
                error = %e,
                "authorize_execution: failed to persist execution record (R3 path); rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }
        let event = create_provenance_event(
            ProvenanceEventKind::ToolCallPrepared,
            now,
            Some(record.intent_id),
            Some(record.proposal_id),
            Some(record.execution_id),
            Some(record.capability_id),
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist provenance event: {}", e);
        }

        // Create and persist an approval request for this R3 execution.
        // The capability is NOT consumed yet — it will be consumed only after approval is granted.
        // Fail-closed: return an error if the approval request cannot be persisted.
        //
        // Alignment: if the capability lease has an approval_binding with a pre-set approval_id,
        // use that ID so the binding is satisfied. Otherwise generate a fresh ID.
        let approval_id = lease
            .approval_binding
            .as_ref()
            .map(|b| b.approval_id)
            .unwrap_or_else(ApprovalId::new);
        let approval = ApprovalRequest {
            approval_id,
            intent_id: record.intent_id,
            proposal_id: record.proposal_id,
            execution_id: Some(record.execution_id),
            requested_by: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("Ferrum Gateway".to_string()),
            },
            reason: "R3 execution requires explicit approval before capability consumption"
                .to_string(),
            action_digest: record.proposal_id.to_string(), // proposal_id is stable: known at mint and resolve time
            expires_at: now + Duration::minutes(15),
            state: ApprovalState::Pending,
            created_at: now,
        };
        if let Err(e) = runtime.store.approvals().insert(&approval).await {
            tracing::error!(
                approval_id = %approval.approval_id,
                execution_id = %record.execution_id,
                error = %e,
                "authorize_execution: failed to persist approval request (R3 path); rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }

        // Add the approval_id to the execution's metadata so it can be discovered
        // by the caller for the resolve step.
        let mut record_with_approval = record;
        record_with_approval.metadata.insert(
            "r3_approval_id".to_string(),
            serde_json::json!(approval.approval_id.to_string()),
        );

        return Ok(Json(AuthorizeExecutionResponse {
            execution: record_with_approval,
            warnings: Vec::new(),
        }));
    }

    // Step 4: Consume the capability (authoritative consume step)
    let lease_used = runtime
        .cap
        .mark_used(request.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

    // Step 5: Immediately persist the used lease
    if let Err(e) = runtime.store.capabilities().update(&lease_used).await {
        tracing::error!("failed to persist used capability: {}", e);
        return Err(ApiProblem::internal(e.into()));
    }

    let now = Utc::now();

    // SINGLE-USE ENFORCEMENT: Mark capability as consumed at authorize time
    // This ensures exactly one authorize per capability
    runtime
        .cap
        .mark_used(request.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

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
        intent_id: lease_used.intent_id,
        capability_id: lease_used.capability_id,
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

    // Fail-closed: return an error if execution record cannot be persisted.
    if let Err(e) = runtime.store.executions().insert(&record).await {
        tracing::error!(
            execution_id = %execution_id,
            error = %e,
            "authorize_execution: failed to persist execution record (non-R3 path); rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }
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

    // Load proposal to get the correct rollback class and approved request arguments.
    // HTTP prepare uses the approved arguments to bind a concrete request digest.
    let proposal = match runtime.store.proposals().get(proposal_id).await {
        Ok(Some(proposal)) => proposal,
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
    let requested_rollback_class = proposal.requested_rollback_class.clone();

    // Determine adapter key and target from capability resource bindings
    // Load capability to inspect resource bindings for adapter routing
    let capability = runtime
        .cap
        .get(existing.capability_id)
        .await
        .map_err(ApiProblem::from_capability)?;

    // Fail-closed: explicitly deny EmailDraft bindings with allow_send=true.
    // These represent a send-capable email binding which is out of scope for v1.
    // Routing to noop would silently succeed; we instead return a clear error.
    let has_send_email = capability.resource_bindings.iter().any(|b| {
        matches!(
            b,
            ResourceBinding::EmailDraft {
                allow_send: true,
                ..
            }
        )
    });

    if has_send_email {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "EmailDraft with allow_send=true is not supported in v1: \
             real send recovery is out of scope; use draft-only (allow_send=false) instead",
        ));
    }

    let adapter_key = determine_adapter_key_from_bindings(&capability.resource_bindings);
    let target = determine_rollback_target_from_bindings(&capability.resource_bindings);

    let mut request = runtime.rollback.default_prepare_request(
        intent_id,
        proposal_id,
        execution_id,
        requested_rollback_class,
        adapter_key,
        target,
    );

    if request.adapter_key == "http" {
        request.metadata.insert(
            "approved_http_request".to_string(),
            proposal.raw_arguments.clone(),
        );
    }

    let response = runtime
        .rollback
        .prepare(request)
        .await
        .map_err(ApiProblem::internal)?;

    let mut contract = response.contract.clone();
    contract.metadata.remove("approved_http_request");
    let now = Utc::now();

    // Fail closed: return an error if rollback contract cannot be persisted.
    if let Err(e) = runtime.store.rollback_contracts().insert(&contract).await {
        tracing::error!(
            contract_id = %contract.contract_id,
            execution_id = %execution_id,
            error = %e,
            "prepare_execution: failed to persist rollback contract; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    let mut updated_execution = existing;
    updated_execution.rollback_contract_id = Some(contract.contract_id);
    updated_execution.state = ExecutionState::Prepared;

    // Fail closed: return an error if execution record cannot be updated.
    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::error!(
            execution_id = %execution_id,
            error = %e,
            "prepare_execution: failed to update execution with rollback contract; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
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

    Ok(Json(ferrum_proto::PrepareExecutionResponse {
        execution_id,
        prepared: response.accepted,
        rollback_contract: Some(contract),
        warnings: response.warnings,
    }))
}

async fn rollback_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id): Path<String>,
) -> Result<Json<RollbackExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id)?;

    // Load the execution record. Fail-closed if not found.
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

    // Fail-closed if no rollback contract is associated with this execution.
    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::RollbackUnsupported,
            "execution has no associated rollback contract",
        )
    })?;

    // Load the rollback contract. Fail-closed if not found.
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

    // Invoke the rollback adapter. Fail-closed on error.
    if let Err(e) = runtime.rollback.rollback(&contract).await {
        tracing::error!(
            execution_id = %execution_id,
            contract_id = %contract_id,
            error = %e,
            "rollback_execution: rollback adapter failed; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e));
    }

    // Update contract state to RolledBack.
    let mut updated_contract = contract.clone();
    updated_contract.state = RollbackState::RolledBack;
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        tracing::error!(
            contract_id = %contract_id,
            error = %e,
            "rollback_execution: failed to persist rolled-back contract state; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    // Update execution state to RolledBack.
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::RolledBack;
    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::error!(
            execution_id = %execution_id,
            error = %e,
            "rollback_execution: failed to persist rolled-back execution state; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    let now = Utc::now();
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectRolledBack,
        now,
        Some(existing.intent_id),
        Some(existing.proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist rollback provenance event: {}", e);
    }

    Ok(Json(RollbackExecutionResponse {
        execution_id,
        rolled_back: true,
        contract_id: Some(contract_id),
        warnings: Vec::new(),
    }))
}

async fn compensate_execution(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(execution_id): Path<String>,
) -> Result<Json<CompensateExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id)?;

    // Load the execution record. Fail-closed if not found.
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

    // Fail-closed if no rollback contract is associated with this execution.
    let contract_id = existing.rollback_contract_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::RollbackUnsupported,
            "execution has no associated rollback contract",
        )
    })?;

    // Load the rollback contract. Fail-closed if not found.
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

    // Invoke the compensate adapter. Fail-closed on error.
    if let Err(e) = runtime.rollback.compensate(&contract).await {
        tracing::error!(
            execution_id = %execution_id,
            contract_id = %contract_id,
            error = %e,
            "compensate_execution: compensate adapter failed; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e));
    }

    // Update contract state to Compensated.
    let mut updated_contract = contract.clone();
    updated_contract.state = RollbackState::Compensated;
    if let Err(e) = runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        tracing::error!(
            contract_id = %contract_id,
            error = %e,
            "compensate_execution: failed to persist compensated contract state; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    // Update execution state to Compensated.
    let mut updated_execution = existing.clone();
    updated_execution.state = ExecutionState::Compensated;
    if let Err(e) = runtime.store.executions().update(&updated_execution).await {
        tracing::error!(
            execution_id = %execution_id,
            error = %e,
            "compensate_execution: failed to persist compensated execution state; rejecting (fail-closed)"
        );
        return Err(ApiProblem::internal(e.into()));
    }

    let now = Utc::now();
    let event = create_provenance_event(
        ProvenanceEventKind::SideEffectCompensated,
        now,
        Some(existing.intent_id),
        Some(existing.proposal_id),
        Some(execution_id),
        None,
        Some(contract_id),
        None,
    );
    if let Err(e) = runtime.store.provenance().append_event(&event).await {
        tracing::warn!("failed to persist compensate provenance event: {}", e);
    }

    Ok(Json(CompensateExecutionResponse {
        execution_id,
        compensated: true,
        contract_id: Some(contract_id),
        warnings: Vec::new(),
    }))
}

async fn list_approvals(
    State(runtime): State<Arc<GatewayRuntime>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListApprovalsResponse>, ApiProblem> {
    match params.validate()? {
        PaginationOutcome::Cursor {
            limit,
            proposal_id,
            execution_id,
            cursor,
        } => {
            // Cursor-based pagination path.
            let (items, next_cursor) = match (proposal_id, execution_id) {
                (Some(pid), Some(eid)) => {
                    // Both filters: use AND semantics
                    runtime
                        .store
                        .approvals()
                        .list_pending_by_proposal_and_execution_id_cursor(pid, eid, limit, Some(&cursor))
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, proposal_id = %pid, execution_id = %eid, cursor = %cursor, "list_approvals: list_pending_by_proposal_and_execution_id_cursor failed; rejecting (fail-closed)");
                            let msg = err.to_string();
                            if msg.contains("cursor") {
                                ApiProblem::new(
                                    StatusCode::BAD_REQUEST,
                                    ApiErrorCode::ValidationError,
                                    msg,
                                )
                            } else {
                                ApiProblem::internal(err.into())
                            }
                        })?
                }
                (Some(pid), None) => {
                    runtime
                        .store
                        .approvals()
                        .list_pending_by_proposal_cursor(pid, limit, Some(&cursor))
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, proposal_id = %pid, cursor = %cursor, "list_approvals: list_pending_by_proposal_cursor failed; rejecting (fail-closed)");
                            let msg = err.to_string();
                            if msg.contains("cursor") {
                                ApiProblem::new(
                                    StatusCode::BAD_REQUEST,
                                    ApiErrorCode::ValidationError,
                                    msg,
                                )
                            } else {
                                ApiProblem::internal(err.into())
                            }
                        })?
                }
                (None, Some(eid)) => {
                    runtime
                        .store
                        .approvals()
                        .list_pending_by_execution_id_cursor(eid, limit, Some(&cursor))
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, execution_id = %eid, cursor = %cursor, "list_approvals: list_pending_by_execution_id_cursor failed; rejecting (fail-closed)");
                            let msg = err.to_string();
                            if msg.contains("cursor") {
                                ApiProblem::new(
                                    StatusCode::BAD_REQUEST,
                                    ApiErrorCode::ValidationError,
                                    msg,
                                )
                            } else {
                                ApiProblem::internal(err.into())
                            }
                        })?
                }
                (None, None) => {
                    runtime
                        .store
                        .approvals()
                        .list_pending_cursor(limit, Some(&cursor))
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, cursor = %cursor, "list_approvals: list_pending_cursor failed; rejecting (fail-closed)");
                            let msg = err.to_string();
                            if msg.contains("cursor") {
                                ApiProblem::new(
                                    StatusCode::BAD_REQUEST,
                                    ApiErrorCode::ValidationError,
                                    msg,
                                )
                            } else {
                                ApiProblem::internal(err.into())
                            }
                        })?
                }
            };
            Ok(Json(ListApprovalsResponse { items, next_cursor }))
        }
        PaginationOutcome::Offset {
            proposal_id,
            execution_id,
            limit,
            offset,
        } => {
            // Offset-based pagination path (for compatibility).
            // Returns wrapped in envelope with next_cursor = null.
            let approvals = match (proposal_id, execution_id) {
                (Some(pid), Some(eid)) => {
                    // Both filters: use AND semantics
                    runtime
                        .store
                        .approvals()
                        .list_pending_by_proposal_and_execution_id_paginated(pid, eid, limit, offset)
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, proposal_id = %pid, execution_id = %eid, "list_approvals: store list_pending_by_proposal_and_execution_id_paginated failed; rejecting (fail-closed)");
                            ApiProblem::internal(err.into())
                        })?
                }
                (Some(pid), None) => {
                    runtime
                        .store
                        .approvals()
                        .list_pending_by_proposal_paginated(pid, limit, offset)
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, proposal_id = %pid, "list_approvals: store list_pending_by_proposal_paginated failed; rejecting (fail-closed)");
                            ApiProblem::internal(err.into())
                        })?
                }
                (None, Some(eid)) => {
                    runtime
                        .store
                        .approvals()
                        .list_pending_by_execution_id_paginated(eid, limit, offset)
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, execution_id = %eid, "list_approvals: store list_pending_by_execution_id_paginated failed; rejecting (fail-closed)");
                            ApiProblem::internal(err.into())
                        })?
                }
                (None, None) => {
                    runtime
                        .store
                        .approvals()
                        .list_pending_paginated(limit, offset)
                        .await
                        .map_err(|err| {
                            tracing::error!(error = %err, "list_approvals: store list_pending_paginated failed; rejecting (fail-closed)");
                            ApiProblem::internal(err.into())
                        })?
                }
            };
            Ok(Json(ListApprovalsResponse {
                items: approvals,
                next_cursor: None,
            }))
        }
    }
}

async fn get_approval(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(approval_id): Path<String>,
) -> Result<Json<ApprovalRequest>, ApiProblem> {
    let id = parse_approval_id(&approval_id)?;
    let approval = runtime
        .store
        .approvals()
        .get(id)
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

async fn resolve_approval(
    State(runtime): State<Arc<GatewayRuntime>>,
    Path(approval_id): Path<String>,
    Json(request): Json<ApprovalResolveRequest>,
) -> Result<Json<ApprovalRequest>, ApiProblem> {
    let id = parse_approval_id(&approval_id)?;

    // Load the approval record. Fail-closed if not found.
    let mut approval = runtime
        .store
        .approvals()
        .get(id)
        .await
        .map_err(|err| ApiProblem::internal(err.into()))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                "approval not found",
            )
        })?;

    // Reject resolution if the approval is no longer pending.
    if !matches!(approval.state, ApprovalState::Pending) {
        return Err(ApiProblem::new(
            StatusCode::CONFLICT,
            ApiErrorCode::Conflict,
            format!("approval is already {:?}; cannot resolve", approval.state),
        ));
    }

    let execution_id = approval.execution_id.ok_or_else(|| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "approval has no linked execution",
        )
    })?;

    if request.approve {
        // Grant approval: load and update the linked execution.
        // Fail-closed if the execution is not found or not in AwaitingApproval state.
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
                    "linked execution not found",
                )
            })?;

        if !matches!(existing.state, ExecutionState::AwaitingApproval) {
            return Err(ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "execution is {:?}; expected AwaitingApproval",
                    existing.state
                ),
            ));
        }

        // Authorization check: verify the actor is in approver_roles if approval_binding is set.
        // Fail-closed: if a binding exists with non-empty roles, require the actor to be authorized.
        {
            let lease = runtime
                .cap
                .get(existing.capability_id)
                .await
                .map_err(ApiProblem::from_capability)?;

            if let Some(ref binding) = lease.approval_binding {
                if !binding.approver_roles.is_empty()
                    && !binding.approver_roles.contains(&request.actor.actor_id)
                {
                    tracing::warn!(
                        approval_id = %id,
                        actor_id = %request.actor.actor_id,
                        approver_roles = ?binding.approver_roles,
                        "resolve_approval: actor not in approver_roles; rejecting (fail-closed)"
                    );
                    return Err(ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::PolicyDenied,
                        format!(
                            "actor {} is not authorized to resolve this approval; required roles: {:?}",
                            request.actor.actor_id, binding.approver_roles
                        ),
                    ));
                }
                // Digest enforcement: if approved_action_digest is set, require it to match.
                if !binding.approved_action_digest.is_empty()
                    && binding.approved_action_digest != approval.action_digest
                {
                    tracing::warn!(
                        approval_id = %id,
                        approved_action_digest = %binding.approved_action_digest,
                        approval_action_digest = %approval.action_digest,
                        "resolve_approval: approved_action_digest mismatch; rejecting (fail-closed)"
                    );
                    return Err(ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::PolicyDenied,
                        format!(
                            "approved_action_digest mismatch: expected {}, got {}",
                            binding.approved_action_digest, approval.action_digest
                        ),
                    ));
                }
                // Approval ID enforcement: if approval_binding is set, require approval_id match.
                if binding.approval_id != approval.approval_id {
                    tracing::warn!(
                        approval_id = %id,
                        binding_approval_id = %binding.approval_id,
                        approval_approval_id = %approval.approval_id,
                        "resolve_approval: approval_id mismatch with binding; rejecting (fail-closed)"
                    );
                    return Err(ApiProblem::new(
                        StatusCode::FORBIDDEN,
                        ApiErrorCode::PolicyDenied,
                        format!(
                            "approval_id mismatch: binding specifies {}, approval has {}",
                            binding.approval_id, approval.approval_id
                        ),
                    ));
                }
            }
        }

        // Consume the capability now that approval has been granted.
        let lease_used = runtime
            .cap
            .mark_used(existing.capability_id)
            .await
            .map_err(ApiProblem::from_capability)?;

        // Persist the used lease.
        if let Err(e) = runtime.store.capabilities().update(&lease_used).await {
            tracing::error!(
                capability_id = %lease_used.capability_id,
                error = %e,
                "resolve_approval: failed to persist used capability; rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }

        let now = Utc::now();

        // Transition execution to Prepared state.
        let mut updated_execution = existing;
        updated_execution.state = ExecutionState::Prepared;
        updated_execution.finished_at = Some(now);
        if let Err(e) = runtime.store.executions().update(&updated_execution).await {
            tracing::error!(
                execution_id = %execution_id,
                error = %e,
                "resolve_approval: failed to update execution to Prepared; rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }

        // Mark approval as Granted.
        approval.state = ApprovalState::Granted;
        if let Err(e) = runtime.store.approvals().update(&approval).await {
            tracing::error!(
                approval_id = %approval.approval_id,
                error = %e,
                "resolve_approval: failed to update approval to Granted; rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }

        let event = create_provenance_event(
            ProvenanceEventKind::ApprovalGranted,
            now,
            Some(updated_execution.intent_id),
            Some(updated_execution.proposal_id),
            Some(execution_id),
            Some(updated_execution.capability_id),
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist approval granted provenance event: {}", e);
        }
    } else {
        // Deny approval: mark the approval as Denied and keep execution in AwaitingApproval.
        let now = Utc::now();
        approval.state = ApprovalState::Denied;
        if let Err(e) = runtime.store.approvals().update(&approval).await {
            tracing::error!(
                approval_id = %approval.approval_id,
                error = %e,
                "resolve_approval: failed to update approval to Denied; rejecting (fail-closed)"
            );
            return Err(ApiProblem::internal(e.into()));
        }

        let event = create_provenance_event(
            ProvenanceEventKind::ApprovalDenied,
            now,
            None,
            None,
            Some(execution_id),
            None,
            None,
            None,
        );
        if let Err(e) = runtime.store.provenance().append_event(&event).await {
            tracing::warn!("failed to persist approval denied provenance event: {}", e);
        }
    }

    Ok(Json(approval))
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

fn parse_proposal_id(value: &str) -> Result<ProposalId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "proposal_id is not a valid uuid",
        )
    })?;
    Ok(ProposalId(parsed))
}

/// Pagination parameters for list endpoints.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[allow(dead_code)]
struct PaginationParams {
    limit: Option<u32>,
    offset: Option<u32>,
    proposal_id: Option<String>,
    /// Cursor for keyset pagination. When present, uses cursor-based pagination
    /// and ignores offset. When absent, uses offset-based pagination.
    cursor: Option<String>,
    /// Optional execution_id filter. When provided, returns only pending approvals
    /// linked to this execution.
    execution_id: Option<String>,
}

impl PaginationParams {
    const DEFAULT_LIMIT: u32 = 50;
    const MAX_LIMIT: u32 = 100;

    /// Validates and returns (limit, offset, proposal_id, execution_id, cursor).
    /// - If cursor is present: uses cursor pagination path, ignores offset.
    /// - If cursor is absent: uses offset-based pagination for compatibility.
    /// Fails closed on invalid (non-positive) params.
    /// Clamps limit to MAX_LIMIT (conservative behavior).
    fn validate(self) -> Result<PaginationOutcome, ApiProblem> {
        let limit = self.limit.unwrap_or(Self::DEFAULT_LIMIT);

        // Reject zero limit (non-positive is invalid).
        if limit == 0 {
            return Err(ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "limit must be a positive integer (1-100)",
            ));
        }
        // Negative values cannot reach here because Option<u32> rejects them at parse time.
        // Clamp limit to MAX_LIMIT (conservative — silently caps rather than rejecting).
        let limit = limit.min(Self::MAX_LIMIT);

        let proposal_id = match self.proposal_id {
            Some(pid) => Some(parse_proposal_id(&pid)?),
            None => None,
        };

        let execution_id = match self.execution_id {
            Some(eid) => Some(parse_execution_id(&eid)?),
            None => None,
        };

        // If cursor is present, use cursor pagination path (ignores offset).
        // Empty cursor means "first page request" - pass None to store for first page.
        if let Some(ref cursor) = self.cursor {
            if cursor.is_empty() {
                // Empty cursor = first page request in cursor mode.
                return Ok(PaginationOutcome::Cursor {
                    limit,
                    proposal_id,
                    execution_id,
                    cursor: String::new(), // Will be treated as "no cursor" in store.
                });
            }
            return Ok(PaginationOutcome::Cursor {
                limit,
                proposal_id,
                execution_id,
                cursor: cursor.clone(),
            });
        }

        // No cursor: use offset-based pagination.
        let offset = self.offset.unwrap_or(0);
        Ok(PaginationOutcome::Offset {
            proposal_id,
            execution_id,
            limit,
            offset,
        })
    }
}

/// Result of pagination param validation.
enum PaginationOutcome {
    /// Cursor-based pagination path.
    Cursor {
        limit: u32,
        proposal_id: Option<ProposalId>,
        execution_id: Option<ExecutionId>,
        cursor: String,
    },
    /// Offset-based pagination path (for compatibility).
    Offset {
        proposal_id: Option<ProposalId>,
        execution_id: Option<ExecutionId>,
        limit: u32,
        offset: u32,
    },
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
        match err {
            ferrum_cap::CapabilityError::NotFound => Self::new(
                StatusCode::NOT_FOUND,
                ApiErrorCode::NotFound,
                err.to_string(),
            ),
            ferrum_cap::CapabilityError::AlreadyUsed => Self::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::CapabilityUsed,
                "capability has already been consumed",
            ),
            ferrum_cap::CapabilityError::Revoked => Self::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::CapabilityRevoked,
                err.to_string(),
            ),
            ferrum_cap::CapabilityError::Expired => Self::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::CapabilityExpired,
                err.to_string(),
            ),
            ferrum_cap::CapabilityError::TtlTooLong => Self::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                err.to_string(),
            ),
            ferrum_cap::CapabilityError::Internal => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiErrorCode::Internal,
                err.to_string(),
            ),
        }
    }

    /// Creates an authentication/authorization error response.
    fn auth_error(message: impl Into<String>) -> Response {
        let problem = Self::new(
            StatusCode::UNAUTHORIZED,
            ApiErrorCode::PolicyDenied,
            message,
        );
        (StatusCode::UNAUTHORIZED, Json(problem.0)).into_response()
    }
}

/// Determine the appropriate adapter key based on resource bindings.
/// Returns "fs" for filesystem bindings, "git" for mutating git bindings,
/// "sqlite" for mutating sqlite bindings, and "noop" for unknown/empty or
/// read-only bindings.
/// Fail-closed: defaults to "noop" when no specific adapter is matched.
fn determine_adapter_key_from_bindings(bindings: &[ResourceBinding]) -> String {
    if bindings.is_empty() {
        return "noop".to_string();
    }

    // Check for filesystem bindings first
    let has_fs_binding = bindings
        .iter()
        .any(|b| matches!(b, ResourceBinding::File { .. }));

    if has_fs_binding {
        return "fs".to_string();
    }

    // Only route git to the adapter for mutation-capable bindings.
    let has_mutating_git_binding = bindings.iter().any(|binding| {
        matches!(
            binding,
            ResourceBinding::Git {
                mode: ResourceMode::ReadWrite,
                ..
            } | ResourceBinding::Git {
                mode: ResourceMode::Write,
                ..
            } | ResourceBinding::Git {
                mode: ResourceMode::Admin,
                ..
            }
        )
    });

    if has_mutating_git_binding {
        return "git".to_string();
    }

    // Only route sqlite to the adapter for mutation-capable bindings.
    let has_mutating_sqlite_binding = bindings.iter().any(|binding| {
        matches!(
            binding,
            ResourceBinding::Sqlite {
                mode: ResourceMode::ReadWrite,
                ..
            } | ResourceBinding::Sqlite {
                mode: ResourceMode::Write,
                ..
            } | ResourceBinding::Sqlite {
                mode: ResourceMode::Admin,
                ..
            }
        )
    });

    if has_mutating_sqlite_binding {
        return "sqlite".to_string();
    }

    // Route only draft-only EmailDraft bindings (allow_send=false) to maildraft.
    // Send-capable bindings (allow_send=true) are denied earlier in prepare_execution,
    // so they should never reach adapter routing.
    let has_draft_only_email_binding = bindings.iter().any(|b| {
        matches!(
            b,
            ResourceBinding::EmailDraft {
                allow_send: false,
                ..
            }
        )
    });

    if has_draft_only_email_binding {
        return "maildraft".to_string();
    }

    // Route HTTP bindings with mutation-capable modes to the HTTP adapter.
    // Read-only HTTP bindings (mode=Read) stay on noop to preserve existing
    // read-only HTTP enforcement tests - those test firewall enforcement at
    // execute-time, not adapter routing.
    let has_mutating_http_binding = bindings.iter().any(|binding| {
        matches!(
            binding,
            ResourceBinding::Http {
                mode: ResourceMode::Write,
                ..
            } | ResourceBinding::Http {
                mode: ResourceMode::ReadWrite,
                ..
            } | ResourceBinding::Http {
                mode: ResourceMode::Admin,
                ..
            }
        )
    });

    if has_mutating_http_binding {
        return "http".to_string();
    }

    // Default to noop for other binding types (fail-closed)
    "noop".to_string()
}

/// Determine the rollback target based on resource bindings.
/// For filesystem bindings, returns a FilePath target with the first file binding path.
/// For mutating git bindings, returns a GitRef target with the repo path.
/// For mutating sqlite bindings, returns a SqliteTxn target with the db path.
fn determine_rollback_target_from_bindings(bindings: &[ResourceBinding]) -> RollbackTarget {
    for binding in bindings {
        match binding {
            ResourceBinding::File { path, .. } => {
                return RollbackTarget::FilePath {
                    path: path.clone(),
                    before_hash: None,
                    after_hash: None,
                };
            }
            ResourceBinding::Sqlite {
                db_path,
                mode: ResourceMode::ReadWrite,
                ..
            }
            | ResourceBinding::Sqlite {
                db_path,
                mode: ResourceMode::Write,
                ..
            }
            | ResourceBinding::Sqlite {
                db_path,
                mode: ResourceMode::Admin,
                ..
            } => {
                // Generate a transaction ID for tracking this execution
                let tx_id = format!("tx-{}", uuid::Uuid::new_v4());
                return RollbackTarget::SqliteTxn {
                    db_path: db_path.clone(),
                    tx_id,
                };
            }
            ResourceBinding::Git {
                repo_path,
                mode: ResourceMode::ReadWrite,
                ..
            }
            | ResourceBinding::Git {
                repo_path,
                mode: ResourceMode::Write,
                ..
            }
            | ResourceBinding::Git {
                repo_path,
                mode: ResourceMode::Admin,
                ..
            } => {
                return RollbackTarget::GitRef {
                    repo_path: repo_path.clone(),
                    before_ref: None,
                    after_ref: None,
                };
            }
            ResourceBinding::EmailDraft { recipients, .. } => {
                return RollbackTarget::EmailDraft {
                    draft_id: None,
                    recipients: recipients.clone(),
                };
            }
            ResourceBinding::Http {
                method,
                base_url,
                path_prefix,
                ..
            } => {
                // Only route mutation-capable HTTP bindings to HTTP adapter.
                // Read-only HTTP bindings (mode=Read) are handled by noop,
                // so they should never reach this path.
                use sha2::{Digest, Sha256};
                let url = format!("{}{}", base_url, path_prefix);
                let mut hasher = Sha256::new();
                hasher.update(format!("{:?}:{}", method, url).as_bytes());
                let request_digest = format!("{:x}", hasher.finalize());
                return RollbackTarget::HttpRequest {
                    method: method.clone(),
                    url,
                    request_digest,
                };
            }
            _ => continue,
        }
    }

    // Default to generic target for non-specific bindings
    RollbackTarget::Generic {
        namespace: "mcp".to_string(),
        identifier: "tool-call".to_string(),
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        (self.1, Json(self.0)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_authenticated_router, determine_adapter_key_from_bindings,
        determine_rollback_target_from_bindings, infer_rollback_class,
    };
    use axum::{
        body::{self, Body},
        http::{Request, StatusCode, header::AUTHORIZATION},
    };
    use ferrum_cap::{CapabilityService, InMemoryCapabilityService};
    use ferrum_firewall::{DefaultFirewall, SemanticFirewall};
    use ferrum_pdp::StaticPdpEngine;
    use ferrum_proto::{
        ApiError, ApiErrorCode, HttpMethod, ResourceBinding, ResourceMode, ResourceSelector,
        RollbackClass, RollbackTarget,
    };
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::SqliteStore;
    use std::sync::Arc;
    use tower::util::ServiceExt;

    use crate::{AuthMode, GatewayRuntime, ServerConfig};

    async fn create_test_runtime() -> GatewayRuntime {
        let pdp: Arc<dyn ferrum_pdp::PdpEngine> = Arc::new(StaticPdpEngine);
        let cap: Arc<dyn CapabilityService> = Arc::new(InMemoryCapabilityService::default());
        let firewall: Arc<dyn SemanticFirewall> = Arc::new(DefaultFirewall::new());

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

        let store = Arc::new(
            SqliteStore::connect("sqlite::memory:?cache=shared")
                .await
                .unwrap(),
        );
        store.apply_embedded_migrations().await.unwrap();

        GatewayRuntime::new(pdp, cap, rollback, store, firewall)
    }

    #[test]
    fn routes_mutating_git_bindings_to_git_adapter() {
        let bindings = vec![ResourceBinding::Git {
            repo_path: "/tmp/repo".to_string(),
            allowed_refs: vec!["refs/heads/main".to_string()],
            mode: ResourceMode::Write,
        }];

        assert_eq!(determine_adapter_key_from_bindings(&bindings), "git");
    }

    #[test]
    fn keeps_read_only_git_bindings_on_noop_adapter() {
        let bindings = vec![ResourceBinding::Git {
            repo_path: "/tmp/repo".to_string(),
            allowed_refs: vec!["refs/heads/main".to_string()],
            mode: ResourceMode::Read,
        }];

        assert_eq!(determine_adapter_key_from_bindings(&bindings), "noop");
    }

    #[test]
    fn produces_git_ref_target_for_mutating_git_bindings() {
        let bindings = vec![ResourceBinding::Git {
            repo_path: "/tmp/repo".to_string(),
            allowed_refs: vec!["refs/heads/main".to_string()],
            mode: ResourceMode::ReadWrite,
        }];

        match determine_rollback_target_from_bindings(&bindings) {
            RollbackTarget::GitRef {
                repo_path,
                before_ref,
                after_ref,
            } => {
                assert_eq!(repo_path, "/tmp/repo");
                assert_eq!(before_ref, None);
                assert_eq!(after_ref, None);
            }
            other => panic!("expected GitRef target, got {:?}", other),
        }
    }

    #[test]
    fn keeps_read_only_git_bindings_on_generic_target() {
        let bindings = vec![ResourceBinding::Git {
            repo_path: "/tmp/repo".to_string(),
            allowed_refs: vec!["refs/heads/main".to_string()],
            mode: ResourceMode::Read,
        }];

        match determine_rollback_target_from_bindings(&bindings) {
            RollbackTarget::Generic {
                namespace,
                identifier,
            } => {
                assert_eq!(namespace, "mcp");
                assert_eq!(identifier, "tool-call");
            }
            other => panic!("expected Generic target, got {:?}", other),
        }
    }

    #[test]
    fn infers_r3_for_http_post() {
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: HttpMethod::Post,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            mode: ResourceMode::Write,
        }];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn infers_r3_for_http_put() {
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: HttpMethod::Put,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            mode: ResourceMode::Write,
        }];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn infers_r3_for_http_patch() {
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: HttpMethod::Patch,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            mode: ResourceMode::Write,
        }];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn infers_r3_for_http_delete() {
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: HttpMethod::Delete,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            mode: ResourceMode::Write,
        }];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn infers_r0_for_http_get() {
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: HttpMethod::Get,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/".to_string(),
            mode: ResourceMode::Read,
        }];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R0NativeReversible
        );
    }

    #[test]
    fn infers_r2_for_email_draft() {
        let scope = vec![ResourceSelector::EmailDraft {
            recipient_allowlist: vec!["@example.com".to_string()],
            subject_prefix_allowlist: vec!["[Test]".to_string()],
            mode: ResourceMode::Write,
        }];
        assert_eq!(infer_rollback_class(&scope), RollbackClass::R2Compensatable);
    }

    #[test]
    fn infers_r0_for_empty_scope() {
        let scope: Vec<ResourceSelector> = vec![];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R0NativeReversible
        );
    }

    #[test]
    fn http_mutating_takes_precedence_over_email() {
        // R3 should take precedence over R2 when both are present.
        let scope = vec![
            ResourceSelector::HttpEndpoint {
                method: HttpMethod::Post,
                base_url: "https://api.example.com".to_string(),
                path_prefix: "/v1/".to_string(),
                mode: ResourceMode::Write,
            },
            ResourceSelector::EmailDraft {
                recipient_allowlist: vec!["@example.com".to_string()],
                subject_prefix_allowlist: vec!["[Test]".to_string()],
                mode: ResourceMode::Write,
            },
        ];
        assert_eq!(
            infer_rollback_class(&scope),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn rollback_class_floor_uses_default_when_higher_than_requested() {
        // Intent default R2, client requests R0 -> floor should be R2
        assert_eq!(
            super::rollback_class_floor(
                RollbackClass::R2Compensatable,
                RollbackClass::R0NativeReversible
            ),
            RollbackClass::R2Compensatable
        );
    }

    #[test]
    fn rollback_class_floor_uses_requested_when_higher_than_default() {
        // Intent default R1, client requests R2 -> floor should be R2
        assert_eq!(
            super::rollback_class_floor(
                RollbackClass::R1SnapshotRecoverable,
                RollbackClass::R2Compensatable
            ),
            RollbackClass::R2Compensatable
        );
    }

    #[test]
    fn rollback_class_floor_uses_default_when_equal_to_requested() {
        // Intent default R3, client requests R3 -> floor should be R3
        assert_eq!(
            super::rollback_class_floor(
                RollbackClass::R3IrreversibleHighConsequence,
                RollbackClass::R3IrreversibleHighConsequence
            ),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn rollback_class_floor_r3_is_highest() {
        // R3 should always be returned if either default or requested is R3
        assert_eq!(
            super::rollback_class_floor(
                RollbackClass::R3IrreversibleHighConsequence,
                RollbackClass::R0NativeReversible
            ),
            RollbackClass::R3IrreversibleHighConsequence
        );
        assert_eq!(
            super::rollback_class_floor(
                RollbackClass::R0NativeReversible,
                RollbackClass::R3IrreversibleHighConsequence
            ),
            RollbackClass::R3IrreversibleHighConsequence
        );
    }

    #[test]
    fn rollback_class_floor_preserves_r0_for_read_only() {
        // R0 should be preserved when both are R0
        assert_eq!(
            super::rollback_class_floor(
                RollbackClass::R0NativeReversible,
                RollbackClass::R0NativeReversible
            ),
            RollbackClass::R0NativeReversible
        );
    }

    #[tokio::test]
    async fn authenticated_router_allows_health_without_bearer_token() {
        let app = build_authenticated_router(
            create_test_runtime().await,
            ServerConfig {
                auth_mode: AuthMode::Bearer,
                bearer_token: Some("test-token".to_string()),
            },
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn authenticated_router_rejects_missing_bearer_token() {
        let app = build_authenticated_router(
            create_test_runtime().await,
            ServerConfig {
                auth_mode: AuthMode::Bearer,
                bearer_token: Some("test-token".to_string()),
            },
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ApiError = serde_json::from_slice(&body).unwrap();
        assert!(matches!(error.code, ApiErrorCode::PolicyDenied));
    }

    #[tokio::test]
    async fn authenticated_router_allows_valid_bearer_token() {
        let app = build_authenticated_router(
            create_test_runtime().await,
            ServerConfig {
                auth_mode: AuthMode::Bearer,
                bearer_token: Some("test-token".to_string()),
            },
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header(AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
