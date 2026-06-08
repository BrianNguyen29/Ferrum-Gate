use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use ferrum_cap::InMemoryCapabilityService;
use ferrum_pdp::StaticPdpEngine;
#[allow(unused_imports)] // IntentStatus is used in `mod tests` via `use super::*;`.
use ferrum_proto::{
    AgentListResponse, ApiError, ApiErrorCode, AuditAction, AuditLogEntry, AuditResourceType,
    Decision, DiffPolicyBundleVersionsResponse, EvaluateOutcomeResponse, EvaluateProposalResponse,
    ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope, IntentStatus,
    ListPolicyBundleVersionsResponse, Matcher, OutcomeClause, OutcomeReport, PolicyBundle,
    PolicyBundleId, PolicyBundleSimulateRequest, PolicyBundleSimulateResponse, PolicyRule,
    PolicySimulateRequest, ProposalId, ProvenanceEventKind, ProvenanceQueryRequest,
    RegisterAgentRequest, RegisterAgentResponse, ResourceSelector, RevokeAgentRequest, RiskTier,
    RollbackClass, RollbackPolicyBundleRequest, RollbackPolicyBundleResponse, RollbackState,
    RollbackTarget, TimeBudget, TrustContextSummary, TrustLabel as ProtoTrustLabel,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use ferrum_store::StoreFacade;
use ferrum_sync::RuntimeBridge;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration as StdDuration, Instant};
use tower::ServiceBuilder;

use ed25519_dalek::Verifier;

use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use tower_http::trace::TraceLayer;

use crate::{AuthMode, GatewayRuntime, OidcJwksCache, ServerConfig};

/// Shared state that includes both runtime and server config for auth.
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) runtime: GatewayRuntime,
    pub(crate) server_config: ServerConfig,
    pub(crate) metrics: Arc<Metrics>,
    pub(crate) jwks_cache: Option<Arc<OidcJwksCache>>,
    /// In-memory nonce cache for Agent auth replay protection.
    nonce_cache: Arc<Mutex<HashMap<String, Instant>>>,
}

/// Metrics state for the /v1/metrics endpoint.
/// Tracks health/metrics request counters, store health gauge, and bounded
/// governance error counters for all governance API endpoints.
pub(crate) struct Metrics {
    pub(crate) healthz_requests: AtomicU64,
    pub(crate) readyz_requests: AtomicU64,
    pub(crate) readyz_deep_requests_200: AtomicU64,
    pub(crate) readyz_deep_requests_503: AtomicU64,
    pub(crate) metrics_scrapes: AtomicU64,
    pub(crate) store_health_up: AtomicU64,
    // Governance error counters keyed by static route template
    pub(crate) governance_errors_v1_intents_compile: AtomicU64,
    pub(crate) governance_errors_v1_intents_list: AtomicU64,
    pub(crate) governance_errors_v1_proposals_evaluate: AtomicU64,
    pub(crate) governance_errors_v1_capabilities_mint: AtomicU64,
    pub(crate) governance_errors_v1_capabilities_revoke: AtomicU64,
    pub(crate) governance_errors_v1_executions_authorize: AtomicU64,
    pub(crate) governance_errors_v1_executions_prepare: AtomicU64,
    pub(crate) governance_errors_v1_executions_execute: AtomicU64,
    pub(crate) governance_errors_v1_executions_verify: AtomicU64,
    pub(crate) governance_errors_v1_executions_compensate: AtomicU64,
    pub(crate) governance_errors_v1_executions_cancel: AtomicU64,
    pub(crate) governance_errors_v1_executions_evaluate_outcome: AtomicU64,
    pub(crate) governance_errors_v1_executions_execution_id: AtomicU64,
    pub(crate) governance_errors_v1_executions_commit: AtomicU64,
    pub(crate) governance_errors_v1_approvals: AtomicU64,
    pub(crate) governance_errors_v1_approvals_approval_id: AtomicU64,
    pub(crate) governance_errors_v1_approvals_resolve: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_create: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_list: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_get: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_update: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_delete: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_set_active: AtomicU64,
    pub(crate) governance_errors_v1_policy_simulate: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_simulate: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_versions: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_diff: AtomicU64,
    pub(crate) governance_errors_v1_policy_bundles_rollback: AtomicU64,
    pub(crate) governance_errors_v1_provenance_query: AtomicU64,
    pub(crate) governance_errors_v1_provenance_lineage: AtomicU64,
    pub(crate) governance_errors_v1_provenance_lineage_execution_id: AtomicU64,
    pub(crate) governance_errors_v1_provenance_ingest: AtomicU64,
    pub(crate) governance_errors_v1_bridges_bridge_id_tools: AtomicU64,
    pub(crate) governance_errors_v1_agents_create: AtomicU64,
    pub(crate) governance_errors_v1_agents_list: AtomicU64,
    pub(crate) governance_errors_v1_agents_revoke: AtomicU64,
    // Governance success counters keyed by static route template
    pub(crate) governance_success_v1_intents_compile: AtomicU64,
    pub(crate) governance_success_v1_intents_list: AtomicU64,
    pub(crate) governance_success_v1_proposals_evaluate: AtomicU64,
    pub(crate) governance_success_v1_capabilities_mint: AtomicU64,
    pub(crate) governance_success_v1_capabilities_revoke: AtomicU64,
    pub(crate) governance_success_v1_executions_authorize: AtomicU64,
    pub(crate) governance_success_v1_executions_prepare: AtomicU64,
    pub(crate) governance_success_v1_executions_execute: AtomicU64,
    pub(crate) governance_success_v1_executions_verify: AtomicU64,
    pub(crate) governance_success_v1_executions_compensate: AtomicU64,
    pub(crate) governance_success_v1_executions_cancel: AtomicU64,
    pub(crate) governance_success_v1_executions_evaluate_outcome: AtomicU64,
    pub(crate) governance_success_v1_executions_execution_id: AtomicU64,
    pub(crate) governance_success_v1_executions_commit: AtomicU64,
    pub(crate) governance_success_v1_approvals: AtomicU64,
    pub(crate) governance_success_v1_approvals_approval_id: AtomicU64,
    pub(crate) governance_success_v1_approvals_resolve: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_create: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_list: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_get: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_update: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_delete: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_set_active: AtomicU64,
    pub(crate) governance_success_v1_policy_simulate: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_simulate: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_versions: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_diff: AtomicU64,
    pub(crate) governance_success_v1_policy_bundles_rollback: AtomicU64,
    pub(crate) governance_success_v1_provenance_query: AtomicU64,
    pub(crate) governance_success_v1_provenance_lineage: AtomicU64,
    pub(crate) governance_success_v1_provenance_lineage_execution_id: AtomicU64,
    pub(crate) governance_success_v1_provenance_ingest: AtomicU64,
    pub(crate) governance_success_v1_bridges_bridge_id_tools: AtomicU64,
    pub(crate) governance_success_v1_agents_create: AtomicU64,
    pub(crate) governance_success_v1_agents_list: AtomicU64,
    pub(crate) governance_success_v1_agents_revoke: AtomicU64,
    // Latency histogram for /v1/healthz (always status 200)
    pub(crate) healthz_latency_buckets: [AtomicU64; 11],
    pub(crate) healthz_latency_sum: AtomicU64,
    pub(crate) healthz_latency_count: AtomicU64,
    // Latency histogram for /v1/readyz (always status 200)
    pub(crate) readyz_latency_buckets: [AtomicU64; 11],
    pub(crate) readyz_latency_sum: AtomicU64,
    pub(crate) readyz_latency_count: AtomicU64,
    // Latency histogram for /v1/readyz/deep (status 200)
    pub(crate) readyz_deep_latency_buckets_200: [AtomicU64; 11],
    pub(crate) readyz_deep_latency_sum_200: AtomicU64,
    pub(crate) readyz_deep_latency_count_200: AtomicU64,
    // Latency histogram for /v1/readyz/deep (status 503)
    pub(crate) readyz_deep_latency_buckets_503: [AtomicU64; 11],
    pub(crate) readyz_deep_latency_sum_503: AtomicU64,
    pub(crate) readyz_deep_latency_count_503: AtomicU64,
    // Latency histogram for /v1/metrics (always status 200)
    pub(crate) metrics_latency_buckets: [AtomicU64; 11],
    pub(crate) metrics_latency_sum: AtomicU64,
    pub(crate) metrics_latency_count: AtomicU64,
}

