use std::sync::atomic::{AtomicU64, Ordering};

use crate::behavioral::BehavioralSeverity;
use crate::monitoring::HISTOGRAM_BOUNDARIES;

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
    pub(crate) governance_errors_v1_quarantines: AtomicU64,
    pub(crate) governance_errors_v1_quarantines_hold_id: AtomicU64,
    pub(crate) governance_errors_v1_quarantines_resolve: AtomicU64,
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
    pub(crate) governance_errors_v1_mfa_enroll: AtomicU64,
    pub(crate) governance_errors_v1_mfa_verify: AtomicU64,
    pub(crate) governance_errors_v1_mfa_disable: AtomicU64,
    pub(crate) governance_errors_v1_mfa_rotate: AtomicU64,
    pub(crate) governance_errors_v1_mfa_list: AtomicU64,
    pub(crate) governance_errors_v1_mfa_get: AtomicU64,
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
    pub(crate) governance_success_v1_quarantines: AtomicU64,
    pub(crate) governance_success_v1_quarantines_hold_id: AtomicU64,
    pub(crate) governance_success_v1_quarantines_resolve: AtomicU64,
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
    pub(crate) governance_success_v1_mfa_enroll: AtomicU64,
    pub(crate) governance_success_v1_mfa_verify: AtomicU64,
    pub(crate) governance_success_v1_mfa_disable: AtomicU64,
    pub(crate) governance_success_v1_mfa_rotate: AtomicU64,
    pub(crate) governance_success_v1_mfa_list: AtomicU64,
    pub(crate) governance_success_v1_mfa_get: AtomicU64,
    // Audit fail-closed rejection counter
    pub(crate) audit_fail_closed_rejections: AtomicU64,
    // WORM sink counters
    pub(crate) audit_worm_sink_exports_total: AtomicU64,
    pub(crate) audit_worm_sink_failures_total: AtomicU64,
    pub(crate) audit_worm_sink_last_success_timestamp_seconds: AtomicU64,
    // Approval timeout counter
    pub(crate) approval_timeouts_total: AtomicU64,
    // Quarantine hold timeout counter
    pub(crate) quarantine_timeouts_total: AtomicU64,
    // HA reconciler counters
    pub(crate) ha_reconciler_canceled_total: AtomicU64,
    pub(crate) ha_reconciler_recovery_required_total: AtomicU64,
    pub(crate) ha_reconciler_errors_total: AtomicU64,
    // Behavioral anomaly advisory counters
    pub(crate) behavioral_anomaly_warnings_total: AtomicU64,
    pub(crate) behavioral_anomaly_critical_total: AtomicU64,
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
            governance_errors_v1_quarantines: AtomicU64::new(0),
            governance_errors_v1_quarantines_hold_id: AtomicU64::new(0),
            governance_errors_v1_quarantines_resolve: AtomicU64::new(0),
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
            governance_errors_v1_mfa_enroll: AtomicU64::new(0),
            governance_errors_v1_mfa_verify: AtomicU64::new(0),
            governance_errors_v1_mfa_disable: AtomicU64::new(0),
            governance_errors_v1_mfa_rotate: AtomicU64::new(0),
            governance_errors_v1_mfa_list: AtomicU64::new(0),
            governance_errors_v1_mfa_get: AtomicU64::new(0),
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
            governance_success_v1_quarantines: AtomicU64::new(0),
            governance_success_v1_quarantines_hold_id: AtomicU64::new(0),
            governance_success_v1_quarantines_resolve: AtomicU64::new(0),
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
            governance_success_v1_mfa_enroll: AtomicU64::new(0),
            governance_success_v1_mfa_verify: AtomicU64::new(0),
            governance_success_v1_mfa_disable: AtomicU64::new(0),
            governance_success_v1_mfa_rotate: AtomicU64::new(0),
            governance_success_v1_mfa_list: AtomicU64::new(0),
            governance_success_v1_mfa_get: AtomicU64::new(0),
            audit_fail_closed_rejections: AtomicU64::new(0),
            audit_worm_sink_exports_total: AtomicU64::new(0),
            audit_worm_sink_failures_total: AtomicU64::new(0),
            audit_worm_sink_last_success_timestamp_seconds: AtomicU64::new(0),
            approval_timeouts_total: AtomicU64::new(0),
            quarantine_timeouts_total: AtomicU64::new(0),
            ha_reconciler_canceled_total: AtomicU64::new(0),
            ha_reconciler_recovery_required_total: AtomicU64::new(0),
            ha_reconciler_errors_total: AtomicU64::new(0),
            behavioral_anomaly_warnings_total: AtomicU64::new(0),
            behavioral_anomaly_critical_total: AtomicU64::new(0),
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
            GovernanceRoute::Quarantines => self
                .governance_errors_v1_quarantines
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::QuarantinesHoldId => self
                .governance_errors_v1_quarantines_hold_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::QuarantinesResolve => self
                .governance_errors_v1_quarantines_resolve
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
            GovernanceRoute::MfaEnroll => self
                .governance_errors_v1_mfa_enroll
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaVerify => self
                .governance_errors_v1_mfa_verify
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaDisable => self
                .governance_errors_v1_mfa_disable
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaRotate => self
                .governance_errors_v1_mfa_rotate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaList => self
                .governance_errors_v1_mfa_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaGet => self
                .governance_errors_v1_mfa_get
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
            GovernanceRoute::Quarantines => self
                .governance_success_v1_quarantines
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::QuarantinesHoldId => self
                .governance_success_v1_quarantines_hold_id
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::QuarantinesResolve => self
                .governance_success_v1_quarantines_resolve
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
            GovernanceRoute::MfaEnroll => self
                .governance_success_v1_mfa_enroll
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaVerify => self
                .governance_success_v1_mfa_verify
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaDisable => self
                .governance_success_v1_mfa_disable
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaRotate => self
                .governance_success_v1_mfa_rotate
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaList => self
                .governance_success_v1_mfa_list
                .fetch_add(1, Ordering::Relaxed),
            GovernanceRoute::MfaGet => self
                .governance_success_v1_mfa_get
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

    /// Increments the behavioral anomaly advisory counter for the given severity.
    pub(crate) fn record_behavioral_anomaly(&self, severity: BehavioralSeverity) {
        match severity {
            BehavioralSeverity::Warning => self
                .behavioral_anomaly_warnings_total
                .fetch_add(1, Ordering::Relaxed),
            BehavioralSeverity::Critical => self
                .behavioral_anomaly_critical_total
                .fetch_add(1, Ordering::Relaxed),
        };
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
    Quarantines,
    QuarantinesHoldId,
    QuarantinesResolve,
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
    MfaEnroll,
    MfaVerify,
    MfaDisable,
    MfaRotate,
    MfaList,
    MfaGet,
}

