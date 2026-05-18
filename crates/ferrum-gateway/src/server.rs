use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use ferrum_cap::{CapabilityError, CapabilityService, InMemoryCapabilityService};
use ferrum_firewall::{FirewallContext, SemanticFirewall, TaintScoringFirewall};
use ferrum_graph::LineageGraph;
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActorRef, ActorType, ApiError, ApiErrorCode, ApprovalBinding, ApprovalId, ApprovalListEnvelope,
    ApprovalMode, ApprovalResolveRequest, ApprovalState, AuthorizeExecutionRequest,
    AuthorizeExecutionResponse, CapabilityId, CapabilityLease, CapabilityMintRequest,
    CapabilityMintResponse, CapabilityStatus, ComponentStatus, Decision, DeepHealthResponse,
    EvaluateOutcomeResponse, EvaluateProposalResponse, EventId, ExecutionDetailResponse,
    ExecutionId, ExecutionRecord, ExecutionState, HashChainRef, HealthResponse,
    IntentCompileRequest, IntentCompileResponse, IntentEnvelope, IntentStatus, LineageDirection,
    LineageQueryRequest, LineageQueryResponse, Matcher, ObjectRef, ObjectType, OutcomeClause,
    OutcomeReport, PolicyBundle, PolicyRule, ProposalId, ProvenanceEvent, ProvenanceEventKind,
    ProvenanceIngestRequest, ProvenanceIngestResponse, ProvenanceQueryRequest,
    ProvenanceQueryResponse, ResourceSelector, RiskTier, RollbackClass, RollbackTarget, TimeBudget,
    TrustContextSummary, TrustLabel as ProtoTrustLabel,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use ferrum_store::StoreFacade;
use ferrum_sync::{BridgeToolInfo, RuntimeBridge};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tower::ServiceBuilder;

/// Prometheus histogram bucket boundaries in seconds.
/// Includes: 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s
const HISTOGRAM_BOUNDARIES: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use tower_http::trace::TraceLayer;

use crate::{AuthMode, GatewayRuntime, ServerConfig};

/// Shared state that includes both runtime and server config for auth.
#[derive(Clone)]
struct AppState {
    runtime: GatewayRuntime,
    #[allow(dead_code)]
    server_config: ServerConfig,
    metrics: Arc<Metrics>,
}

/// Metrics state for the /v1/metrics endpoint.
/// Tracks health/metrics request counters, store health gauge, and bounded
/// governance error counters for all governance API endpoints.
struct Metrics {
    healthz_requests: AtomicU64,
    readyz_requests: AtomicU64,
    readyz_deep_requests_200: AtomicU64,
    readyz_deep_requests_503: AtomicU64,
    metrics_scrapes: AtomicU64,
    store_health_up: AtomicU64,
    // Governance error counters keyed by static route template
    governance_errors_v1_intents_compile: AtomicU64,
    governance_errors_v1_intents_list: AtomicU64,
    governance_errors_v1_proposals_evaluate: AtomicU64,
    governance_errors_v1_capabilities_mint: AtomicU64,
    governance_errors_v1_capabilities_revoke: AtomicU64,
    governance_errors_v1_executions_authorize: AtomicU64,
    governance_errors_v1_executions_prepare: AtomicU64,
    governance_errors_v1_executions_execute: AtomicU64,
    governance_errors_v1_executions_verify: AtomicU64,
    governance_errors_v1_executions_compensate: AtomicU64,
    governance_errors_v1_executions_cancel: AtomicU64,
    governance_errors_v1_executions_evaluate_outcome: AtomicU64,
    governance_errors_v1_executions_execution_id: AtomicU64,
    governance_errors_v1_approvals: AtomicU64,
    governance_errors_v1_approvals_approval_id: AtomicU64,
    governance_errors_v1_approvals_resolve: AtomicU64,
    governance_errors_v1_policy_bundles_create: AtomicU64,
    governance_errors_v1_policy_bundles_list: AtomicU64,
    governance_errors_v1_policy_bundles_get: AtomicU64,
    governance_errors_v1_policy_bundles_update: AtomicU64,
    governance_errors_v1_policy_bundles_delete: AtomicU64,
    governance_errors_v1_policy_bundles_set_active: AtomicU64,
    governance_errors_v1_provenance_query: AtomicU64,
    governance_errors_v1_provenance_lineage: AtomicU64,
    governance_errors_v1_provenance_lineage_execution_id: AtomicU64,
    governance_errors_v1_provenance_ingest: AtomicU64,
    governance_errors_v1_bridges_bridge_id_tools: AtomicU64,
    // Governance success counters keyed by static route template
    governance_success_v1_intents_compile: AtomicU64,
    governance_success_v1_intents_list: AtomicU64,
    governance_success_v1_proposals_evaluate: AtomicU64,
    governance_success_v1_capabilities_mint: AtomicU64,
    governance_success_v1_capabilities_revoke: AtomicU64,
    governance_success_v1_executions_authorize: AtomicU64,
    governance_success_v1_executions_prepare: AtomicU64,
    governance_success_v1_executions_execute: AtomicU64,
    governance_success_v1_executions_verify: AtomicU64,
    governance_success_v1_executions_compensate: AtomicU64,
    governance_success_v1_executions_cancel: AtomicU64,
    governance_success_v1_executions_evaluate_outcome: AtomicU64,
    governance_success_v1_executions_execution_id: AtomicU64,
    governance_success_v1_approvals: AtomicU64,
    governance_success_v1_approvals_approval_id: AtomicU64,
    governance_success_v1_approvals_resolve: AtomicU64,
    governance_success_v1_policy_bundles_create: AtomicU64,
    governance_success_v1_policy_bundles_list: AtomicU64,
    governance_success_v1_policy_bundles_get: AtomicU64,
    governance_success_v1_policy_bundles_update: AtomicU64,
    governance_success_v1_policy_bundles_delete: AtomicU64,
    governance_success_v1_policy_bundles_set_active: AtomicU64,
    governance_success_v1_provenance_query: AtomicU64,
    governance_success_v1_provenance_lineage: AtomicU64,
    governance_success_v1_provenance_lineage_execution_id: AtomicU64,
    governance_success_v1_provenance_ingest: AtomicU64,
    governance_success_v1_bridges_bridge_id_tools: AtomicU64,
    // Latency histogram for /v1/healthz (always status 200)
    healthz_latency_buckets: [AtomicU64; 11],
    healthz_latency_sum: AtomicU64,
    healthz_latency_count: AtomicU64,
    // Latency histogram for /v1/readyz (always status 200)
    readyz_latency_buckets: [AtomicU64; 11],
    readyz_latency_sum: AtomicU64,
    readyz_latency_count: AtomicU64,
    // Latency histogram for /v1/readyz/deep (status 200)
    readyz_deep_latency_buckets_200: [AtomicU64; 11],
    readyz_deep_latency_sum_200: AtomicU64,
    readyz_deep_latency_count_200: AtomicU64,
    // Latency histogram for /v1/readyz/deep (status 503)
    readyz_deep_latency_buckets_503: [AtomicU64; 11],
    readyz_deep_latency_sum_503: AtomicU64,
    readyz_deep_latency_count_503: AtomicU64,
    // Latency histogram for /v1/metrics (always status 200)
    metrics_latency_buckets: [AtomicU64; 11],
    metrics_latency_sum: AtomicU64,
    metrics_latency_count: AtomicU64,
}

impl Metrics {
    fn new() -> Self {
        Self {
            healthz_requests: AtomicU64::new(0),
            readyz_requests: AtomicU64::new(0),
            readyz_deep_requests_200: AtomicU64::new(0),
            readyz_deep_requests_503: AtomicU64::new(0),
            metrics_scrapes: AtomicU64::new(0),
            store_health_up: AtomicU64::new(0),
            governance_errors_v1_intents_compile: AtomicU64::new(0),
            governance_errors_v1_intents_list: AtomicU64::new(0),
            governance_errors_v1_proposals_evaluate: AtomicU64::new(0),
            governance_errors_v1_capabilities_mint: AtomicU64::new(0),
            governance_errors_v1_capabilities_revoke: AtomicU64::new(0),
            governance_errors_v1_executions_authorize: AtomicU64::new(0),
            governance_errors_v1_executions_prepare: AtomicU64::new(0),
            governance_errors_v1_executions_execute: AtomicU64::new(0),
            governance_errors_v1_executions_verify: AtomicU64::new(0),
            governance_errors_v1_executions_compensate: AtomicU64::new(0),
            governance_errors_v1_executions_cancel: AtomicU64::new(0),
            governance_errors_v1_executions_evaluate_outcome: AtomicU64::new(0),
            governance_errors_v1_executions_execution_id: AtomicU64::new(0),
            governance_errors_v1_approvals: AtomicU64::new(0),
            governance_errors_v1_approvals_approval_id: AtomicU64::new(0),
            governance_errors_v1_approvals_resolve: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_create: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_list: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_get: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_update: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_delete: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_set_active: AtomicU64::new(0),
            governance_errors_v1_provenance_query: AtomicU64::new(0),
            governance_errors_v1_provenance_lineage: AtomicU64::new(0),
            governance_errors_v1_provenance_lineage_execution_id: AtomicU64::new(0),
            governance_errors_v1_provenance_ingest: AtomicU64::new(0),
            governance_errors_v1_bridges_bridge_id_tools: AtomicU64::new(0),
            governance_success_v1_intents_compile: AtomicU64::new(0),
            governance_success_v1_intents_list: AtomicU64::new(0),
            governance_success_v1_proposals_evaluate: AtomicU64::new(0),
            governance_success_v1_capabilities_mint: AtomicU64::new(0),
            governance_success_v1_capabilities_revoke: AtomicU64::new(0),
            governance_success_v1_executions_authorize: AtomicU64::new(0),
            governance_success_v1_executions_prepare: AtomicU64::new(0),
            governance_success_v1_executions_execute: AtomicU64::new(0),
            governance_success_v1_executions_verify: AtomicU64::new(0),
            governance_success_v1_executions_compensate: AtomicU64::new(0),
            governance_success_v1_executions_cancel: AtomicU64::new(0),
            governance_success_v1_executions_evaluate_outcome: AtomicU64::new(0),
            governance_success_v1_executions_execution_id: AtomicU64::new(0),
            governance_success_v1_approvals: AtomicU64::new(0),
            governance_success_v1_approvals_approval_id: AtomicU64::new(0),
            governance_success_v1_approvals_resolve: AtomicU64::new(0),
            governance_success_v1_policy_bundles_create: AtomicU64::new(0),
            governance_success_v1_policy_bundles_list: AtomicU64::new(0),
            governance_success_v1_policy_bundles_get: AtomicU64::new(0),
            governance_success_v1_policy_bundles_update: AtomicU64::new(0),
            governance_success_v1_policy_bundles_delete: AtomicU64::new(0),
            governance_success_v1_policy_bundles_set_active: AtomicU64::new(0),
            governance_success_v1_provenance_query: AtomicU64::new(0),
            governance_success_v1_provenance_lineage: AtomicU64::new(0),
            governance_success_v1_provenance_lineage_execution_id: AtomicU64::new(0),
            governance_success_v1_provenance_ingest: AtomicU64::new(0),
            governance_success_v1_bridges_bridge_id_tools: AtomicU64::new(0),
            // Latency histogram fields
            healthz_latency_buckets: [const { AtomicU64::new(0) }; 11],
            healthz_latency_sum: AtomicU64::new(0),
            healthz_latency_count: AtomicU64::new(0),
            readyz_latency_buckets: [const { AtomicU64::new(0) }; 11],
            readyz_latency_sum: AtomicU64::new(0),
            readyz_latency_count: AtomicU64::new(0),
            readyz_deep_latency_buckets_200: [const { AtomicU64::new(0) }; 11],
            readyz_deep_latency_sum_200: AtomicU64::new(0),
            readyz_deep_latency_count_200: AtomicU64::new(0),
            readyz_deep_latency_buckets_503: [const { AtomicU64::new(0) }; 11],
            readyz_deep_latency_sum_503: AtomicU64::new(0),
            readyz_deep_latency_count_503: AtomicU64::new(0),
            metrics_latency_buckets: [const { AtomicU64::new(0) }; 11],
            metrics_latency_sum: AtomicU64::new(0),
            metrics_latency_count: AtomicU64::new(0),
        }
    }

    /// Increments the governance error counter for the given route.
    fn increment_governance_error(&self, route: GovernanceRoute) {
        match route {
            GovernanceRoute::IntentsCompile => self
                .governance_errors_v1_intents_compile
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::IntentsList => self
                .governance_errors_v1_intents_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProposalsEvaluate => self
                .governance_errors_v1_proposals_evaluate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::CapabilitiesMint => self
                .governance_errors_v1_capabilities_mint
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::CapabilitiesRevoke => self
                .governance_errors_v1_capabilities_revoke
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsAuthorize => self
                .governance_errors_v1_executions_authorize
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsPrepare => self
                .governance_errors_v1_executions_prepare
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsExecute => self
                .governance_errors_v1_executions_execute
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsVerify => self
                .governance_errors_v1_executions_verify
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsCompensate => self
                .governance_errors_v1_executions_compensate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsCancel => self
                .governance_errors_v1_executions_cancel
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsEvaluateOutcome => self
                .governance_errors_v1_executions_evaluate_outcome
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsExecutionId => self
                .governance_errors_v1_executions_execution_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::Approvals => self
                .governance_errors_v1_approvals
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ApprovalsApprovalId => self
                .governance_errors_v1_approvals_approval_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ApprovalsResolve => self
                .governance_errors_v1_approvals_resolve
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesCreate => self
                .governance_errors_v1_policy_bundles_create
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesList => self
                .governance_errors_v1_policy_bundles_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesGet => self
                .governance_errors_v1_policy_bundles_get
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesUpdate => self
                .governance_errors_v1_policy_bundles_update
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesDelete => self
                .governance_errors_v1_policy_bundles_delete
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesSetActive => self
                .governance_errors_v1_policy_bundles_set_active
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceQuery => self
                .governance_errors_v1_provenance_query
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceLineage => self
                .governance_errors_v1_provenance_lineage
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceLineageExecutionId => self
                .governance_errors_v1_provenance_lineage_execution_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceIngest => self
                .governance_errors_v1_provenance_ingest
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::BridgesBridgeIdTools => self
                .governance_errors_v1_bridges_bridge_id_tools
                .fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Increments the governance success counter for the given route.
    fn increment_governance_success(&self, route: GovernanceRoute) {
        match route {
            GovernanceRoute::IntentsCompile => self
                .governance_success_v1_intents_compile
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::IntentsList => self
                .governance_success_v1_intents_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProposalsEvaluate => self
                .governance_success_v1_proposals_evaluate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::CapabilitiesMint => self
                .governance_success_v1_capabilities_mint
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::CapabilitiesRevoke => self
                .governance_success_v1_capabilities_revoke
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsAuthorize => self
                .governance_success_v1_executions_authorize
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsPrepare => self
                .governance_success_v1_executions_prepare
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsExecute => self
                .governance_success_v1_executions_execute
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsVerify => self
                .governance_success_v1_executions_verify
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsCompensate => self
                .governance_success_v1_executions_compensate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsCancel => self
                .governance_success_v1_executions_cancel
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsEvaluateOutcome => self
                .governance_success_v1_executions_evaluate_outcome
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ExecutionsExecutionId => self
                .governance_success_v1_executions_execution_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::Approvals => self
                .governance_success_v1_approvals
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ApprovalsApprovalId => self
                .governance_success_v1_approvals_approval_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ApprovalsResolve => self
                .governance_success_v1_approvals_resolve
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesCreate => self
                .governance_success_v1_policy_bundles_create
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesList => self
                .governance_success_v1_policy_bundles_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesGet => self
                .governance_success_v1_policy_bundles_get
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesUpdate => self
                .governance_success_v1_policy_bundles_update
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesDelete => self
                .governance_success_v1_policy_bundles_delete
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesSetActive => self
                .governance_success_v1_policy_bundles_set_active
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceQuery => self
                .governance_success_v1_provenance_query
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceLineage => self
                .governance_success_v1_provenance_lineage
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceLineageExecutionId => self
                .governance_success_v1_provenance_lineage_execution_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::ProvenanceIngest => self
                .governance_success_v1_provenance_ingest
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::BridgesBridgeIdTools => self
                .governance_success_v1_bridges_bridge_id_tools
                .fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Increments the governance error counter for the given route and returns the error.
    /// Use this in `map_err` closures: `.map_err(|e| state.metrics.record_governance_error(route, e))`
    fn record_governance_error(&self, route: GovernanceRoute, err: ApiProblem) -> ApiProblem {
        self.increment_governance_error(route);
        err
    }

    /// Records a latency sample in the appropriate histogram based on route and status.
    /// `elapsed_ns` is the elapsed time in nanoseconds.
    fn record_latency(&self, route: PublicRoute, status: u16, elapsed_ns: u64) {
        let (buckets, sum, count) = match (route, status) {
            (PublicRoute::Healthz, 200) => (
                &self.healthz_latency_buckets,
                &self.healthz_latency_sum,
                &self.healthz_latency_count,
            ),
            (PublicRoute::Readyz, 200) => (
                &self.readyz_latency_buckets,
                &self.readyz_latency_sum,
                &self.readyz_latency_count,
            ),
            (PublicRoute::ReadyzDeep, 200) => (
                &self.readyz_deep_latency_buckets_200,
                &self.readyz_deep_latency_sum_200,
                &self.readyz_deep_latency_count_200,
            ),
            (PublicRoute::ReadyzDeep, 503) => (
                &self.readyz_deep_latency_buckets_503,
                &self.readyz_deep_latency_sum_503,
                &self.readyz_deep_latency_count_503,
            ),
            (PublicRoute::Metrics, 200) => (
                &self.metrics_latency_buckets,
                &self.metrics_latency_sum,
                &self.metrics_latency_count,
            ),
            // Ignore unknown combinations (shouldn't happen for public endpoints)
            _ => return,
        };

        let elapsed_s = elapsed_ns as f64 / 1e9_f64;

        // Update sum and count
        sum.fetch_add(elapsed_ns, Ordering::Relaxed);
        count.fetch_add(1, Ordering::Relaxed);

        // Update buckets - increment all buckets where elapsed >= boundary
        for (i, boundary) in HISTOGRAM_BOUNDARIES.iter().enumerate() {
            if elapsed_s >= *boundary {
                buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Static route templates for governance error counters.
/// Each variant corresponds to a route path template with {param} placeholders normalized to fixed strings.
/// Variants are split by method to avoid counter collisions for same-path-different-method routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, clippy::enum_variant_names)]
enum GovernanceRoute {
    IntentsCompile,
    IntentsList,
    ProposalsEvaluate,
    CapabilitiesMint,
    CapabilitiesRevoke,
    ExecutionsAuthorize,
    ExecutionsPrepare,
    ExecutionsExecute,
    ExecutionsVerify,
    ExecutionsCompensate,
    ExecutionsCancel,
    ExecutionsEvaluateOutcome,
    ExecutionsExecutionId,
    Approvals,
    ApprovalsApprovalId,
    ApprovalsResolve,
    PolicyBundlesCreate,
    PolicyBundlesList,
    PolicyBundlesGet,
    PolicyBundlesUpdate,
    PolicyBundlesDelete,
    PolicyBundlesSetActive,
    ProvenanceQuery,
    ProvenanceLineage,
    ProvenanceLineageExecutionId,
    ProvenanceIngest,
    BridgesBridgeIdTools,
}

impl GovernanceRoute {
    #[allow(dead_code)]
    fn path(&self) -> &'static str {
        match self {
            GovernanceRoute::IntentsCompile => "/v1/intents/compile",
            GovernanceRoute::IntentsList => "/v1/intents",
            GovernanceRoute::ProposalsEvaluate => "/v1/proposals/{proposal_id}/evaluate",
            GovernanceRoute::CapabilitiesMint => "/v1/capabilities/mint",
            GovernanceRoute::CapabilitiesRevoke => "/v1/capabilities/{capability_id}/revoke",
            GovernanceRoute::ExecutionsAuthorize => "/v1/executions/authorize",
            GovernanceRoute::ExecutionsPrepare => "/v1/executions/{execution_id}/prepare",
            GovernanceRoute::ExecutionsExecute => "/v1/executions/{execution_id}/execute",
            GovernanceRoute::ExecutionsVerify => "/v1/executions/{execution_id}/verify",
            GovernanceRoute::ExecutionsCompensate => "/v1/executions/{execution_id}/compensate",
            GovernanceRoute::ExecutionsCancel => "/v1/executions/{execution_id}/cancel",
            GovernanceRoute::ExecutionsEvaluateOutcome => {
                "/v1/executions/{execution_id}/evaluate-outcome"
            }
            GovernanceRoute::ExecutionsExecutionId => "/v1/executions/{execution_id}",
            GovernanceRoute::Approvals => "/v1/approvals",
            GovernanceRoute::ApprovalsApprovalId => "/v1/approvals/{approval_id}",
            GovernanceRoute::ApprovalsResolve => "/v1/approvals/{approval_id}/resolve",
            GovernanceRoute::PolicyBundlesCreate => "/v1/policy-bundles",
            GovernanceRoute::PolicyBundlesList => "/v1/policy-bundles",
            GovernanceRoute::PolicyBundlesGet => "/v1/policy-bundles/{bundle_id}",
            GovernanceRoute::PolicyBundlesUpdate => "/v1/policy-bundles/{bundle_id}",
            GovernanceRoute::PolicyBundlesDelete => "/v1/policy-bundles/{bundle_id}",
            GovernanceRoute::PolicyBundlesSetActive => "/v1/policy-bundles/{bundle_id}/active",
            GovernanceRoute::ProvenanceQuery => "/v1/provenance/query",
            GovernanceRoute::ProvenanceLineage => "/v1/provenance/lineage",
            GovernanceRoute::ProvenanceLineageExecutionId => {
                "/v1/provenance/lineage/{execution_id}"
            }
            GovernanceRoute::ProvenanceIngest => "/v1/provenance/ingest",
            GovernanceRoute::BridgesBridgeIdTools => "/v1/bridges/{bridge_id}/tools",
        }
    }

    /// Returns the HTTP method for this route as a static string.
    #[allow(dead_code)]
    fn method(&self) -> &'static str {
        match self {
            GovernanceRoute::IntentsCompile => "POST",
            GovernanceRoute::IntentsList => "GET",
            GovernanceRoute::ProposalsEvaluate => "POST",
            GovernanceRoute::CapabilitiesMint => "POST",
            GovernanceRoute::CapabilitiesRevoke => "POST",
            GovernanceRoute::ExecutionsAuthorize => "POST",
            GovernanceRoute::ExecutionsPrepare => "POST",
            GovernanceRoute::ExecutionsExecute => "POST",
            GovernanceRoute::ExecutionsVerify => "POST",
            GovernanceRoute::ExecutionsCompensate => "POST",
            GovernanceRoute::ExecutionsCancel => "POST",
            GovernanceRoute::ExecutionsEvaluateOutcome => "POST",
            GovernanceRoute::ExecutionsExecutionId => "GET",
            GovernanceRoute::Approvals => "GET",
            GovernanceRoute::ApprovalsApprovalId => "GET",
            GovernanceRoute::ApprovalsResolve => "POST",
            GovernanceRoute::PolicyBundlesCreate => "POST",
            GovernanceRoute::PolicyBundlesList => "GET",
            GovernanceRoute::PolicyBundlesGet => "GET",
            GovernanceRoute::PolicyBundlesUpdate => "PUT",
            GovernanceRoute::PolicyBundlesDelete => "DELETE",
            GovernanceRoute::PolicyBundlesSetActive => "PUT",
            GovernanceRoute::ProvenanceQuery => "POST",
            GovernanceRoute::ProvenanceLineage => "POST",
            GovernanceRoute::ProvenanceLineageExecutionId => "GET",
            GovernanceRoute::ProvenanceIngest => "POST",
            GovernanceRoute::BridgesBridgeIdTools => "GET",
        }
    }
}

/// Public endpoint routes that have latency histograms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublicRoute {
    Healthz,
    Readyz,
    ReadyzDeep,
    Metrics,
}

/// Macro to increment governance error counter and return an ApiProblem error.
/// Usage (governance route + ApiProblem, increments counter):
///   `governance_err!(state, GovernanceRoute::IntentsCompile, ApiProblem::new(...))`
/// Usage (error code + message, no counter increment, status defaults to BAD_REQUEST):
///   `governance_err!(ApiErrorCode::NotFound, "resource not found")`
///   (use in `ok_or_else(|| governance_err!(...))` or `return Err(governance_err!(...))`)
macro_rules! governance_err {
    ($state:expr, $route:expr, $err:expr) => {{
        $state.metrics.increment_governance_error($route);
        Err($err)
    }};
    ($code:expr, $msg:expr) => {{ ApiProblem::new(StatusCode::BAD_REQUEST, $code, $msg) }};
}

/// Macro to increment governance success counter and return an Ok value.
/// Usage: `governance_ok!(state, GovernanceRoute::IntentsCompile, Ok(Json(response)))`
macro_rules! governance_ok {
    ($state:expr, $route:expr, $ok:expr) => {{
        $state.metrics.increment_governance_success($route);
        $ok
    }};
}

// ---------------------------------------------------------------------------
// I11 Output Sanitization helpers
// ---------------------------------------------------------------------------

/// Sanitizes a serde_json::Value by stripping control characters from all string values.
/// Preserves JSON structure (keys, numeric values, bools, nulls unchanged).
fn sanitize_json(fw: &TaintScoringFirewall, value: serde_json::Value) -> serde_json::Value {
    fw.sanitize_output(value)
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM on unix).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install SIGINT handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections...");
}

pub async fn run_http_server(config: ServerConfig, runtime: GatewayRuntime) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        runtime,
        server_config: config.clone(),
        metrics: Arc::new(Metrics::new()),
    });

    let monitoring_router = build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state);