impl Metrics {
    pub(crate) fn new() -> Self {
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
            governance_errors_v1_executions_commit: AtomicU64::new(0),
            governance_errors_v1_approvals: AtomicU64::new(0),
            governance_errors_v1_approvals_approval_id: AtomicU64::new(0),
            governance_errors_v1_approvals_resolve: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_create: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_list: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_get: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_update: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_delete: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_set_active: AtomicU64::new(0),
            governance_errors_v1_policy_simulate: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_simulate: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_versions: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_diff: AtomicU64::new(0),
            governance_errors_v1_policy_bundles_rollback: AtomicU64::new(0),
            governance_errors_v1_provenance_query: AtomicU64::new(0),
            governance_errors_v1_provenance_lineage: AtomicU64::new(0),
            governance_errors_v1_provenance_lineage_execution_id: AtomicU64::new(0),
            governance_errors_v1_provenance_ingest: AtomicU64::new(0),
            governance_errors_v1_bridges_bridge_id_tools: AtomicU64::new(0),
            governance_errors_v1_agents_create: AtomicU64::new(0),
            governance_errors_v1_agents_list: AtomicU64::new(0),
            governance_errors_v1_agents_revoke: AtomicU64::new(0),
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
            governance_success_v1_executions_commit: AtomicU64::new(0),
            governance_success_v1_approvals: AtomicU64::new(0),
            governance_success_v1_approvals_approval_id: AtomicU64::new(0),
            governance_success_v1_approvals_resolve: AtomicU64::new(0),
            governance_success_v1_policy_bundles_create: AtomicU64::new(0),
            governance_success_v1_policy_bundles_list: AtomicU64::new(0),
            governance_success_v1_policy_bundles_get: AtomicU64::new(0),
            governance_success_v1_policy_bundles_update: AtomicU64::new(0),
            governance_success_v1_policy_bundles_delete: AtomicU64::new(0),
            governance_success_v1_policy_bundles_set_active: AtomicU64::new(0),
            governance_success_v1_policy_simulate: AtomicU64::new(0),
            governance_success_v1_policy_bundles_simulate: AtomicU64::new(0),
            governance_success_v1_policy_bundles_versions: AtomicU64::new(0),
            governance_success_v1_policy_bundles_diff: AtomicU64::new(0),
            governance_success_v1_policy_bundles_rollback: AtomicU64::new(0),
            governance_success_v1_provenance_query: AtomicU64::new(0),
            governance_success_v1_provenance_lineage: AtomicU64::new(0),
            governance_success_v1_provenance_lineage_execution_id: AtomicU64::new(0),
            governance_success_v1_provenance_ingest: AtomicU64::new(0),
            governance_success_v1_bridges_bridge_id_tools: AtomicU64::new(0),
            governance_success_v1_agents_create: AtomicU64::new(0),
            governance_success_v1_agents_list: AtomicU64::new(0),
            governance_success_v1_agents_revoke: AtomicU64::new(0),
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
    pub(crate) fn increment_governance_error(&self, route: GovernanceRoute) {
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
            GovernanceRoute::ExecutionsCommit => self
                .governance_errors_v1_executions_commit
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
            GovernanceRoute::PolicySimulate => self
                .governance_errors_v1_policy_simulate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesSimulate => self
                .governance_errors_v1_policy_bundles_simulate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesVersions => self
                .governance_errors_v1_policy_bundles_versions
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesDiff => self
                .governance_errors_v1_policy_bundles_diff
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesRollback => self
                .governance_errors_v1_policy_bundles_rollback
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
            GovernanceRoute::AgentsCreate => self
                .governance_errors_v1_agents_create
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::AgentsList => self
                .governance_errors_v1_agents_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::AgentsRevoke => self
                .governance_errors_v1_agents_revoke
                .fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Increments the governance success counter for the given route.
    pub(crate) fn increment_governance_success(&self, route: GovernanceRoute) {
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
            GovernanceRoute::ExecutionsCommit => self
                .governance_success_v1_executions_commit
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
            GovernanceRoute::PolicySimulate => self
                .governance_success_v1_policy_simulate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesSimulate => self
                .governance_success_v1_policy_bundles_simulate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesVersions => self
                .governance_success_v1_policy_bundles_versions
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesDiff => self
                .governance_success_v1_policy_bundles_diff
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::PolicyBundlesRollback => self
                .governance_success_v1_policy_bundles_rollback
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
            GovernanceRoute::AgentsCreate => self
                .governance_success_v1_agents_create
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::AgentsList => self
                .governance_success_v1_agents_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::AgentsRevoke => self
                .governance_success_v1_agents_revoke
                .fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Increments the governance error counter for the given route and returns the error.
    /// Use this in `map_err` closures: `.map_err(|e| state.metrics.record_governance_error(route, e))`
    ///
    /// Generic over the error type so it accepts both the server-local
    /// `ApiProblem` and the extracted `crate::problem::ApiProblem` used by
    /// handler modules declared alongside `server`.
    pub(crate) fn record_governance_error<T>(&self, route: GovernanceRoute, err: T) -> T {
        self.increment_governance_error(route);
        err
    }

    /// Records a latency sample in the appropriate histogram based on route and status.
    /// `elapsed_ns` is the elapsed time in nanoseconds.
    pub(crate) fn record_latency(&self, route: PublicRoute, status: u16, elapsed_ns: u64) {
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
        for (i, boundary) in crate::monitoring::HISTOGRAM_BOUNDARIES.iter().enumerate() {
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
#[allow(clippy::enum_variant_names)]
pub(crate) enum GovernanceRoute {
    IntentsCompile,
    IntentsList,
    ProposalsEvaluate,
    CapabilitiesMint,
    CapabilitiesRevoke,
    ExecutionsAuthorize,
    ExecutionsPrepare,
    ExecutionsExecute,
    ExecutionsVerify,
    ExecutionsCommit,
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
    PolicySimulate,
    PolicyBundlesSimulate,
    PolicyBundlesVersions,
    PolicyBundlesDiff,
    PolicyBundlesRollback,
    ProvenanceQuery,
    ProvenanceLineage,
    ProvenanceLineageExecutionId,
    ProvenanceIngest,
    BridgesBridgeIdTools,
    AgentsCreate,
    AgentsList,
    AgentsRevoke,
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
            GovernanceRoute::ExecutionsCommit => "/v1/executions/{execution_id}/commit",
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
            GovernanceRoute::PolicySimulate => "/v1/policy/simulate",
            GovernanceRoute::PolicyBundlesSimulate => "/v1/policy-bundles/simulate",
            GovernanceRoute::PolicyBundlesVersions => "/v1/policy-bundles/{bundle_id}/versions",
            GovernanceRoute::PolicyBundlesDiff => "/v1/policy-bundles/{bundle_id}/diff",
            GovernanceRoute::PolicyBundlesRollback => "/v1/policy-bundles/{bundle_id}/rollback",
            GovernanceRoute::ProvenanceQuery => "/v1/provenance/query",
            GovernanceRoute::ProvenanceLineage => "/v1/provenance/lineage",
            GovernanceRoute::ProvenanceLineageExecutionId => {
                "/v1/provenance/lineage/{execution_id}"
            }
            GovernanceRoute::ProvenanceIngest => "/v1/provenance/ingest",
            GovernanceRoute::BridgesBridgeIdTools => "/v1/bridges/{bridge_id}/tools",
            GovernanceRoute::AgentsCreate => "/v1/admin/agents",
            GovernanceRoute::AgentsList => "/v1/admin/agents",
            GovernanceRoute::AgentsRevoke => "/v1/admin/agents/{agent_id}",
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
            GovernanceRoute::ExecutionsCommit => "POST",
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
            GovernanceRoute::PolicySimulate => "POST",
            GovernanceRoute::PolicyBundlesSimulate => "POST",
            GovernanceRoute::PolicyBundlesVersions => "GET",
            GovernanceRoute::PolicyBundlesDiff => "GET",
            GovernanceRoute::PolicyBundlesRollback => "POST",
            GovernanceRoute::ProvenanceQuery => "POST",
            GovernanceRoute::ProvenanceLineage => "POST",
            GovernanceRoute::ProvenanceLineageExecutionId => "GET",
            GovernanceRoute::ProvenanceIngest => "POST",
            GovernanceRoute::BridgesBridgeIdTools => "GET",
            GovernanceRoute::AgentsCreate => "POST",
            GovernanceRoute::AgentsList => "GET",
            GovernanceRoute::AgentsRevoke => "DELETE",
        }
    }
}

/// Public endpoint routes that have latency histograms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicRoute {
    Healthz,
    Readyz,
    ReadyzDeep,
    Metrics,
}

// ---------------------------------------------------------------------------
// I11 Output Sanitization helpers
// ---------------------------------------------------------------------------

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
    let jwks_cache = config.oidc_config.as_ref().and_then(|oidc| {
        oidc.jwks_url
            .as_ref()
            .map(|url| Arc::new(OidcJwksCache::new(url.clone(), oidc.jwks_cache_ttl_secs)))
    });
    let state = Arc::new(AppState {
        runtime,
        server_config: config.clone(),
        metrics: Arc::new(Metrics::new()),
        jwks_cache,
        nonce_cache: Arc::new(Mutex::new(HashMap::new())),
    });

    let monitoring_router = crate::monitoring::build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state.clone());

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

    // Add auth layer if auth mode requires authentication
    if config.auth_mode == AuthMode::Bearer
        || config.auth_mode == AuthMode::Scoped
        || config.auth_mode == AuthMode::Oidc
    {
        let auth_layer = ServiceBuilder::new()
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
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
///
/// ⚠️ TEST-ONLY: This router skips authentication. Production must always use
/// `build_router_with_auth` with `AuthMode::Bearer` and a strong bearer token.
/// Using `build_router` in production bypasses the auth layer and exposes all
/// governance endpoints without credential checks.
#[cfg(any(test, feature = "test-utils"))]
pub fn build_router(runtime: GatewayRuntime) -> Router {
    let state = Arc::new(AppState {
        runtime,
        server_config: ServerConfig::default(),
        metrics: Arc::new(Metrics::new()),
        jwks_cache: None,
        nonce_cache: Arc::new(Mutex::new(HashMap::new())),
    });
    let monitoring_router = crate::monitoring::build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state);
    monitoring_router.merge(workload_router)
}

/// Build a router with auth middleware using the given server config.
pub fn build_router_with_auth(runtime: GatewayRuntime, server_config: ServerConfig) -> Router {
    let jwks_cache = server_config.oidc_config.as_ref().and_then(|oidc| {
        oidc.jwks_url
            .as_ref()
            .map(|url| Arc::new(OidcJwksCache::new(url.clone(), oidc.jwks_cache_ttl_secs)))
    });
    let state = Arc::new(AppState {
        runtime,
        server_config: server_config.clone(),
        metrics: Arc::new(Metrics::new()),
        jwks_cache,
        nonce_cache: Arc::new(Mutex::new(HashMap::new())),
    });
    let monitoring_router = crate::monitoring::build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state.clone());
    let mut router = monitoring_router.merge(workload_router);

    // Add auth layer if auth mode requires authentication
    if server_config.auth_mode == AuthMode::Bearer
        || server_config.auth_mode == AuthMode::Scoped
        || server_config.auth_mode == AuthMode::Oidc
        || server_config.auth_mode == AuthMode::Agent
    {
        let auth_layer = ServiceBuilder::new()
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
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
#[cfg(any(test, feature = "test-utils"))]
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
        jwks_cache: None,
        nonce_cache: Arc::new(Mutex::new(HashMap::new())),
    });

    let monitoring_router = crate::monitoring::build_monitoring_router(state.clone());
    let workload_router = build_workload_router(state).layer(GovernorLayer::new(governor_conf));
    monitoring_router.merge(workload_router)
}

fn build_workload_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Provenance query endpoint
        .route(
            "/v1/provenance/query",
            post(crate::bridge::query_provenance),
        )
        // Execution lineage endpoint
        .route(
            "/v1/provenance/lineage/{execution_id}",
            get(crate::lineage::get_execution_lineage),
        )
        // Multi-hop lineage query endpoint
        .route(
            "/v1/provenance/lineage",
            post(crate::lineage::query_lineage),
        )
        // Provenance ingest endpoint
        .route(
            "/v1/provenance/ingest",
            post(crate::bridge::ingest_provenance),
        )
        // Bridge endpoints
        .route("/v1/bridges", get(crate::bridge::list_bridges))
        .route(
            "/v1/bridges/{bridge_id}/tools",
            get(crate::bridge::list_bridge_tools),
        )
        // Execution inspection endpoint
        .route(
            "/v1/executions/{execution_id}",
            get(crate::lineage::get_execution),
        )
        // Approvals endpoints
        .route("/v1/approvals", get(crate::approval::list_approvals))
        .route(
            "/v1/approvals/{approval_id}",
            get(crate::approval::get_approval),
        )
        .route(
            "/v1/approvals/{approval_id}/resolve",
            post(crate::approval::resolve_approval),
        )
        // Policy/evaluation endpoints
        .route("/v1/intents/compile", post(crate::intents::compile_intent))
        .route("/v1/intents", get(crate::intents::list_intents))
        .route(
            "/v1/proposals/{proposal_id}/evaluate",
            post(crate::proposals::evaluate_proposal),
        )
        .route(
            "/v1/capabilities/mint",
            post(crate::capabilities::mint_capability),
        )
        .route(
            "/v1/capabilities/{capability_id}/revoke",
            post(crate::capabilities::revoke_capability),
        )
        .route(
            "/v1/executions/authorize",
            post(crate::execution::authorize_execution),
        )
        .route(
            "/v1/executions/{execution_id}/prepare",
            post(crate::execution::prepare_execution),
        )
        .route(
            "/v1/executions/{execution_id}/execute",
            post(crate::execution::execute_execution),
        )
        .route(
            "/v1/executions/{execution_id}/verify",
            post(crate::execution::verify_execution),
        )
        .route(
            "/v1/executions/{execution_id}/commit",
            post(crate::execution::commit_execution),
        )
        .route(
            "/v1/executions/{execution_id}/compensate",
            post(crate::execution::compensate_execution),
        )
        .route(
            "/v1/executions/{execution_id}/cancel",
            post(crate::execution::cancel_execution),
        )
        .route(
            "/v1/executions/{execution_id}/evaluate-outcome",
            post(crate::execution::evaluate_outcome),
        )
        // Policy bundle endpoints
        .route(
            "/v1/policy-bundles",
            post(crate::policy::create_policy_bundle),
        )
        .route(
            "/v1/policy-bundles",
            get(crate::policy::list_policy_bundles),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}",
            get(crate::policy::get_policy_bundle),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}",
            put(crate::policy::update_policy_bundle),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}",
            delete(crate::policy::delete_policy_bundle),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}/active",
            put(crate::policy::set_policy_bundle_active),
        )
        .route("/v1/policy/simulate", post(crate::policy::simulate_policy))
        .route(
            "/v1/policy-bundles/simulate",
            post(crate::policy::simulate_policy_bundle),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}/versions",
            get(crate::policy::list_policy_bundle_versions),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}/diff",
            get(crate::policy::diff_policy_bundle_versions),
        )
        .route(
            "/v1/policy-bundles/{bundle_id}/rollback",
            post(crate::policy::rollback_policy_bundle),
        )
        // Admin token endpoints
        .route("/v1/admin/tokens", post(crate::admin::tokens::create_token))
        .route("/v1/admin/tokens", get(crate::admin::tokens::list_tokens))
        .route(
            "/v1/admin/tokens/{token_id}",
            delete(crate::admin::tokens::revoke_token),
        )
        .route(
            "/v1/admin/tokens/{token_id}/rotate",
            post(crate::admin::tokens::rotate_token),
        )
        // Admin agent endpoints
        .route("/v1/admin/agents", post(crate::admin::agents::create_agent))
        .route("/v1/admin/agents", get(crate::admin::agents::list_agents))
        .route(
            "/v1/admin/agents/{agent_id}",
            delete(crate::admin::agents::revoke_agent),
        )
        // Audit log endpoints
        .route("/v1/admin/audit-logs", get(crate::audit::list_audit_logs))
        .route(
            "/v1/admin/audit-logs/export",
            get(crate::audit::export_audit_logs),
        )
        .route(
            "/v1/admin/audit/verify",
            get(crate::audit::verify_audit_chain),
        )
        .route(
            "/v1/admin/audit/merkle-verify",
            get(crate::audit::verify_audit_merkle_root),
        )
        .route(
            "/v1/admin/audit/merkle-roots",
            get(crate::audit::list_audit_merkle_roots),
        )
        .route(
            "/v1/admin/audit/checkpoints",
            post(crate::audit::create_checkpoint),
        )
        .route(
            "/v1/admin/audit/checkpoints",
            get(crate::audit::list_checkpoints),
        )
        .route(
            "/v1/admin/audit/checkpoints/{window_start}/verify",
            get(crate::audit::verify_checkpoint),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

/// Authentication middleware supporting Bearer, Scoped, OIDC, and Agent modes.
async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().as_str().to_string();

    // Skip auth for health and metrics endpoints
    if path == "/v1/healthz"
        || path == "/v1/readyz"
        || path == "/v1/readyz/deep"
        || path == "/v1/metrics"
    {
        return next.run(request).await;
    }

    let config = &state.server_config;

    match config.auth_mode {
        AuthMode::Disabled => next.run(request).await,
        AuthMode::Bearer => {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());
            let Some(header) = auth_header else {
                return auth_error("missing authorization header");
            };
            if !header.starts_with("Bearer ") {
                return auth_error("invalid authorization header format");
            }
            let provided = &header[7..];
            let token = config.bearer_token.as_deref().unwrap_or("");
            if constant_time_eq::constant_time_eq(provided.as_bytes(), token.as_bytes()) {
                next.run(request).await
            } else {
                auth_error("invalid bearer token")
            }
        }
        AuthMode::Oidc => {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());
            let Some(header) = auth_header else {
                return auth_error("missing authorization header");
            };
            if !header.starts_with("Bearer ") {
                return auth_error("invalid authorization header format");
            }
            let provided = &header[7..];
            let oidc = match &config.oidc_config {
                Some(c) => c,
                None => {
                    tracing::error!("oidc config missing");
                    crate::audit::append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AuthFailed,
                        AuditResourceType::Auth,
                        "oidc",
                        "oidc auth misconfigured",
                        Some(serde_json::json!({"reason": "oidc config missing"})),
                    )
                    .await;
                    return auth_error("oidc auth misconfigured");
                }
            };
            match validate_oidc_token(provided, oidc, state.jwks_cache.as_ref(), &method, &path)
                .await
            {
                Ok(()) => next.run(request).await,
                Err(OidcAuthError::Unauthorized(msg)) => {
                    crate::audit::append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AuthFailed,
                        AuditResourceType::Auth,
                        "oidc",
                        "unauthorized",
                        Some(serde_json::json!({"reason": msg})),
                    )
                    .await;
                    auth_error(&msg)
                }
                Err(OidcAuthError::Forbidden(msg)) => {
                    crate::audit::append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AuthFailed,
                        AuditResourceType::Auth,
                        "oidc",
                        "forbidden",
                        Some(serde_json::json!({"reason": msg})),
                    )
                    .await;
                    forbidden_error(&msg)
                }
            }
        }
        AuthMode::Scoped => {
            let auth_header = request
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok());
            let Some(header) = auth_header else {
                return auth_error("missing authorization header");
            };
            if !header.starts_with("Bearer ") {
                return auth_error("invalid authorization header format");
            }
            let provided = &header[7..];
            // Step 1: deterministic lookup hash (fast DB lookup)
            let lookup_hash = hash_token_value(provided);
            let token_repo = state.runtime.store.tokens();
            let token = match token_repo.get_by_lookup_hash(&lookup_hash).await {
                Ok(Some(t)) => t,
                Ok(None) => return auth_error("invalid scoped token"),
                Err(e) => {
                    tracing::error!(error = %e, "token lookup failed");
                    return auth_error("token lookup failed");
                }
            };

            // Step 2: verify presented token against secure salted hash
            let expected_hash = hash_token_with_salt(provided, &token.token_salt);
            if !constant_time_eq::constant_time_eq(
                expected_hash.as_bytes(),
                token.token_hash.as_bytes(),
            ) {
                return auth_error("invalid scoped token");
            }

            // Check revocation
            if token.revoked_at.is_some() {
                return auth_error("token revoked");
            }

            // Check expiration
            if token.expires_at < chrono::Utc::now() {
                return auth_error("token expired");
            }

            // Check scope
            let required_scope = required_scope_for_path(&method, &path);
            if let Some(scope) = required_scope {
                if !token_has_scope(&token, scope) {
                    return forbidden_error(&format!("required scope {}", scope));
                }
            }

            // Update last_used_at (best-effort, fire-and-forget)
            let token_id = token.token_id.clone();
            tokio::spawn(async move {
                let _ = token_repo.touch(&token_id).await;
            });

            next.run(request).await
        }
        AuthMode::Agent => {
            match verify_agent_request(&state, request, next, &method, &path).await {
                Ok(response) => response,
                Err(AgentAuthError::Unauthorized(msg)) => {
                    crate::audit::append_audit(
                        &state.runtime.store,
                        "unknown",
                        AuditAction::AgentAuthFailed,
                        AuditResourceType::Auth,
                        "agent",
                        "unauthorized",
                        Some(serde_json::json!({"reason": msg})),
                    )
                    .await;
                    auth_error(&msg)
                }
                Err(AgentAuthError::Forbidden(msg)) => forbidden_error(&msg),
            }
        }
    }
}