impl GovernanceRoute {
    #[allow(dead_code)]
    pub(crate) fn path(&self) -> &'static str {
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
            GovernanceRoute::Quarantines => "/v1/quarantines",
            GovernanceRoute::QuarantinesHoldId => "/v1/quarantines/{hold_id}",
            GovernanceRoute::QuarantinesResolve => "/v1/quarantines/{hold_id}/resolve",
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
            GovernanceRoute::MfaEnroll => "/v1/admin/agents/{agent_id}/mfa/enroll",
            GovernanceRoute::MfaVerify => "/v1/admin/agents/{agent_id}/mfa/verify",
            GovernanceRoute::MfaDisable => "/v1/admin/agents/{agent_id}/mfa/disable",
            GovernanceRoute::MfaRotate => "/v1/admin/agents/{agent_id}/mfa/rotate",
            GovernanceRoute::MfaList => "/v1/admin/agents/{agent_id}/mfa",
            GovernanceRoute::MfaGet => "/v1/admin/agents/{agent_id}/mfa/{mfa_factor_id}",
        }
    }

    /// Returns the HTTP method for this route as a static string.
    #[allow(dead_code)]
    pub(crate) fn method(&self) -> &'static str {
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
            GovernanceRoute::Quarantines => "GET",
            GovernanceRoute::QuarantinesHoldId => "GET",
            GovernanceRoute::QuarantinesResolve => "POST",
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
            GovernanceRoute::MfaEnroll => "POST",
            GovernanceRoute::MfaVerify => "POST",
            GovernanceRoute::MfaDisable => "POST",
            GovernanceRoute::MfaRotate => "POST",
            GovernanceRoute::MfaList => "GET",
            GovernanceRoute::MfaGet => "GET",
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