    // Rate limiting: configurable per IP via config
    // P1: Use SmartIpKeyExtractor to align production with test helper.
    // This supports x-real-ip / x-forwarded-for headers so workload
    // generators can distribute traffic across distinct buckets.
    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(SmartIpKeyExtractor)
        .per_second(config.rate_limit_per_second)
        .burst_size(config.rate_limit_burst)
        .finish()
        .unwrap();

    // Spawn periodic cleanup of rate limiter entries
    let limiter = governor_conf.limiter().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            limiter.retain_recent();
        }
    });

    let workload_router = workload_router.layer(GovernorLayer::new(governor_conf));

    let mut app = monitoring_router.merge(workload_router);

    // Add auth layer if auth mode is Bearer
    if config.auth_mode == AuthMode::Bearer {
        let auth_layer = ServiceBuilder::new()
            .layer(axum::middleware::from_fn_with_state(
                config.clone(),
                bearer_auth_middleware,
            ))
            .into_inner();
        app = app.layer(auth_layer);
    }

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("ferrumd listening on {}", config.bind_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// Build a router without auth middleware for tests/backward compatibility.
pub fn build_router(runtime: GatewayRuntime) -> Router {
    let state = Arc::new(AppState {
        runtime,
        server_config: ServerConfig::default(),
        metrics: Arc::new(Metrics::new()),
    });
    let monitoring_router = build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state);
    monitoring_router.merge(workload_router)
}

/// Build a router with auth middleware using the given server config.
pub fn build_router_with_auth(runtime: GatewayRuntime, server_config: ServerConfig) -> Router {
    let state = Arc::new(AppState {
        runtime,
        server_config: server_config.clone(),
        metrics: Arc::new(Metrics::new()),
    });
    let monitoring_router = build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state);
    let mut router = monitoring_router.merge(workload_router);

    // Add auth layer if auth mode is Bearer
    if server_config.auth_mode == AuthMode::Bearer {
        let auth_layer = ServiceBuilder::new()
            .layer(axum::middleware::from_fn_with_state(
                server_config.clone(),
                bearer_auth_middleware,
            ))
            .into_inner();
        router = router.layer(auth_layer);
    }

    router
}

/// Build a router with rate limiting enabled using a custom GovernorConfig.
/// This is a test-only helper that allows configuring rate limits for integration tests.
/// For production, rate limiting is applied in `run_http_server` with 2 req/s and burst 50.
///
/// Uses SmartIpKeyExtractor which supports x-real-ip header for client IP identification,
/// allowing tests to set the IP via header without needing MockConnectInfo.
///
/// Monitoring endpoints (`/v1/metrics`, `/v1/readyz`, `/v1/readyz/deep`) are exempt
/// from rate limiting to match production behavior.
pub fn build_router_with_governor(
    runtime: GatewayRuntime,
    per_second: u64,
    burst_size: u32,
) -> Router {
    // Use SmartIpKeyExtractor to support x-real-ip header
    let governor_conf = GovernorConfigBuilder::default()
        .key_extractor(SmartIpKeyExtractor)
        .per_second(per_second)
        .burst_size(burst_size)
        .finish()
        .unwrap();

    let state = Arc::new(AppState {
        runtime,
        server_config: ServerConfig::default(),
        metrics: Arc::new(Metrics::new()),
    });

    let monitoring_router = build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state).layer(GovernorLayer::new(governor_conf));
    monitoring_router.merge(workload_router)
}

fn build_monitoring_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health endpoints - always unauthenticated
        .route("/v1/healthz", get(healthz))
        .route("/v1/readyz", get(readyz))
        .route("/v1/readyz/deep", get(readyz_deep))
        // Metrics endpoint - always unauthenticated
        .route("/v1/metrics", get(metrics_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

fn build_workload_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Provenance query endpoint
        .route("/v1/provenance/query", post(query_provenance))
        // Execution lineage endpoint
        .route(
            "/v1/provenance/lineage/{execution_id}",
            get(get_execution_lineage),
        )
        // Multi-hop lineage query endpoint
        .route("/v1/provenance/lineage", post(query_lineage))
        // Provenance ingest endpoint
        .route("/v1/provenance/ingest", post(ingest_provenance))
        // Bridge endpoints
        .route("/v1/bridges", get(list_bridges))
        .route("/v1/bridges/{bridge_id}/tools", get(list_bridge_tools))
        // Execution inspection endpoint
        .route("/v1/executions/{execution_id}", get(get_execution))
        // Approvals endpoints
        .route("/v1/approvals", get(list_approvals))
        .route("/v1/approvals/{approval_id}", get(get_approval))
        .route(
            "/v1/approvals/{approval_id}/resolve",
            post(resolve_approval),
        )
        // Policy/evaluation endpoints
        .route("/v1/intents/compile", post(compile_intent))
        .route("/v1/intents", get(list_intents))
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
            "/v1/executions/{execution_id}/compensate",
            post(compensate_execution),
        )
        .route(
            "/v1/executions/{execution_id}/cancel",
            post(cancel_execution),
        )
        .route(
            "/v1/executions/{execution_id}/evaluate-outcome",
            post(evaluate_outcome),
        )
        // Policy bundle endpoints
        .route("/v1/policy-bundles", post(create_policy_bundle))
        .route("/v1/policy-bundles", get(list_policy_bundles))
        .route("/v1/policy-bundles/{bundle_id}", get(get_policy_bundle))
        .route("/v1/policy-bundles/{bundle_id}", put(update_policy_bundle))
        .route(
            "/v1/policy-bundles/{bundle_id}",
            delete(delete_policy_bundle),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}/active",
            put(set_policy_bundle_active),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

/// Middleware to enforce bearer token authentication.
async fn bearer_auth_middleware(
    State(config): State<ServerConfig>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    // Skip auth for health and metrics endpoints
    let path = request.uri().path();
    if path == "/v1/healthz"
        || path == "/v1/readyz"
        || path == "/v1/readyz/deep"
        || path == "/v1/metrics"
    {
        return next.run(request).await;
    }

    // Check for Authorization header
    let auth_header = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = config.bearer_token.as_deref().unwrap_or("");

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let provided = &header[7..];
            if constant_time_eq::constant_time_eq(provided.as_bytes(), token.as_bytes()) {
                return next.run(request).await;
            }
        }
        _ => {}
    }

    // Return 401 Unauthorized
    let error = ApiError {
        code: ApiErrorCode::Unauthorized,
        message: "missing or invalid bearer token".to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        retriable: false,
        details: serde_json::json!({}),
    };
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

async fn healthz(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let start = Instant::now();
    state
        .metrics
        .healthz_requests
        .fetch_add(1, Ordering::Relaxed);
    let response = Json(HealthResponse {
        status: "ok".to_string(),
    });
    state
        .metrics
        .record_latency(PublicRoute::Healthz, 200, start.elapsed().as_nanos() as u64);
    response
}

async fn readyz(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let start = Instant::now();
    state
        .metrics
        .readyz_requests
        .fetch_add(1, Ordering::Relaxed);
    let response = Json(HealthResponse {
        status: "ready".to_string(),
    });
    state
        .metrics
        .record_latency(PublicRoute::Readyz, 200, start.elapsed().as_nanos() as u64);
    response
}

/// Deep readiness probe that checks the store health and write queue backpressure.
///
/// Returns HTTP 200 with "ok" status when store is healthy and queue depth is within threshold.
/// Returns HTTP 503 with "degraded" status when store is unhealthy or queue depth exceeds threshold.
/// The `write_queue` component provides bounded backpressure detection only; it does not
/// indicate full dependency health, ledger scan status, adapter health, rollback health,
/// connection pool saturation, or schema integrity.
async fn readyz_deep(State(state): State<Arc<AppState>>) -> (StatusCode, Json<DeepHealthResponse>) {
    let start = Instant::now();
    let threshold = state.server_config.write_queue_threshold;

    let store_status = match state.runtime.store.health_check().await {
        Ok(()) => ComponentStatus {
            component: "store".to_string(),
            status: "ok".to_string(),
            healthy: true,
            error: None,
        },
        Err(e) => ComponentStatus {
            component: "store".to_string(),
            status: format!("unhealthy: {}", e),
            healthy: false,
            error: Some(e.to_string()),
        },
    };

    let queue_depth = state.runtime.store.write_queue_depth();
    let queue_healthy = queue_depth <= threshold as usize;
    let queue_status = if queue_healthy {
        ComponentStatus {
            component: "write_queue".to_string(),
            status: format!("ok: depth={}, threshold={}", queue_depth, threshold),
            healthy: true,
            error: None,
        }
    } else {
        ComponentStatus {
            component: "write_queue".to_string(),
            status: format!(
                "degraded: depth={} exceeds threshold={}",
                queue_depth, threshold
            ),
            healthy: false,
            error: Some(format!(
                "queue depth {} exceeds threshold {}",
                queue_depth, threshold
            )),
        }
    };

    let healthy = store_status.healthy && queue_healthy;
    let status = if healthy { "ok" } else { "degraded" };

    let response = DeepHealthResponse {
        status: status.to_string(),
        healthy,
        components: vec![store_status, queue_status],
    };

    let elapsed_ns = start.elapsed().as_nanos() as u64;

    // Track request with status label and latency
    if healthy {
        state
            .metrics
            .readyz_deep_requests_200
            .fetch_add(1, Ordering::Relaxed);
        state
            .metrics
            .record_latency(PublicRoute::ReadyzDeep, 200, elapsed_ns);
        (StatusCode::OK, Json(response))
    } else {
        state
            .metrics
            .readyz_deep_requests_503
            .fetch_add(1, Ordering::Relaxed);
        state
            .metrics
            .record_latency(PublicRoute::ReadyzDeep, 503, elapsed_ns);
        (StatusCode::SERVICE_UNAVAILABLE, Json(response))
    }
}

/// Metrics endpoint handler.
/// Returns Prometheus-compatible text format with request counters, store health, and latency histograms.
async fn metrics_handler(State(state): State<Arc<AppState>>) -> Response {
    let start = Instant::now();
    state
        .metrics
        .metrics_scrapes
        .fetch_add(1, Ordering::Relaxed);

    let store_healthy = match state.runtime.store.health_check().await {
        Ok(()) => 1u64,
        Err(_) => 0u64,
    };
    state
        .metrics
        .store_health_up
        .store(store_healthy, Ordering::Relaxed);

    let healthz_count = state.metrics.healthz_requests.load(Ordering::Relaxed);
    let readyz_count = state.metrics.readyz_requests.load(Ordering::Relaxed);
    let readyz_deep_count_200 = state
        .metrics
        .readyz_deep_requests_200
        .load(Ordering::Relaxed);
    let readyz_deep_count_503 = state
        .metrics
        .readyz_deep_requests_503
        .load(Ordering::Relaxed);
    let metrics_count = state.metrics.metrics_scrapes.load(Ordering::Relaxed);
    let store_up = state.metrics.store_health_up.load(Ordering::Relaxed);
    let write_queue_depth = state.runtime.store.write_queue_depth();

    // Load governance error counters
    let gov_err_intents_compile = state
        .metrics
        .governance_errors_v1_intents_compile
        .load(Ordering::Relaxed);
    let gov_err_proposals_evaluate = state
        .metrics
        .governance_errors_v1_proposals_evaluate
        .load(Ordering::Relaxed);
    let gov_err_capabilities_mint = state
        .metrics
        .governance_errors_v1_capabilities_mint
        .load(Ordering::Relaxed);
    let gov_err_capabilities_revoke = state
        .metrics
        .governance_errors_v1_capabilities_revoke
        .load(Ordering::Relaxed);
    let gov_err_executions_authorize = state
        .metrics
        .governance_errors_v1_executions_authorize
        .load(Ordering::Relaxed);
    let gov_err_executions_prepare = state
        .metrics
        .governance_errors_v1_executions_prepare
        .load(Ordering::Relaxed);
    let gov_err_executions_execute = state
        .metrics
        .governance_errors_v1_executions_execute
        .load(Ordering::Relaxed);
    let gov_err_executions_verify = state
        .metrics
        .governance_errors_v1_executions_verify
        .load(Ordering::Relaxed);
    let gov_err_executions_compensate = state
        .metrics
        .governance_errors_v1_executions_compensate
        .load(Ordering::Relaxed);
    let gov_err_executions_cancel = state
        .metrics
        .governance_errors_v1_executions_cancel
        .load(Ordering::Relaxed);
    let gov_err_executions_evaluate_outcome = state
        .metrics
        .governance_errors_v1_executions_evaluate_outcome
        .load(Ordering::Relaxed);
    let gov_err_executions_execution_id = state
        .metrics
        .governance_errors_v1_executions_execution_id
        .load(Ordering::Relaxed);
    let gov_err_approvals = state
        .metrics
        .governance_errors_v1_approvals
        .load(Ordering::Relaxed);
    let gov_err_approvals_approval_id = state
        .metrics
        .governance_errors_v1_approvals_approval_id
        .load(Ordering::Relaxed);
    let gov_err_approvals_resolve = state
        .metrics
        .governance_errors_v1_approvals_resolve
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_create = state
        .metrics
        .governance_errors_v1_policy_bundles_create
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_list = state
        .metrics
        .governance_errors_v1_policy_bundles_list
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_get = state
        .metrics
        .governance_errors_v1_policy_bundles_get
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_update = state
        .metrics
        .governance_errors_v1_policy_bundles_update
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_delete = state
        .metrics
        .governance_errors_v1_policy_bundles_delete
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_set_active = state
        .metrics
        .governance_errors_v1_policy_bundles_set_active
        .load(Ordering::Relaxed);
    let gov_err_intents_list = state
        .metrics
        .governance_errors_v1_intents_list
        .load(Ordering::Relaxed);
    let gov_err_provenance_query = state
        .metrics
        .governance_errors_v1_provenance_query
        .load(Ordering::Relaxed);
    let gov_err_provenance_lineage = state
        .metrics
        .governance_errors_v1_provenance_lineage
        .load(Ordering::Relaxed);
    let gov_err_provenance_lineage_execution_id = state
        .metrics
        .governance_errors_v1_provenance_lineage_execution_id
        .load(Ordering::Relaxed);
    let gov_err_provenance_ingest = state
        .metrics
        .governance_errors_v1_provenance_ingest
        .load(Ordering::Relaxed);
    let gov_err_bridges_bridge_id_tools = state
        .metrics
        .governance_errors_v1_bridges_bridge_id_tools
        .load(Ordering::Relaxed);

    // Load governance success counters
    let gov_ok_intents_compile = state
        .metrics
        .governance_success_v1_intents_compile
        .load(Ordering::Relaxed);
    let gov_ok_proposals_evaluate = state
        .metrics
        .governance_success_v1_proposals_evaluate
        .load(Ordering::Relaxed);
    let gov_ok_capabilities_mint = state
        .metrics
        .governance_success_v1_capabilities_mint
        .load(Ordering::Relaxed);
    let gov_ok_capabilities_revoke = state
        .metrics
        .governance_success_v1_capabilities_revoke
        .load(Ordering::Relaxed);
    let gov_ok_executions_authorize = state
        .metrics
        .governance_success_v1_executions_authorize
        .load(Ordering::Relaxed);
    let gov_ok_executions_prepare = state
        .metrics
        .governance_success_v1_executions_prepare
        .load(Ordering::Relaxed);
    let gov_ok_executions_execute = state
        .metrics
        .governance_success_v1_executions_execute
        .load(Ordering::Relaxed);
    let gov_ok_executions_verify = state
        .metrics
        .governance_success_v1_executions_verify
        .load(Ordering::Relaxed);
    let gov_ok_executions_compensate = state
        .metrics
        .governance_success_v1_executions_compensate
        .load(Ordering::Relaxed);
    let gov_ok_executions_cancel = state
        .metrics
        .governance_success_v1_executions_cancel
        .load(Ordering::Relaxed);
    let gov_ok_executions_evaluate_outcome = state
        .metrics
        .governance_success_v1_executions_evaluate_outcome
        .load(Ordering::Relaxed);
    let gov_ok_executions_execution_id = state
        .metrics
        .governance_success_v1_executions_execution_id
        .load(Ordering::Relaxed);
    let gov_ok_approvals = state
        .metrics
        .governance_success_v1_approvals
        .load(Ordering::Relaxed);
    let gov_ok_approvals_approval_id = state
        .metrics
        .governance_success_v1_approvals_approval_id
        .load(Ordering::Relaxed);
    let gov_ok_approvals_resolve = state
        .metrics
        .governance_success_v1_approvals_resolve
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_create = state
        .metrics
        .governance_success_v1_policy_bundles_create
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_list = state
        .metrics
        .governance_success_v1_policy_bundles_list
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_get = state
        .metrics
        .governance_success_v1_policy_bundles_get
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_update = state
        .metrics
        .governance_success_v1_policy_bundles_update
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_delete = state
        .metrics
        .governance_success_v1_policy_bundles_delete
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_set_active = state
        .metrics
        .governance_success_v1_policy_bundles_set_active
        .load(Ordering::Relaxed);
    let gov_ok_intents_list = state
        .metrics
        .governance_success_v1_intents_list
        .load(Ordering::Relaxed);
    let gov_ok_provenance_query = state
        .metrics
        .governance_success_v1_provenance_query
        .load(Ordering::Relaxed);
    let gov_ok_provenance_lineage = state
        .metrics
        .governance_success_v1_provenance_lineage
        .load(Ordering::Relaxed);
    let gov_ok_provenance_lineage_execution_id = state
        .metrics
        .governance_success_v1_provenance_lineage_execution_id
        .load(Ordering::Relaxed);
    let gov_ok_provenance_ingest = state
        .metrics
        .governance_success_v1_provenance_ingest
        .load(Ordering::Relaxed);
    let gov_ok_bridges_bridge_id_tools = state
        .metrics
        .governance_success_v1_bridges_bridge_id_tools
        .load(Ordering::Relaxed);

    // Load latency histogram data for /v1/healthz
    let healthz_latency_buckets: Vec<u64> = state
        .metrics
        .healthz_latency_buckets
        .iter()
        .map(|b| b.load(Ordering::Relaxed))
        .collect();
    let healthz_latency_sum = state.metrics.healthz_latency_sum.load(Ordering::Relaxed);
    let healthz_latency_count = state.metrics.healthz_latency_count.load(Ordering::Relaxed);

    // Load latency histogram data for /v1/readyz
    let readyz_latency_buckets: Vec<u64> = state
        .metrics
        .readyz_latency_buckets
        .iter()
        .map(|b| b.load(Ordering::Relaxed))
        .collect();
    let readyz_latency_sum = state.metrics.readyz_latency_sum.load(Ordering::Relaxed);
    let readyz_latency_count = state.metrics.readyz_latency_count.load(Ordering::Relaxed);

    // Load latency histogram data for /v1/readyz/deep (status 200)
    let readyz_deep_latency_buckets_200: Vec<u64> = state
        .metrics
        .readyz_deep_latency_buckets_200
        .iter()
        .map(|b| b.load(Ordering::Relaxed))
        .collect();
    let readyz_deep_latency_sum_200 = state
        .metrics
        .readyz_deep_latency_sum_200
        .load(Ordering::Relaxed);
    let readyz_deep_latency_count_200 = state
        .metrics
        .readyz_deep_latency_count_200
        .load(Ordering::Relaxed);

    // Load latency histogram data for /v1/readyz/deep (status 503)
    let readyz_deep_latency_buckets_503: Vec<u64> = state
        .metrics
        .readyz_deep_latency_buckets_503
        .iter()
        .map(|b| b.load(Ordering::Relaxed))
        .collect();
    let readyz_deep_latency_sum_503 = state
        .metrics
        .readyz_deep_latency_sum_503
        .load(Ordering::Relaxed);
    let readyz_deep_latency_count_503 = state
        .metrics
        .readyz_deep_latency_count_503
        .load(Ordering::Relaxed);

    // Load latency histogram data for /v1/metrics
    let metrics_latency_buckets: Vec<u64> = state
        .metrics
        .metrics_latency_buckets
        .iter()
        .map(|b| b.load(Ordering::Relaxed))
        .collect();
    let metrics_latency_sum = state.metrics.metrics_latency_sum.load(Ordering::Relaxed);
    let metrics_latency_count = state.metrics.metrics_latency_count.load(Ordering::Relaxed);

    // Helper macro to build histogram lines for a given route/status combination
    macro_rules! histogram_lines {
        ($route:expr, $method:expr, $status:expr, $buckets:expr, $sum:expr, $count:expr) => {{
            let mut lines = String::new();
            for (i, boundary) in HISTOGRAM_BOUNDARIES.iter().enumerate() {
                lines.push_str(&format!(
                    "ferrumgate_request_duration_seconds{{route=\"{}\",method=\"{}\",status=\"{}\",le=\"{}\"}} {}\n",
                    $route, $method, $status, boundary, $buckets[i]
                ));
            }
            lines.push_str(&format!(
                "ferrumgate_request_duration_seconds{{route=\"{}\",method=\"{}\",status=\"{}\",le=\"+Inf\"}} {}\n",
                $route, $method, $status, $count
            ));
            lines.push_str(&format!(
                "ferrumgate_request_duration_seconds_sum{{route=\"{}\",method=\"{}\",status=\"{}\"}} {}\n",
                $route, $method, $status, $sum as f64 / 1e9_f64
            ));
            lines.push_str(&format!(
                "ferrumgate_request_duration_seconds_count{{route=\"{}\",method=\"{}\",status=\"{}\"}} {}\n",
                $route, $method, $status, $count
            ));
            lines
        }};
    }

    let healthz_histogram = histogram_lines!(
        "/v1/healthz",
        "GET",
        "200",
        healthz_latency_buckets,
        healthz_latency_sum,
        healthz_latency_count
    );
    let readyz_histogram = histogram_lines!(
        "/v1/readyz",
        "GET",
        "200",
        readyz_latency_buckets,
        readyz_latency_sum,
        readyz_latency_count
    );
    let readyz_deep_histogram_200 = histogram_lines!(
        "/v1/readyz/deep",
        "GET",
        "200",
        readyz_deep_latency_buckets_200,
        readyz_deep_latency_sum_200,
        readyz_deep_latency_count_200
    );
    let readyz_deep_histogram_503 = histogram_lines!(
        "/v1/readyz/deep",
        "GET",
        "503",
        readyz_deep_latency_buckets_503,
        readyz_deep_latency_sum_503,
        readyz_deep_latency_count_503
    );
    let metrics_histogram = histogram_lines!(
        "/v1/metrics",
        "GET",
        "200",
        metrics_latency_buckets,
        metrics_latency_sum,
        metrics_latency_count
    );

    let mut body = format!(
        "# HELP ferrumgate_http_requests_total HTTP requests total by route and status\n\
         # TYPE ferrumgate_http_requests_total counter\n\
         ferrumgate_http_requests_total{{route=\"/v1/healthz\",method=\"GET\",status=\"200\"}} {}\n\
         ferrumgate_http_requests_total{{route=\"/v1/readyz\",method=\"GET\",status=\"200\"}} {}\n\
         ferrumgate_http_requests_total{{route=\"/v1/readyz/deep\",method=\"GET\",status=\"200\"}} {}\n\
         ferrumgate_http_requests_total{{route=\"/v1/readyz/deep\",method=\"GET\",status=\"503\"}} {}\n\
         ferrumgate_http_requests_total{{route=\"/v1/metrics\",method=\"GET\",status=\"200\"}} {}\n\
         # HELP ferrumgate_store_health_up Store health status (1=ok, 0=unhealthy)\n\
         # TYPE ferrumgate_store_health_up gauge\n\
         ferrumgate_store_health_up {}\n\
         # HELP ferrumgate_write_queue_depth Number of pending SQLite write operations\n\
         # TYPE ferrumgate_write_queue_depth gauge\n\
         ferrumgate_write_queue_depth {}\n\
         # HELP ferrumgate_rate_limit_per_second Effective rate limit per second per IP\n\
         # TYPE ferrumgate_rate_limit_per_second gauge\n\
         ferrumgate_rate_limit_per_second {}\n\
         # HELP ferrumgate_rate_limit_burst Effective rate limit burst size per IP\n\
         # TYPE ferrumgate_rate_limit_burst gauge\n\
         ferrumgate_rate_limit_burst {}\n\
         # HELP ferrumgate_metrics_scrapes_total Number of times /v1/metrics was scraped\n\
         # TYPE ferrumgate_metrics_scrapes_total counter\n\
         ferrumgate_metrics_scrapes_total {}\n\
         # HELP ferrumgate_governance_errors_total Governance errors by route and method\n\
         # TYPE ferrumgate_governance_errors_total counter\n\
         ferrumgate_governance_errors_total{{route=\"/v1/intents/compile\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/intents\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/proposals/{{proposal_id}}/evaluate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/capabilities/mint\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/capabilities/{{capability_id}}/revoke\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/authorize\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/prepare\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/execute\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/verify\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/compensate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/cancel\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/evaluate-outcome\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/approvals\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/approvals/{{approval_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/approvals/{{approval_id}}/resolve\",method=\"POST\"}} {}\n\
\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"PUT\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"DELETE\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}/active\",method=\"PUT\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/query\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/lineage\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/lineage/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/ingest\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/bridges/{{bridge_id}}/tools\",method=\"GET\"}} {}\n\
         # HELP ferrumgate_governance_success_total Governance successes by route and method\n\
         # TYPE ferrumgate_governance_success_total counter\n\
         ferrumgate_governance_success_total{{route=\"/v1/intents/compile\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/intents\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/proposals/{{proposal_id}}/evaluate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/capabilities/mint\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/capabilities/{{capability_id}}/revoke\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/authorize\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/prepare\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/execute\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/verify\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/compensate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/cancel\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/evaluate-outcome\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/approvals\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/approvals/{{approval_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/approvals/{{approval_id}}/resolve\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"PUT\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"DELETE\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}/active\",method=\"PUT\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/query\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/lineage\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/lineage/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/ingest\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/bridges/{{bridge_id}}/tools\",method=\"GET\"}} {}\n",
        healthz_count,
        readyz_count,
        readyz_deep_count_200,
        readyz_deep_count_503,
        metrics_count,
        store_up,
        write_queue_depth,
        state.server_config.rate_limit_per_second,
        state.server_config.rate_limit_burst,
        metrics_count,
        gov_err_intents_compile,
        gov_err_intents_list,
        gov_err_proposals_evaluate,
        gov_err_capabilities_mint,
        gov_err_capabilities_revoke,
        gov_err_executions_authorize,
        gov_err_executions_prepare,
        gov_err_executions_execute,
        gov_err_executions_verify,
        gov_err_executions_compensate,
        gov_err_executions_cancel,
        gov_err_executions_evaluate_outcome,
        gov_err_executions_execution_id,
        gov_err_approvals,
        gov_err_approvals_approval_id,
        gov_err_approvals_resolve,
        gov_err_policy_bundles_create,
        gov_err_policy_bundles_list,
        gov_err_policy_bundles_get,
        gov_err_policy_bundles_update,
        gov_err_policy_bundles_delete,
        gov_err_policy_bundles_set_active,
        gov_err_provenance_query,
        gov_err_provenance_lineage,
        gov_err_provenance_lineage_execution_id,
        gov_err_provenance_ingest,
        gov_err_bridges_bridge_id_tools,
        gov_ok_intents_compile,
        gov_ok_intents_list,
        gov_ok_proposals_evaluate,
        gov_ok_capabilities_mint,
        gov_ok_capabilities_revoke,
        gov_ok_executions_authorize,
        gov_ok_executions_prepare,
        gov_ok_executions_execute,
        gov_ok_executions_verify,
        gov_ok_executions_compensate,
        gov_ok_executions_cancel,
        gov_ok_executions_evaluate_outcome,
        gov_ok_executions_execution_id,
        gov_ok_approvals,
        gov_ok_approvals_approval_id,
        gov_ok_approvals_resolve,
        gov_ok_policy_bundles_create,
        gov_ok_policy_bundles_list,
        gov_ok_policy_bundles_get,
        gov_ok_policy_bundles_update,
        gov_ok_policy_bundles_delete,
        gov_ok_policy_bundles_set_active,
        gov_ok_provenance_query,
        gov_ok_provenance_lineage,
        gov_ok_provenance_lineage_execution_id,
        gov_ok_provenance_ingest,
        gov_ok_bridges_bridge_id_tools,
    );

    // Append histogram output to body
    body.push_str("# HELP ferrumgate_request_duration_seconds HTTP request latency histogram by route, method, and status\n");
    body.push_str("# TYPE ferrumgate_request_duration_seconds histogram\n");
    body.push_str(&healthz_histogram);
    body.push_str(&readyz_histogram);
    body.push_str(&readyz_deep_histogram_200);
    body.push_str(&readyz_deep_histogram_503);
    body.push_str(&metrics_histogram);

    // Record metrics handler's own latency
    state
        .metrics
        .record_latency(PublicRoute::Metrics, 200, start.elapsed().as_nanos() as u64);

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
        .into_response()
}