/// Error type for Agent auth failures.
enum AgentAuthError {
    Unauthorized(String),
    Forbidden(String),
}

/// Verify an Ed25519-signed agent request.
///
/// Flow:
/// 1. Extract required headers.
/// 2. Verify timestamp skew.
/// 3. Check nonce replay cache.
/// 4. Recompute body hash and compare.
/// 5. Look up agent and check revocation.
/// 6. Verify Ed25519 signature over canonical payload.
/// 7. Enforce route scope.
async fn verify_agent_request(
    state: &AppState,
    request: axum::extract::Request,
    next: axum::middleware::Next,
    method: &str,
    path: &str,
) -> Result<Response, AgentAuthError> {
    let headers = request.headers().clone();
    let agent_id = headers
        .get("X-Ferrum-Agent-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Agent-Id".to_string()))?;
    let timestamp = headers
        .get("X-Ferrum-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Timestamp".to_string()))?;
    let nonce = headers
        .get("X-Ferrum-Nonce")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Nonce".to_string()))?;
    let body_hash_header = headers
        .get("X-Ferrum-Body-Hash")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Body-Hash".to_string()))?;
    let signature_b64 = headers
        .get("X-Ferrum-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AgentAuthError::Unauthorized("missing X-Ferrum-Signature".to_string()))?;

    // Verify timestamp
    let ts = chrono::DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| AgentAuthError::Unauthorized("invalid timestamp format".to_string()))?
        .with_timezone(&chrono::Utc);
    let now = chrono::Utc::now();
    let skew = chrono::Duration::seconds(state.server_config.agent_clock_skew_secs);
    if ts < now - skew || ts > now + skew {
        return Err(AgentAuthError::Unauthorized(
            "timestamp out of skew window".to_string(),
        ));
    }

    // Verify nonce (replay protection)
    let nonce_ttl =
        StdDuration::from_secs((state.server_config.agent_clock_skew_secs * 2).max(60) as u64);
    {
        let mut cache = state.nonce_cache.lock().unwrap();
        let now_instant = Instant::now();
        cache.retain(|_, &mut inserted| now_instant.duration_since(inserted) < nonce_ttl);
        if cache.contains_key(nonce) {
            return Err(AgentAuthError::Unauthorized("replayed nonce".to_string()));
        }
        cache.insert(nonce.to_string(), now_instant);
    }

    // Read body and verify body hash
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024)
        .await
        .map_err(|_| AgentAuthError::Unauthorized("failed to read body".to_string()))?;
    let computed_body_hash = if bytes.is_empty() {
        "null".to_string()
    } else {
        blake3::hash(&bytes).to_hex().to_string()
    };
    if computed_body_hash != body_hash_header {
        return Err(AgentAuthError::Unauthorized(
            "body hash mismatch".to_string(),
        ));
    }

    // Look up agent
    let agent = match state.runtime.store.agents().get(agent_id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return Err(AgentAuthError::Unauthorized("agent not found".to_string()));
        }
        Err(e) => {
            tracing::error!(error = %e, "agent lookup failed");
            return Err(AgentAuthError::Unauthorized(
                "agent lookup failed".to_string(),
            ));
        }
    };

    if agent.revoked_at.is_some() {
        return Err(AgentAuthError::Unauthorized("agent revoked".to_string()));
    }

    // Canonical payload
    let payload = format!(
        "{}:{}:{}:{}:{}:{}",
        agent_id, timestamp, nonce, body_hash_header, method, path
    );

    // Decode and verify signature
    let sig_bytes =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature_b64)
            .map_err(|_| AgentAuthError::Unauthorized("invalid signature encoding".to_string()))?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| AgentAuthError::Unauthorized("invalid signature length".to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    let pk_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &agent.public_key,
    )
    .map_err(|_| AgentAuthError::Unauthorized("invalid public key encoding".to_string()))?;
    let pk_array: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| AgentAuthError::Unauthorized("invalid public key length".to_string()))?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pk_array)
        .map_err(|_| AgentAuthError::Unauthorized("invalid public key".to_string()))?;

    verifying_key
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| AgentAuthError::Unauthorized("signature verification failed".to_string()))?;

    // Scope enforcement
    if let Some(required) = required_scope_for_path(method, path) {
        let has_scope = agent
            .allowed_scopes
            .iter()
            .any(|s| s == "*" || s == required);
        if !has_scope {
            return Err(AgentAuthError::Forbidden(format!(
                "required scope {}",
                required
            )));
        }
    }

    // Reconstruct request and proceed
    let request = axum::http::Request::from_parts(parts, axum::body::Body::from(bytes));
    Ok(next.run(request).await)
}

fn auth_error(message: &str) -> Response {
    let error = ApiError {
        code: ApiErrorCode::Unauthorized,
        message: message.to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        retriable: false,
        details: serde_json::json!({}),
    };
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

fn forbidden_error(message: &str) -> Response {
    let error = ApiError {
        code: ApiErrorCode::Forbidden,
        message: message.to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        retriable: false,
        details: serde_json::json!({}),
    };
    (StatusCode::FORBIDDEN, Json(error)).into_response()
}

/// Deterministic lookup hash: blake3(raw_token_value).
/// Used for fast DB lookup — NOT for authentication.
pub(crate) fn hash_token_value(token_value: &str) -> String {
    blake3::hash(token_value.as_bytes()).to_hex().to_string()
}

/// Secure verification hash: blake3(salt || token_value).
pub(crate) fn hash_token_with_salt(token_value: &str, salt: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(salt.as_bytes());
    hasher.update(token_value.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Generate a new opaque token value.
pub(crate) fn generate_token_value() -> String {
    let mut bytes = [0u8; 48];
    let u1 = uuid::Uuid::new_v4();
    let u2 = uuid::Uuid::new_v4();
    let u3 = uuid::Uuid::new_v4();
    bytes[0..16].copy_from_slice(u1.as_bytes());
    bytes[16..32].copy_from_slice(u2.as_bytes());
    bytes[32..48].copy_from_slice(u3.as_bytes());
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes);
    format!("fgt_{}", encoded)
}

/// Generate a random 16-byte salt (hex-encoded, 32 chars).
pub(crate) fn generate_token_salt() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

/// Check if a token has a given scope (or wildcard).
fn token_has_scope(token: &ferrum_proto::ScopedToken, scope: &str) -> bool {
    token.scopes.iter().any(|s| s == "*" || s == scope)
}

/// Map HTTP method + path to required scope.
fn required_scope_for_path(method: &str, path: &str) -> Option<&'static str> {
    // Public endpoints (no scope required) are handled before this is called
    match (method, path) {
        // Intent and proposal
        ("POST", "/v1/intents/compile") => Some("intent:submit"),
        ("GET", "/v1/intents") => Some("intent:submit"),
        ("POST", p) if p.starts_with("/v1/proposals/") && p.ends_with("/evaluate") => {
            Some("proposal:evaluate")
        }
        // Capability
        ("POST", "/v1/capabilities/mint") => Some("capability:mint"),
        ("POST", p) if p.starts_with("/v1/capabilities/") && p.ends_with("/revoke") => {
            Some("capability:mint")
        }
        // Execution
        ("POST", "/v1/executions/authorize") => Some("execution:authorize"),
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/prepare") => {
            Some("execution:prepare")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/execute") => {
            Some("execution:execute")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/verify") => {
            Some("execution:verify")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/commit") => {
            Some("execution:execute")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/compensate") => {
            Some("execution:compensate")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/cancel") => {
            Some("execution:execute")
        }
        ("POST", p) if p.starts_with("/v1/executions/") && p.ends_with("/evaluate-outcome") => {
            Some("execution:verify")
        }
        ("GET", p) if p.starts_with("/v1/executions/") => Some("provenance:read"),
        // Approvals
        ("GET", "/v1/approvals") => Some("approval:resolve"),
        ("GET", p) if p.starts_with("/v1/approvals/") && !p.ends_with("/resolve") => {
            Some("approval:resolve")
        }
        ("POST", p) if p.starts_with("/v1/approvals/") && p.ends_with("/resolve") => {
            Some("approval:resolve")
        }
        // Policy bundles
        ("POST", "/v1/policy-bundles") => Some("policy:write"),
        ("GET", "/v1/policy-bundles") => Some("policy:read"),
        ("GET", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/versions") => {
            Some("policy:read")
        }
        ("GET", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/diff") => {
            Some("policy:read")
        }
        ("POST", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/rollback") => {
            Some("policy:write")
        }
        ("POST", "/v1/policy/simulate") => Some("policy:read"),
        ("POST", "/v1/policy-bundles/simulate") => Some("policy:read"),
        ("GET", p) if p.starts_with("/v1/policy-bundles/") => Some("policy:read"),
        ("PUT", p) if p.starts_with("/v1/policy-bundles/") && p.ends_with("/active") => {
            Some("policy:write")
        }
        ("PUT", p) if p.starts_with("/v1/policy-bundles/") => Some("policy:write"),
        ("DELETE", p) if p.starts_with("/v1/policy-bundles/") => Some("policy:write"),
        // Provenance
        ("POST", "/v1/provenance/query") => Some("provenance:read"),
        ("POST", "/v1/provenance/lineage") => Some("provenance:read"),
        ("GET", p) if p.starts_with("/v1/provenance/lineage/") => Some("provenance:read"),
        ("POST", "/v1/provenance/ingest") => Some("provenance:read"),
        // Bridge
        ("GET", "/v1/bridges") => Some("provenance:read"),
        ("GET", p) if p.starts_with("/v1/bridges/") && p.ends_with("/tools") => {
            Some("provenance:read")
        }
        // Admin tokens
        ("POST", "/v1/admin/tokens") => Some("admin:tokens"),
        ("GET", "/v1/admin/tokens") => Some("admin:tokens"),
        ("DELETE", p) if p.starts_with("/v1/admin/tokens/") => Some("admin:tokens"),
        ("POST", p) if p.starts_with("/v1/admin/tokens/") && p.ends_with("/rotate") => {
            Some("admin:tokens")
        }
        // Admin agents
        ("POST", "/v1/admin/agents") => Some("admin:agents"),
        ("GET", "/v1/admin/agents") => Some("admin:agents"),
        ("DELETE", p) if p.starts_with("/v1/admin/agents/") => Some("admin:agents"),
        // Audit logs
        ("GET", "/v1/admin/audit-logs") => Some("admin:audit"),
        ("GET", "/v1/admin/audit-logs/export") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/verify") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/merkle-verify") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/merkle-roots") => Some("admin:audit"),
        ("POST", "/v1/admin/audit/checkpoints") => Some("admin:audit"),
        ("GET", "/v1/admin/audit/checkpoints") => Some("admin:audit"),
        ("GET", p) if p.starts_with("/v1/admin/audit/checkpoints/") && p.ends_with("/verify") => {
            Some("admin:audit")
        }
        _ => Some("admin:tokens"), // Deny-by-default for unknown paths
    }
}

