//! Monitoring endpoints: `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics`.
//!
//! These handlers are always unauthenticated and are the primary health/observability
//! surface of the gateway. This module owns the `Metrics` aggregate, the governance
//! route catalog (`GovernanceRoute`), the per-endpoint latency routing (`PublicRoute`),
//! and the Prometheus histogram boundary table (`HISTOGRAM_BOUNDARIES`) used by both
//! the handlers and `Metrics::record_latency`.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use ferrum_proto::{ComponentStatus, DeepHealthResponse, HealthResponse};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Prometheus histogram bucket boundaries in seconds.
/// Includes: 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, 10s
pub(crate) const HISTOGRAM_BOUNDARIES: &[f64] = &[
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

pub(crate) use crate::server::{GovernanceRoute, PublicRoute};

pub(crate) async fn healthz(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
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

pub(crate) async fn readyz(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
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

/// Deep readiness probe that checks the store health, write queue backpressure, and
/// connection pool saturation.
///
/// Returns HTTP 200 with "ok" status when store is healthy, queue depth is within threshold,
/// and the connection pool is not saturated.
/// Returns HTTP 503 with "degraded" status when store is unhealthy, queue depth exceeds
/// threshold, or the pool is saturated (no idle connections and total connections at or above
/// the configured maximum).
/// The `write_queue` component provides bounded backpressure detection only; it does not
/// indicate full dependency health, ledger scan status, adapter health, rollback health,
/// or schema integrity.
pub(crate) async fn readyz_deep(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<DeepHealthResponse>) {
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

    // Pool saturation check: report degraded when no idle connections remain
    // and the pool is at or above its configured maximum.
    let pool_status = state.runtime.store.pool_status();
    let pool_healthy = match pool_status {
        Some(ps) if ps.max_connections > 0 => {
            !(ps.idle_connections == 0 && ps.total_connections >= ps.max_connections)
        }
        _ => true,
    };
    let pool_status_component = match pool_status {
        Some(ps) if ps.max_connections > 0 && !pool_healthy => ComponentStatus {
            component: "pool".to_string(),
            status: format!(
                "degraded: saturated (idle={}/total={}/max={})",
                ps.idle_connections, ps.total_connections, ps.max_connections
            ),
            healthy: false,
            error: Some("pool saturated: no idle connections available".to_string()),
        },
        Some(ps) => ComponentStatus {
            component: "pool".to_string(),
            status: format!(
                "ok: idle={}/total={}/max={}",
                ps.idle_connections, ps.total_connections, ps.max_connections
            ),
            healthy: true,
            error: None,
        },
        None => ComponentStatus {
            component: "pool".to_string(),
            status: "not applicable".to_string(),
            healthy: true,
            error: None,
        },
    };

    let healthy = store_status.healthy && queue_healthy && pool_healthy;
    let status = if healthy { "ok" } else { "degraded" };

    let response = DeepHealthResponse {
        status: status.to_string(),
        healthy,
        components: vec![store_status, queue_status, pool_status_component],
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
pub(crate) async fn metrics_handler(State(state): State<Arc<AppState>>) -> Response {
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
    let pool_status = state.runtime.store.pool_status();

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
    let gov_err_executions_commit = state
        .metrics
        .governance_errors_v1_executions_commit
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
    let gov_err_policy_simulate = state
        .metrics
        .governance_errors_v1_policy_simulate
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_simulate = state
        .metrics
        .governance_errors_v1_policy_bundles_simulate
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_versions = state
        .metrics
        .governance_errors_v1_policy_bundles_versions
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_diff = state
        .metrics
        .governance_errors_v1_policy_bundles_diff
        .load(Ordering::Relaxed);
    let gov_err_policy_bundles_rollback = state
        .metrics
        .governance_errors_v1_policy_bundles_rollback
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
    let gov_err_agents_create = state
        .metrics
        .governance_errors_v1_agents_create
        .load(Ordering::Relaxed);
    let gov_err_agents_list = state
        .metrics
        .governance_errors_v1_agents_list
        .load(Ordering::Relaxed);
    let gov_err_agents_revoke = state
        .metrics
        .governance_errors_v1_agents_revoke
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
    let gov_ok_executions_commit = state
        .metrics
        .governance_success_v1_executions_commit
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
    let gov_ok_policy_simulate = state
        .metrics
        .governance_success_v1_policy_simulate
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_simulate = state
        .metrics
        .governance_success_v1_policy_bundles_simulate
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_versions = state
        .metrics
        .governance_success_v1_policy_bundles_versions
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_diff = state
        .metrics
        .governance_success_v1_policy_bundles_diff
        .load(Ordering::Relaxed);
    let gov_ok_policy_bundles_rollback = state
        .metrics
        .governance_success_v1_policy_bundles_rollback
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
    let gov_ok_agents_create = state
        .metrics
        .governance_success_v1_agents_create
        .load(Ordering::Relaxed);
    let gov_ok_agents_list = state
        .metrics
        .governance_success_v1_agents_list
        .load(Ordering::Relaxed);
    let gov_ok_agents_revoke = state
        .metrics
        .governance_success_v1_agents_revoke
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
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/commit\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/compensate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/cancel\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}/evaluate-outcome\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/executions/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/approvals\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/approvals/{{approval_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/approvals/{{approval_id}}/resolve\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"PUT\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}\",method=\"DELETE\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}/active\",method=\"PUT\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy/simulate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/simulate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}/versions\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}/diff\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/policy-bundles/{{bundle_id}}/rollback\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/query\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/lineage\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/lineage/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/provenance/ingest\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/bridges/{{bridge_id}}/tools\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/admin/agents\",method=\"POST\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/admin/agents\",method=\"GET\"}} {}\n\
         ferrumgate_governance_errors_total{{route=\"/v1/admin/agents/{{agent_id}}\",method=\"DELETE\"}} {}\n\
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
         ferrumgate_governance_success_total{{route=\"/v1/executions/{{execution_id}}/commit\",method=\"POST\"}} {}\n\
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
         ferrumgate_governance_success_total{{route=\"/v1/policy/simulate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/simulate\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}/versions\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}/diff\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/policy-bundles/{{bundle_id}}/rollback\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/query\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/lineage\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/lineage/{{execution_id}}\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/provenance/ingest\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/bridges/{{bridge_id}}/tools\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/admin/agents\",method=\"POST\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/admin/agents\",method=\"GET\"}} {}\n\
         ferrumgate_governance_success_total{{route=\"/v1/admin/agents/{{agent_id}}\",method=\"DELETE\"}} {}\n",
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
        gov_err_executions_commit,
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
        gov_err_policy_simulate,
        gov_err_policy_bundles_simulate,
        gov_err_policy_bundles_versions,
        gov_err_policy_bundles_diff,
        gov_err_policy_bundles_rollback,
        gov_err_provenance_query,
        gov_err_provenance_lineage,
        gov_err_provenance_lineage_execution_id,
        gov_err_provenance_ingest,
        gov_err_bridges_bridge_id_tools,
        gov_err_agents_create,
        gov_err_agents_list,
        gov_err_agents_revoke,
        gov_ok_intents_compile,
        gov_ok_intents_list,
        gov_ok_proposals_evaluate,
        gov_ok_capabilities_mint,
        gov_ok_capabilities_revoke,
        gov_ok_executions_authorize,
        gov_ok_executions_prepare,
        gov_ok_executions_execute,
        gov_ok_executions_verify,
        gov_ok_executions_commit,
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
        gov_ok_policy_simulate,
        gov_ok_policy_bundles_simulate,
        gov_ok_policy_bundles_versions,
        gov_ok_policy_bundles_diff,
        gov_ok_policy_bundles_rollback,
        gov_ok_provenance_query,
        gov_ok_provenance_lineage,
        gov_ok_provenance_lineage_execution_id,
        gov_ok_provenance_ingest,
        gov_ok_bridges_bridge_id_tools,
        gov_ok_agents_create,
        gov_ok_agents_list,
        gov_ok_agents_revoke,
    );

    // Append histogram output to body
    body.push_str("# HELP ferrumgate_request_duration_seconds HTTP request latency histogram by route, method, and status\n");
    body.push_str("# TYPE ferrumgate_request_duration_seconds histogram\n");
    body.push_str(&healthz_histogram);
    body.push_str(&readyz_histogram);
    body.push_str(&readyz_deep_histogram_200);
    body.push_str(&readyz_deep_histogram_503);
    body.push_str(&metrics_histogram);

    // Append PostgreSQL pool metrics when available
    if let Some(ps) = pool_status {
        body.push_str("# HELP ferrumgate_store_pg_pool_size Current number of connections in the PostgreSQL pool\n");
        body.push_str("# TYPE ferrumgate_store_pg_pool_size gauge\n");
        body.push_str(&format!(
            "ferrumgate_store_pg_pool_size {}\n",
            ps.total_connections
        ));
        body.push_str("# HELP ferrumgate_store_pg_pool_idle Current number of idle connections in the PostgreSQL pool\n");
        body.push_str("# TYPE ferrumgate_store_pg_pool_idle gauge\n");
        body.push_str(&format!(
            "ferrumgate_store_pg_pool_idle {}\n",
            ps.idle_connections
        ));
        body.push_str("# HELP ferrumgate_store_pg_pool_max Maximum number of connections configured for the PostgreSQL pool\n");
        body.push_str("# TYPE ferrumgate_store_pg_pool_max gauge\n");
        body.push_str(&format!(
            "ferrumgate_store_pg_pool_max {}\n",
            ps.max_connections
        ));
        body.push_str("# HELP ferrumgate_store_pg_acquire_timeouts_total Cumulative count of PostgreSQL pool acquire timeouts\n");
        body.push_str("# TYPE ferrumgate_store_pg_acquire_timeouts_total counter\n");
        body.push_str(&format!(
            "ferrumgate_store_pg_acquire_timeouts_total {}\n",
            ps.acquire_timeouts
        ));
    }

    // Append JWKS cache age metric when cache exists and has been fetched
    if let Some(ref cache) = state.jwks_cache {
        if let Some(age) = cache.cache_age_seconds() {
            body.push_str(
                "# HELP ferrumgate_oidc_jwks_cache_age_seconds Age of the JWKS cache in seconds\n",
            );
            body.push_str("# TYPE ferrumgate_oidc_jwks_cache_age_seconds gauge\n");
            body.push_str(&format!("ferrumgate_oidc_jwks_cache_age_seconds {}\n", age));
        }
    }

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

/// Build the router for monitoring endpoints (`/v1/healthz`, `/v1/readyz`,
/// `/v1/readyz/deep`, `/v1/metrics`). These routes are always unauthenticated
/// and are merged into the workload router by the top-level builder.
pub(crate) fn build_monitoring_router(state: Arc<AppState>) -> Router {
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