async fn compile_intent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<IntentCompileRequest>,
) -> Result<Json<IntentCompileResponse>, ApiProblem> {
    let now = Utc::now();
    let requested_risk = req.requested_risk_tier.unwrap_or(RiskTier::Medium);
    let default_rollback_class = infer_rollback_class(&req.requested_resource_scope);

    let input_labels = req
        .raw_inputs
        .iter()
        .flat_map(|r| r.trust_labels.clone())
        .collect::<Vec<_>>();
    let sensitivity_labels = req
        .raw_inputs
        .iter()
        .flat_map(|r| r.sensitivity_labels.clone())
        .collect::<Vec<_>>();

    let envelope = IntentEnvelope {
        intent_id: ferrum_proto::IntentId::new(),
        principal_id: req.principal_id,
        session_id: req.session_id,
        channel_id: req.channel_id,
        title: req.title.clone(),
        goal: req.goal.clone(),
        normalized_goal: req.goal.trim().to_lowercase(),
        allowed_outcomes: vec![OutcomeClause {
            id: "read".to_string(),
            description: "read only analysis".to_string(),
            effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
            required: true,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: req.requested_resource_scope,
        risk_tier: requested_risk,
        approval_mode: req
            .approval_mode
            .unwrap_or(ferrum_proto::ApprovalMode::None),
        default_rollback_class,
        time_budget: TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: TrustContextSummary {
            input_labels,
            sensitivity_labels,
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: req.raw_inputs.iter().filter_map(|r| r.event_id).collect(),
        tags: Vec::new(),
        metadata: req.metadata,
        status: IntentStatus::Active,
        created_at: now,
        expires_at: now + Duration::minutes(15),
    };

    // I1: Validate envelope before persisting.
    if let Err(msg) = envelope.validate() {
        return governance_err!(
            state,
            GovernanceRoute::IntentsCompile,
            ApiProblem::new(StatusCode::BAD_REQUEST, ApiErrorCode::ValidationError, msg,)
        );
    }

    // Persist the intent envelope so foreign-key constraints in proposals
    // and capabilities tables are satisfied.
    // Synchronous write: must complete before response to guarantee FK constraints.
    if let Err(e) = state.runtime.store.intents().insert(&envelope).await {
        tracing::warn!(error = %e, "failed to persist intent to DB");
        return governance_err!(
            state,
            GovernanceRoute::IntentsCompile,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::IntentsCompile,
        Ok(Json(IntentCompileResponse {
            envelope,
            warnings: Vec::new(),
        }))
    )
}

/// Query parameters for GET /v1/intents
#[derive(Debug, Deserialize)]
struct ListIntentsParams {
    #[serde(default)]
    intent_id: Option<String>,
    #[serde(default)]
    state: Vec<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default = "default_intent_list_limit")]
    limit: u32,
}

fn default_intent_list_limit() -> u32 {
    50
}

const MAX_INTENT_LIST_LIMIT: u32 = 200;

/// Response item for intent list
#[derive(Debug, serde::Serialize)]
struct IntentListItem {
    intent_id: String,
    principal_id: String,
    title: String,
    status: String,
    risk_tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_state: Option<String>,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
}

/// Response envelope for intent list
#[derive(Debug, serde::Serialize)]
struct IntentListEnvelope {
    items: Vec<IntentListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

async fn list_intents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListIntentsParams>,
) -> Result<Json<IntentListEnvelope>, ApiProblem> {
    // Validate and clamp limit
    let limit = if params.limit == 0 || params.limit > MAX_INTENT_LIST_LIMIT {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            format!("limit must be between 1 and {}", MAX_INTENT_LIST_LIMIT),
        ));
    } else {
        params.limit
    };

    // Parse intent_id filter if provided
    let intent_id = if let Some(ref id) = params.intent_id {
        let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "invalid intent_id format",
            )
        })?;
        Some(ferrum_proto::IntentId(uuid))
    } else {
        None
    };

    // Parse status filters - convert string to IntentStatus
    let mut statuses = Vec::new();
    for s in &params.state {
        let status = match s.to_lowercase().as_str() {
            "active" => IntentStatus::Active,
            "closed" => IntentStatus::Closed,
            "expired" => IntentStatus::Expired,
            "quarantined" => IntentStatus::Quarantined,
            "revoked" => IntentStatus::Revoked,
            _ => {
                return Err(ApiProblem::new(
                    StatusCode::BAD_REQUEST,
                    ApiErrorCode::ValidationError,
                    format!("unknown intent status: {}", s),
                ));
            }
        };
        statuses.push(status);
    }

    // Query the store
    let (intents_with_state, next_cursor) = state
        .runtime
        .store
        .intents()
        .list_intents_with_exec_state(intent_id, &statuses, params.cursor.as_deref(), limit)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?;

    let items: Vec<IntentListItem> = intents_with_state
        .into_iter()
        .map(|(intent, exec_state)| IntentListItem {
            intent_id: intent.intent_id.to_string(),
            principal_id: intent.principal_id.to_string(),
            title: intent.title.clone(),
            status: format!("{:?}", intent.status),
            risk_tier: format!("{:?}", intent.risk_tier),
            exec_state,
            created_at: intent.created_at.to_rfc3339(),
            expires_at: Some(intent.expires_at.to_rfc3339()),
        })
        .collect();

    governance_ok!(
        state,
        GovernanceRoute::IntentsList,
        Ok(Json(IntentListEnvelope { items, next_cursor }))
    )
}

async fn evaluate_proposal(
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

    // Check active policy bundles before falling back to PDP.
    // Use firewall-derived trust context for bundle evaluation to properly assess taint and other trust attributes.
    let out = if let Some(bundle_response) =
        evaluate_active_policy_bundles(&state.runtime.store, &intent, &proposal, &trust).await
    {
        let out = bundle_response;
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
        out
    } else {
        let out = match state.runtime.pdp.evaluate(&intent, &proposal, &trust).await {
            Ok(out) => out,
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ProposalsEvaluate,
                    ApiProblem::internal(e)
                );
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
        out
    };

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

async fn mint_capability(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CapabilityMintRequest>,
) -> Result<Json<CapabilityMintResponse>, ApiProblem> {
    let response = match state.runtime.cap.mint(request).await {
        Ok(response) => response,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesMint,
                ApiProblem::from_capability(e)
            );
        }
    };

    // Persist the capability to the store so foreign-key constraints in
    // executions and other tables are satisfied.
    // Write-queue ensures serialized writes - no more SQLite lock contention.
    if let Err(e) = state
        .runtime
        .store
        .capabilities()
        .insert(&response.lease)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesMint,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit CapabilityMinted provenance event.
    let cap_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::CapabilityMinted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Capability,
            object_id: response.lease.capability_id.to_string(),
            summary: Some("Capability minted".to_string()),
        },
        intent_id: Some(response.lease.intent_id),
        proposal_id: Some(response.lease.proposal_id),
        execution_id: None,
        capability_id: Some(response.lease.capability_id),
        rollback_contract_id: None,
        policy_bundle_id: Some(response.lease.policy_bundle_id),
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
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&cap_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesMint,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(state, GovernanceRoute::CapabilitiesMint, Ok(Json(response)))
}

async fn revoke_capability(
    State(state): State<Arc<AppState>>,
    Path(capability_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    let id = parse_capability_id(&capability_id).inspect_err(|_| {
        state
            .metrics
            .increment_governance_error(GovernanceRoute::CapabilitiesRevoke)
    })?;

    // Revoke the capability in the capability service (in-memory)
    // If NotFound, fall back to store and revoke there synchronously
    let lease = match state.runtime.cap.revoke(id).await {
        Ok(lease) => lease,
        Err(CapabilityError::NotFound) => {
            // In-memory miss: load from store, validate, revoke, persist synchronously
            let lease = match state.runtime.store.capabilities().get(id).await {
                Ok(Some(lease)) => lease,
                Ok(None) => {
                    return governance_err!(
                        state,
                        GovernanceRoute::CapabilitiesRevoke,
                        ApiProblem::from_capability(CapabilityError::NotFound)
                    );
                }
                Err(e) => {
                    return governance_err!(
                        state,
                        GovernanceRoute::CapabilitiesRevoke,
                        ApiProblem::internal(anyhow::Error::from(e))
                    );
                }
            };

            // Validate status
            if matches!(lease.status, CapabilityStatus::Revoked) {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::from_capability(CapabilityError::Revoked)
                );
            }
            if matches!(lease.status, CapabilityStatus::Used) {
                // Already used capabilities cannot be revoked (they're consumed, not active)
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::from_capability(CapabilityError::AlreadyUsed)
                );
            }
            if lease.expires_at < Utc::now() {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::from_capability(CapabilityError::Expired)
                );
            }

            // Set revoked status
            let mut lease = lease;
            lease.status = CapabilityStatus::Revoked;
            lease.revoked_at = Some(Utc::now());

            // Persist synchronously before returning
            if let Err(e) = state.runtime.store.capabilities().update(&lease).await {
                return governance_err!(
                    state,
                    GovernanceRoute::CapabilitiesRevoke,
                    ApiProblem::internal(anyhow::Error::from(e))
                );
            }

            lease
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::CapabilitiesRevoke,
                ApiProblem::from_capability(e)
            );
        }
    };

    // Build provenance event
    let event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::CapabilityRevoked,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "gateway".to_string(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::Capability,
            object_id: lease.capability_id.to_string(),
            summary: None,
        },
        intent_id: Some(lease.intent_id),
        proposal_id: Some(lease.proposal_id),
        execution_id: None,
        capability_id: Some(lease.capability_id),
        rollback_contract_id: None,
        policy_bundle_id: Some(lease.policy_bundle_id),
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
        source_runtime_id: None,
    };

    // Persist capability revocation and append provenance event synchronously.
    // Return error if persistence fails rather than fire-and-forget.
    if let Err(e) = state.runtime.store.capabilities().update(&lease).await {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesRevoke,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    if let Err(e) = state.runtime.store.provenance().append_event(&event).await {
        return governance_err!(
            state,
            GovernanceRoute::CapabilitiesRevoke,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    let response = serde_json::json!({
        "ok": true,
        "capability_id": lease.capability_id.to_string()
    });
    let sanitized = sanitize_json(&state.runtime.firewall, response);
    governance_ok!(
        state,
        GovernanceRoute::CapabilitiesRevoke,
        Ok(Json(sanitized))
    )
}

// ---------------------------------------------------------------------------
// Durable capability helpers
// ---------------------------------------------------------------------------

/// Load capability from in-memory service, falling back to persisted store.
/// Returns NotFound if not found in either.
async fn get_capability_for_authorize(
    cap: &Arc<dyn CapabilityService>,
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> Result<CapabilityLease, CapabilityError> {
    // Try in-memory first
    match cap.get(capability_id).await {
        Ok(lease) => return Ok(lease),
        Err(CapabilityError::NotFound) => {}
        Err(e) => return Err(e),
    }

    // Fall back to persisted store
    let Some(lease) = store
        .capabilities()
        .get(capability_id)
        .await
        .map_err(|_e| CapabilityError::NotFound)?
    // Treat store errors as NotFound for authorize
    else {
        return Err(CapabilityError::NotFound);
    };

    // Validate persisted capability status
    if matches!(lease.status, CapabilityStatus::Used) {
        return Err(CapabilityError::AlreadyUsed);
    }
    if matches!(lease.status, CapabilityStatus::Revoked) {
        return Err(CapabilityError::Revoked);
    }
    if lease.expires_at < Utc::now() {
        return Err(CapabilityError::Expired);
    }

    Ok(lease)
}

/// Mark capability as used in memory and persist the updated status.
/// If the capability is not found in memory, falls back to store and persists there.
async fn mark_capability_used_durable(
    cap: &Arc<dyn CapabilityService>,
    store: &Arc<dyn StoreFacade>,
    capability_id: CapabilityId,
) -> Result<CapabilityLease, CapabilityError> {
    // Try in-memory mark_used first
    match cap.mark_used(capability_id).await {
        Ok(lease) => {
            // In-memory succeeded; persist the updated lease synchronously
            store.capabilities().update(&lease).await.map_err(|e| {
                tracing::error!(error = %e, "failed to persist used capability status");
                CapabilityError::NotFound // Map to NotFound for API error handling
            })?;
            Ok(lease)
        }
        Err(CapabilityError::NotFound) => {
            // In-memory miss: load from store, validate, then atomically update
            let Some(lease) = store.capabilities().get(capability_id).await.map_err(|e| {
                tracing::error!(error = %e, "failed to load capability from store for mark_used");
                CapabilityError::NotFound
            })?
            else {
                return Err(CapabilityError::NotFound);
            };

            // Validate status before attempting atomic update
            if matches!(lease.status, CapabilityStatus::Used) {
                return Err(CapabilityError::AlreadyUsed);
            }
            if matches!(lease.status, CapabilityStatus::Revoked) {
                return Err(CapabilityError::Revoked);
            }
            if lease.expires_at < Utc::now() {
                return Err(CapabilityError::Expired);
            }

            // Atomically update only if still Active; if another writer won, fail
            let updated = store
                .capabilities()
                .update_status_if_active(capability_id, CapabilityStatus::Used)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "failed to atomically update capability status");
                    CapabilityError::NotFound
                })?;

            if !updated {
                return Err(CapabilityError::AlreadyUsed);
            }

            // Reconstruct the used lease for the caller
            let mut used_lease = lease;
            used_lease.status = CapabilityStatus::Used;
            Ok(used_lease)
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// I6 Approval Binding Digest Validation
// ---------------------------------------------------------------------------

/// Validates the approval binding digest per I6 invariant.
///
/// Checks when `approval_binding=Some`:
/// 1. Approval exists (404 -> 403 IntegrityMismatch)
/// 2. Approval state is Granted (403 PolicyDenied)
/// 3. Binding not expired (403 PolicyDenied)
/// 4. Approval not expired (403 PolicyDenied)
/// 5. Binding digest matches approval digest (403 IntegrityMismatch)
/// 6. Computed proposal digest matches binding digest (403 IntegrityMismatch)
///
/// Skips all checks when `approval_binding=None` (backward compatible).
async fn validate_approval_binding_digest(
    store: &Arc<dyn StoreFacade>,
    binding: &ApprovalBinding,
    proposal_id: ProposalId,
) -> Result<(), ApiProblem> {
    // Step 1: Fetch the approval by ID
    let approval = store
        .approvals()
        .get(binding.approval_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "approval not found for binding",
            )
        })?;

    // Step 2: Check approval state is Granted
    if !matches!(approval.state, ferrum_proto::ApprovalState::Granted) {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            format!("approval state is {:?}, expected Granted", approval.state),
        ));
    }

    // Step 3: Check binding not expired
    if binding.expires_at < Utc::now() {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "approval binding has expired",
        ));
    }

    // Step 4: Check approval not expired
    if approval.expires_at < Utc::now() {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::PolicyDenied,
            "approval has expired",
        ));
    }

    // Step 5: Check binding digest matches approval digest
    if binding.approved_action_digest != approval.action_digest {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::IntegrityMismatch,
            "binding digest does not match approval digest",
        ));
    }

    // Step 6: Fetch proposal and verify computed digest matches binding digest
    let proposal = store
        .proposals()
        .get(proposal_id)
        .await
        .map_err(|e| ApiProblem::internal(anyhow::Error::from(e)))?
        .ok_or_else(|| {
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::IntegrityMismatch,
                "proposal not found",
            )
        })?;

    let computed_digest = proposal.canonical_action_digest();
    if computed_digest != binding.approved_action_digest {
        return Err(ApiProblem::new(
            StatusCode::FORBIDDEN,
            ApiErrorCode::IntegrityMismatch,
            "computed proposal digest does not match binding digest",
        ));
    }

    Ok(())
}