// ---------------------------------------------------------------------------
// Phase 4.3: OIDC/JWT offline validation helpers
// ---------------------------------------------------------------------------

/// Error type for OIDC auth failures.
enum OidcAuthError {
    Unauthorized(String),
    Forbidden(String),
}

/// Validate a Bearer JWT against OIDC config.
///
/// Flow:
/// 1. Decode header, select static key by `kid`.
/// 2. If static key missing and jwks_url configured, fetch from JWKS cache.
/// 3. Validate signature, algorithm allowlist, issuer, audience, exp, nbf.
/// 4. Map actor_id from configured claim.
/// 5. Map role from configured role/group claims via explicit mapping table.
/// 6. Derive scopes via `TokenRole::default_scopes()`.
/// 7. Enforce `required_scope_for_path()`.
///
/// Fail closed: any validation failure returns `OidcAuthError::Unauthorized`.
/// Unmapped role or missing required scope returns `OidcAuthError::Forbidden`.
async fn validate_oidc_token(
    token: &str,
    oidc: &crate::OidcConfig,
    jwks_cache: Option<&Arc<OidcJwksCache>>,
    method: &str,
    path: &str,
) -> Result<(), OidcAuthError> {
    // Step 1: decode header to get kid and alg
    let header = match jsonwebtoken::decode_header(token) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(error = %e, "failed to decode jwt header");
            return Err(OidcAuthError::Unauthorized("invalid jwt".to_string()));
        }
    };

    // Reject "none" algorithm unconditionally
    if header.alg == jsonwebtoken::Algorithm::HS256
        && !oidc
            .allowed_algorithms
            .contains(&jsonwebtoken::Algorithm::HS256)
    {
        // HS256 is only allowed if explicitly listed (tests)
    }
    if !oidc.allowed_algorithms.contains(&header.alg) {
        tracing::warn!(alg = ?header.alg, "jwt algorithm not in allowlist");
        return Err(OidcAuthError::Unauthorized(
            "unsupported jwt algorithm".to_string(),
        ));
    }

    // Step 2: select key by kid (empty string fallback for JWTs without kid)
    let kid = header.kid.as_deref().unwrap_or("");
    let key_material = if let Some(km) = oidc.static_keys.get(kid) {
        km.clone()
    } else if let Some(cache) = jwks_cache {
        match cache.get_key(kid).await {
            Ok(Some(km)) => km,
            Ok(None) => {
                tracing::warn!(kid = %kid, "jwt key not found in static keys or jwks");
                return Err(OidcAuthError::Unauthorized("jwt key not found".to_string()));
            }
            Err(e) => {
                tracing::warn!(error = %e, kid = %kid, "jwks fetch failed");
                return Err(OidcAuthError::Unauthorized(
                    "jwt key unavailable".to_string(),
                ));
            }
        }
    } else {
        tracing::warn!(kid = %kid, "jwt key not found in static keys");
        return Err(OidcAuthError::Unauthorized("jwt key not found".to_string()));
    };

    let decoding_key = match key_material.to_decoding_key() {
        Ok(k) => k,
        Err(e) => {
            tracing::error!(error = %e, "failed to build decoding key");
            return Err(OidcAuthError::Unauthorized("jwt key invalid".to_string()));
        }
    };

    // Step 3: build validation
    let mut validation = jsonwebtoken::Validation::new(header.alg);
    validation.leeway = oidc.clock_skew_secs.max(0) as u64;
    validation.validate_nbf = true;
    validation.set_issuer(&[&oidc.issuer]);
    validation.set_audience(&oidc.audiences);
    validation.algorithms = oidc.allowed_algorithms.clone();

    // Step 4: decode and validate signature + claims
    let token_data: jsonwebtoken::TokenData<serde_json::Map<String, serde_json::Value>> =
        match jsonwebtoken::decode(token, &decoding_key, &validation) {
            Ok(td) => td,
            Err(e) => {
                tracing::warn!(error = %e, "jwt validation failed");
                return Err(OidcAuthError::Unauthorized("invalid jwt".to_string()));
            }
        };

    let claims = token_data.claims;

    // Step 4b: explicit future-iat rejection (fail closed).
    // If `iat` is present and beyond now + clock_skew, reject.
    // Missing `iat` is tolerated to avoid breaking IdPs that omit it.
    if let Some(iat_val) = claims.get("iat").and_then(|v| v.as_i64()) {
        let now = chrono::Utc::now().timestamp();
        let skew = oidc.clock_skew_secs.max(0);
        if iat_val > now + skew {
            tracing::warn!(iat = %iat_val, now = %now, skew = %skew, "jwt iat is in the future");
            return Err(OidcAuthError::Unauthorized("invalid jwt".to_string()));
        }
    }

    // Step 5: email verification check
    if oidc.require_email_verified {
        let verified = claims
            .get("email_verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !verified {
            tracing::warn!("jwt email_verified is false or missing");
            return Err(OidcAuthError::Unauthorized(
                "email not verified".to_string(),
            ));
        }
    }

    // Step 6: extract actor_id
    let actor_id = match claims.get(&oidc.actor_id_claim).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            tracing::warn!(claim = %oidc.actor_id_claim, "jwt missing actor_id claim");
            return Err(OidcAuthError::Unauthorized(
                "missing actor_id claim".to_string(),
            ));
        }
    };

    // Step 7: extract role_source claim and map to TokenRole
    let role_source_values = match claims.get(&oidc.role_source_claim) {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => {
            tracing::warn!(
                claim = %oidc.role_source_claim,
                "jwt missing role_source claim"
            );
            return Err(OidcAuthError::Forbidden("unmapped role".to_string()));
        }
    };

    let mapped_role = role_source_values
        .iter()
        .filter_map(|name| oidc.role_mappings.get(name))
        .next()
        .copied();

    let role = match mapped_role {
        Some(r) => r,
        None => {
            tracing::warn!(
                values = ?role_source_values,
                "jwt role not mapped"
            );
            return Err(OidcAuthError::Forbidden("unmapped role".to_string()));
        }
    };

    // Step 8: derive scopes from role
    let scopes = role.default_scopes();

    // Step 9: enforce required scope for path
    if let Some(required) = required_scope_for_path(method, path) {
        let has_scope = scopes.iter().any(|s| s == "*" || s == required);
        if !has_scope {
            tracing::warn!(
                actor_id = %actor_id,
                role = ?role,
                required = %required,
                "jwt insufficient scope"
            );
            return Err(OidcAuthError::Forbidden(format!(
                "required scope {}",
                required
            )));
        }
    }

    tracing::debug!(actor_id = %actor_id, role = ?role, "oidc auth succeeded");
    Ok(())
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
#[cfg(test)]
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

