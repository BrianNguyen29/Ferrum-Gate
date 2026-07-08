use axum::Router;
use axum::routing::{delete, get, post, put};
use std::sync::Arc;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Merge the monitoring router with a pre-built workload router.
/// The workload router is expected to already include any rate-limit governor
/// and tracing layer; the auth layer is applied by the caller (`server.rs`) so
/// auth wraps both monitoring and workload endpoints consistently.
pub(crate) fn build_app_router(state: Arc<AppState>, workload_router: Router) -> Router {
    let monitoring_router = crate::monitoring::build_monitoring_router(state);
    monitoring_router.merge(workload_router)
}

/// Build the workload route table with tracing and shared state.
pub(crate) fn build_workload_router(state: Arc<AppState>) -> Router {
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
        // Quarantine hold endpoints
        .route(
            "/v1/quarantines",
            get(crate::quarantine::list_quarantine_holds),
        )
        .route(
            "/v1/quarantines/{hold_id}",
            get(crate::quarantine::get_quarantine_hold),
        )
        .route(
            "/v1/quarantines/{hold_id}/resolve",
            post(crate::quarantine::resolve_quarantine_hold),
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
        // Admin MFA endpoints
        .route(
            "/v1/admin/agents/{agent_id}/mfa/enroll",
            post(crate::admin::mfa::enroll_mfa),
        )
        .route(
            "/v1/admin/agents/{agent_id}/mfa/verify",
            post(crate::admin::mfa::verify_mfa),
        )
        .route(
            "/v1/admin/agents/{agent_id}/mfa/disable",
            post(crate::admin::mfa::disable_mfa),
        )
        .route(
            "/v1/admin/agents/{agent_id}/mfa/rotate",
            post(crate::admin::mfa::rotate_mfa),
        )
        .route(
            "/v1/admin/agents/{agent_id}/mfa",
            get(crate::admin::mfa::list_mfa_factors),
        )
        .route(
            "/v1/admin/agents/{agent_id}/mfa/{mfa_factor_id}",
            get(crate::admin::mfa::get_mfa_factor),
        )
        // Admin lifecycle outbox operator endpoints
        .route(
            "/v1/admin/lifecycle-outbox",
            get(crate::admin::lifecycle_outbox::list_lifecycle_outbox),
        )
        .route(
            "/v1/admin/lifecycle-outbox/{outbox_id}",
            get(crate::admin::lifecycle_outbox::get_lifecycle_outbox),
        )
        .route(
            "/v1/admin/lifecycle-outbox/{outbox_id}/retry",
            post(crate::admin::lifecycle_outbox::retry_lifecycle_outbox),
        )
        .route(
            "/v1/admin/lifecycle-outbox/{outbox_id}/resolve",
            post(crate::admin::lifecycle_outbox::resolve_lifecycle_outbox),
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