async fn authorize_execution(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AuthorizeExecutionRequest>,
) -> Result<Json<AuthorizeExecutionResponse>, ApiProblem> {
    // Load capability from in-memory service, falling back to persisted store.
    // This ensures capability survives in-memory state loss.
    let lease = match get_capability_for_authorize(
        &state.runtime.cap,
        &state.runtime.store,
        request.capability_id,
    )
    .await
    {
        Ok(lease) => lease,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(e)
            );
        }
    };

    // I5: Validate that capability resource_bindings is a subset of intent resource_scope.
    // This prevents a capability from expanding beyond the intent's authorized scope.
    let intent = match state.runtime.store.intents().get(lease.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for capability",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if let Err(scope_violation) =
        validate_resource_bindings_subset_of_scope(&lease.resource_bindings, &intent.resource_scope)
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                scope_violation,
            )
        );
    }

    // I6: Validate approval binding digest if present.
    // This ensures the proposal digest matches the approved action digest.
    // Skipped when approval_binding=None (backward compatible).
    if let Some(ref binding) = lease.approval_binding {
        validate_approval_binding_digest(&state.runtime.store, binding, request.proposal_id)
            .await
            .map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::ExecutionsAuthorize, e)
            })?;
    }

    // Mark the capability as used - returns AlreadyUsed if already consumed.
    // This enforces single-use: first authorize succeeds, subsequent ones fail.
    // Persists the updated status to store for durability.
    match mark_capability_used_durable(
        &state.runtime.cap,
        &state.runtime.store,
        request.capability_id,
    )
    .await
    {
        Ok(lease) => lease,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsAuthorize,
                ApiProblem::from_capability(e)
            );
        }
    };

    let record = ExecutionRecord {
        execution_id: ExecutionId::new(),
        proposal_id: request.proposal_id,
        intent_id: lease.intent_id,
        capability_id: lease.capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: if request.dry_run {
            ExecutionState::Authorized
        } else {
            ExecutionState::Prepared
        },
        started_at: Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata: ferrum_proto::JsonMap::new(),
    };

    // Persist the execution record so subsequent prepare/execute can find it.
    // Write-queue ensures serialized writes - no more SQLite lock contention.
    if let Err(e) = state.runtime.store.executions().insert(&record).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit provenance event for authorization (Q1-P5 conservative chain: authorize).
    let auth_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ActionProposalSubmitted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: record.execution_id.to_string(),
            summary: Some("Execution authorized".to_string()),
        },
        intent_id: Some(record.intent_id),
        proposal_id: Some(record.proposal_id),
        execution_id: Some(record.execution_id),
        capability_id: Some(record.capability_id),
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
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&auth_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsAuthorize,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsAuthorize,
        Ok(Json(AuthorizeExecutionResponse {
            execution: record,
            warnings: Vec::new(),
        }))
    )
}

async fn prepare_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::PrepareExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsPrepare, e);
        }
    };

    // Look up the existing execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // D1.5 mandatory: Reject prepare for non-preparable execution states.
    // Only Authorized or Prepared executions can transition to Prepared.
    // All other states (Proposed, Running, Committed, Compensated, etc.) return 409 Conflict.
    match execution.state {
        ExecutionState::Authorized | ExecutionState::Prepared => {
            // Valid state - proceed with prepare
        }
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "execution in state '{:?}' cannot be prepared; only '{:?}' or '{:?}' are preparable",
                        execution.state,
                        ExecutionState::Authorized,
                        ExecutionState::Prepared
                    ),
                )
            );
        }
    }

    // Look up the proposal to retrieve the real rollback_class.
    // The proposal is the most reliable existing linked record for this execution.
    let proposal = match state
        .runtime
        .store
        .proposals()
        .get(execution.proposal_id)
        .await
    {
        Ok(Some(proposal)) => proposal,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "proposal not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    let rollback_class = proposal.requested_rollback_class.clone();

    // WS3: Enforce draft-only guard at prepare checkpoint.
    // Look up the intent and reject preparation if the intent enforces draft-only mode.
    // This prevents a draft-only intent from bypassing evaluate and reaching prepare.
    let intent = match state.runtime.store.intents().get(execution.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if matches!(intent.approval_mode, ApprovalMode::DraftOnly) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "draft-only intent cannot proceed to prepare",
            )
        );
    }

    let request = build_prepare_request_for_proposal(
        &state.runtime.rollback,
        execution.intent_id,
        execution.proposal_id,
        execution_id,
        &rollback_class,
        &proposal.tool_name,
        &intent.resource_scope,
        &proposal.raw_arguments,
    );

    let response = match state.runtime.rollback.prepare(request).await {
        Ok(response) => response,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsPrepare,
                ApiProblem::internal(e)
            );
        }
    };

    // Store the contract in the database
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .insert(&response.contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Capture execution IDs for provenance before moving into updated_execution
    let execution_intent_id = execution.intent_id;
    let execution_proposal_id = execution.proposal_id;

    // Link the contract to the execution by updating rollback_contract_id
    let mut updated_execution = execution;
    updated_execution.rollback_contract_id = Some(response.contract.contract_id);
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit provenance event for preparation (Q1-P5 conservative chain: prepare).
    let prepare_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectPrepared,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: response.contract.contract_id.to_string(),
            summary: Some("Execution prepared with rollback contract".to_string()),
        },
        intent_id: Some(execution_intent_id),
        proposal_id: Some(execution_proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(response.contract.contract_id),
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
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&prepare_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit ToolCallPrepared provenance event.
    let tool_prepared_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ToolCallPrepared,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Tool call prepared for execution".to_string()),
        },
        intent_id: Some(execution_intent_id),
        proposal_id: Some(execution_proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(response.contract.contract_id),
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
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&tool_prepared_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsPrepare,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsPrepare,
        Ok(Json(ferrum_proto::PrepareExecutionResponse {
            execution_id,
            prepared: response.accepted,
            rollback_contract: Some(response.contract),
            warnings: response.warnings,
        }))
    )
}

async fn execute_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    Json(request): Json<ferrum_proto::ExecuteExecutionRequest>,
) -> Result<Json<ferrum_proto::ExecuteExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsExecute, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS3: Defense-in-depth — enforce draft-only guard at execute checkpoint.
    // Look up the intent and reject execution if the intent enforces draft-only mode.
    // This is defense-in-depth; prepare already blocks DraftOnly, but execute also
    // guards against any path that might bypass prepare.
    let intent = match state.runtime.store.intents().get(execution.intent_id).await {
        Ok(Some(intent)) => intent,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    if matches!(intent.approval_mode, ApprovalMode::DraftOnly) {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "draft-only intent cannot proceed to execute",
            )
        );
    }

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Execute guard: contract must be Prepared and execution must be Prepared or Authorized.
    // Return 409 Conflict for invalid state transitions.
    match (&contract.state, &execution.state) {
        (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Prepared)
        | (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Authorized)
        | (ferrum_proto::RollbackState::Prepared, ferrum_proto::ExecutionState::Proposed) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "execute not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Call execute on the adapter via the rollback service
    let receipt = match state
        .runtime
        .rollback
        .execute(&contract, &request.payload)
        .await
    {
        Ok(receipt) => receipt,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecute,
                ApiProblem::internal(e)
            );
        }
    };

    // Update contract state to ExecutedAwaitingVerify and capture after_hash from
    // the execute receipt so after_hash is available for inspection immediately
    // after execute (before verify has run).
    let mut updated_contract = contract.clone();
    updated_contract.state = ferrum_proto::RollbackState::ExecutedAwaitingVerify;
    if let ferrum_proto::RollbackTarget::FilePath {
        ref mut after_hash, ..
    } = updated_contract.target
    {
        *after_hash = receipt.result_digest.clone();
    }
    // For HTTP targets, propagate request_digest from execute receipt into target
    // so that compensation replay can validate digest matching.
    if let ferrum_proto::RollbackTarget::HttpRequest {
        ref mut request_digest,
        ..
    } = updated_contract.target
    {
        if let Some(digest) = receipt
            .adapter_metadata
            .get("request_digest")
            .and_then(|v| v.as_str())
        {
            *request_digest = digest.to_string();
        }
    }
    // Propagate adapter_metadata from execute receipt into contract metadata so that
    // rollback/compensate can access critical fields (e.g., branch_name for GitBranchCreate).
    for (key, value) in &receipt.adapter_metadata {
        updated_contract.metadata.insert(key.clone(), value.clone());
    }
    // Store execute payload for later compensation enrichment (HTTP replay).
    updated_contract
        .metadata
        .insert("execute_payload".to_string(), request.payload.clone());
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Update execution state to Running
    let mut updated_execution = execution;
    updated_execution.state = ferrum_proto::ExecutionState::Running;
    updated_execution.result_digest = receipt.result_digest.clone();
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit ToolCallExecuted provenance event.
    let tool_executed_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::ToolCallExecuted,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Tool call executed".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: updated_execution.rollback_contract_id,
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
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&tool_executed_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsExecute,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsExecute,
        Ok(Json(ferrum_proto::ExecuteExecutionResponse {
            execution_id,
            executed: true,
            result_digest: receipt.result_digest,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

async fn verify_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::VerifyExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsVerify, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Verify guard: contract must be ExecutedAwaitingVerify and execution must be
    // Running or AwaitingVerification. Return 409 Conflict for invalid state transitions.
    match (&contract.state, &execution.state) {
        (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::Running,
        )
        | (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::AwaitingVerification,
        ) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "verify not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Call verify on the adapter via the rollback service.
    // Before calling verify, update FileHashMatches checks with the result_digest
    // so that they can verify post-execute content hash.
    let mut verify_contract = contract.clone();
    if let Some(ref result_digest) = execution.result_digest {
        for check in &mut verify_contract.verify_checks {
            if matches!(check.check_type, ferrum_proto::CheckType::FileHashMatches) {
                check.config.insert(
                    "expected_hash".to_string(),
                    serde_json::json!(result_digest),
                );
            }
        }
        // Also update after_hash on the persisted contract for future reference
        if let ferrum_proto::RollbackTarget::FilePath {
            ref mut after_hash, ..
        } = verify_contract.target
        {
            *after_hash = Some(result_digest.clone());
        }
    }

    let verified = match state.runtime.rollback.verify(&verify_contract).await {
        Ok(verified) => verified,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(e)
            );
        }
    };

    // Update contract state based on verification result.
    // Persist verify_contract (not the original contract) so that verify-time
    // mutations (expected_hash on FileHashMatches checks, after_hash on target)
    // are stored for future inspection.
    let mut updated_contract = verify_contract;
    updated_contract.state = if verified {
        ferrum_proto::RollbackState::Verified
    } else {
        ferrum_proto::RollbackState::Failed
    };
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // D1.6 auto_commit fix: Only set execution to Committed (and emit SideEffectCommitted)
    // when verified=true AND contract.auto_commit=true. When auto_commit=false, the execution
    // remains in Running/AwaitingVerification state to await explicit commit.
    // This preserves the verified result in contract state while respecting rollback semantics.
    let mut updated_execution = execution;
    if verified {
        if updated_contract.auto_commit {
            // auto_commit=true: normal path - execution becomes Committed
            updated_execution.state = ferrum_proto::ExecutionState::Committed;
        } else {
            // auto_commit=false: verified but not committed - keep execution in current state
            // Contract is Verified but execution stays Running/AwaitingVerification
            // This allows explicit commit via separate flow when auto_commit=false
        }
    } else {
        updated_execution.state = ferrum_proto::ExecutionState::Failed;
    }
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit SideEffectVerified provenance event (regardless of verification result).
    let verified_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectVerified,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Side effect verified".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(updated_contract.contract_id),
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
        metadata: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert("verified".to_string(), serde_json::json!(verified));
            m
        },
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&verified_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsVerify,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit SideEffectCommitted provenance event only when verification succeeded AND auto_commit=true.
    // When auto_commit=false, SideEffectCommitted is suppressed to preserve rollback semantics.
    if verified && updated_contract.auto_commit {
        let committed_event = ProvenanceEvent {
            event_id: EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::SideEffectCommitted,
            occurred_at: Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::Gateway,
                actor_id: "ferrum-gateway".to_string(),
                display_name: Some("FerrumGate Gateway".to_string()),
            },
            object: ObjectRef {
                object_type: ObjectType::RollbackContract,
                object_id: updated_contract.contract_id.to_string(),
                summary: Some("Side effect committed".to_string()),
            },
            intent_id: Some(updated_execution.intent_id),
            proposal_id: Some(updated_execution.proposal_id),
            execution_id: Some(execution_id),
            capability_id: None,
            rollback_contract_id: Some(updated_contract.contract_id),
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
            metadata: ferrum_proto::JsonMap::new(),
            source_runtime_id: None,
        };
        if let Err(e) = state
            .runtime
            .store
            .provenance()
            .append_event(&committed_event)
            .await
        {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsVerify,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsVerify,
        Ok(Json(ferrum_proto::VerifyExecutionResponse {
            execution_id,
            verified,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

async fn compensate_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::CompensateExecutionResponse>, ApiProblem> {
    let execution_id = match parse_execution_id(&execution_id) {
        Ok(id) => id,
        Err(e) => {
            return governance_err!(state, GovernanceRoute::ExecutionsCompensate, e);
        }
    };

    // Look up the execution record
    let execution = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Get the rollback contract ID from the execution
    let rollback_contract_id = match execution.rollback_contract_id {
        Some(id) => id,
        None => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution has no rollback contract",
                )
            );
        }
    };

    // Look up the rollback contract
    let contract = match state
        .runtime
        .store
        .rollback_contracts()
        .get(rollback_contract_id)
        .await
    {
        Ok(Some(contract)) => contract,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "rollback contract not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // WS-Compensate state guard
    match (&contract.state, &execution.state) {
        (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::Running,
        )
        | (
            ferrum_proto::RollbackState::ExecutedAwaitingVerify,
            ferrum_proto::ExecutionState::AwaitingVerification,
        ) => {}
        _ => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsCompensate,
                ApiProblem::new(
                    StatusCode::CONFLICT,
                    ApiErrorCode::Conflict,
                    format!(
                        "compensate not allowed in current state: contract={:?}, execution={:?}",
                        contract.state, execution.state,
                    ),
                )
            );
        }
    }

    // Enrich HTTP placeholder compensation plans before compensate so that
    // parse_replay_contract can validate method/payload/expected_statuses.
    let contract = enrich_http_compensation_if_needed(contract);

    // Call compensate on the contract
    if let Err(e) = state.runtime.rollback.compensate(&contract).await {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(e)
        );
    }

    // Update contract state to Compensated
    let mut updated_contract = contract.clone();
    updated_contract.state = ferrum_proto::RollbackState::Compensated;
    if let Err(e) = state
        .runtime
        .store
        .rollback_contracts()
        .update(&updated_contract)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Update execution state to Compensated
    let mut updated_execution = execution;
    updated_execution.state = ferrum_proto::ExecutionState::Compensated;
    if let Err(e) = state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    // Emit provenance event
    let terminal_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectCompensated,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::RollbackContract,
            object_id: updated_contract.contract_id.to_string(),
            summary: Some("Execution compensated".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: None,
        rollback_contract_id: Some(updated_contract.contract_id),
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
        metadata: ferrum_proto::JsonMap::new(),
        source_runtime_id: None,
    };
    if let Err(e) = state
        .runtime
        .store
        .provenance()
        .append_event(&terminal_event)
        .await
    {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCompensate,
            ApiProblem::internal(anyhow::Error::from(e))
        );
    }

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCompensate,
        Ok(Json(ferrum_proto::CompensateExecutionResponse {
            execution_id,
            compensated: true,
            rollback_contract: Some(updated_contract),
            warnings: Vec::new(),
        }))
    )
}

async fn cancel_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ferrum_proto::CancelExecutionResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsCancel, e)
    })?;

    // Look up the execution record
    let execution = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                ),
            )
        })?;

    let previous_state = execution.state.clone();

    // ------------------------------------------------------------------
    // Cancel guard: only non-terminal states can be canceled.
    // Terminal states: Verified, Committed, Compensated, RolledBack, Failed,
    //   Expired, Denied, Quarantined
    // Non-terminal states that can be canceled: Proposed, Authorized, Prepared,
    //   Running, AwaitingApproval, AwaitingVerification
    // ------------------------------------------------------------------
    let is_cancelable = matches!(
        previous_state,
        ExecutionState::Proposed
            | ExecutionState::Authorized
            | ExecutionState::Prepared
            | ExecutionState::Running
            | ExecutionState::AwaitingApproval
            | ExecutionState::AwaitingVerification
    );

    if !is_cancelable {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsCancel,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                "cancel not allowed: execution is in terminal state",
            )
        );
    }

    // Update execution state to Canceled
    let mut updated_execution = execution;
    updated_execution.state = ExecutionState::Canceled;
    updated_execution.finished_at = Some(Utc::now());
    state
        .runtime
        .store
        .executions()
        .update(&updated_execution)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Emit SideEffectRolledBack provenance event for cancel operation.
    // Cancel triggers a rollback-like effect even if no contract exists.
    let cancel_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: ferrum_proto::ProvenanceEventKind::SideEffectRolledBack,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::SideEffect,
            object_id: execution_id.to_string(),
            summary: Some("Execution canceled".to_string()),
        },
        intent_id: Some(updated_execution.intent_id),
        proposal_id: Some(updated_execution.proposal_id),
        execution_id: Some(execution_id),
        capability_id: Some(updated_execution.capability_id),
        rollback_contract_id: updated_execution.rollback_contract_id,
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
        metadata: {
            let mut m = ferrum_proto::JsonMap::new();
            m.insert(
                "previous_state".to_string(),
                serde_json::json!(format!("{:?}", previous_state)),
            );
            m
        },
        source_runtime_id: None,
    };
    state
        .runtime
        .store
        .provenance()
        .append_event(&cancel_event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsCancel,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsCancel,
        Ok(Json(ferrum_proto::CancelExecutionResponse {
            execution_id,
            previous_state,
            current_state: ExecutionState::Canceled,
            canceled_at: Utc::now(),
        }))
    )
}

async fn evaluate_outcome(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
    Json(report): Json<OutcomeReport>,
) -> Result<Json<EvaluateOutcomeResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsEvaluateOutcome, e)
    })?;

    // Validate execution_id matches report
    if report.execution_id != execution_id {
        return governance_err!(
            state,
            GovernanceRoute::ExecutionsEvaluateOutcome,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "execution_id in path does not match report",
            )
        );
    }

    // Look up execution to get intent_id
    let execution = state
        .runtime
        .store
        .executions()
        .get(execution_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                ),
            )
        })?;

    // Look up intent
    let intent = state
        .runtime
        .store
        .intents()
        .get(execution.intent_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "intent not found for execution",
                ),
            )
        })?;

    let response = state
        .runtime
        .pdp
        .evaluate_outcome(&intent, &report)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ExecutionsEvaluateOutcome,
                ApiProblem::internal(e),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ExecutionsEvaluateOutcome,
        Ok(Json(response))
    )
}

async fn get_execution_lineage(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<LineageResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ProvenanceLineageExecutionId, e)
    })?;

    let request = ProvenanceQueryRequest {
        intent_id: None,
        execution_id: Some(execution_id),
        capability_id: None,
        event_kind: None,
        since: None,
        until: None,
        edge_types: Vec::new(),
    };

    let events = state
        .runtime
        .store
        .provenance()
        .query(&request)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineageExecutionId,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Build a map of target_event_id -> edges for efficient parent edge lookup
    let mut edges_by_target: HashMap<String, Vec<ferrum_proto::ProvenanceEdge>> = HashMap::new();
    for event in &events {
        let edges = state
            .runtime
            .store
            .provenance()
            .get_edges_to(event.event_id)
            .await
            .map_err(|e| {
                state.metrics.record_governance_error(
                    GovernanceRoute::ProvenanceLineageExecutionId,
                    ApiProblem::internal(anyhow::Error::from(e)),
                )
            })?;
        edges_by_target.insert(event.event_id.to_string(), edges);
    }

    let mut graph = LineageGraph::default();
    for event in &events {
        graph.add_event(event.clone());
    }
    for (child_id, edges) in &edges_by_target {
        for edge in edges {
            let parent_id = edge.from_event_id.to_string();
            let child_id = child_id.to_string();
            graph.add_edge(&parent_id, &child_id);
        }
    }

    let response = LineageResponse {
        execution_id,
        events,
    };
    // I11: sanitize response to strip control characters from string fields
    let json_val = serde_json::to_value(&response).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ProvenanceLineageExecutionId,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: LineageResponse = serde_json::from_value(sanitized).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ProvenanceLineageExecutionId,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    governance_ok!(
        state,
        GovernanceRoute::ProvenanceLineageExecutionId,
        Ok(Json(sanitized_response))
    )
}