// ── Audit Log Handler ──
// Audit log endpoints (list/verify/merkle/checkpoint/export) and the
// `append_audit` helper are now defined in `crate::audit` and re-exported
// through that module. They were previously duplicated inline here while
// the staged extraction was in progress; with this slice they are fully
// served by the extracted module so the auth/audit chain stays canonical.

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
    use crate::{KeyMaterial, OidcConfig};
    use axum::{body::Body, http::Request};
    use chrono::DurationRound;
    use ferrum_cap::InMemoryCapabilityService;
    use ferrum_pdp::StaticPdpEngine;
    use ferrum_proto::{DeepHealthResponse, ProvenanceIngestRequest, ProvenanceIngestResponse};
    use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
    use ferrum_store::repos::{
        AgentRepo, ApprovalRepo, AuditCheckpointRepo, AuditLogRepo, AuditMerkleRootRepo,
        CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, PolicyBundleRepo, ProposalRepo,
        ProvenanceRepo, RollbackRepo, TokenRepo,
    };
    use ferrum_store::{SqliteStore, StoreError, StoreFacade};
    use ferrum_sync::{BridgeToolInfo, ExternalEventSource, McpBridge};
    use sha2::Digest;
    use std::sync::Arc;
    use tower::ServiceExt;

    use ed25519_dalek::Signer;

    fn generate_agent_keypair() -> (ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey) {
        let mut csprng = rand::rngs::OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    fn compute_fingerprint(public_key: &ed25519_dalek::VerifyingKey) -> String {
        use sha2::Digest;
        let hash = sha2::Sha256::digest(public_key.as_bytes());
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, hash)
    }

    async fn register_test_agent(
        store: &Arc<dyn StoreFacade>,
        agent_id: &str,
        public_key_b64: &str,
        fingerprint: &str,
        scopes: Vec<String>,
    ) {
        let agent = ferrum_proto::AgentRecord {
            agent_id: agent_id.to_string(),
            public_key: public_key_b64.to_string(),
            key_fingerprint: fingerprint.to_string(),
            allowed_scopes: scopes,
            created_at: chrono::Utc::now(),
            revoked_at: None,
            description: None,
        };
        store.agents().insert(&agent).await.unwrap();
    }

    fn sign_agent_request(
        signing_key: &ed25519_dalek::SigningKey,
        agent_id: &str,
        timestamp: &str,
        nonce: &str,
        body_hash: &str,
        method: &str,
        path: &str,
    ) -> String {
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            agent_id, timestamp, nonce, body_hash, method, path
        );
        let signature = signing_key.sign(payload.as_bytes());
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signature.to_bytes(),
        )
    }

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
        fn tokens(&self) -> Arc<dyn TokenRepo> {
            self.inner.tokens()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            self.inner.audit_log()
        }
        fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
            self.inner.audit_merkle_roots()
        }
        fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
            self.inner.audit_checkpoints()
        }
        fn agents(&self) -> Arc<dyn AgentRepo> {
            self.inner.agents()
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
        assert_eq!(deep.components.len(), 3);
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
        // Third component: pool (not applicable for non-PG stores)
        assert_eq!(deep.components[2].component, "pool");
        assert!(deep.components[2].healthy);
        assert!(deep.components[2].error.is_none());
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

        // Verify all components are present
        assert_eq!(deep.status, "ok");
        assert!(deep.healthy);
        assert_eq!(deep.components.len(), 3);

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

        // Third component: pool (not applicable for non-PG stores)
        assert_eq!(deep.components[2].component, "pool");
        assert!(deep.components[2].healthy);
        assert!(deep.components[2].error.is_none());
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
        fn tokens(&self) -> Arc<dyn TokenRepo> {
            self.inner.tokens()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            self.inner.audit_log()
        }
        fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
            self.inner.audit_merkle_roots()
        }
        fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
            self.inner.audit_checkpoints()
        }
        fn agents(&self) -> Arc<dyn AgentRepo> {
            self.inner.agents()
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
        assert_eq!(deep.components.len(), 3);

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

        // Third component: pool (not applicable for non-PG stores)
        assert_eq!(deep.components[2].component, "pool");
        assert!(deep.components[2].healthy);
        assert!(deep.components[2].error.is_none());
    }

    /// A test-only StoreFacade that wraps a real store and reports a fixed pool_status.
    struct MockPgPoolStoreFacade {
        inner: Arc<dyn StoreFacade>,
        pool_status: Option<ferrum_store::PoolStatus>,
    }

    impl MockPgPoolStoreFacade {
        fn new(inner: Arc<dyn StoreFacade>, pool_status: Option<ferrum_store::PoolStatus>) -> Self {
            Self { inner, pool_status }
        }
    }

    #[async_trait::async_trait]
    impl StoreFacade for MockPgPoolStoreFacade {
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
        fn tokens(&self) -> Arc<dyn TokenRepo> {
            self.inner.tokens()
        }
        fn audit_log(&self) -> Arc<dyn AuditLogRepo> {
            self.inner.audit_log()
        }
        fn audit_merkle_roots(&self) -> Arc<dyn AuditMerkleRootRepo> {
            self.inner.audit_merkle_roots()
        }
        fn audit_checkpoints(&self) -> Arc<dyn AuditCheckpointRepo> {
            self.inner.audit_checkpoints()
        }
        fn agents(&self) -> Arc<dyn AgentRepo> {
            self.inner.agents()
        }
        fn write_queue_depth(&self) -> usize {
            self.inner.write_queue_depth()
        }
        async fn health_check(&self) -> Result<(), StoreError> {
            self.inner.health_check().await
        }
        fn pool_status(&self) -> Option<ferrum_store::PoolStatus> {
            self.pool_status
        }
    }

    async fn test_runtime_with_pool_status(
        pool_status: Option<ferrum_store::PoolStatus>,
    ) -> GatewayRuntime {
        let pdp = Arc::new(StaticPdpEngine);
        let cap = Arc::new(InMemoryCapabilityService::default());

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

        let store = Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
        store.apply_embedded_migrations().await.unwrap();

        let mock_store =
            Arc::new(MockPgPoolStoreFacade::new(store, pool_status)) as Arc<dyn StoreFacade>;

        GatewayRuntime::new(pdp, cap, rollback, mock_store, vec![])
    }

    #[tokio::test]
    async fn test_metrics_includes_pg_pool_status_when_present() {
        let runtime = test_runtime_with_pool_status(Some(ferrum_store::PoolStatus {
            total_connections: 7,
            idle_connections: 3,
            max_connections: 20,
            acquire_timeouts: 0,
        }))
        .await;

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

        assert!(body_str.contains("# HELP ferrumgate_store_pg_pool_size"));
        assert!(body_str.contains("# TYPE ferrumgate_store_pg_pool_size gauge"));
        assert!(body_str.contains("ferrumgate_store_pg_pool_size 7"));
        assert!(body_str.contains("ferrumgate_store_pg_pool_idle 3"));
        assert!(body_str.contains("ferrumgate_store_pg_pool_max 20"));
    }

    #[tokio::test]
    async fn test_metrics_omits_pg_pool_status_when_absent() {
        let runtime = test_runtime_with_pool_status(None).await;

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

        assert!(!body_str.contains("ferrumgate_store_pg_pool_size"));
        assert!(!body_str.contains("ferrumgate_store_pg_pool_idle"));
        assert!(!body_str.contains("ferrumgate_store_pg_pool_max"));
    }

    #[tokio::test]
    async fn test_metrics_includes_pg_acquire_timeouts_when_present() {
        let runtime = test_runtime_with_pool_status(Some(ferrum_store::PoolStatus {
            total_connections: 5,
            idle_connections: 1,
            max_connections: 10,
            acquire_timeouts: 3,
        }))
        .await;

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

        assert!(body_str.contains("# HELP ferrumgate_store_pg_acquire_timeouts_total"));
        assert!(body_str.contains("# TYPE ferrumgate_store_pg_acquire_timeouts_total counter"));
        assert!(body_str.contains("ferrumgate_store_pg_acquire_timeouts_total 3"));
    }

    #[tokio::test]
    async fn test_readyz_deep_returns_503_when_pool_saturated() {
        let runtime = test_runtime_with_pool_status(Some(ferrum_store::PoolStatus {
            total_connections: 10,
            idle_connections: 0,
            max_connections: 10,
            acquire_timeouts: 0,
        }))
        .await;

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
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "degraded");
        assert_eq!(json["healthy"], false);
        let components = json["components"].as_array().unwrap();
        let pool_component = components
            .iter()
            .find(|c| c["component"] == "pool")
            .unwrap();
        assert_eq!(pool_component["healthy"], false);
        assert!(
            pool_component["status"]
                .as_str()
                .unwrap()
                .contains("saturated")
        );
    }

    #[tokio::test]
    async fn test_readyz_deep_returns_200_when_pool_not_saturated() {
        let runtime = test_runtime_with_pool_status(Some(ferrum_store::PoolStatus {
            total_connections: 8,
            idle_connections: 2,
            max_connections: 10,
            acquire_timeouts: 0,
        }))
        .await;

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
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["healthy"], true);
        let components = json["components"].as_array().unwrap();
        let pool_component = components
            .iter()
            .find(|c| c["component"] == "pool")
            .unwrap();
        assert_eq!(pool_component["healthy"], true);
        assert!(pool_component["status"].as_str().unwrap().contains("ok"));
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
        let response: crate::bridge::BridgeListResponse = serde_json::from_slice(&body).unwrap();
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
        let response: crate::bridge::BridgeListResponse = serde_json::from_slice(&body).unwrap();
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
        let response: crate::bridge::BridgeToolsResponse = serde_json::from_slice(&body).unwrap();
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
                GovernanceRoute::ExecutionsCommit,
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
                GovernanceRoute::PolicySimulate,
                GovernanceRoute::PolicyBundlesSimulate,
                GovernanceRoute::PolicyBundlesVersions,
                GovernanceRoute::PolicyBundlesDiff,
                GovernanceRoute::PolicyBundlesRollback,
                GovernanceRoute::ProvenanceQuery,
                GovernanceRoute::ProvenanceLineage,
                GovernanceRoute::ProvenanceLineageExecutionId,
                GovernanceRoute::ProvenanceIngest,
                GovernanceRoute::BridgesBridgeIdTools,
                GovernanceRoute::AgentsCreate,
                GovernanceRoute::AgentsList,
                GovernanceRoute::AgentsRevoke,
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
                GovernanceRoute::ExecutionsCommit => (),
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
                GovernanceRoute::PolicySimulate => (),
                GovernanceRoute::PolicyBundlesSimulate => (),
                GovernanceRoute::PolicyBundlesVersions => (),
                GovernanceRoute::PolicyBundlesDiff => (),
                GovernanceRoute::PolicyBundlesRollback => (),
                GovernanceRoute::ProvenanceQuery => (),
                GovernanceRoute::ProvenanceLineage => (),
                GovernanceRoute::ProvenanceLineageExecutionId => (),
                GovernanceRoute::ProvenanceIngest => (),
                GovernanceRoute::BridgesBridgeIdTools => (),
                GovernanceRoute::AgentsCreate => (),
                GovernanceRoute::AgentsList => (),
                GovernanceRoute::AgentsRevoke => (),
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

    // ---------------------------------------------------------------------------
    // POL-2: Policy bundle simulate endpoint tests (side-effect free)
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_simulate_policy_bundle_returns_matched_decision() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        let bundle_yaml = r#"version: "0.1.0"
bundle_id: "test-simulate-bundle"
rules:
  - id: "deny.mutation"
    description: "Deny mutating actions"
    decision: "Deny"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;

        let proposal = ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.write".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "write file".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Medium,
            requested_rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };

        let request = PolicyBundleSimulateRequest {
            bundle_yaml: bundle_yaml.to_string(),
            proposal,
            intent: None,
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles/simulate")
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
        let result: PolicyBundleSimulateResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.decision, ferrum_proto::Decision::Deny);
        assert!(
            result.reason.contains("deny.mutation"),
            "reason should mention matched rule: {}",
            result.reason
        );
        assert_eq!(result.matched_rule_ids.len(), 1);
    }

    #[tokio::test]
    async fn test_simulate_policy_bundle_no_match_returns_allow() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        let bundle_yaml = r#"version: "0.1.0"
bundle_id: "test-simulate-bundle"
rules:
  - id: "deny.scope.mismatch"
    description: "Deny scope mismatch"
    decision: "Deny"
    priority: 100
    matchers:
      - type: "scope_mismatch"
"#;

        let proposal = ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.read".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "read file".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };

        let request = PolicyBundleSimulateRequest {
            bundle_yaml: bundle_yaml.to_string(),
            proposal,
            intent: None,
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles/simulate")
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
        let result: PolicyBundleSimulateResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.decision, ferrum_proto::Decision::Allow);
        assert!(result.reason.contains("no rules matched"));
        assert!(result.matched_rule_ids.is_empty());
    }

    #[tokio::test]
    async fn test_simulate_policy_bundle_does_not_persist() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        let bundle_yaml = r#"version: "0.1.0"
bundle_id: "test-simulate-bundle"
rules:
  - id: "quarantine.high.taint"
    description: "Quarantine high taint"
    decision: "Quarantine"
    priority: 100
    matchers:
      - type: "taint_at_least"
        value: 50
"#;

        let proposal = ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.read".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "read file".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: vec!["external".to_string()],
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };

        let request = PolicyBundleSimulateRequest {
            bundle_yaml: bundle_yaml.to_string(),
            proposal: proposal.clone(),
            intent: None,
        };

        // Call simulate
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles/simulate")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify the proposal was NOT persisted
        let persisted_proposal = runtime
            .store
            .proposals()
            .get(proposal.proposal_id)
            .await
            .unwrap();
        assert!(
            persisted_proposal.is_none(),
            "simulate must not persist proposals"
        );

        // Verify the bundle was NOT persisted
        let bundle_list = runtime.store.policy_bundles().list().await.unwrap();
        assert!(bundle_list.is_empty(), "simulate must not persist bundles");
    }

    #[tokio::test]
    async fn test_simulate_policy_runtime_returns_pdp_decision() {
        let runtime = test_runtime().await;
        let router = build_router(runtime);

        let proposal = ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.read".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "read file".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };

        let request = PolicySimulateRequest {
            proposal,
            intent: None,
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/simulate")
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
        let result: EvaluateProposalResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(result.decision, ferrum_proto::Decision::Allow);
    }

    #[tokio::test]
    async fn test_simulate_policy_runtime_does_not_persist() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        let proposal = ferrum_proto::ActionProposal {
            proposal_id: ferrum_proto::ProposalId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.write".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "write file".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Medium,
            requested_rollback_class: ferrum_proto::RollbackClass::R1SnapshotRecoverable,
            taint_inputs: Vec::new(),
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        };

        let request = PolicySimulateRequest {
            proposal: proposal.clone(),
            intent: None,
        };

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy/simulate")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Verify the proposal was NOT persisted
        let persisted_proposal = runtime
            .store
            .proposals()
            .get(proposal.proposal_id)
            .await
            .unwrap();
        assert!(
            persisted_proposal.is_none(),
            "runtime simulate must not persist proposals"
        );
    }

    #[tokio::test]
    async fn test_list_policy_bundle_versions() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create a bundle
        let yaml = r#"version: "0.1.0"
bundle_id: "version-test-bundle"
rules:
  - id: "rule1"
    description: "Test rule"
    decision: "Allow"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;
        let create_req = ferrum_proto::CreatePolicyBundleRequest {
            yaml_content: yaml.to_string(),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Update the bundle
        let yaml2 = r#"version: "0.1.0"
bundle_id: "version-test-bundle"
rules:
  - id: "rule1"
    description: "Test rule updated"
    decision: "Deny"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;
        let update_req = ferrum_proto::UpdatePolicyBundleRequest {
            yaml_content: yaml2.to_string(),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/policy-bundles/version-test-bundle")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&update_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // List versions
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/policy-bundles/version-test-bundle/versions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: ListPolicyBundleVersionsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.total, 2);
        assert_eq!(resp.versions[0].version, 2);
        assert_eq!(resp.versions[1].version, 1);
    }

    #[tokio::test]
    async fn test_diff_policy_bundle_versions() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create a bundle
        let yaml = r#"version: "0.1.0"
