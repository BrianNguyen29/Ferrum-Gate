//! Gateway metrics instrumentation.
//!
//! Exports Prometheus metrics for request count, latency, and error count.
//! Metrics are updated by the [`MetricsLayer`] tower middleware on every HTTP request.

use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, exponential_buckets};
use std::sync::Arc;
use std::time::Instant;
use tower::{Layer, Service};

/// Gateway metrics registry containing all Prometheus instruments.
#[derive(Clone)]
pub struct GatewayMetrics {
    /// Total HTTP requests received, labeled by method, path, and status code.
    pub requests_total: IntCounterVec,
    /// Request duration histogram in seconds, labeled by method and path.
    pub request_duration_seconds: HistogramVec,
    /// Total gateway errors, labeled by method, path, and error kind.
    pub errors_total: IntCounterVec,
    /// Custom registry that owns the metric families above.
    registry: Arc<Registry>,
}

impl GatewayMetrics {
    /// Creates a new [`GatewayMetrics`] instance and registers all instruments
    /// with a new custom registry to avoid conflicts when creating multiple instances.
    pub fn new() -> Self {
        // Use a new custom registry to avoid "already registered" errors
        // when creating multiple GatewayMetrics instances in tests.
        let registry = Arc::new(
            Registry::new_custom(Some("ferrum_gateway".to_string()), None)
                .expect("failed to create custom registry"),
        );
        Self::with_registry(&registry)
    }

    /// Creates a new [`GatewayMetrics`] instance and registers all instruments
    /// with the provided `registry`.
    pub fn with_registry(registry: &Arc<Registry>) -> Self {
        let requests_total = IntCounterVec::new(
            Opts::new(
                "ferrum_gateway_http_requests_total",
                "Total HTTP requests received",
            )
            .namespace("ferrum_gateway"),
            &["method", "path", "status"],
        )
        .expect("metric registration failed");

        let request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "ferrum_gateway_http_request_duration_seconds",
                "HTTP request duration in seconds",
            )
            .namespace("ferrum_gateway")
            .buckets(exponential_buckets(0.0001, 2.0, 16).expect("valid histogram buckets")),
            &["method", "path"],
        )
        .expect("metric registration failed");

        let errors_total = IntCounterVec::new(
            Opts::new("ferrum_gateway_errors_total", "Total gateway errors")
                .namespace("ferrum_gateway"),
            &["method", "path", "error_kind"],
        )
        .expect("metric registration failed");

        registry
            .register(Box::new(requests_total.clone()))
            .expect("metric registration failed");
        registry
            .register(Box::new(request_duration_seconds.clone()))
            .expect("metric registration failed");
        registry
            .register(Box::new(errors_total.clone()))
            .expect("metric registration failed");

        Self {
            requests_total,
            request_duration_seconds,
            errors_total,
            registry: Arc::clone(registry),
        }
    }

    /// Records a successful HTTP request.
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration_secs: f64) {
        self.requests_total
            .with_label_values(&[method, path, &status.to_string()])
            .inc();
        self.request_duration_seconds
            .with_label_values(&[method, path])
            .observe(duration_secs);
    }

    /// Records a gateway error for an HTTP request.
    pub fn record_error(&self, method: &str, path: &str, error_kind: &str) {
        self.errors_total
            .with_label_values(&[method, path, error_kind])
            .inc();
    }

    /// Gathers all registered metric families for Prometheus exposition.
    pub fn gather(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }
}

impl Default for GatewayMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Tower middleware that instruments HTTP requests with gateway metrics.
#[derive(Clone)]
pub struct MetricsLayer {
    metrics: Arc<GatewayMetrics>,
}

impl MetricsLayer {
    /// Creates a new [`MetricsLayer`] wrapping the provided [`GatewayMetrics`].
    pub fn new(metrics: GatewayMetrics) -> Self {
        Self {
            metrics: Arc::new(metrics),
        }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsService {
            inner,
            metrics: self.metrics.clone(),
        }
    }
}

/// Tower service wrapper that instruments requests with gateway metrics.
#[derive(Clone)]
pub struct MetricsService<S> {
    inner: S,
    metrics: Arc<GatewayMetrics>,
}

impl<S> Service<axum::http::Request<axum::body::Body>> for MetricsService<S>
where
    S: Service<axum::http::Request<axum::body::Body>, Response = axum::response::Response>
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: axum::http::Request<axum::body::Body>) -> Self::Future {
        let metrics = self.metrics.clone();
        let method = request.method().as_str().to_string();
        let path = normalize_path(request.uri().path());

        let start = Instant::now();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let response = inner.call(request).await;
            let duration = start.elapsed().as_secs_f64();

            let (status, error_kind) = match &response {
                Ok(resp) => (resp.status().as_u16(), None),
                Err(_) => {
                    // Map error to a status code and error kind
                    (500u16, Some("internal_error"))
                }
            };

            // Record request metrics
            metrics.record_request(&method, &path, status, duration);

            // Record error metric if applicable
            if let Some(kind) = error_kind {
                metrics.record_error(&method, &path, kind);
            }

            response
        })
    }
}

/// Normalizes a path for use as a metric label.
///
/// Returns a simplified path template that replaces dynamic segments
/// (UUIDs, numeric IDs) with placeholders to avoid high-cardinality labels.
fn normalize_path(path: &str) -> String {
    // Common dynamic path patterns that should be normalized to avoid
    // high-cardinality metric labels
    let normalized = uuid_prefix_regex().replace_all(path, "/{id}").to_string();

    // Normalize trailing slashes
    let normalized = normalized.trim_end_matches('/').to_string();

    if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    }
}

/// Returns a regex that matches UUID segments in paths.
fn uuid_prefix_regex() -> regex::Regex {
    // Matches UUID-like strings (8-4-4-4-12 hex groups) in path segments
    regex::Regex::new(
        r"/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
    )
    .expect("regex construction failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_replaces_uuid_segments() {
        let path = "/v1/executions/550e8400-e29b-41d4-a716-446655440000/commit";
        let normalized = normalize_path(path);
        assert_eq!(normalized, "/v1/executions/{id}/commit");
    }

    #[test]
    fn normalize_path_preserves_static_paths() {
        let path = "/v1/healthz";
        let normalized = normalize_path(path);
        assert_eq!(normalized, "/v1/healthz");
    }

    #[test]
    fn normalize_path_handles_root() {
        let normalized = normalize_path("/");
        assert_eq!(normalized, "/");
    }

    #[test]
    fn normalize_path_trims_trailing_slash() {
        let normalized = normalize_path("/v1/approvals/");
        assert_eq!(normalized, "/v1/approvals");
    }
}