/// Multi-hop lineage query from a seed event_id.
/// Traverses ancestor and/or descendant edges up to max_hops depth.
async fn query_lineage(
    State(state): State<Arc<AppState>>,
    Json(request): Json<LineageQueryRequest>,
) -> Result<Json<LineageQueryResponse>, ApiProblem> {
    let max_hops = request.max_hops.clamp(1, 10);

    // Fetch the seed event
    let seed_event = state
        .runtime
        .store
        .provenance()
        .get_event(request.event_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineage,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineage,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "seed event not found",
                ),
            )
        })?;

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    visited.insert(request.event_id.to_string());

    let mut events: Vec<ferrum_proto::ProvenanceEvent> = vec![seed_event];
    let mut edges: Vec<ferrum_proto::ProvenanceEdge> = Vec::new();

    // BFS traversal
    let mut frontier: Vec<String> = vec![request.event_id.to_string()];
    let mut next_frontier: Vec<String> = Vec::new();

    for _hop in 0..max_hops {
        if frontier.is_empty() {
            break;
        }

        for event_id_str in &frontier {
            let event_id: EventId =
                event_id_str
                    .parse::<uuid::Uuid>()
                    .map(EventId)
                    .map_err(|_| {
                        state.metrics.record_governance_error(
                            GovernanceRoute::ProvenanceLineage,
                            ApiProblem::new(
                                StatusCode::BAD_REQUEST,
                                ApiErrorCode::ValidationError,
                                "invalid event_id format: must be a valid UUID",
                            ),
                        )
                    })?;

            // Ancestor traversal: get_edges_to returns parent edges (from_event_id is parent)
            if matches!(
                request.direction,
                LineageDirection::Ancestors | LineageDirection::Both
            ) {
                let parent_edges = state
                    .runtime
                    .store
                    .provenance()
                    .get_edges_to(event_id)
                    .await
                    .map_err(|e| {
                        state.metrics.record_governance_error(
                            GovernanceRoute::ProvenanceLineage,
                            ApiProblem::internal(anyhow::Error::from(e)),
                        )
                    })?;

                for edge in &parent_edges {
                    if visited.insert(edge.from_event_id.to_string()) {
                        // Fetch the parent event
                        if let Some(parent_event) = state
                            .runtime
                            .store
                            .provenance()
                            .get_event(edge.from_event_id)
                            .await
                            .map_err(|e| {
                                state.metrics.record_governance_error(
                                    GovernanceRoute::ProvenanceLineage,
                                    ApiProblem::internal(anyhow::Error::from(e)),
                                )
                            })?
                        {
                            events.push(parent_event);
                            next_frontier.push(edge.from_event_id.to_string());
                        }
                    }
                    edges.push(edge.clone());
                }
            }

            // Descendant traversal: get_edges_from returns child edges (to_event_id is child)
            if matches!(
                request.direction,
                LineageDirection::Descendants | LineageDirection::Both
            ) {
                let child_edges = state
                    .runtime
                    .store
                    .provenance()
                    .get_edges_from(&[event_id])
                    .await
                    .map_err(|e| {
                        state.metrics.record_governance_error(
                            GovernanceRoute::ProvenanceLineage,
                            ApiProblem::internal(anyhow::Error::from(e)),
                        )
                    })?;

                for edge in &child_edges {
                    if let Some(to_id) = &edge.to_event_id {
                        if visited.insert(to_id.to_string()) {
                            // Fetch the child event
                            if let Some(child_event) = state
                                .runtime
                                .store
                                .provenance()
                                .get_event(*to_id)
                                .await
                                .map_err(|e| {
                                    state.metrics.record_governance_error(
                                        GovernanceRoute::ProvenanceLineage,
                                        ApiProblem::internal(anyhow::Error::from(e)),
                                    )
                                })?
                            {
                                events.push(child_event);
                                next_frontier.push(to_id.to_string());
                            }
                        }
                    }
                    edges.push(edge.clone());
                }
            }
        }

        frontier = next_frontier;
        next_frontier = Vec::new();
    }

    let response = LineageQueryResponse { events, edges };
    // I11: sanitize response to strip control characters from string fields
    let json_val = serde_json::to_value(&response).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::ProvenanceLineage,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: LineageQueryResponse =
        serde_json::from_value(sanitized).map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceLineage,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    governance_ok!(
        state,
        GovernanceRoute::ProvenanceLineage,
        Ok(Json(sanitized_response))
    )
}

async fn get_execution(
    State(state): State<Arc<AppState>>,
    Path(execution_id): Path<String>,
) -> Result<Json<ExecutionDetailResponse>, ApiProblem> {
    let execution_id = parse_execution_id(&execution_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ExecutionsExecutionId, e)
    })?;
    let record = match state.runtime.store.executions().get(execution_id).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "execution not found",
                )
            );
        }
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };

    // Look up the rollback contract if present, for fs-first rollback inspection.
    // This enables operators to inspect contract state, target path, before_hash,
    // after_hash, compensation_plan, and verify_checks via GET /v1/executions/{id}.
    let rollback_contract = if let Some(contract_id) = record.rollback_contract_id {
        match state
            .runtime
            .store
            .rollback_contracts()
            .get(contract_id)
            .await
        {
            Ok(contract) => contract,
            Err(e) => {
                return governance_err!(
                    state,
                    GovernanceRoute::ExecutionsExecutionId,
                    ApiProblem::internal(anyhow::Error::from(e))
                );
            }
        }
    } else {
        None
    };

    let response = ExecutionDetailResponse {
        execution: record,
        rollback_contract,
    };
    // I11: sanitize response to strip control characters from string fields
    let json_val = match serde_json::to_value(&response) {
        Ok(val) => val,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: ExecutionDetailResponse = match serde_json::from_value(sanitized) {
        Ok(resp) => resp,
        Err(e) => {
            return governance_err!(
                state,
                GovernanceRoute::ExecutionsExecutionId,
                ApiProblem::internal(anyhow::Error::from(e))
            );
        }
    };
    governance_ok!(
        state,
        GovernanceRoute::ExecutionsExecutionId,
        Ok(Json(sanitized_response))
    )
}

async fn list_approvals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ApprovalListEnvelope>, ApiProblem> {
    let limit = params.limit().map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::Approvals, e)
    })?;

    // Determine whether to use cursor-based or offset-based pagination
    let (items, next_cursor) = if let Some(ref cursor) = params.cursor {
        // Cursor-based pagination path
        let (created_at, approval_id) = decode_cursor(cursor).map_err(|e| {
            state
                .metrics
                .record_governance_error(GovernanceRoute::Approvals, e)
        })?;
        let limit_plus_one = limit + 1; // Fetch one extra to determine if there are more

        let approvals = if let Some(ref proposal_id) = params.proposal_id {
            // Validate proposal_id format - fail closed on invalid UUID
            let parsed_proposal_id = parse_proposal_id(proposal_id).map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::Approvals, e)
            })?;
            state
                .runtime
                .store
                .approvals()
                .list_pending_by_proposal_cursor(
                    parsed_proposal_id,
                    created_at,
                    approval_id,
                    limit_plus_one,
                )
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        } else {
            state
                .runtime
                .store
                .approvals()
                .list_pending_cursor(created_at, approval_id, limit_plus_one)
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        };

        // Determine if there are more results
        let has_more = approvals.len() > limit as usize;
        let items: Vec<_> = approvals.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            items
                .last()
                .map(|a| encode_cursor(a.created_at, a.approval_id))
        } else {
            None
        };
        (items, next_cursor)
    } else {
        // Offset-based pagination path (for backwards compatibility)
        let offset = params.offset();
        let approvals = if let Some(ref proposal_id) = params.proposal_id {
            // Validate proposal_id format - fail closed on invalid UUID
            let parsed_proposal_id = parse_proposal_id(proposal_id).map_err(|e| {
                state
                    .metrics
                    .record_governance_error(GovernanceRoute::Approvals, e)
            })?;
            state
                .runtime
                .store
                .approvals()
                .list_pending_by_proposal_paginated(parsed_proposal_id, limit, offset)
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        } else {
            state
                .runtime
                .store
                .approvals()
                .list_pending_paginated(limit, offset)
                .await
                .map_err(|e| {
                    state.metrics.record_governance_error(
                        GovernanceRoute::Approvals,
                        ApiProblem::internal(anyhow::Error::from(e)),
                    )
                })?
        };
        // Offset pagination cannot determine next_cursor reliably, so we return None
        (approvals, None)
    };

    governance_ok!(
        state,
        GovernanceRoute::Approvals,
        Ok(Json(ApprovalListEnvelope { items, next_cursor }))
    )
}

async fn get_approval(
    State(state): State<Arc<AppState>>,
    Path(approval_id): Path<String>,
) -> Result<Json<ferrum_proto::ApprovalRequest>, ApiProblem> {
    let approval_id = parse_approval_id(&approval_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ApprovalsApprovalId, e)
    })?;
    let approval = state
        .runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsApprovalId,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsApprovalId,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "approval not found",
                ),
            )
        })?;
    governance_ok!(
        state,
        GovernanceRoute::ApprovalsApprovalId,
        Ok(Json(approval))
    )
}

async fn resolve_approval(
    State(state): State<Arc<AppState>>,
    Path(approval_id): Path<String>,
    Json(request): Json<ApprovalResolveRequest>,
) -> Result<Json<ferrum_proto::ApprovalRequest>, ApiProblem> {
    let approval_id = parse_approval_id(&approval_id).map_err(|e| {
        state
            .metrics
            .record_governance_error(GovernanceRoute::ApprovalsResolve, e)
    })?;

    // Fetch the approval from the store
    let approval = state
        .runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "approval not found",
                ),
            )
        })?;

    // Check if approval is already terminal
    if !matches!(approval.state, ApprovalState::Pending) {
        return governance_err!(
            state,
            GovernanceRoute::ApprovalsResolve,
            ApiProblem::new(
                StatusCode::CONFLICT,
                ApiErrorCode::Conflict,
                format!(
                    "approval is in terminal state {:?}, cannot resolve",
                    approval.state
                ),
            )
        );
    }

    // Check if approval has expired
    if approval.expires_at < Utc::now() {
        return governance_err!(
            state,
            GovernanceRoute::ApprovalsResolve,
            ApiProblem::new(
                StatusCode::FORBIDDEN,
                ApiErrorCode::PolicyDenied,
                "approval has expired, cannot resolve"
            )
        );
    }

    // Map approve to target state
    let target_state = if request.approve {
        ApprovalState::Granted
    } else {
        ApprovalState::Denied
    };

    // Call store to resolve the approval (validates transition)
    state
        .runtime
        .store
        .approvals()
        .resolve(approval_id, target_state.clone())
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    // Fetch the updated approval
    let updated_approval = state
        .runtime
        .store
        .approvals()
        .get(approval_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    "approval not found after resolve",
                ),
            )
        })?;

    // Emit gateway-owned provenance event
    let event_kind = if request.approve {
        ProvenanceEventKind::ApprovalGranted
    } else {
        ProvenanceEventKind::ApprovalDenied
    };
    let event_kind_for_summary = event_kind.clone();

    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert(
        "actor_id".to_string(),
        serde_json::json!(request.actor.actor_id),
    );
    if let Some(reason) = &request.reason {
        metadata.insert("reason".to_string(), serde_json::json!(reason));
    }

    let provenance_event = ProvenanceEvent {
        event_id: EventId::new(),
        kind: event_kind,
        occurred_at: Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: "ferrum-gateway".to_string(),
            display_name: Some("FerrumGate Gateway".to_string()),
        },
        object: ObjectRef {
            object_type: ObjectType::Approval,
            object_id: approval_id.to_string(),
            summary: Some(format!(
                "Approval {:?} for proposal",
                event_kind_for_summary
            )),
        },
        intent_id: Some(approval.intent_id),
        proposal_id: Some(approval.proposal_id),
        execution_id: approval.execution_id,
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
        metadata,
        source_runtime_id: None,
    };

    state
        .runtime
        .store
        .provenance()
        .append_event(&provenance_event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ApprovalsResolve,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ApprovalsResolve,
        Ok(Json(updated_approval))
    )
}

async fn query_provenance(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ProvenanceQueryRequest>,
) -> Result<Json<ProvenanceQueryResponse>, ApiProblem> {
    let events = state
        .runtime
        .store
        .provenance()
        .query(&request)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceQuery,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    governance_ok!(
        state,
        GovernanceRoute::ProvenanceQuery,
        Ok(Json(ProvenanceQueryResponse { events }))
    )
}

async fn ingest_provenance(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ProvenanceIngestRequest>,
) -> Result<Json<ProvenanceIngestResponse>, ApiProblem> {
    // Validate source_runtime_id against registered bridges - fail closed
    let bridge = state
        .runtime
        .bridges
        .iter()
        .find(|b| b.runtime_id() == request.source_runtime_id);

    if bridge.is_none() {
        return governance_err!(
            state,
            GovernanceRoute::ProvenanceIngest,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("unknown source_runtime_id: {}", request.source_runtime_id),
            )
        );
    }

    // Build ProvenanceEvent from request
    let event_id = EventId::new();
    let event = ProvenanceEvent {
        event_id,
        kind: request.kind,
        occurred_at: chrono::Utc::now(),
        actor: ActorRef {
            actor_type: ActorType::Gateway,
            actor_id: request.source_runtime_id.clone(),
            display_name: None,
        },
        object: ObjectRef {
            object_type: ObjectType::ProvenanceEvent,
            object_id: request.source_runtime_id.clone(),
            summary: Some(request.description.clone()),
        },
        intent_id: request.intent_id,
        proposal_id: None,
        execution_id: request.execution_id,
        capability_id: None,
        rollback_contract_id: None,
        policy_bundle_id: None,
        trust_labels: request.trust_labels,
        sensitivity_labels: request.sensitivity_labels,
        parent_edges: Vec::new(),
        hash_chain: HashChainRef {
            content_hash: None,
            manifest_hash: None,
            policy_bundle_hash: None,
            previous_ledger_hash: None,
        },
        metadata: request.metadata,
        source_runtime_id: Some(request.source_runtime_id),
    };

    // Persist - FAIL CLOSED on store errors
    state
        .runtime
        .store
        .provenance()
        .append_event(&event)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::ProvenanceIngest,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::ProvenanceIngest,
        Ok(Json(ProvenanceIngestResponse {
            event_id,
            linked: true,
        }))
    )
}

async fn list_bridges(State(state): State<Arc<AppState>>) -> Json<BridgeListResponse> {
    let bridges: Vec<BridgeInfo> = state
        .runtime
        .bridges
        .iter()
        .map(|b| BridgeInfo {
            runtime_id: b.runtime_id().to_string(),
            connected: b.is_connected(),
        })
        .collect();
    Json(BridgeListResponse { bridges })
}

async fn list_bridge_tools(
    State(state): State<Arc<AppState>>,
    Path(bridge_id): Path<String>,
) -> Result<Json<BridgeToolsResponse>, ApiProblem> {
    let bridge = state
        .runtime
        .bridges
        .iter()
        .find(|b| b.runtime_id() == bridge_id)
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::BridgesBridgeIdTools,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    format!("bridge not found: {}", bridge_id),
                ),
            )
        })?;

    if !bridge.is_connected() {
        return governance_err!(
            state,
            GovernanceRoute::BridgesBridgeIdTools,
            ApiProblem::new(
                StatusCode::SERVICE_UNAVAILABLE,
                ApiErrorCode::Internal,
                format!("bridge '{}' is not connected", bridge_id),
            )
        );
    }

    let tools = bridge.list_tools().await.map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::BridgesBridgeIdTools,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;

    let response = BridgeToolsResponse {
        runtime_id: bridge_id,
        tools,
    };
    // I11: sanitize response to strip control characters from string fields
    let json_val = serde_json::to_value(&response).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::BridgesBridgeIdTools,
            ApiProblem::internal(anyhow::Error::from(e)),
        )
    })?;
    let sanitized = sanitize_json(&state.runtime.firewall, json_val);
    let sanitized_response: BridgeToolsResponse =
        serde_json::from_value(sanitized).map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::BridgesBridgeIdTools,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;
    governance_ok!(
        state,
        GovernanceRoute::BridgesBridgeIdTools,
        Ok(Json(sanitized_response))
    )
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct LineageResponse {
    execution_id: ExecutionId,
    events: Vec<ferrum_proto::ProvenanceEvent>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BridgeInfo {
    runtime_id: String,
    connected: bool,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BridgeListResponse {
    bridges: Vec<BridgeInfo>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BridgeToolsResponse {
    runtime_id: String,
    tools: Vec<BridgeToolInfo>,
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

/// Validates that `resource_bindings` is a subset of `resource_scope`.
///
/// Returns `Ok(())` if all capability resource bindings are within the intent's
/// resource scope, `Err(reason)` if any binding exceeds the scope.
///
/// Uses conservative prefix semantics: a binding path/uri is within scope if it
/// starts with any matching scope entry's path/uri prefix. For example:
/// - binding path `/tmp/subdir/file.txt` is within scope path `/tmp` ✓
/// - binding path `/other/file.txt` is NOT within scope path `/tmp` ✗
///
/// An empty `resource_bindings` is always valid (represents no specific resources).
/// An empty `resource_scope` with non-empty `resource_bindings` is always invalid.
fn validate_resource_bindings_subset_of_scope(
    resource_bindings: &[ferrum_proto::ResourceBinding],
    resource_scope: &[ResourceSelector],
) -> Result<(), String> {
    // Empty bindings is always valid (no specific resources requested)
    if resource_bindings.is_empty() {
        return Ok(());
    }

    // Empty scope with non-empty bindings = invalid (cannot expand beyond empty scope)
    if resource_scope.is_empty() {
        return Err("resource scope is empty but capability has resource bindings".to_string());
    }

    for binding in resource_bindings {
        let covered = match binding {
            ferrum_proto::ResourceBinding::File { path, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::FilesystemPath {
                        path: scope_path, ..
                    } = scope
                    {
                        path.starts_with(scope_path)
                    } else {
                        false
                    }
                })
            }
            ferrum_proto::ResourceBinding::Git { repo_path, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::GitRepository {
                        repo_path: scope_repo_path,
                        ..
                    } = scope
                    {
                        repo_path.starts_with(scope_repo_path)
                    } else {
                        false
                    }
                })
            }
            ferrum_proto::ResourceBinding::Sqlite { db_path, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::SqliteDatabase {
                        db_path: scope_db_path,
                        ..
                    } = scope
                    {
                        db_path.starts_with(scope_db_path)
                    } else {
                        false
                    }
                })
            }
            ferrum_proto::ResourceBinding::Http {
                base_url,
                path_prefix,
                ..
            } => resource_scope.iter().any(|scope| {
                if let ResourceSelector::HttpEndpoint {
                    base_url: scope_base_url,
                    path_prefix: scope_path_prefix,
                    ..
                } = scope
                {
                    base_url.starts_with(scope_base_url)
                        && path_prefix.starts_with(scope_path_prefix)
                } else {
                    false
                }
            }),
            ferrum_proto::ResourceBinding::EmailDraft { recipients, .. } => {
                resource_scope.iter().any(|scope| {
                    if let ResourceSelector::EmailDraft {
                        recipient_allowlist,
                        ..
                    } = scope
                    {
                        // Email matching: recipient must end with an allowlist entry.
                        // E.g., "user@example.com" ends with "@example.com" ✓
                        recipients
                            .iter()
                            .all(|r| recipient_allowlist.iter().any(|a| r.ends_with(a)))
                    } else {
                        false
                    }
                })
            }
        };

        if !covered {
            return Err(format!(
                "capability resource binding {:?} is not within intent resource scope",
                binding
            ));
        }
    }

    Ok(())
}

/// Infers the action_type and adapter_key from the tool_name.
/// For FileWrite-related tools (containing "file_write", "write_file", "fs_", etc.),
/// returns ActionType::FileWrite and adapter_key "fs".
/// For sql_mutate, returns ActionType::SqlMutation and adapter_key "sqlite".
/// For maildraft/draft-create/email_draft tools, returns ActionType::MailDraft and adapter_key "maildraft".
/// For git_branch_create, returns ActionType::GitBranchCreate and adapter_key "git".
/// For git_tag_create, returns ActionType::GitTagCreate and adapter_key "git".
/// For git_branch_delete, returns ActionType::GitBranchDelete and adapter_key "git".
/// For git_tag_delete, returns ActionType::GitTagDelete and adapter_key "git".
/// For git_push, returns ActionType::GitPush and adapter_key "git".
/// For git_pull, returns ActionType::GitPull and adapter_key "git".
/// For git_fetch, returns ActionType::GitFetch and adapter_key "git".
/// Otherwise, defaults to ActionType::McpToolMutation and adapter_key "noop".
fn infer_action_type_and_adapter(tool_name: &str) -> (ferrum_proto::ActionType, String) {
    let tool_lower = tool_name.to_lowercase();
    if tool_lower.contains("file_write")
        || tool_lower.contains("write_file")
        || tool_lower.contains("fs_")
        || tool_lower.contains("file-mutation")
    {
        (ferrum_proto::ActionType::FileWrite, "fs".to_string())
    } else if tool_lower.contains("sql_mutate") {
        (ferrum_proto::ActionType::SqlMutation, "sqlite".to_string())
    } else if tool_lower.contains("maildraft")
        || tool_lower.contains("draft_create")
        || tool_lower.contains("email_draft")
    {
        (ferrum_proto::ActionType::MailDraft, "maildraft".to_string())
    } else if tool_lower.contains("git_branch_create") {
        (ferrum_proto::ActionType::GitBranchCreate, "git".to_string())
    } else if tool_lower.contains("git_tag_create") {
        (ferrum_proto::ActionType::GitTagCreate, "git".to_string())
    } else if tool_lower.contains("git_branch_delete") {
        (ferrum_proto::ActionType::GitBranchDelete, "git".to_string())
    } else if tool_lower.contains("git_tag_delete") {
        (ferrum_proto::ActionType::GitTagDelete, "git".to_string())
    } else if tool_lower.contains("git_push") {
        (ferrum_proto::ActionType::GitPush, "git".to_string())
    } else if tool_lower.contains("git_pull") {
        (ferrum_proto::ActionType::GitPull, "git".to_string())
    } else if tool_lower.contains("git_fetch") {
        (ferrum_proto::ActionType::GitFetch, "git".to_string())
    } else if tool_lower.contains("http_post")
        || tool_lower.contains("http_put")
        || tool_lower.contains("http_patch")
        || tool_lower.contains("http_delete")
    {
        (ferrum_proto::ActionType::HttpMutation, "http".to_string())
    } else {
        (
            ferrum_proto::ActionType::McpToolMutation,
            "noop".to_string(),
        )
    }
}