bundle_id: "diff-test-bundle"
rules:
  - id: "rule1"
    description: "Test rule"
    decision: "Allow"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;
        let create_req = ferrum_proto::CreatePolicyBundleRequest {
            yaml_content: yaml.to_string(),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Update the bundle
        let yaml2 = r#"version: "0.1.0"
bundle_id: "diff-test-bundle"
rules:
  - id: "rule1"
    description: "Test rule updated"
    decision: "Deny"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;
        let update_req = ferrum_proto::UpdatePolicyBundleRequest {
            yaml_content: yaml2.to_string(),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/policy-bundles/diff-test-bundle")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&update_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Diff versions
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/policy-bundles/diff-test-bundle/diff?from=1&to=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: DiffPolicyBundleVersionsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.bundle_id, "diff-test-bundle");
        assert_eq!(resp.from_version, 1);
        assert_eq!(resp.to_version, 2);
        // The diff should contain changes to the rule description/decision
        assert!(resp.diff.get("changed").is_some());
    }

    #[tokio::test]
    async fn test_rollback_policy_bundle() {
        let runtime = test_runtime().await;
        let router = build_router(runtime.clone());

        // Create a bundle
        let yaml = r#"version: "0.1.0"
bundle_id: "rollback-test-bundle"
rules:
  - id: "rule1"
    description: "Test rule"
    decision: "Allow"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;
        let create_req = ferrum_proto::CreatePolicyBundleRequest {
            yaml_content: yaml.to_string(),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Update the bundle
        let yaml2 = r#"version: "0.1.0"
bundle_id: "rollback-test-bundle"
rules:
  - id: "rule1"
    description: "Test rule updated"
    decision: "Deny"
    priority: 100
    matchers:
      - type: "action_is_mutation"
"#;
        let update_req = ferrum_proto::UpdatePolicyBundleRequest {
            yaml_content: yaml2.to_string(),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/v1/policy-bundles/rollback-test-bundle")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&update_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Rollback to v1
        let rollback_req = RollbackPolicyBundleRequest {
            target_version: 1,
            actor: Some("test-operator".to_string()),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles/rollback-test-bundle/rollback")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&rollback_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: RollbackPolicyBundleResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.rolled_back_to_version, 1);
        assert_eq!(resp.new_version, 3);

        // Verify the bundle content is back to v1
        let bundle = runtime
            .store
            .policy_bundles()
            .get("rollback-test-bundle")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bundle.rules[0].decision, ferrum_proto::Decision::Allow);

        // Verify provenance event was emitted
        let events = runtime
            .store
            .provenance()
            .query(&ProvenanceQueryRequest {
                intent_id: None,
                execution_id: None,
                capability_id: None,
                event_kind: Some(ProvenanceEventKind::PolicyBundleRolledBack),
                since: None,
                until: None,
                edge_types: Vec::new(),
            })
            .await
            .unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e.kind, ProvenanceEventKind::PolicyBundleRolledBack)),
            "rollback should emit PolicyBundleRolledBack provenance event"
        );
    }

    // ── Scoped Token Tests ──

    async fn test_runtime_with_scoped_auth() -> (GatewayRuntime, ServerConfig) {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        (runtime, config)
    }

    #[tokio::test]
    async fn test_scoped_token_create_and_list() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        // Create a token with global bearer fallback not available in Scoped mode;
        // we need an admin token first. In Scoped mode, the first token creation
        // requires a bootstrap mechanism. For testing, we insert directly via store.
        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_admin_1".to_string(),
            actor_id: "admin".to_string(),
            role: ferrum_proto::TokenRole::Admin,
            scopes: vec!["*".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        // List tokens via API
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/admin/tokens")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_scoped_token_revoked_fails() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_revoke_test".to_string(),
            actor_id: "operator".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: vec!["approval:resolve".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: Some(chrono::Utc::now()),
            revoked_reason: Some("test".to_string()),
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_scoped_token_expired_fails() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_expired_test".to_string(),
            actor_id: "operator".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: vec!["approval:resolve".to_string()],
            description: None,
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
            created_at: chrono::Utc::now() - chrono::Duration::days(2),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_sec1_read_only_token_cannot_mutate() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_readonly_test".to_string(),
            actor_id: "auditor".to_string(),
            role: ferrum_proto::TokenRole::ReadOnly,
            scopes: vec!["policy:read".to_string(), "provenance:read".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        // Attempt to create a policy bundle (requires policy:write)
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/policy-bundles")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_sec2_agent_token_cannot_approve() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_agent_test".to_string(),
            actor_id: "agent".to_string(),
            role: ferrum_proto::TokenRole::Agent,
            scopes: vec![
                "intent:submit".to_string(),
                "proposal:evaluate".to_string(),
                "capability:mint".to_string(),
            ],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        // Attempt to resolve an approval (requires approval:resolve)
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/approvals/test-approval/resolve")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_sec3_auditor_token_cannot_execute() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_auditor_test".to_string(),
            actor_id: "auditor".to_string(),
            role: ferrum_proto::TokenRole::Auditor,
            scopes: vec!["provenance:read".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        // Attempt to authorize an execution (requires execution:authorize)
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/executions/authorize")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_sec4_revoked_token_returns_401() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_sec4_test".to_string(),
            actor_id: "operator".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: vec!["approval:resolve".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: Some(chrono::Utc::now()),
            revoked_reason: Some("test".to_string()),
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_sec5_expired_token_returns_401() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_sec5_test".to_string(),
            actor_id: "operator".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: vec!["approval:resolve".to_string()],
            description: None,
            expires_at: chrono::Utc::now() - chrono::Duration::hours(1),
            created_at: chrono::Utc::now() - chrono::Duration::days(2),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_sec6_unmapped_route_fail_closed_with_valid_token() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_sec6_test".to_string(),
            actor_id: "readonly".to_string(),
            role: ferrum_proto::TokenRole::ReadOnly,
            scopes: vec!["policy:read".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        // Unmapped routes deny-by-default (require admin:tokens scope)
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/unknown")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_sec7_no_auth_unknown_route_returns_401() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        // Unmapped route without auth header should fail closed with 401
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_token_admin_api_create_and_revoke() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        // Bootstrap an admin token directly in the store
        let admin_token_value = generate_token_value();
        let admin_token_salt = generate_token_salt();
        let admin_token_lookup_hash = hash_token_value(&admin_token_value);
        let admin_token_hash = hash_token_with_salt(&admin_token_value, &admin_token_salt);
        let admin_token = ferrum_proto::ScopedToken {
            token_id: "tok_admin_bootstrap".to_string(),
            actor_id: "admin".to_string(),
            role: ferrum_proto::TokenRole::Admin,
            scopes: vec!["*".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash: admin_token_lookup_hash,
            token_hash: admin_token_hash,
            token_salt: admin_token_salt,
        };
        runtime.store.tokens().insert(&admin_token).await.unwrap();

        // Create a new token via API
        let create_req = ferrum_proto::CreateTokenRequest {
            actor_id: "operator-alice".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: None,
            description: Some("Test token".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::days(7),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/tokens")
                    .header("Authorization", format!("Bearer {}", admin_token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let create_resp: ferrum_proto::CreateTokenResponse = serde_json::from_slice(&body).unwrap();
        assert!(
            create_resp.token_value.starts_with("fgt_"),
            "token value should start with fgt_ prefix"
        );

        // Revoke the newly created token
        let revoke_req = ferrum_proto::RevokeTokenRequest {
            reason: Some("test cleanup".to_string()),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/admin/tokens/{}", create_resp.token.token_id))
                    .header("Authorization", format!("Bearer {}", admin_token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&revoke_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify the revoked token cannot be used
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/approvals")
                    .header(
                        "Authorization",
                        format!("Bearer {}", create_resp.token_value),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_token_rejects_excessive_ttl() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        // Bootstrap an admin token
        let admin_token_value = generate_token_value();
        let admin_token_salt = generate_token_salt();
        let admin_token_lookup_hash = hash_token_value(&admin_token_value);
        let admin_token_hash = hash_token_with_salt(&admin_token_value, &admin_token_salt);
        let admin_token = ferrum_proto::ScopedToken {
            token_id: "tok_admin_ttl".to_string(),
            actor_id: "admin".to_string(),
            role: ferrum_proto::TokenRole::Admin,
            scopes: vec!["*".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash: admin_token_lookup_hash,
            token_hash: admin_token_hash,
            token_salt: admin_token_salt,
        };
        runtime.store.tokens().insert(&admin_token).await.unwrap();

        // Request a token with 91-day expiry
        let create_req = ferrum_proto::CreateTokenRequest {
            actor_id: "operator-alice".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: None,
            description: Some("Excessive TTL test".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::days(91),
        };
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/tokens")
                    .header("Authorization", format!("Bearer {}", admin_token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rotate_token_rejects_excessive_ttl() {
        let (runtime, config) = test_runtime_with_scoped_auth().await;
        let router = build_router_with_auth(runtime.clone(), config);

        // Bootstrap an admin token
        let admin_token_value = generate_token_value();
        let admin_token_salt = generate_token_salt();
        let admin_token_lookup_hash = hash_token_value(&admin_token_value);
        let admin_token_hash = hash_token_with_salt(&admin_token_value, &admin_token_salt);
        let admin_token = ferrum_proto::ScopedToken {
            token_id: "tok_admin_rot".to_string(),
            actor_id: "admin".to_string(),
            role: ferrum_proto::TokenRole::Admin,
            scopes: vec!["*".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash: admin_token_lookup_hash,
            token_hash: admin_token_hash,
            token_salt: admin_token_salt,
        };
        runtime.store.tokens().insert(&admin_token).await.unwrap();

        // Create a token to rotate
        let target_token_value = generate_token_value();
        let target_token_salt = generate_token_salt();
        let target_token_lookup_hash = hash_token_value(&target_token_value);
        let target_token_hash = hash_token_with_salt(&target_token_value, &target_token_salt);
        let target_token = ferrum_proto::ScopedToken {
            token_id: "tok_to_rotate".to_string(),
            actor_id: "operator".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: vec!["approval:resolve".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash: target_token_lookup_hash,
            token_hash: target_token_hash,
            token_salt: target_token_salt,
        };
        runtime.store.tokens().insert(&target_token).await.unwrap();

        // Rotate with 91-day expiry
        let rotate_req = ferrum_proto::RotateTokenRequest {
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(91)),
            reason: Some("test rotation".to_string()),
        };
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/tokens/tok_to_rotate/rotate")
                    .header("Authorization", format!("Bearer {}", admin_token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&rotate_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── OIDC/JWT Offline Validation Tests (Phase 4.3) ──

    fn test_oidc_config() -> OidcConfig {
        let mut role_mappings = std::collections::HashMap::new();
        role_mappings.insert("fg-admins".to_string(), ferrum_proto::TokenRole::Admin);
        role_mappings.insert(
            "fg-operators".to_string(),
            ferrum_proto::TokenRole::Operator,
        );
        role_mappings.insert("fg-readonly".to_string(), ferrum_proto::TokenRole::ReadOnly);

        let mut static_keys = std::collections::HashMap::new();
        static_keys.insert(
            "test-key-1".to_string(),
            KeyMaterial::Hmac(b"test-secret-key-for-hs256-only".to_vec()),
        );
        // Also register a fallback key for JWTs without kid
        static_keys.insert(
            "".to_string(),
            KeyMaterial::Hmac(b"test-secret-key-for-hs256-only".to_vec()),
        );

        OidcConfig {
            issuer: "https://test-issuer.example.com".to_string(),
            audiences: vec!["ferrumgate-test".to_string()],
            clock_skew_secs: 30,
            actor_id_claim: "sub".to_string(),
            role_source_claim: "groups".to_string(),
            role_mappings,
            allowed_algorithms: vec![jsonwebtoken::Algorithm::HS256],
            static_keys,
            require_email_verified: false,
            jwks_url: None,
            jwks_cache_ttl_secs: 300,
        }
    }

    fn mint_test_jwt(
        claims: serde_json::Map<String, serde_json::Value>,
        kid: Option<&str>,
    ) -> String {
        let header = jsonwebtoken::Header {
            typ: Some("JWT".to_string()),
            alg: jsonwebtoken::Algorithm::HS256,
            kid: kid.map(|s| s.to_string()),
            ..Default::default()
        };
        let key = jsonwebtoken::EncodingKey::from_secret(b"test-secret-key-for-hs256-only");
        jsonwebtoken::encode(&header, &claims, &key).unwrap()
    }

    fn test_oidc_server_config() -> ServerConfig {
        ServerConfig {
            auth_mode: AuthMode::Oidc,
            oidc_config: Some(test_oidc_config()),
            ..ServerConfig::default()
        }
    }

    #[tokio::test]
    async fn test_oidc_valid_jwt_with_mapped_role_allows_access() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        // /v1/approvals requires "approval:resolve" which Operator has
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_oidc_expired_jwt_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() - chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_wrong_issuer_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://wrong-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_wrong_audience_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("wrong-audience"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_unmapped_role_returns_403() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-unknown-role"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_oidc_valid_role_but_missing_scope_returns_403() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        // ReadOnly role has "policy:read" and "provenance:read" but not "approval:resolve"
        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-readonly"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        // /v1/approvals requires "approval:resolve" which ReadOnly does NOT have
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_oidc_missing_auth_header_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let response = router
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
    async fn test_oidc_invalid_signature_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        // Sign with a different secret to create an invalid signature
        let header = jsonwebtoken::Header {
            typ: Some("JWT".to_string()),
            alg: jsonwebtoken::Algorithm::HS256,
            kid: Some("test-key-1".to_string()),
            ..Default::default()
        };
        let bad_key = jsonwebtoken::EncodingKey::from_secret(b"wrong-secret");
        let jwt = jsonwebtoken::encode(&header, &claims, &bad_key).unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_missing_actor_id_claim_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        // No "sub" claim
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_readonly_role_can_access_allowed_route() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-readonly"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        // /v1/policy-bundles GET requires "policy:read" which ReadOnly has
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/policy-bundles")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_oidc_healthz_is_public_under_oidc_auth() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();

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
    async fn test_oidc_admin_role_wildcard_scope_allows_all_routes() {
        let runtime = test_runtime().await;
        let mut config = test_oidc_server_config();
        let mut role_mappings = std::collections::HashMap::new();
        role_mappings.insert("fg-admins".to_string(), ferrum_proto::TokenRole::Admin);
        if let Some(ref mut oidc) = config.oidc_config {
            oidc.role_mappings = role_mappings;
        }
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("admin-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-admins"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        // Admin wildcard scope should allow any route
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // ── Phase 4.4: JWKS cache/fetch tests ──

    #[tokio::test]
    async fn test_jwk_to_key_material_rsa_ok() {
        let jwk = serde_json::json!({
            "kty": "RSA",
            "kid": "test-rsa-key",
            "n": "k4BWME9tVOIreUI5ROut2R594BH3kxnrUFJ26SAtBG3s0mYE6VM_uyvM1Lmc11oA1mzp0u_ilPOBUdDF8J2sCQ",
            "e": "AQAB"
        });
        let km = crate::jwk_to_key_material(&jwk).unwrap();
        assert!(
            matches!(km, crate::KeyMaterial::RsaJwk { n, e } if n == "k4BWME9tVOIreUI5ROut2R594BH3kxnrUFJ26SAtBG3s0mYE6VM_uyvM1Lmc11oA1mzp0u_ilPOBUdDF8J2sCQ" && e == "AQAB")
        );
    }

    #[tokio::test]
    async fn test_jwk_to_key_material_unsupported_kty() {
        let jwk = serde_json::json!({
            "kty": "EC",
            "kid": "test-ec-key",
            "crv": "P-256",
            "x": "test-x",
            "y": "test-y"
        });
        let err = crate::jwk_to_key_material(&jwk).unwrap_err();
        assert!(err.contains("unsupported jwk key type: EC"), "got: {err}");
    }

    #[tokio::test]
    async fn test_oidc_jwks_cache_fetches_from_server() {
        use axum::{Json, Router, routing::get};

        let jwks = serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-rsa-key",
                "n": "k4BWME9tVOIreUI5ROut2R594BH3kxnrUFJ26SAtBG3s0mYE6VM_uyvM1Lmc11oA1mzp0u_ilPOBUdDF8J2sCQ",
                "e": "AQAB"
            }]
        });

        let app = Router::new().route("/jwks", get(|| async { Json(jwks) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        // Small delay to let the server start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let cache = crate::OidcJwksCache::new(format!("http://{}/jwks", addr), 300);
        let key = cache.get_key("test-rsa-key").await.unwrap();
        assert!(key.is_some());
        assert!(
            matches!(key.unwrap(), crate::KeyMaterial::RsaJwk { n, e } if n == "k4BWME9tVOIreUI5ROut2R594BH3kxnrUFJ26SAtBG3s0mYE6VM_uyvM1Lmc11oA1mzp0u_ilPOBUdDF8J2sCQ" && e == "AQAB")
        );
    }

    #[tokio::test]
    async fn test_oidc_jwks_unavailable_returns_401() {
        let runtime = test_runtime().await;
        let mut config = test_oidc_server_config();
        if let Some(ref mut oidc) = config.oidc_config {
            // Remove static keys so JWKS fallback is attempted
            oidc.static_keys.clear();
            // Point to an unreachable URL
            oidc.jwks_url = Some("http://127.0.0.1:1/unreachable/jwks".to_string());
        }
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        // Sign with the original test secret, but kid won't be in static keys
        let jwt = mint_test_jwt(claims, Some("missing-kid"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Must fail closed with 401, not allow access
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_future_iat_returns_401() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert(
            "iat".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_oidc_missing_iat_is_tolerated() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime, config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        // No "iat" claim
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        let jwt = mint_test_jwt(claims, Some("test-key-1"));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_oidc_auth_failure_emits_audit_entry() {
        let runtime = test_runtime().await;
        let config = test_oidc_server_config();
        let router = build_router_with_auth(runtime.clone(), config);

        let mut claims = serde_json::Map::new();
        claims.insert("sub".to_string(), serde_json::json!("user-123"));
        claims.insert(
            "iss".to_string(),
            serde_json::json!("https://test-issuer.example.com"),
        );
        claims.insert("aud".to_string(), serde_json::json!("ferrumgate-test"));
        claims.insert(
            "exp".to_string(),
            serde_json::json!((chrono::Utc::now() + chrono::Duration::hours(1)).timestamp()),
        );
        claims.insert("groups".to_string(), serde_json::json!(["fg-operators"]));

        // Sign with a different secret to trigger an auth failure
        let header = jsonwebtoken::Header {
            typ: Some("JWT".to_string()),
            alg: jsonwebtoken::Algorithm::HS256,
            kid: Some("test-key-1".to_string()),
            ..Default::default()
        };
        let bad_key = jsonwebtoken::EncodingKey::from_secret(b"wrong-secret");
        let jwt = jsonwebtoken::encode(&header, &claims, &bad_key).unwrap();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("Authorization", format!("Bearer {}", jwt))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Verify audit entry was persisted
        let (items, _) = runtime
            .store
            .audit_log()
            .list(
                Some(AuditAction::AuthFailed),
                Some(AuditResourceType::Auth),
                None,
                None,
                10,
                None,
                None,
            )
            .await
            .unwrap();

        assert!(
            !items.is_empty(),
            "expected at least one AuthFailed audit entry"
        );
        let entry = items
            .iter()
            .find(|e| e.action == AuditAction::AuthFailed)
            .expect("auth_failed audit entry");
        assert_eq!(entry.actor_id, "unknown");
        assert_eq!(entry.result, "unauthorized");
        let meta = entry.metadata.as_ref().expect("metadata present");
        assert!(meta.get("reason").is_some(), "reason in metadata");
    }

    #[tokio::test]
    async fn test_oidc_jwks_cache_age_metric_in_output() {
        use axum::{Json, Router, routing::get};

        let jwks = serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-rsa-key",
                "n": "k4BWME9tVOIreUI5ROut2R594BH3kxnrUFJ26SAtBG3s0mYE6VM_uyvM1Lmc11oA1mzp0u_ilPOBUdDF8J2sCQ",
                "e": "AQAB"
            }]
        });

        let app = Router::new().route("/jwks", get(|| async { Json(jwks) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let cache = Arc::new(crate::OidcJwksCache::new(
            format!("http://{}/jwks", addr),
            300,
        ));
        // Populate cache
        let _ = cache.get_key("test-rsa-key").await.unwrap();

        let runtime = test_runtime().await;
        let state = Arc::new(AppState {
            runtime,
            server_config: ServerConfig::default(),
            metrics: Arc::new(Metrics::new()),
            jwks_cache: Some(cache),
            nonce_cache: Arc::new(Mutex::new(HashMap::new())),
        });

        let response = crate::monitoring::metrics_handler(axum::extract::State(state)).await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            text.contains("ferrumgate_oidc_jwks_cache_age_seconds"),
            "metrics output missing JWKS cache age gauge: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_agent_auth_success() {
        let runtime = test_runtime().await;
        let (signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["approval:resolve".to_string()],
        )
        .await;

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = sign_agent_request(
            &signing_key,
            "agent_1",
            &timestamp,
            &nonce,
            &body_hash,
            "GET",
            "/v1/approvals",
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .header("X-Ferrum-Timestamp", &timestamp)
                    .header("X-Ferrum-Nonce", &nonce)
                    .header("X-Ferrum-Body-Hash", &body_hash)
                    .header("X-Ferrum-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_agent_auth_missing_header() {
        let runtime = test_runtime().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_agent_auth_bad_signature() {
        let runtime = test_runtime().await;
        let (_signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["approval:resolve".to_string()],
        )
        .await;

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = "invalidsignature".to_string();

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .header("X-Ferrum-Timestamp", &timestamp)
                    .header("X-Ferrum-Nonce", &nonce)
                    .header("X-Ferrum-Body-Hash", &body_hash)
                    .header("X-Ferrum-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_agent_auth_replay_rejected() {
        let runtime = test_runtime().await;
        let (signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["approval:resolve".to_string()],
        )
        .await;

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = sign_agent_request(
            &signing_key,
            "agent_1",
            &timestamp,
            &nonce,
            &body_hash,
            "GET",
            "/v1/approvals",
        );

        let req = Request::builder()
            .uri("/v1/approvals")
            .header("X-Ferrum-Agent-Id", "agent_1")
            .header("X-Ferrum-Timestamp", &timestamp)
            .header("X-Ferrum-Nonce", &nonce)
            .header("X-Ferrum-Body-Hash", &body_hash)
            .header("X-Ferrum-Signature", &signature)
            .body(Body::empty())
            .unwrap();

        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Replay with same nonce should be rejected
        let req2 = Request::builder()
            .uri("/v1/approvals")
            .header("X-Ferrum-Agent-Id", "agent_1")
            .header("X-Ferrum-Timestamp", &timestamp)
            .header("X-Ferrum-Nonce", &nonce)
            .header("X-Ferrum-Body-Hash", &body_hash)
            .header("X-Ferrum-Signature", &signature)
            .body(Body::empty())
            .unwrap();
        let response2 = router.oneshot(req2).await.unwrap();
        assert_eq!(response2.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_agent_auth_body_hash_mismatch() {
        let runtime = test_runtime().await;
        let (signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["approval:resolve".to_string()],
        )
        .await;

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = sign_agent_request(
            &signing_key,
            "agent_1",
            &timestamp,
            &nonce,
            &body_hash,
            "GET",
            "/v1/approvals",
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .header("X-Ferrum-Timestamp", &timestamp)
                    .header("X-Ferrum-Nonce", &nonce)
                    .header("X-Ferrum-Body-Hash", "wrong_hash")
                    .header("X-Ferrum-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_agent_auth_timestamp_skew() {
        let runtime = test_runtime().await;
        let (signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["approval:resolve".to_string()],
        )
        .await;

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 5,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let timestamp = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = sign_agent_request(
            &signing_key,
            "agent_1",
            &timestamp,
            &nonce,
            &body_hash,
            "GET",
            "/v1/approvals",
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .header("X-Ferrum-Timestamp", &timestamp)
                    .header("X-Ferrum-Nonce", &nonce)
                    .header("X-Ferrum-Body-Hash", &body_hash)
                    .header("X-Ferrum-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_agent_auth_revoked() {
        let runtime = test_runtime().await;
        let (signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["approval:resolve".to_string()],
        )
        .await;
        runtime.store.agents().revoke("agent_1").await.unwrap();

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = sign_agent_request(
            &signing_key,
            "agent_1",
            &timestamp,
            &nonce,
            &body_hash,
            "GET",
            "/v1/approvals",
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .header("X-Ferrum-Timestamp", &timestamp)
                    .header("X-Ferrum-Nonce", &nonce)
                    .header("X-Ferrum-Body-Hash", &body_hash)
                    .header("X-Ferrum-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_agent_auth_missing_scope() {
        let runtime = test_runtime().await;
        let (signing_key, verifying_key) = generate_agent_keypair();
        let pk_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.as_bytes(),
        );
        let fingerprint = compute_fingerprint(&verifying_key);
        register_test_agent(
            &runtime.store,
            "agent_1",
            &pk_b64,
            &fingerprint,
            vec!["provenance:read".to_string()],
        )
        .await;

        let config = ServerConfig {
            auth_mode: AuthMode::Agent,
            agent_clock_skew_secs: 30,
            ..ServerConfig::default()
        };
        let router = build_router_with_auth(runtime, config);

        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = uuid::Uuid::new_v4().to_string();
        let body_hash = "null".to_string();
        let signature = sign_agent_request(
            &signing_key,
            "agent_1",
            &timestamp,
            &nonce,
            &body_hash,
            "GET",
            "/v1/approvals",
        );

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/approvals")
                    .header("X-Ferrum-Agent-Id", "agent_1")
                    .header("X-Ferrum-Timestamp", &timestamp)
                    .header("X-Ferrum-Nonce", &nonce)
                    .header("X-Ferrum-Body-Hash", &body_hash)
                    .header("X-Ferrum-Signature", &signature)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // ── Admin Agent Tests ──

    async fn test_runtime_with_admin_token() -> (GatewayRuntime, String) {
        let runtime = test_runtime().await;
        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_admin_1".to_string(),
            actor_id: "admin".to_string(),
            role: ferrum_proto::TokenRole::Admin,
            scopes: vec!["*".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();
        (runtime, token_value)
    }

    #[tokio::test]
    async fn test_admin_agent_register_list_revoke() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        // Register an agent
        let request = ferrum_proto::RegisterAgentRequest {
            agent_id: "agent_cli_1".to_string(),
            public_key: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                vec![0u8; 32],
            ),
            scopes: None,
            description: Some("test agent".to_string()),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/agents")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let register_resp: ferrum_proto::RegisterAgentResponse =
            serde_json::from_slice(&body).unwrap();
        assert_eq!(register_resp.agent.agent_id, "agent_cli_1");
        assert!(!register_resp.agent.key_fingerprint.is_empty());

        // List agents
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/admin/agents")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let list_resp: ferrum_proto::AgentListResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(list_resp.items.len(), 1);
        assert_eq!(list_resp.items[0].agent_id, "agent_cli_1");
        assert_eq!(list_resp.total, 1);

        // Duplicate agent_id should return 409 Conflict
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/agents")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        // Duplicate public_key should return 409 Conflict
        let request2 = ferrum_proto::RegisterAgentRequest {
            agent_id: "agent_cli_2".to_string(),
            public_key: request.public_key.clone(),
            scopes: None,
            description: None,
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/agents")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request2).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        // Revoke agent
        let revoke_request = ferrum_proto::RevokeAgentRequest {
            reason: Some("test revocation".to_string()),
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/admin/agents/agent_cli_1")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&revoke_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Revoke again should return 404
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/admin/agents/agent_cli_1")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&revoke_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_admin_agent_scope_enforcement() {
        let runtime = test_runtime().await;
        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_op_1".to_string(),
            actor_id: "operator".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: ferrum_proto::TokenRole::Operator.default_scopes(),
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let request = ferrum_proto::RegisterAgentRequest {
            agent_id: "agent_cli_2".to_string(),
            public_key: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                vec![0u8; 32],
            ),
            scopes: None,
            description: None,
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/agents")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Operator does not have admin:agents scope
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_admin_agent_invalid_public_key() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let request = ferrum_proto::RegisterAgentRequest {
            agent_id: "agent_cli_3".to_string(),
            public_key: "not-valid-base64!!!".to_string(),
            scopes: None,
            description: None,
        };
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/agents")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ── Audit Export Tests ──

    async fn setup_audit_entries(runtime: &GatewayRuntime) {
        let entries = vec![
            AuditLogEntry {
                id: 0,
                actor_id: "alice".to_string(),
                action: AuditAction::TokenCreate,
                resource_type: AuditResourceType::Token,
                resource_id: "t1".to_string(),
                result: "ok".to_string(),
                metadata: Some(serde_json::json!({"role": "admin"})),
                created_at: chrono::Utc::now(),
                content_hash: None,
                previous_hash: None,
            },
            AuditLogEntry {
                id: 0,
                actor_id: "bob".to_string(),
                action: AuditAction::TokenRevoke,
                resource_type: AuditResourceType::Token,
                resource_id: "t2".to_string(),
                result: "ok".to_string(),
                metadata: None,
                created_at: chrono::Utc::now(),
                content_hash: None,
                previous_hash: None,
            },
        ];
        for entry in entries {
            runtime.store.audit_log().append(&entry).await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_audit_export_ndjson() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        setup_audit_entries(&runtime).await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit-logs/export?format=ndjson")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        // DESC ordering: bob appended second, appears first
        assert!(lines[0].contains("bob"));
        assert!(lines[1].contains("alice"));
    }

    #[tokio::test]
    async fn test_audit_export_json() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        setup_audit_entries(&runtime).await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit-logs/export?format=json")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let items: Vec<AuditLogEntry> = serde_json::from_slice(&body).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_audit_export_csv() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        setup_audit_entries(&runtime).await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit-logs/export?format=csv")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("id,actor_id,action"));
        assert_eq!(lines.len(), 3); // header + 2 rows
    }

    #[tokio::test]
    async fn test_audit_export_requires_admin_audit_scope() {
        let runtime = test_runtime().await;
        // Insert a token without admin:audit scope
        let token_value = generate_token_value();
        let token_salt = generate_token_salt();
        let token_lookup_hash = hash_token_value(&token_value);
        let token_hash = hash_token_with_salt(&token_value, &token_salt);
        let token = ferrum_proto::ScopedToken {
            token_id: "tok_no_audit".to_string(),
            actor_id: "user".to_string(),
            role: ferrum_proto::TokenRole::Operator,
            scopes: vec!["policy:read".to_string()],
            description: None,
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
            created_at: chrono::Utc::now(),
            last_used_at: None,
            revoked_at: None,
            revoked_reason: None,
            rotated_from: None,
            token_lookup_hash,
            token_hash,
            token_salt,
        };
        runtime.store.tokens().insert(&token).await.unwrap();

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit-logs/export")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_audit_export_invalid_format() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit-logs/export?format=xml")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_merkle_verify_computes_root() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let store = runtime.store.clone();

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();

        let e1 = ferrum_proto::AuditLogEntry {
            id: 0,
            actor_id: "alice".to_string(),
            action: ferrum_proto::AuditAction::TokenCreate,
            resource_type: ferrum_proto::AuditResourceType::Token,
            resource_id: "t1".to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: window + chrono::Duration::minutes(5),
            content_hash: None,
            previous_hash: None,
        };
        store.audit_log().append(&e1).await.unwrap();

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v1/admin/audit/merkle-verify?window_start={}",
                        window.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    ))
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: ferrum_proto::AuditMerkleVerifyResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.valid);
        assert_eq!(resp.entry_count, 1);
        assert!(!resp.root.is_empty());
    }

    #[tokio::test]
    async fn test_merkle_verify_rejects_non_hour_alignment() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let misaligned = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap()
            + chrono::Duration::minutes(5);

        let response = router
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v1/admin/audit/merkle-verify?window_start={}",
                        misaligned.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    ))
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_merkle_roots_list_and_pagination() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let store = runtime.store.clone();

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();
        let prev_window = window - chrono::Duration::hours(1);

        let e = ferrum_proto::AuditLogEntry {
            id: 0,
            actor_id: "alice".to_string(),
            action: ferrum_proto::AuditAction::TokenCreate,
            resource_type: ferrum_proto::AuditResourceType::Token,
            resource_id: "t1".to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: prev_window + chrono::Duration::minutes(5),
            content_hash: None,
            previous_hash: None,
        };
        store.audit_log().append(&e).await.unwrap();
        store
            .audit_merkle_roots()
            .compute_and_cache_root(prev_window)
            .await
            .unwrap();
        store
            .audit_merkle_roots()
            .compute_and_cache_root(window)
            .await
            .unwrap();

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit/merkle-roots?limit=1")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: ferrum_proto::AuditMerkleRootListResponse =
            serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.items.len(), 1);
        assert!(resp.next_cursor.is_some());
    }

    #[tokio::test]
    async fn test_checkpoint_create_and_verify() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let store = runtime.store.clone();

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();

        let e = ferrum_proto::AuditLogEntry {
            id: 0,
            actor_id: "alice".to_string(),
            action: ferrum_proto::AuditAction::TokenCreate,
            resource_type: ferrum_proto::AuditResourceType::Token,
            resource_id: "t1".to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: window + chrono::Duration::minutes(5),
            content_hash: None,
            previous_hash: None,
        };
        store.audit_log().append(&e).await.unwrap();
        let merkle = store
            .audit_merkle_roots()
            .compute_and_cache_root(window)
            .await
            .unwrap();

        // Generate an Ed25519 keypair.
        let mut rng = rand::thread_rng();
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let signed_at = chrono::Utc::now();
        let payload_hash = ferrum_proto::canonical_checkpoint_hash(
            &window,
            &merkle.root,
            merkle.entry_count,
            &signed_at,
        );
        let signature = signing_key.sign(&payload_hash);
        let signature_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signature.to_bytes(),
        );
        let public_key_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.to_bytes(),
        );
        let mut hasher = sha2::Sha256::new();
        hasher.update(verifying_key.to_bytes());
        let fingerprint = hex::encode(hasher.finalize());

        let request = ferrum_proto::CreateCheckpointRequest {
            window_start: window,
            merkle_root: merkle.root.clone(),
            entry_count: merkle.entry_count,
            signer_id: "operator-1".to_string(),
            signer_key_fingerprint: fingerprint,
            signed_at,
            signature: signature_b64,
            public_key: public_key_b64,
        };

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        // Create checkpoint
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/audit/checkpoints")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        // Verify checkpoint
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v1/admin/audit/checkpoints/{}/verify",
                        window.to_rfc3339()
                    ))
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: ferrum_proto::AuditCheckpointVerifyResponse =
            serde_json::from_slice(&body).unwrap();
        assert!(resp.valid);
        assert_eq!(resp.current_root, Some(merkle.root));
    }

    #[tokio::test]
    async fn test_checkpoint_create_rejects_tampered_root() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let store = runtime.store.clone();

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();

        let e = ferrum_proto::AuditLogEntry {
            id: 0,
            actor_id: "alice".to_string(),
            action: ferrum_proto::AuditAction::TokenCreate,
            resource_type: ferrum_proto::AuditResourceType::Token,
            resource_id: "t1".to_string(),
            result: "ok".to_string(),
            metadata: None,
            created_at: window + chrono::Duration::minutes(5),
            content_hash: None,
            previous_hash: None,
        };
        store.audit_log().append(&e).await.unwrap();
        let merkle = store
            .audit_merkle_roots()
            .compute_and_cache_root(window)
            .await
            .unwrap();

        let mut rng = rand::thread_rng();
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
        let verifying_key = signing_key.verifying_key();
        let signed_at = chrono::Utc::now();
        // Sign the *correct* payload
        let payload_hash = ferrum_proto::canonical_checkpoint_hash(
            &window,
            &merkle.root,
            merkle.entry_count,
            &signed_at,
        );
        let signature = signing_key.sign(&payload_hash);
        let signature_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            signature.to_bytes(),
        );
        let public_key_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            verifying_key.to_bytes(),
        );
        let mut hasher = sha2::Sha256::new();
        hasher.update(verifying_key.to_bytes());
        let fingerprint = hex::encode(hasher.finalize());

        // But submit a tampered merkle_root in the request
        let request = ferrum_proto::CreateCheckpointRequest {
            window_start: window,
            merkle_root: "tampered".to_string(),
            entry_count: merkle.entry_count,
            signer_id: "operator-1".to_string(),
            signer_key_fingerprint: fingerprint,
            signed_at,
            signature: signature_b64,
            public_key: public_key_b64,
        };

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/audit/checkpoints")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_checkpoint_list_and_pagination() {
        let (runtime, token_value) = test_runtime_with_admin_token().await;
        let store = runtime.store.clone();

        let window = chrono::Utc::now()
            .duration_trunc(chrono::Duration::hours(1))
            .unwrap();
        let prev_window = window - chrono::Duration::hours(1);

        for ws in [prev_window, window] {
            let cp = ferrum_proto::AuditCheckpoint {
                window_start: ws,
                merkle_root: "root".to_string(),
                entry_count: 0,
                signer_id: "op".to_string(),
                signer_key_fingerprint: "fp".to_string(),
                signed_at: chrono::Utc::now(),
                signature: "sig".to_string(),
                public_key: "pk".to_string(),
            };
            store.audit_checkpoints().insert(&cp).await.unwrap();
        }

        let config = ServerConfig {
            auth_mode: AuthMode::Scoped,
            ..Default::default()
        };
        let router = build_router_with_auth(runtime.clone(), config);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/audit/checkpoints?limit=1")
                    .header("Authorization", format!("Bearer {}", token_value))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resp: ferrum_proto::AuditCheckpointListResponse =
            serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.items.len(), 1);
        assert!(resp.next_cursor.is_some());
    }
}