/// Builds a RollbackPrepareRequest with adapter_key inferred from tool_name.
/// This allows the gateway to select the appropriate adapter based on the proposal's tool.
fn build_prepare_request_for_proposal(
    rollback: &RollbackService,
    intent_id: ferrum_proto::IntentId,
    proposal_id: ferrum_proto::ProposalId,
    execution_id: ExecutionId,
    rollback_class: &RollbackClass,
    tool_name: &str,
    resource_scope: &[ferrum_proto::ResourceSelector],
    raw_arguments: &serde_json::Value,
) -> ferrum_proto::RollbackPrepareRequest {
    let (action_type, adapter_key) = infer_action_type_and_adapter(tool_name);
    let target = infer_target_from_scope(resource_scope, &action_type);
    let mut request = rollback.build_prepare_request_with_target(
        intent_id,
        proposal_id,
        execution_id,
        rollback_class.clone(),
        action_type,
        adapter_key,
        target,
    );

    // Merge proposal raw_arguments into metadata for git tools so prepare can
    // validate branch_name/remote_name during prepare (fail-closed).
    if let Some(args) = raw_arguments.as_object() {
        match request.action_type {
            ferrum_proto::ActionType::GitBranchCreate => {
                if let Some(branch) = args.get("branch").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("branch_name".to_string(), serde_json::json!(branch));
                }
            }
            ferrum_proto::ActionType::GitPush
            | ferrum_proto::ActionType::GitPull
            | ferrum_proto::ActionType::GitFetch => {
                if let Some(refspec) = args.get("refspec").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("branch_name".to_string(), serde_json::json!(refspec));
                }
                if let Some(remote) = args.get("remote").and_then(|v| v.as_str()) {
                    request
                        .metadata
                        .insert("remote_name".to_string(), serde_json::json!(remote));
                }
            }
            _ => {}
        }
    }

    request
}

/// If the contract has an HTTP placeholder compensation plan (only url present),
/// enrich it with method, payload, and expected_statuses from contract target
/// and metadata so that http.replay_v1 validation succeeds.
/// Fails closed by leaving the contract unchanged when required data is missing.
fn enrich_http_compensation_if_needed(
    mut contract: ferrum_proto::RollbackContract,
) -> ferrum_proto::RollbackContract {
    if contract.adapter_key != "http" || contract.compensation_plan.len() != 1 {
        return contract;
    }
    let step = &contract.compensation_plan[0];
    if step.operation != "http.replay_v1" || step.args.contains_key("method") {
        return contract;
    }

    let method = match &contract.target {
        ferrum_proto::RollbackTarget::HttpRequest { method, .. } => format!("{:?}", method),
        _ => return contract,
    };

    let payload = contract
        .metadata
        .get("execute_payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let expected_statuses: Vec<u16> = contract
        .metadata
        .get("response_status")
        .and_then(|v| v.as_u64())
        .map(|s| vec![s as u16])
        .unwrap_or_else(|| vec![200]);

    let enriched_step = ferrum_proto::CompensationStep {
        order: step.order,
        adapter_key: step.adapter_key.clone(),
        operation: step.operation.clone(),
        idempotency_key: step.idempotency_key.clone(),
        args: {
            let mut args = step.args.clone();
            args.insert("method".to_string(), serde_json::json!(method));
            args.insert("payload".to_string(), payload);
            args.insert(
                "expected_statuses".to_string(),
                serde_json::json!(expected_statuses),
            );
            args
        },
    };

    contract.compensation_plan = vec![enriched_step];
    contract
}

/// Infers the RollbackTarget from resource_scope.
/// For FilesystemPath selectors, returns RollbackTarget::FilePath with the path.
/// For SqliteDatabase selectors with SqlMutation action, returns RollbackTarget::SqliteTxn.
/// For other selectors, returns Generic fallback.
fn infer_target_from_scope(
    scope: &[ferrum_proto::ResourceSelector],
    action_type: &ferrum_proto::ActionType,
) -> RollbackTarget {
    // Only use FilePath target for file-related action types
    let is_file_action = matches!(
        action_type,
        ferrum_proto::ActionType::FileWrite
            | ferrum_proto::ActionType::FileDelete
            | ferrum_proto::ActionType::FileMove
            | ferrum_proto::ActionType::FileCopy
            | ferrum_proto::ActionType::FileAppend
            | ferrum_proto::ActionType::FileChmod
    );

    if is_file_action {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::FilesystemPath {
                path,
                mode: _,
                content_hash: _,
            } = selector
            {
                return RollbackTarget::FilePath {
                    path: path.clone(),
                    before_hash: None,
                    after_hash: None,
                };
            }
        }
    }

    // SqliteDatabase selector for SqlMutation action type
    if matches!(action_type, ferrum_proto::ActionType::SqlMutation) {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::SqliteDatabase {
                db_path,
                tables: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::SqliteTxn {
                    db_path: db_path.clone(),
                    tx_id: format!("tx-{}", uuid::Uuid::new_v4()),
                };
            }
        }
    }

    // EmailDraft selector for MailDraft action type
    if matches!(action_type, ferrum_proto::ActionType::MailDraft) {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::EmailDraft {
                recipient_allowlist,
                subject_prefix_allowlist: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::EmailDraft {
                    draft_id: None, // draft_id is set at runtime by execute
                    recipients: recipient_allowlist.clone(),
                };
            }
        }
    }

    // GitRepository selector for git action types (GitBranchCreate, GitTagCreate, etc.)
    let is_git_action = matches!(
        action_type,
        ferrum_proto::ActionType::GitBranchCreate
            | ferrum_proto::ActionType::GitTagCreate
            | ferrum_proto::ActionType::GitBranchDelete
            | ferrum_proto::ActionType::GitTagDelete
            | ferrum_proto::ActionType::GitPush
            | ferrum_proto::ActionType::GitPull
            | ferrum_proto::ActionType::GitFetch
            | ferrum_proto::ActionType::GitCommit
    );

    if is_git_action {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::GitRepository {
                repo_path,
                allowed_refs: _,
                mode: _,
            } = selector
            {
                return RollbackTarget::GitRef {
                    repo_path: repo_path.clone(),
                    before_ref: None,
                    after_ref: None,
                };
            }
        }
    }

    // HttpEndpoint selector for HttpMutation action type
    if matches!(action_type, ferrum_proto::ActionType::HttpMutation) {
        for selector in scope {
            if let ferrum_proto::ResourceSelector::HttpEndpoint {
                method,
                base_url,
                path_prefix,
                mode: _,
            } = selector
            {
                let url = if path_prefix.starts_with('/') {
                    format!("{}{}", base_url, path_prefix)
                } else {
                    format!("{}/{}", base_url, path_prefix)
                };
                return RollbackTarget::HttpRequest {
                    method: method.clone(),
                    url,
                    request_digest: String::new(),
                };
            }
        }
    }

    // Default fallback
    RollbackTarget::Generic {
        namespace: "mcp".to_string(),
        identifier: "tool-call".to_string(),
    }
}

/// Determines the appropriate adapter key for git-related resource selectors.
/// Returns "git" for GitRepository selectors, otherwise "noop".
pub fn infer_git_adapter_key(scope: &[ResourceSelector]) -> &'static str {
    if scope
        .iter()
        .any(|selector| matches!(selector, ResourceSelector::GitRepository { .. }))
    {
        "git"
    } else {
        "noop"
    }
}

/// Determines the rollback target from resource selectors.
/// For GitRepository selectors, returns RollbackTarget::GitRef with repo_path.
/// For other selectors, returns Generic fallback.
pub fn determine_rollback_target_from_bindings(scope: &[ResourceSelector]) -> RollbackTarget {
    for selector in scope {
        if let ResourceSelector::GitRepository {
            repo_path,
            allowed_refs: _,
            mode: _,
        } = selector
        {
            return RollbackTarget::GitRef {
                repo_path: repo_path.clone(),
                before_ref: None,
                after_ref: None,
            };
        }
    }
    // Default fallback for unspecified bindings
    RollbackTarget::Generic {
        namespace: "unknown".to_string(),
        identifier: "binding".to_string(),
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

// ---------------------------------------------------------------------------
// Firewall taint derivation helpers
// ---------------------------------------------------------------------------

/// Returns true if the intent's trust context contains any external/trusted-label.
fn intent_has_external_label(intent: &IntentEnvelope) -> bool {
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
fn proposal_has_external_metadata(proposal: &ferrum_proto::ActionProposal) -> bool {
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
fn has_tool_output_label(intent: &IntentEnvelope) -> bool {
    intent
        .trust_context
        .input_labels
        .contains(&ProtoTrustLabel::ExternalToolOutput)
}

/// Returns true if intent trust context has untrusted text labels.
fn has_untrusted_text_label(intent: &IntentEnvelope) -> bool {
    intent
        .trust_context
        .input_labels
        .contains(&ProtoTrustLabel::Untrusted)
}

/// Builds a FirewallContext from intent and proposal for taint scoring.
fn build_firewall_context(
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

fn parse_proposal_id(value: &str) -> Result<ferrum_proto::ProposalId, ApiProblem> {
    let parsed = value.parse::<uuid::Uuid>().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "proposal_id is not a valid uuid",
        )
    })?;
    Ok(ferrum_proto::ProposalId(parsed))
}

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Deserialize)]
struct PaginationParams {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    proposal_id: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

impl PaginationParams {
    fn limit(&self) -> Result<u32, ApiProblem> {
        match self.limit {
            Some(l) if l > MAX_LIMIT => Err(ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("limit exceeds maximum of {}", MAX_LIMIT),
            )),
            Some(l) => Ok(l),
            None => Ok(DEFAULT_LIMIT),
        }
    }

    fn offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

/// Cursor encoding for stable DESC ordering.
/// The cursor encodes (created_at_rfc3339, approval_id) to allow keyset pagination.
fn encode_cursor(
    created_at: chrono::DateTime<chrono::Utc>,
    approval_id: ferrum_proto::ApprovalId,
) -> String {
    let cursor_data = format!("{}:{}", created_at.to_rfc3339(), approval_id);
    URL_SAFE_NO_PAD.encode(cursor_data.as_bytes())
}

/// Cursor decoding for keyset pagination.
/// Returns (created_at, approval_id) on success.
fn decode_cursor(
    cursor: &str,
) -> Result<(chrono::DateTime<chrono::Utc>, ferrum_proto::ApprovalId), ApiProblem> {
    let decoded = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor format",
        )
    })?;
    let decoded_str = String::from_utf8(decoded).map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor encoding",
        )
    })?;
    let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor structure",
        ));
    }
    let created_at = chrono::DateTime::parse_from_rfc3339(parts[0])
        .map_err(|_| {
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                "invalid cursor timestamp",
            )
        })?
        .with_timezone(&chrono::Utc);
    let approval_id: uuid::Uuid = parts[1].parse().map_err(|_| {
        ApiProblem::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ValidationError,
            "invalid cursor approval_id",
        )
    })?;
    Ok((created_at, ferrum_proto::ApprovalId(approval_id)))
}

// ---------------------------------------------------------------------------
// Policy Bundle handlers
// ---------------------------------------------------------------------------

use ferrum_proto::parse_policy_bundle_yaml;

async fn create_policy_bundle(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ferrum_proto::CreatePolicyBundleRequest>,
) -> Result<Json<ferrum_proto::PolicyBundleResponse>, ApiProblem> {
    // Parse and validate the YAML
    let bundle = parse_policy_bundle_yaml(&request.yaml_content).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::PolicyBundlesCreate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("invalid policy bundle YAML: {}", e),
            ),
        )
    })?;

    let content_hash = bundle.content_hash.clone().unwrap_or_default();

    // Check for idempotency: if a bundle with the same content hash exists, return it
    if let Ok(Some(existing)) = state
        .runtime
        .store
        .policy_bundles()
        .get_by_content_hash(&content_hash)
        .await
    {
        return governance_ok!(
            state,
            GovernanceRoute::PolicyBundlesCreate,
            Ok(Json(ferrum_proto::PolicyBundleResponse {
                bundle: existing,
                content_hash,
            }))
        );
    }

    // Insert the new bundle
    state
        .runtime
        .store
        .policy_bundles()
        .insert(&bundle)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesCreate,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesCreate,
        Ok(Json(ferrum_proto::PolicyBundleResponse {
            bundle,
            content_hash,
        }))
    )
}

async fn list_policy_bundles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ferrum_proto::PolicyBundleListResponse>, ApiProblem> {
    let bundles = state
        .runtime
        .store
        .policy_bundles()
        .list()
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesList,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    let total = bundles.len();
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesList,
        Ok(Json(ferrum_proto::PolicyBundleListResponse {
            bundles,
            total,
        }))
    )
}

async fn get_policy_bundle(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
) -> Result<Json<ferrum_proto::PolicyBundleResponse>, ApiProblem> {
    let bundle = state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesGet,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesGet,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    format!("policy bundle '{}' not found", bundle_id),
                ),
            )
        })?;

    let content_hash = bundle.content_hash.clone().unwrap_or_default();
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesGet,
        Ok(Json(ferrum_proto::PolicyBundleResponse {
            bundle,
            content_hash,
        }))
    )
}

async fn update_policy_bundle(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
    Json(request): Json<ferrum_proto::UpdatePolicyBundleRequest>,
) -> Result<Json<ferrum_proto::PolicyBundleResponse>, ApiProblem> {
    // Parse and validate the YAML
    let mut bundle = parse_policy_bundle_yaml(&request.yaml_content).map_err(|e| {
        state.metrics.record_governance_error(
            GovernanceRoute::PolicyBundlesUpdate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!("invalid policy bundle YAML: {}", e),
            ),
        )
    })?;

    // Ensure the bundle_id matches the path
    if bundle.bundle_id != bundle_id {
        return governance_err!(
            state,
            GovernanceRoute::PolicyBundlesUpdate,
            ApiProblem::new(
                StatusCode::BAD_REQUEST,
                ApiErrorCode::ValidationError,
                format!(
                    "bundle_id in YAML ('{}') does not match path ('{}')",
                    bundle.bundle_id, bundle_id
                ),
            )
        );
    }

    // Get existing bundle to preserve created_at and check existence
    let existing = state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesUpdate,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesUpdate,
                ApiProblem::new(
                    StatusCode::NOT_FOUND,
                    ApiErrorCode::NotFound,
                    format!("policy bundle '{}' not found", bundle_id),
                ),
            )
        })?;

    // Preserve created_at and update updated_at
    bundle.created_at = existing.created_at;
    bundle.updated_at = chrono::Utc::now();

    // Recompute content hash
    let content_hash = bundle.compute_content_hash();
    bundle.content_hash = Some(content_hash.clone());

    state
        .runtime
        .store
        .policy_bundles()
        .update(&bundle)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesUpdate,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesUpdate,
        Ok(Json(ferrum_proto::PolicyBundleResponse {
            bundle,
            content_hash,
        }))
    )
}

async fn delete_policy_bundle(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    // Check if bundle exists
    let not_found = {
        let msg = format!("policy bundle '{}' not found", bundle_id);
        state.runtime.firewall.sanitize(&msg)
    };
    state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesDelete,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesDelete,
                ApiProblem::new(StatusCode::NOT_FOUND, ApiErrorCode::NotFound, not_found),
            )
        })?;

    state
        .runtime
        .store
        .policy_bundles()
        .delete(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesDelete,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    let response = serde_json::json!({ "ok": true, "bundle_id": bundle_id });
    let sanitized = sanitize_json(&state.runtime.firewall, response);
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesDelete,
        Ok(Json(sanitized))
    )
}

async fn set_policy_bundle_active(
    State(state): State<Arc<AppState>>,
    Path(bundle_id): Path<String>,
    Json(request): Json<ferrum_proto::SetPolicyBundleActiveRequest>,
) -> Result<Json<serde_json::Value>, ApiProblem> {
    // Check if bundle exists
    let not_found = {
        let msg = format!("policy bundle '{}' not found", bundle_id);
        state.runtime.firewall.sanitize(&msg)
    };
    state
        .runtime
        .store
        .policy_bundles()
        .get(&bundle_id)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesSetActive,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?
        .ok_or_else(|| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesSetActive,
                ApiProblem::new(StatusCode::NOT_FOUND, ApiErrorCode::NotFound, not_found),
            )
        })?;

    state
        .runtime
        .store
        .policy_bundles()
        .set_active(&bundle_id, request.active)
        .await
        .map_err(|e| {
            state.metrics.record_governance_error(
                GovernanceRoute::PolicyBundlesSetActive,
                ApiProblem::internal(anyhow::Error::from(e)),
            )
        })?;

    let response = serde_json::json!({
        "ok": true,
        "bundle_id": bundle_id,
        "active": request.active
    });
    let sanitized = sanitize_json(&state.runtime.firewall, response);
    governance_ok!(
        state,
        GovernanceRoute::PolicyBundlesSetActive,
        Ok(Json(sanitized))
    )
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
        let (status, code) = match err {
            ferrum_cap::CapabilityError::NotFound => {
                (StatusCode::NOT_FOUND, ApiErrorCode::NotFound)
            }
            ferrum_cap::CapabilityError::AlreadyUsed => {
                (StatusCode::CONFLICT, ApiErrorCode::Conflict)
            }
            ferrum_cap::CapabilityError::Revoked => {
                (StatusCode::BAD_REQUEST, ApiErrorCode::CapabilityRevoked)
            }
            ferrum_cap::CapabilityError::Expired => {
                (StatusCode::BAD_REQUEST, ApiErrorCode::CapabilityExpired)
            }
            ferrum_cap::CapabilityError::TtlTooLong => {
                (StatusCode::BAD_REQUEST, ApiErrorCode::ValidationError)
            }
        };
        Self::new(status, code, err.to_string())
    }
}

impl IntoResponse for ApiProblem {
    fn into_response(self) -> Response {
        (self.1, Json(self.0)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Policy bundle evaluation helpers
// ---------------------------------------------------------------------------

/// Load active policy bundles and evaluate their rules against the given context.
/// Returns `Some(EvaluateProposalResponse)` if a bundle rule matches, `None` otherwise.
async fn evaluate_active_policy_bundles(
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
fn evaluate_bundle_rules(
    bundle: &PolicyBundle,
    intent: &IntentEnvelope,
    proposal: &ferrum_proto::ActionProposal,
    trust: &TrustContextSummary,
) -> Option<EvaluateProposalResponse> {
    // Sort rules by descending priority
    let mut rules = bundle.rules.clone();
    rules.sort_by(|a, b| b.priority.cmp(&a.priority));

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
fn evaluate_rule_matchers(
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
fn evaluate_matcher(
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

/// Test helper: create a GatewayRuntime with an in-memory SQLite store.
///
/// Intended for integration tests that need a real gateway runtime.
/// Not for production use.
pub async fn test_runtime() -> GatewayRuntime {
    test_runtime_with_bridges(vec![]).await
}

/// Test helper: create a GatewayRuntime with an in-memory SQLite store
/// and the given runtime bridges.
///
/// Intended for integration tests that need a real gateway runtime.
/// Not for production use.
pub async fn test_runtime_with_bridges(bridges: Vec<Arc<dyn RuntimeBridge>>) -> GatewayRuntime {
    let pdp = Arc::new(StaticPdpEngine);
    let cap = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    store.apply_embedded_migrations().await.unwrap();

    GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, bridges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use ferrum_cap::InMemoryCapabilityService;
    use ferrum_pdp::StaticPdpEngine;
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::repos::{
        ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, PolicyBundleRepo,
        ProposalRepo, ProvenanceRepo, RollbackRepo,
    };
    use ferrum_store::{SqliteStore, StoreError, StoreFacade};
    use ferrum_sync::{BridgeToolInfo, ExternalEventSource, McpBridge};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// A test-only StoreFacade that wraps a real store but always fails health_check.
    /// Used to verify /v1/readyz/deep returns 503 when the store is unhealthy.
    struct UnhealthyTestStoreFacade {
        inner: Arc<dyn StoreFacade>,
    }

    impl UnhealthyTestStoreFacade {
        fn new(inner: Arc<dyn StoreFacade>) -> Self {
            Self { inner }
        }
    }

    #[async_trait::async_trait]
    impl StoreFacade for UnhealthyTestStoreFacade {
        fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
            self.inner.capabilities()
        }
        fn executions(&self) -> Arc<dyn ExecutionRepo> {
            self.inner.executions()
        }
        fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
            self.inner.rollback_contracts()
        }
        fn approvals(&self) -> Arc<dyn ApprovalRepo> {
            self.inner.approvals()
        }
        fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
            self.inner.provenance()
        }
        fn ledger(&self) -> Arc<dyn LedgerRepo> {
            self.inner.ledger()
        }
        fn intents(&self) -> Arc<dyn IntentRepo> {
            self.inner.intents()
        }
        fn proposals(&self) -> Arc<dyn ProposalRepo> {
            self.inner.proposals()
        }
        fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
            self.inner.policy_bundles()
        }
        fn write_queue_depth(&self) -> usize {
            self.inner.write_queue_depth()
        }
        async fn health_check(&self) -> Result<(), StoreError> {
            Err(StoreError::Other(
                "store unavailable for testing".to_string(),
            ))
        }
    }

    async fn test_runtime_with_unhealthy_store() -> GatewayRuntime {
        let pdp = Arc::new(StaticPdpEngine);
        let cap = Arc::new(InMemoryCapabilityService::default());

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();

        let unhealthy_store =
            Arc::new(UnhealthyTestStoreFacade::new(store)) as Arc<dyn StoreFacade>;

        GatewayRuntime::new(pdp, cap, rollback, unhealthy_store, vec![])
    }

    #[tokio::test]
    async fn test_healthz_is_public_under_bearer_auth() {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Bearer,
            bearer_token: Some("secret-token".to_string()),
            ..ServerConfig::default()
        };

        let response = build_router_with_auth(runtime, config)
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
    async fn test_protected_route_requires_bearer_auth() {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Bearer,
            bearer_token: Some("secret-token".to_string()),
            ..ServerConfig::default()
        };

        let response = build_router_with_auth(runtime, config)
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_protected_route_accepts_valid_bearer_auth() {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Bearer,
            bearer_token: Some("secret-token".to_string()),
            ..ServerConfig::default()
        };

        let response = build_router_with_auth(runtime, config)
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_readyz_deep_is_public_under_bearer_auth() {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Bearer,
            bearer_token: Some("secret-token".to_string()),
            ..ServerConfig::default()
        };

        let response = build_router_with_auth(runtime, config)
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should be 200 OK since store is healthy
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_readyz_deep_returns_200_when_store_healthy() {
        let runtime = test_runtime().await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let _deep: DeepHealthResponse = serde_json::from_slice(&body).unwrap();
        // DeepHealthResponse fields are verified via the JSON structure
    }

    #[tokio::test]
    async fn test_readyz_deep_returns_503_when_store_unhealthy() {
        let runtime = test_runtime_with_unhealthy_store().await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should return 503 Service Unavailable when store is unhealthy
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_readyz_deep_response_body_degraded_when_store_unhealthy() {
        let runtime = test_runtime_with_unhealthy_store().await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let deep: DeepHealthResponse = serde_json::from_slice(&body).unwrap();

        // Verify the response indicates degraded status
        assert_eq!(deep.status, "degraded");
        assert!(!deep.healthy);
        assert_eq!(deep.components.len(), 2);
        // First component: store (unhealthy)
        assert_eq!(deep.components[0].component, "store");
        assert!(!deep.components[0].healthy);
        assert!(deep.components[0].error.is_some());
        assert!(
            deep.components[0]
                .error
                .as_ref()
                .unwrap()
                .contains("store unavailable")
        );
        // Second component: write_queue (healthy since queue depth is 0 in test)
        assert_eq!(deep.components[1].component, "write_queue");
        assert!(deep.components[1].healthy);
        assert!(deep.components[1].error.is_none());
    }

    #[tokio::test]
    async fn test_readyz_deep_includes_write_queue_component_when_healthy() {
        let runtime = test_runtime().await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let deep: DeepHealthResponse = serde_json::from_slice(&body).unwrap();

        // Verify both components are present
        assert_eq!(deep.status, "ok");
        assert!(deep.healthy);
        assert_eq!(deep.components.len(), 2);

        // First component: store
        assert_eq!(deep.components[0].component, "store");
        assert!(deep.components[0].healthy);
        assert!(deep.components[0].error.is_none());

        // Second component: write_queue
        assert_eq!(deep.components[1].component, "write_queue");
        assert!(deep.components[1].healthy);
        assert!(deep.components[1].error.is_none());
        // Status text should include depth and threshold
        assert!(deep.components[1].status.contains("depth="));
        assert!(deep.components[1].status.contains("threshold="));
    }

    /// A test-only StoreFacade that wraps a real store but allows controlling queue depth.
    /// Used to verify /v1/readyz/deep returns 503 when queue depth exceeds threshold.
    struct HighQueueDepthStoreFacade {
        inner: Arc<dyn StoreFacade>,
        queue_depth: usize,
    }

    impl HighQueueDepthStoreFacade {
        fn new(inner: Arc<dyn StoreFacade>, queue_depth: usize) -> Self {
            Self { inner, queue_depth }
        }
    }

    #[async_trait::async_trait]
    impl StoreFacade for HighQueueDepthStoreFacade {
        fn capabilities(&self) -> Arc<dyn CapabilityRepo> {
            self.inner.capabilities()
        }
        fn executions(&self) -> Arc<dyn ExecutionRepo> {
            self.inner.executions()
        }
        fn rollback_contracts(&self) -> Arc<dyn RollbackRepo> {
            self.inner.rollback_contracts()
        }
        fn approvals(&self) -> Arc<dyn ApprovalRepo> {
            self.inner.approvals()
        }
        fn provenance(&self) -> Arc<dyn ProvenanceRepo> {
            self.inner.provenance()
        }
        fn ledger(&self) -> Arc<dyn LedgerRepo> {
            self.inner.ledger()
        }
        fn intents(&self) -> Arc<dyn IntentRepo> {
            self.inner.intents()
        }
        fn proposals(&self) -> Arc<dyn ProposalRepo> {
            self.inner.proposals()
        }
        fn policy_bundles(&self) -> Arc<dyn PolicyBundleRepo> {
            self.inner.policy_bundles()
        }
        fn write_queue_depth(&self) -> usize {
            self.queue_depth
        }
        async fn health_check(&self) -> Result<(), StoreError> {
            self.inner.health_check().await
        }
    }

    async fn test_runtime_with_high_queue_depth(queue_depth: usize) -> GatewayRuntime {
        let pdp = Arc::new(StaticPdpEngine);
        let cap = Arc::new(InMemoryCapabilityService::default());

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();

        let high_queue_store =
            Arc::new(HighQueueDepthStoreFacade::new(store, queue_depth)) as Arc<dyn StoreFacade>;

        GatewayRuntime::new(pdp, cap, rollback, high_queue_store, vec![])
    }

    #[tokio::test]
    async fn test_readyz_deep_returns_503_when_queue_depth_exceeds_threshold() {
        // Queue depth > 100 should make write_queue component unhealthy
        let runtime = test_runtime_with_high_queue_depth(101).await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should return 503 Service Unavailable when queue depth exceeds threshold
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let deep: DeepHealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(deep.status, "degraded");
        assert!(!deep.healthy);
        assert_eq!(deep.components.len(), 2);

        // First component: store (healthy)
        assert_eq!(deep.components[0].component, "store");
        assert!(deep.components[0].healthy);

        // Second component: write_queue (unhealthy due to high depth)
        assert_eq!(deep.components[1].component, "write_queue");
        assert!(!deep.components[1].healthy);
        assert!(deep.components[1].error.is_some());
        assert!(
            deep.components[1]
                .error
                .as_ref()
                .unwrap()
                .contains("exceeds threshold")
        );
    }

    #[test]
    fn test_infer_git_adapter_key_git_repository() {
        let scope = vec![ResourceSelector::GitRepository {
            repo_path: "/tmp/test-repo".to_string(),
            allowed_refs: vec!["main".to_string(), "develop".to_string()],
            mode: ferrum_proto::ResourceMode::ReadWrite,
        }];
        assert_eq!(infer_git_adapter_key(&scope), "git");
    }

    #[test]
    fn test_infer_git_adapter_key_no_git() {
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::ReadWrite,
            content_hash: None,
        }];
        assert_eq!(infer_git_adapter_key(&scope), "noop");
    }

    #[test]
    fn test_infer_git_adapter_key_empty_scope() {
        let scope: Vec<ResourceSelector> = vec![];
        assert_eq!(infer_git_adapter_key(&scope), "noop");
    }

    #[test]
    fn test_infer_git_adapter_key_mixed_scope() {
        let scope = vec![
            ResourceSelector::FilesystemPath {
                path: "/tmp/file.txt".to_string(),
                mode: ferrum_proto::ResourceMode::ReadWrite,
                content_hash: None,
            },
            ResourceSelector::GitRepository {
                repo_path: "/tmp/test-repo".to_string(),
                allowed_refs: vec!["main".to_string()],
                mode: ferrum_proto::ResourceMode::ReadWrite,
            },
        ];
        assert_eq!(infer_git_adapter_key(&scope), "git");
    }

    #[test]
    fn test_determine_rollback_target_from_bindings_git_ref() {
        let scope = vec![ResourceSelector::GitRepository {
            repo_path: "/opt/myrepo".to_string(),
            allowed_refs: vec!["main".to_string()],
            mode: ferrum_proto::ResourceMode::ReadWrite,
        }];
        let target = determine_rollback_target_from_bindings(&scope);
        match target {
            RollbackTarget::GitRef {
                repo_path,
                before_ref,
                after_ref,
            } => {
                assert_eq!(repo_path, "/opt/myrepo");
                assert!(before_ref.is_none());
                assert!(after_ref.is_none());
            }
            other => panic!("expected GitRef target, got {:?}", other),
        }
    }

    #[test]
    fn test_determine_rollback_target_from_bindings_generic_fallback() {
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::ReadWrite,
            content_hash: None,
        }];
        let target = determine_rollback_target_from_bindings(&scope);
        match target {
            RollbackTarget::Generic {
                namespace,
                identifier,
            } => {
                assert_eq!(namespace, "unknown");
                assert_eq!(identifier, "binding");
            }
            other => panic!("expected Generic fallback, got {:?}", other),
        }
    }

    #[test]
    fn test_determine_rollback_target_from_bindings_empty_scope() {
        let scope: Vec<ResourceSelector> = vec![];
        let target = determine_rollback_target_from_bindings(&scope);
        match target {
            RollbackTarget::Generic {
                namespace,
                identifier,
            } => {
                assert_eq!(namespace, "unknown");
                assert_eq!(identifier, "binding");
            }
            other => panic!("expected Generic fallback, got {:?}", other),
        }
    }

    #[test]
    fn test_determine_rollback_target_from_bindings_first_git_wins() {
        // When multiple git repos are in scope, returns the first one
        let scope = vec![
            ResourceSelector::GitRepository {
                repo_path: "/repo/one".to_string(),
                allowed_refs: vec![],
                mode: ferrum_proto::ResourceMode::Read,
            },
            ResourceSelector::GitRepository {
                repo_path: "/repo/two".to_string(),
                allowed_refs: vec![],
                mode: ferrum_proto::ResourceMode::Read,
            },
        ];
        let target = determine_rollback_target_from_bindings(&scope);
        match target {
            RollbackTarget::GitRef { repo_path, .. } => {
                assert_eq!(repo_path, "/repo/one");
            }
            other => panic!("expected GitRef target, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_evaluate_outcome_endpoint_aligned() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create an intent with allowed outcome
        let intent_id = ferrum_proto::IntentId::new();
        let intent = IntentEnvelope {
            intent_id,
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![OutcomeClause {
                id: "read".to_string(),
                description: "read only analysis".to_string(),
                effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
                required: true,
            }],
            forbidden_outcomes: Vec::new(),
            resource_scope: Vec::new(),
            risk_tier: RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
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
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };
        runtime.store.intents().insert(&intent).await.unwrap();

        // Create a proposal to satisfy foreign key constraints
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = ferrum_proto::ActionProposal {
            proposal_id,
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "read only analysis".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };
        runtime.store.proposals().insert(&proposal).await.unwrap();

        // Mint a capability to satisfy foreign key constraints
        let mint_request = ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test_server".to_string(),
                tool_name: "test_tool".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 60,
            metadata: ferrum_proto::JsonMap::new(),
        };
        let capability_response = runtime.cap.mint(mint_request).await.unwrap();
        runtime
            .store
            .capabilities()
            .insert(&capability_response.lease)
            .await
            .unwrap();

        // Create an execution for this intent
        let execution_id = ExecutionId::new();
        let record = ExecutionRecord {
            execution_id,
            proposal_id,
            intent_id,
            capability_id: capability_response.lease.capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Committed,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        };
        runtime.store.executions().insert(&record).await.unwrap();

        // Build an aligned outcome report
        let report = OutcomeReport {
            execution_id,
            actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
            description: "completed read-only analysis".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&report).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let result: EvaluateOutcomeResponse = serde_json::from_slice(&body).unwrap();
        assert!(result.aligned);
    }

    #[tokio::test]
    async fn test_evaluate_outcome_endpoint_forbidden() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create an intent that explicitly forbids GitMutation.
        let intent_id = ferrum_proto::IntentId::new();
        let intent = IntentEnvelope {
            intent_id,
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![OutcomeClause {
                id: "read".to_string(),
                description: "read only analysis".to_string(),
                effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
                required: true,
            }],
            forbidden_outcomes: vec![OutcomeClause {
                id: "no-git".to_string(),
                description: "no git mutations allowed".to_string(),
                effect_type: ferrum_proto::EffectType::GitMutation,
                required: true,
            }],
            resource_scope: Vec::new(),
            risk_tier: RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
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
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };
        runtime.store.intents().insert(&intent).await.unwrap();

        // Create a proposal to satisfy foreign key constraints
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = ferrum_proto::ActionProposal {
            proposal_id,
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "read only analysis".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };
        runtime.store.proposals().insert(&proposal).await.unwrap();

        // Mint a capability to satisfy foreign key constraints
        let mint_request = ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test_server".to_string(),
                tool_name: "test_tool".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 60,
            metadata: ferrum_proto::JsonMap::new(),
        };
        let capability_response = runtime.cap.mint(mint_request).await.unwrap();
        runtime
            .store
            .capabilities()
            .insert(&capability_response.lease)
            .await
            .unwrap();

        // Create an execution
        let execution_id = ExecutionId::new();
        let record = ExecutionRecord {
            execution_id,
            proposal_id,
            intent_id,
            capability_id: capability_response.lease.capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Committed,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        };
        runtime.store.executions().insert(&record).await.unwrap();

        // Build an outcome report with a non-allowed effect (git mutation instead of read-only)
        let report = OutcomeReport {
            execution_id,
            actual_effect: ferrum_proto::EffectType::GitMutation,
            description: "mutated git repository".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&report).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let result: EvaluateOutcomeResponse = serde_json::from_slice(&body).unwrap();
        assert!(!result.aligned);
    }

    #[tokio::test]
    async fn test_evaluate_outcome_execution_not_found() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        let execution_id = ExecutionId::new();
        let report = OutcomeReport {
            execution_id,
            actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
            description: "test".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&report).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_evaluate_outcome_id_mismatch() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create an intent
        let intent_id = ferrum_proto::IntentId::new();
        let intent = IntentEnvelope {
            intent_id,
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![OutcomeClause {
                id: "read".to_string(),
                description: "read only analysis".to_string(),
                effect_type: ferrum_proto::EffectType::ReadOnlyAnalysis,
                required: true,
            }],
            forbidden_outcomes: Vec::new(),
            resource_scope: Vec::new(),
            risk_tier: RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: RollbackClass::R0NativeReversible,
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
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };
        runtime.store.intents().insert(&intent).await.unwrap();

        // Create a proposal to satisfy foreign key constraints
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = ferrum_proto::ActionProposal {
            proposal_id,
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "read only analysis".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };
        runtime.store.proposals().insert(&proposal).await.unwrap();

        // Mint a capability to satisfy foreign key constraints
        let mint_request = ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test_server".to_string(),
                tool_name: "test_tool".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 60,
            metadata: ferrum_proto::JsonMap::new(),
        };
        let capability_response = runtime.cap.mint(mint_request).await.unwrap();
        runtime
            .store
            .capabilities()
            .insert(&capability_response.lease)
            .await
            .unwrap();

        // Create an execution
        let execution_id = ExecutionId::new();
        let record = ExecutionRecord {
            execution_id,
            proposal_id,
            intent_id,
            capability_id: capability_response.lease.capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: ExecutionState::Committed,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        };
        runtime.store.executions().insert(&record).await.unwrap();

        // Report with mismatched execution_id in body
        let report = OutcomeReport {
            execution_id: ExecutionId::new(), // different id
            actual_effect: ferrum_proto::EffectType::ReadOnlyAnalysis,
            description: "test".to_string(),
            result_digest: None,
            adapter_success: true,
            adapter_metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/evaluate-outcome", execution_id))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&report).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_provenance_success() {
        use ferrum_proto::ProvenanceEventKind;

        let bridge = Arc::new(McpBridge::new("test-runtime"));
        let runtime = test_runtime_with_bridges(vec![bridge.clone()]).await;
        let router = build_router(runtime);

        let request = ProvenanceIngestRequest {
            source_runtime_id: "test-runtime".to_string(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            description: "test event".to_string(),
            execution_id: None,
            intent_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/provenance/ingest")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let response: ProvenanceIngestResponse = serde_json::from_slice(&body).unwrap();
        assert!(response.linked);
        assert!(!response.event_id.0.is_nil());
    }

    #[tokio::test]
    async fn test_ingest_provenance_unknown_source() {
        use ferrum_proto::ProvenanceEventKind;

        let runtime = test_runtime_with_bridges(vec![]).await;
        let router = build_router(runtime);

        let request = ProvenanceIngestRequest {
            source_runtime_id: "unknown-runtime".to_string(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            description: "test event".to_string(),
            execution_id: None,
            intent_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/provenance/ingest")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_ingest_provenance_with_execution_id() {
        use ferrum_proto::{ExecutionId, ProvenanceEventKind};

        let bridge = Arc::new(McpBridge::new("test-runtime"));
        let runtime = test_runtime_with_bridges(vec![bridge.clone()]).await;
        let router = build_router(runtime);

        let execution_id = ExecutionId::new();

        let request = ProvenanceIngestRequest {
            source_runtime_id: "test-runtime".to_string(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            description: "test event with execution".to_string(),
            execution_id: Some(execution_id),
            intent_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/provenance/ingest")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let response: ProvenanceIngestResponse = serde_json::from_slice(&body).unwrap();
        assert!(response.linked);
    }

    #[tokio::test]
    async fn test_ingest_provenance_empty_description_accepted() {
        use ferrum_proto::ProvenanceEventKind;

        let bridge = Arc::new(McpBridge::new("test-runtime"));
        let runtime = test_runtime_with_bridges(vec![bridge.clone()]).await;
        let router = build_router(runtime);

        let request = ProvenanceIngestRequest {
            source_runtime_id: "test-runtime".to_string(),
            kind: ProvenanceEventKind::ExternalEventReceived,
            description: "".to_string(),
            execution_id: None,
            intent_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            metadata: ferrum_proto::JsonMap::new(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/provenance/ingest")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Empty description should be accepted (no validation rejecting it)
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_bridges_empty() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/bridges")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let response: BridgeListResponse = serde_json::from_slice(&body).unwrap();
        assert!(response.bridges.is_empty());
    }

    #[tokio::test]
    async fn test_list_bridges_with_registered_bridge() {
        let bridge = Arc::new(McpBridge::new("test-runtime"));
        bridge.try_connect().await.unwrap();
        let runtime = test_runtime_with_bridges(vec![bridge.clone()]).await;
        let router = build_router(runtime);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/bridges")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let response: BridgeListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(response.bridges.len(), 1);
        assert_eq!(response.bridges[0].runtime_id, "test-runtime");
        assert!(response.bridges[0].connected);
    }

    #[tokio::test]
    async fn test_list_bridge_tools_success() {
        let tools = vec![
            BridgeToolInfo {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                input_schema: None,
            },
            BridgeToolInfo {
                name: "write_file".to_string(),
                description: "Write a file".to_string(),
                input_schema: None,
            },
        ];
        let bridge = Arc::new(McpBridge::new("test-runtime").with_tools(tools));
        bridge.try_connect().await.unwrap();
        let runtime = test_runtime_with_bridges(vec![bridge.clone()]).await;
        let router = build_router(runtime);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/bridges/test-runtime/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let response: BridgeToolsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(response.runtime_id, "test-runtime");
        assert_eq!(response.tools.len(), 2);
        assert_eq!(response.tools[0].name, "read_file");
        assert_eq!(response.tools[1].name, "write_file");
    }

    #[tokio::test]
    async fn test_list_bridge_tools_not_found() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/bridges/nonexistent/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_bridge_tools_disconnected_returns_503() {
        // Create a bridge WITHOUT calling try_connect() — it stays disconnected
        let bridge = Arc::new(McpBridge::new("disconnected-runtime"));
        // Note: do NOT call bridge.try_connect().await — bridge remains disconnected
        let runtime = test_runtime_with_bridges(vec![bridge.clone()]).await;
        let router = build_router(runtime);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/bridges/disconnected-runtime/tools")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Disconnected bridge returns 503 Service Unavailable
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // I5: validate_resource_bindings_subset_of_scope tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_scope_validation_empty_bindings_always_allowed() {
        // Empty resource_bindings is always valid regardless of scope
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let bindings = vec![];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_empty_scope_with_bindings_denied() {
        // Empty scope with non-empty bindings is always invalid
        let scope: Vec<ResourceSelector> = vec![];
        let bindings = vec![ferrum_proto::ResourceBinding::File {
            path: "/tmp/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("empty but capability has resource bindings")
        );
    }

    #[test]
    fn test_scope_validation_exact_match_allowed() {
        // Exact path match should be allowed
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::File {
            path: "/tmp/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_subset_path_allowed() {
        // Binding path within scope directory should be allowed
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::File {
            path: "/tmp/subdir/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_superset_path_denied() {
        // Binding path outside scope directory should be denied
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::File {
            path: "/other/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("not within intent resource scope")
        );
    }

    #[test]
    fn test_scope_validation_disjoint_paths_denied() {
        // Completely different paths should be denied
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/home/user/project".to_string(),
            mode: ferrum_proto::ResourceMode::Read,
            content_hash: None,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::File {
            path: "/var/log/app.log".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_validation_multiple_scope_entries_one_match_allowed() {
        // If any scope entry covers the binding, it's allowed
        let scope = vec![
            ResourceSelector::FilesystemPath {
                path: "/tmp".to_string(),
                mode: ferrum_proto::ResourceMode::Write,
                content_hash: None,
            },
            ResourceSelector::FilesystemPath {
                path: "/home/user".to_string(),
                mode: ferrum_proto::ResourceMode::Write,
                content_hash: None,
            },
        ];
        let bindings = vec![ferrum_proto::ResourceBinding::File {
            path: "/home/user/file.txt".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            required_hash: None,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_git_subset_allowed() {
        // Git binding repo_path within scope should be allowed
        let scope = vec![ResourceSelector::GitRepository {
            repo_path: "/home/user/repos".to_string(),
            allowed_refs: vec!["main".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::Git {
            repo_path: "/home/user/repos/myproject".to_string(),
            allowed_refs: vec!["main".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_git_superset_denied() {
        // Git binding repo_path outside scope should be denied
        let scope = vec![ResourceSelector::GitRepository {
            repo_path: "/home/user/repos".to_string(),
            allowed_refs: vec!["main".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::Git {
            repo_path: "/opt/otherrepo".to_string(),
            allowed_refs: vec!["main".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_validation_sqlite_subset_allowed() {
        // Sqlite binding db_path within scope should be allowed
        let scope = vec![ResourceSelector::SqliteDatabase {
            db_path: "/home/user/data".to_string(),
            tables: vec![],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::Sqlite {
            db_path: "/home/user/data/mydb.db".to_string(),
            tables: vec!["users".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_http_subset_allowed() {
        // Http binding base_url and path_prefix within scope should be allowed
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: ferrum_proto::HttpMethod::Post,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Post,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1/users".to_string(),
            header_allowlist: vec![],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_http_superset_denied() {
        // Http binding with different base_url should be denied
        let scope = vec![ResourceSelector::HttpEndpoint {
            method: ferrum_proto::HttpMethod::Post,
            base_url: "https://api.example.com".to_string(),
            path_prefix: "/v1".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::Http {
            method: ferrum_proto::HttpMethod::Post,
            base_url: "https://other-api.example.com".to_string(),
            path_prefix: "/v1/users".to_string(),
            header_allowlist: vec![],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_validation_email_subset_allowed() {
        // EmailDraft binding with recipient within scope allowlist should be allowed
        let scope = vec![ResourceSelector::EmailDraft {
            recipient_allowlist: vec!["@example.com".to_string()],
            subject_prefix_allowlist: vec!["[Admin]".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::EmailDraft {
            recipients: vec!["user@example.com".to_string()],
            allow_send: true,
            mode: ferrum_proto::ResourceMode::Write,
        }];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_email_superset_denied() {
        // EmailDraft binding with recipient outside scope allowlist should be denied
        let scope = vec![ResourceSelector::EmailDraft {
            recipient_allowlist: vec!["@example.com".to_string()],
            subject_prefix_allowlist: vec!["[Admin]".to_string()],
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let bindings = vec![ferrum_proto::ResourceBinding::EmailDraft {
            recipients: vec!["user@other.com".to_string()],
            allow_send: true,
            mode: ferrum_proto::ResourceMode::Write,
        }];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
    }

    #[test]
    fn test_scope_validation_mixed_binding_types() {
        // Multiple binding types - all within scope
        let scope = vec![
            ResourceSelector::FilesystemPath {
                path: "/tmp".to_string(),
                mode: ferrum_proto::ResourceMode::Write,
                content_hash: None,
            },
            ResourceSelector::GitRepository {
                repo_path: "/home/user/repos".to_string(),
                allowed_refs: vec!["main".to_string()],
                mode: ferrum_proto::ResourceMode::Write,
            },
        ];
        let bindings = vec![
            ferrum_proto::ResourceBinding::File {
                path: "/tmp/file.txt".to_string(),
                mode: ferrum_proto::ResourceMode::Write,
                required_hash: None,
            },
            ferrum_proto::ResourceBinding::Git {
                repo_path: "/home/user/repos/myproject".to_string(),
                allowed_refs: vec!["main".to_string()],
                mode: ferrum_proto::ResourceMode::Write,
            },
        ];
        assert!(validate_resource_bindings_subset_of_scope(&bindings, &scope).is_ok());
    }

    #[test]
    fn test_scope_validation_one_binding_outside_denies_all() {
        // If any binding is outside scope, the whole validation fails
        let scope = vec![ResourceSelector::FilesystemPath {
            path: "/tmp".to_string(),
            mode: ferrum_proto::ResourceMode::Write,
            content_hash: None,
        }];
        let bindings = vec![
            ferrum_proto::ResourceBinding::File {
                path: "/tmp/file.txt".to_string(),
                mode: ferrum_proto::ResourceMode::Write,
                required_hash: None,
            },
            ferrum_proto::ResourceBinding::File {
                path: "/other/file.txt".to_string(), // outside scope
                mode: ferrum_proto::ResourceMode::Write,
                required_hash: None,
            },
        ];
        let result = validate_resource_bindings_subset_of_scope(&bindings, &scope);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_metrics_endpoint_is_public_under_bearer_auth() {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Bearer,
            bearer_token: Some("secret-token".to_string()),
            ..ServerConfig::default()
        };

        let response = build_router_with_auth(runtime, config)
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_endpoint_returns_prometheus_text() {
        let runtime = test_runtime().await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify Prometheus text format
        assert!(body_str.contains("# HELP ferrumgate_http_requests_total"));
        assert!(body_str.contains("# TYPE ferrumgate_http_requests_total counter"));
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/healthz\",method=\"GET\",status=\"200\"}"
        ));
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/readyz\",method=\"GET\",status=\"200\"}"
        ));
        assert!(
            body_str.contains(
                "ferrumgate_http_requests_total{route=\"/v1/readyz/deep\",method=\"GET\",status=\"200\"}"
            )
        );
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/metrics\",method=\"GET\",status=\"200\"}"
        ));
        assert!(body_str.contains("ferrumgate_store_health_up"));
        assert!(body_str.contains("ferrumgate_write_queue_depth"));
        assert!(body_str.contains("ferrumgate_rate_limit_per_second"));
        assert!(body_str.contains("ferrumgate_rate_limit_burst"));
        assert!(body_str.contains("ferrumgate_metrics_scrapes_total"));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_counters_increment() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        // Call healthz
        let _ = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Call readyz
        let _ = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Call readyz/deep
        let _ = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz/deep")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Call metrics
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify counters have incremented
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/healthz\",method=\"GET\",status=\"200\"} 1"
        ));
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/readyz\",method=\"GET\",status=\"200\"} 1"
        ));
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/readyz/deep\",method=\"GET\",status=\"200\"} 1"
        ));
        assert!(body_str.contains(
            "ferrumgate_http_requests_total{route=\"/v1/metrics\",method=\"GET\",status=\"200\"} 1"
        ));
        assert!(body_str.contains("ferrumgate_metrics_scrapes_total 1"));
        // Store should be healthy
        assert!(body_str.contains("ferrumgate_store_health_up 1"));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_store_health_reflects_status() {
        let runtime = test_runtime_with_unhealthy_store().await;

        let response = build_router(runtime)
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Store should be unhealthy (0)
        assert!(body_str.contains("ferrumgate_store_health_up 0"));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_governance_errors_increment() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        // Trigger a governance error by calling get_execution with non-existent execution_id
        // Use a valid UUID format that doesn't exist in the store
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/executions/00000000-0000-0000-0000-000000000001")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should return NOT_FOUND (404)
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Now check metrics endpoint
        let metrics_response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(metrics_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(metrics_response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify governance error counter incremented for the route
        assert!(body_str.contains("ferrumgate_governance_errors_total"));
        assert!(body_str.contains(
            "ferrumgate_governance_errors_total{route=\"/v1/executions/{execution_id}\",method=\"GET\"} 1"
        ));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_governance_errors_zero_when_no_errors() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        // Call metrics without triggering any governance errors
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify governance error counters exist but are zero
        assert!(body_str.contains("ferrumgate_governance_errors_total"));
        // Check that at least one route shows 0 (healthz/readyz/metrics should not be tracked)
        assert!(body_str.contains(
            "ferrumgate_governance_errors_total{route=\"/v1/intents/compile\",method=\"POST\"} 0"
        ));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_governance_success_counters_present() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        // Call metrics without triggering any governance successes
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify governance success counters exist (HELP and TYPE lines)
        assert!(body_str.contains("ferrumgate_governance_success_total"));
        // Check that at least one route shows 0
        assert!(body_str.contains(
            "ferrumgate_governance_success_total{route=\"/v1/intents/compile\",method=\"POST\"} 0"
        ));
    }

    #[tokio::test]
    async fn test_metrics_endpoint_latency_histogram_present() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        // Call healthz and readyz to generate some latency samples
        let _ = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let _ = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Call metrics to get the histogram output
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify histogram HELP and TYPE lines exist
        assert!(body_str.contains("# HELP ferrumgate_request_duration_seconds HTTP request latency histogram by route, method, and status"));
        assert!(body_str.contains("# TYPE ferrumgate_request_duration_seconds histogram"));

        // Verify histogram bucket lines with le= label exist for /v1/healthz
        assert!(body_str.contains("ferrumgate_request_duration_seconds{"));

        // Verify histogram has bucket, sum, and count for at least one route
        // Check for bucket with le= label
        assert!(body_str.contains("le=\"0.005\""));
        assert!(body_str.contains("le=\"+Inf\""));
        // Check for sum and count
        assert!(body_str.contains("_sum{"));
        assert!(body_str.contains("_count{"));

        // Verify at least one public endpoint route label is present
        assert!(body_str.contains("route=\"/v1/healthz\""));
        assert!(body_str.contains("route=\"/v1/readyz\""));

        // Verify method and status labels are present
        assert!(body_str.contains("method=\"GET\""));
        assert!(body_str.contains("status=\"200\""));
    }

    /// Test guard: ensures every GovernanceRoute variant has both success and error
    /// Prometheus metric lines in /v1/metrics output. This prevents silent observability
    /// gaps when new governance routes are added.
    ///
    /// The macro uses an exhaustive match to ensure that adding a new GovernanceRoute
    /// variant without updating this macro causes a compile error.
    macro_rules! all_governance_routes {
        () => {{
            // Compile-time exhaustive list of all GovernanceRoute variants.
            // If a new variant is added to GovernanceRoute but not listed here,
            // the match below will produce a "non-exhaustive patterns" compile error.
            const ROUTES: &[GovernanceRoute] = &[
                GovernanceRoute::IntentsCompile,
                GovernanceRoute::IntentsList,
                GovernanceRoute::ProposalsEvaluate,
                GovernanceRoute::CapabilitiesMint,
                GovernanceRoute::CapabilitiesRevoke,
                GovernanceRoute::ExecutionsAuthorize,
                GovernanceRoute::ExecutionsPrepare,
                GovernanceRoute::ExecutionsExecute,
                GovernanceRoute::ExecutionsVerify,
                GovernanceRoute::ExecutionsCompensate,
                GovernanceRoute::ExecutionsCancel,
                GovernanceRoute::ExecutionsEvaluateOutcome,
                GovernanceRoute::ExecutionsExecutionId,
                GovernanceRoute::Approvals,
                GovernanceRoute::ApprovalsApprovalId,
                GovernanceRoute::ApprovalsResolve,
                GovernanceRoute::PolicyBundlesCreate,
                GovernanceRoute::PolicyBundlesList,
                GovernanceRoute::PolicyBundlesGet,
                GovernanceRoute::PolicyBundlesUpdate,
                GovernanceRoute::PolicyBundlesDelete,
                GovernanceRoute::PolicyBundlesSetActive,
                GovernanceRoute::ProvenanceQuery,
                GovernanceRoute::ProvenanceLineage,
                GovernanceRoute::ProvenanceLineageExecutionId,
                GovernanceRoute::ProvenanceIngest,
                GovernanceRoute::BridgesBridgeIdTools,
            ];

            // Exhaustiveness check: match against all variants.
            // This will fail to compile if a new GovernanceRoute variant exists
            // that is NOT handled in the match arms below.
            match GovernanceRoute::IntentsCompile {
                GovernanceRoute::IntentsCompile => (),
                GovernanceRoute::IntentsList => (),
                GovernanceRoute::ProposalsEvaluate => (),
                GovernanceRoute::CapabilitiesMint => (),
                GovernanceRoute::CapabilitiesRevoke => (),
                GovernanceRoute::ExecutionsAuthorize => (),
                GovernanceRoute::ExecutionsPrepare => (),
                GovernanceRoute::ExecutionsExecute => (),
                GovernanceRoute::ExecutionsVerify => (),
                GovernanceRoute::ExecutionsCompensate => (),
                GovernanceRoute::ExecutionsCancel => (),
                GovernanceRoute::ExecutionsEvaluateOutcome => (),
                GovernanceRoute::ExecutionsExecutionId => (),
                GovernanceRoute::Approvals => (),
                GovernanceRoute::ApprovalsApprovalId => (),
                GovernanceRoute::ApprovalsResolve => (),
                GovernanceRoute::PolicyBundlesCreate => (),
                GovernanceRoute::PolicyBundlesList => (),
                GovernanceRoute::PolicyBundlesGet => (),
                GovernanceRoute::PolicyBundlesUpdate => (),
                GovernanceRoute::PolicyBundlesDelete => (),
                GovernanceRoute::PolicyBundlesSetActive => (),
                GovernanceRoute::ProvenanceQuery => (),
                GovernanceRoute::ProvenanceLineage => (),
                GovernanceRoute::ProvenanceLineageExecutionId => (),
                GovernanceRoute::ProvenanceIngest => (),
                GovernanceRoute::BridgesBridgeIdTools => (),
            };

            ROUTES
        }};
    }

    #[tokio::test]
    async fn test_resolve_approval_not_found() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Use a valid UUID format that doesn't exist in the DB
        let resolve_request = ferrum_proto::ApprovalResolveRequest {
            actor: ferrum_proto::ActorRef {
                actor_type: ferrum_proto::ActorType::Operator,
                actor_id: "test-operator".to_string(),
                display_name: Some("Test Operator".to_string()),
            },
            approve: true,
            reason: None,
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/approvals/00000000-0000-0000-0000-000000000000/resolve")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&resolve_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // Note: Tests for pending→granted, pending→denied, terminal→409, expired→403, and
    // provenance event emission require foreign key constraints (approval references intent/proposal).
    // These scenarios are covered by integration tests in integration_gateway_flow.rs
    // (test_i6_pending_approval_denied, test_i6_approval_with_valid_binding_succeeds, etc.).

    #[tokio::test]
    async fn test_all_governance_routes_have_metrics_representation() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        // Call metrics to get the full output
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();

        // Invoke the macro to get the route list (and trigger compile-time exhaustiveness check)
        let routes = all_governance_routes!();

        // Verify both success and error metrics exist for each route using path() and method()
        for route in routes {
            let path = route.path();
            let method = route.method();
            let success_metric = format!(
                "ferrumgate_governance_success_total{{route=\"{}\",method=\"{}\"}}",
                path, method
            );
            let error_metric = format!(
                "ferrumgate_governance_errors_total{{route=\"{}\",method=\"{}\"}}",
                path, method
            );

            assert!(
                body_str.contains(&success_metric),
                "Missing governance success metric for {:?} (path={}, method={})",
                route,
                path,
                method
            );
            assert!(
                body_str.contains(&error_metric),
                "Missing governance error metric for {:?} (path={}, method={})",
                route,
                path,
                method
            );
        }
    }

    // ---------------------------------------------------------------------------
    // P0: Monitoring endpoints bypass workload rate limiter
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_monitoring_endpoints_bypass_rate_limiter() {
        let runtime = test_runtime().await;
        // Very restrictive rate limit: 1 req/sec, burst 1
        let router = build_router_with_governor(runtime, 1, 1);

        // Monitoring endpoints should NOT be rate limited
        for endpoint in ["/v1/metrics", "/v1/readyz", "/v1/readyz/deep"] {
            for i in 0..5 {
                let response = router
                    .clone()
                    .oneshot(
                        Request::builder()
                            .uri(endpoint)
                            .header("x-real-ip", "192.168.1.1")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();

                assert_eq!(
                    response.status(),
                    StatusCode::OK,
                    "monitoring endpoint {} request {} should bypass rate limiter",
                    endpoint,
                    i
                );
            }
        }
    }

    #[tokio::test]
    async fn test_workload_endpoint_is_rate_limited() {
        let runtime = test_runtime().await;
        // Very restrictive rate limit: 1 req/sec, burst 1
        let router = build_router_with_governor(runtime, 1, 1);

        // First request to a workload endpoint should succeed
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "192.168.1.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Subsequent requests should eventually be rate limited
        let mut got_429 = false;
        for _ in 0..10 {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/v1/approvals")
                        .header("x-real-ip", "192.168.1.1")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                got_429 = true;
                break;
            }
        }

        assert!(got_429, "workload endpoint should be rate limited");
    }

    // ---------------------------------------------------------------------------
    // P1: SmartIpKeyExtractor separate-bucket behavior
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_distinct_x_real_ip_get_separate_buckets() {
        let runtime = test_runtime().await;
        // Restrictive rate limit: 1 req/sec, burst 1
        let router = build_router_with_governor(runtime, 1, 1);

        // Exhaust the burst for IP A
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.36.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // IP A should now be rate limited
        let mut ip_a_limited = false;
        for _ in 0..10 {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/v1/approvals")
                        .header("x-real-ip", "10.36.0.1")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                ip_a_limited = true;
                break;
            }
        }
        assert!(ip_a_limited, "IP A should be rate limited after burst");

        // IP B should still succeed because it has its own bucket
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.36.0.2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "IP B should have a separate bucket and succeed"
        );
    }

    #[tokio::test]
    async fn test_same_x_real_ip_is_limited_across_adapters() {
        let runtime = test_runtime().await;
        // Restrictive rate limit: 1 req/sec, burst 1
        let router = build_router_with_governor(runtime, 1, 1);

        // First request from IP X to /v1/approvals succeeds
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("x-real-ip", "10.36.0.5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Second request from same IP X to /v1/intents should be rate limited
        // because the bucket is keyed by IP, not by route.
        let mut got_429 = false;
        for _ in 0..10 {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/v1/intents")
                        .header("x-real-ip", "10.36.0.5")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                got_429 = true;
                break;
            }
        }
        assert!(
            got_429,
            "same x-real-ip should be limited across different workload routes"
        );
    }

    // -------------------------------------------------------------------------
    // D-1 Slice 4: Local Gateway State-Machine Negative Smoke Tests
    // -------------------------------------------------------------------------

    use ferrum_proto::{
        ActionType, RollbackContract, RollbackContractId, RollbackState, RollbackTarget,
    };

    /// Helper: create intent + proposal + capability + execution in a specific state.
    /// Returns (runtime, router, execution_id) with the execution already stored.
    async fn setup_lifecycle_test_runtime(
        execution_state: ExecutionState,
    ) -> (GatewayRuntime, axum::Router, ExecutionId) {
        setup_lifecycle_test_runtime_with_mode(execution_state, ferrum_proto::ApprovalMode::None)
            .await
    }

    /// Helper: create intent + proposal + capability + execution in a specific state
    /// with a specific approval mode.
    /// Returns (runtime, router, execution_id) with the execution already stored.
    async fn setup_lifecycle_test_runtime_with_mode(
        execution_state: ExecutionState,
        approval_mode: ferrum_proto::ApprovalMode,
    ) -> (GatewayRuntime, axum::Router, ExecutionId) {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create intent
        let intent_id = ferrum_proto::IntentId::new();
        let intent = IntentEnvelope {
            intent_id,
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test intent".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: RiskTier::Low,
            approval_mode,
            default_rollback_class: RollbackClass::R0NativeReversible,
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
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };
        runtime.store.intents().insert(&intent).await.unwrap();

        // Create proposal
        let proposal_id = ferrum_proto::ProposalId::new();
        let proposal = ferrum_proto::ActionProposal {
            proposal_id,
            intent_id,
            step_index: 0,
            title: "test proposal".to_string(),
            tool_name: "test_tool".to_string(),
            server_name: "test_server".to_string(),
            raw_arguments: serde_json::json!({}),
            expected_effect: "test effect".to_string(),
            estimated_risk: RiskTier::Low,
            requested_rollback_class: RollbackClass::R0NativeReversible,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };
        runtime.store.proposals().insert(&proposal).await.unwrap();

        // Mint capability
        let mint_request = ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "test_server".to_string(),
                tool_name: "test_tool".to_string(),
                tool_version: None,
            },
            resource_bindings: Vec::new(),
            argument_constraints: Vec::new(),
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 0,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 60,
            metadata: ferrum_proto::JsonMap::new(),
        };
        let capability_response = runtime.cap.mint(mint_request).await.unwrap();
        runtime
            .store
            .capabilities()
            .insert(&capability_response.lease)
            .await
            .unwrap();

        // Create execution in the requested state
        let execution_id = ExecutionId::new();
        let record = ExecutionRecord {
            execution_id,
            proposal_id,
            intent_id,
            capability_id: capability_response.lease.capability_id,
            rollback_contract_id: None,
            decision: Decision::Allow,
            state: execution_state,
            started_at: chrono::Utc::now(),
            finished_at: None,
            result_digest: None,
            metadata: ferrum_proto::JsonMap::new(),
        };
        runtime.store.executions().insert(&record).await.unwrap();

        (runtime, router, execution_id)
    }

    /// Helper: create a rollback contract and link it to an execution.
    async fn link_rollback_contract(
        runtime: &GatewayRuntime,
        execution_id: ExecutionId,
        intent_id: ferrum_proto::IntentId,
        proposal_id: ProposalId,
        state: RollbackState,
    ) -> RollbackContractId {
        let contract_id = RollbackContractId::new();
        let contract = RollbackContract {
            contract_id,
            intent_id,
            proposal_id,
            execution_id,
            action_type: ActionType::FileWrite,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: "noop".to_string(),
            target: RollbackTarget::FilePath {
                path: "/tmp/test.txt".to_string(),
                before_hash: None,
                after_hash: None,
            },
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            state,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata: ferrum_proto::JsonMap::new(),
        };
        runtime
            .store
            .rollback_contracts()
            .insert(&contract)
            .await
            .unwrap();

        // Link execution to contract
        let mut execution = runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();
        execution.rollback_contract_id = Some(contract_id);
        runtime.store.executions().update(&execution).await.unwrap();

        contract_id
    }

    /// D-1 Slice 4: prepare_execution on Proposed execution returns 409.
    #[tokio::test]
    async fn test_prepare_without_authorization_returns_409() {
        let (_runtime, router, execution_id) =
            setup_lifecycle_test_runtime(ExecutionState::Proposed).await;

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/prepare", execution_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "prepare on Proposed execution should return 409 Conflict"
        );
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("cannot be prepared"),
            "Error should indicate state mismatch: {}",
            body_str
        );
    }

    /// D-1 Slice 4: execute_execution on Authorized execution with PendingPrepare contract returns 409.
    #[tokio::test]
    async fn test_execute_before_prepare_returns_409() {
        let (runtime, router, execution_id) =
            setup_lifecycle_test_runtime(ExecutionState::Authorized).await;

        // Get the execution to retrieve intent/proposal ids
        let execution = runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();

        // Link a rollback contract in PendingPrepare state (not Prepared)
        link_rollback_contract(
            &runtime,
            execution_id,
            execution.intent_id,
            execution.proposal_id,
            RollbackState::PendingPrepare,
        )
        .await;

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/execute", execution_id))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"payload": {}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "execute before prepare should return 409 Conflict"
        );
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("execute not allowed in current state"),
            "Error should indicate state mismatch: {}",
            body_str
        );
    }

    /// WS3: execute_execution on DraftOnly intent returns 403 (defense-in-depth).
    #[tokio::test]
    async fn test_execute_draft_only_returns_403() {
        let (runtime, router, execution_id) = setup_lifecycle_test_runtime_with_mode(
            ExecutionState::Prepared,
            ferrum_proto::ApprovalMode::DraftOnly,
        )
        .await;

        let execution = runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();

        // Link a rollback contract in Prepared state so state-machine would otherwise allow
        link_rollback_contract(
            &runtime,
            execution_id,
            execution.intent_id,
            execution.proposal_id,
            RollbackState::Prepared,
        )
        .await;

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/execute", execution_id))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"payload": {}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "execute on DraftOnly intent should return 403 Forbidden"
        );
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("draft-only intent cannot proceed to execute"),
            "Error should indicate draft-only guard: {}",
            body_str
        );
    }

    /// D-1 Slice 4: verify_execution on Prepared execution with Prepared contract returns 409.
    #[tokio::test]
    async fn test_verify_before_execute_returns_409() {
        let (runtime, router, execution_id) =
            setup_lifecycle_test_runtime(ExecutionState::Prepared).await;

        let execution = runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();

        // Link a rollback contract in Prepared state (not ExecutedAwaitingVerify)
        link_rollback_contract(
            &runtime,
            execution_id,
            execution.intent_id,
            execution.proposal_id,
            RollbackState::Prepared,
        )
        .await;

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/verify", execution_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "verify before execute should return 409 Conflict"
        );
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("verify not allowed in current state"),
            "Error should indicate state mismatch: {}",
            body_str
        );
    }

    /// D-1 Slice 4: compensate_execution on Prepared execution with Prepared contract returns 409.
    #[tokio::test]
    async fn test_compensate_before_verify_returns_409() {
        let (runtime, router, execution_id) =
            setup_lifecycle_test_runtime(ExecutionState::Prepared).await;

        let execution = runtime
            .store
            .executions()
            .get(execution_id)
            .await
            .unwrap()
            .unwrap();

        // Link a rollback contract in Prepared state (not ExecutedAwaitingVerify)
        link_rollback_contract(
            &runtime,
            execution_id,
            execution.intent_id,
            execution.proposal_id,
            RollbackState::Prepared,
        )
        .await;

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/executions/{}/compensate", execution_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "compensate before verify should return 409 Conflict"
        );
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("compensate not allowed in current state"),
            "Error should indicate state mismatch: {}",
            body_str
        );
    }
}
