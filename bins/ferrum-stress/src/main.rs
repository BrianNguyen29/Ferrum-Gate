use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use chrono::Utc;
use clap::{Parser, ValueEnum};
use ferrum_cap::InMemoryCapabilityService;
use ferrum_gateway::{AuthMode, GatewayRuntime, ServerConfig};
use ferrum_pdp::StaticPdpEngine;
use ferrum_proto::{
    ActionProposal, AuthorizeExecutionRequest, AuthorizeExecutionResponse, CapabilityMintRequest,
    CapabilityMintResponse, EffectType, IntentCompileRequest, IntentCompileResponse, JsonMap,
    OutcomeReport, PrincipalId, ProvenanceEventKind, ProvenanceIngestRequest, RiskTier,
    RollbackClass,
};
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::{SqliteStore, StoreFacade};
use ferrum_sync::{BridgeSubmitResult, BridgeToolInfo, RuntimeBridge};
use reqwest::Client;
use serde::Serialize;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::sleep as tokio_sleep;

/// Global error counter for rate-limited debug logging
static ERROR_COUNT: AtomicU64 = AtomicU64::new(0);
static TEXT_OUTPUT_ENABLED: AtomicBool = AtomicBool::new(true);
const MAX_ERROR_LOGS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum StressOutputFormat {
    Text,
    Json,
    Junit,
}

/// CLI arguments for ferrum-stress
#[derive(Debug, Parser)]
#[command(name = "ferrum-stress")]
#[command(about = "FerrumGate in-process stress test tool")]
struct Args {
    /// Which scenario to run
    #[arg(long, default_value = "all")]
    scenario: String,

    /// Number of concurrent workers
    #[arg(long, default_value = "50")]
    concurrency: usize,

    /// Test duration in seconds
    #[arg(long, default_value = "10")]
    duration: u64,

    /// Enable bearer authentication
    #[arg(long, default_value = "false")]
    auth: bool,

    /// Bearer token when --auth is enabled
    #[arg(long, default_value = "test-stress-token")]
    token: String,

    /// Enable rate limiting
    #[arg(long, default_value = "false")]
    rate_limit: bool,

    /// Target an already-running FerrumGate server instead of starting an in-process server.
    #[arg(long)]
    server_url: Option<String>,

    /// Maximum allowed error rate as a ratio in [0.0, 1.0], e.g. 0.01 for 1%.
    #[arg(long)]
    max_error_rate: Option<f64>,

    /// Maximum allowed p95 latency in milliseconds.
    #[arg(long)]
    max_p95_ms: Option<u64>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = StressOutputFormat::Text)]
    output_format: StressOutputFormat,
}

/// Fake bridge for testing ingest without real MCP runtime
struct FakeBridge;

#[async_trait]
impl ferrum_sync::ExternalEventSource for FakeBridge {
    fn runtime_id(&self) -> &str {
        "stress://test"
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn try_connect(&self) -> ferrum_sync::Result<()> {
        Ok(())
    }

    async fn poll_events(&self) -> ferrum_sync::Result<Vec<ferrum_proto::ProvenanceEvent>> {
        Ok(vec![])
    }
}

#[async_trait]
impl RuntimeBridge for FakeBridge {
    async fn list_tools(&self) -> ferrum_sync::Result<Vec<BridgeToolInfo>> {
        Ok(vec![])
    }

    async fn submit_event(
        &self,
        _event: &ferrum_proto::ProvenanceEvent,
    ) -> ferrum_sync::Result<BridgeSubmitResult> {
        Ok(BridgeSubmitResult {
            accepted: true,
            message: Some("fake bridge accepted".to_string()),
        })
    }
}

/// Statistics collector for stress test results
#[derive(Debug, Default)]
struct Stats {
    latencies: Mutex<Vec<Duration>>,
    errors: AtomicU64,
    status_codes: Mutex<Vec<u16>>,
}

impl Stats {
    fn new() -> Self {
        Self {
            latencies: Mutex::new(Vec::new()),
            errors: AtomicU64::new(0),
            status_codes: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, latency: Duration, status_code: u16) {
        self.latencies.lock().unwrap().push(latency);
        self.status_codes.lock().unwrap().push(status_code);
    }

    fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn report(&self) -> StatsReport {
        let latencies = self.latencies.lock().unwrap();
        let total = latencies.len() as u64;
        let errors = self.errors.load(Ordering::Relaxed);

        if total == 0 {
            drop(latencies);
            return StatsReport {
                min: Duration::ZERO,
                max: Duration::ZERO,
                mean: Duration::ZERO,
                p50: Duration::ZERO,
                p90: Duration::ZERO,
                p95: Duration::ZERO,
                p99: Duration::ZERO,
                total_requests: 0,
                errors,
                req_per_sec: 0.0,
                status_histogram: std::collections::HashMap::new(),
            };
        }

        let mut sorted = latencies.clone();
        let total_count = total;
        drop(latencies); // Release lock before acquiring status_codes

        sorted.sort();

        let sum: Duration = sorted.iter().sum();
        let mean = sum / total_count as u32;

        let percentile = |p: f64| -> Duration {
            let idx = ((p / 100.0) * (total_count as f64 - 1.0)).round() as usize;
            sorted[idx.min(total_count as usize - 1)]
        };

        let mut status_histogram = std::collections::HashMap::new();
        for code in self.status_codes.lock().unwrap().iter() {
            *status_histogram.entry(*code).or_insert(0) += 1;
        }

        StatsReport {
            min: sorted.first().copied().unwrap_or(Duration::ZERO),
            max: sorted.last().copied().unwrap_or(Duration::ZERO),
            mean,
            p50: percentile(50.0),
            p90: percentile(90.0),
            p95: percentile(95.0),
            p99: percentile(99.0),
            total_requests: total_count,
            errors,
            req_per_sec: 0.0, // Caller overrides this
            status_histogram,
        }
    }
}

#[derive(Debug)]
struct StatsReport {
    min: Duration,
    max: Duration,
    mean: Duration,
    p50: Duration,
    p90: Duration,
    p95: Duration,
    p99: Duration,
    total_requests: u64,
    errors: u64,
    req_per_sec: f64,
    status_histogram: std::collections::HashMap<u16, u64>,
}

/// Test server harness for in-process stress testing
struct StressServer {
    base_url: String,
    _task: JoinHandle<()>,
    _temp_db: tempfile::NamedTempFile,
}

impl StressServer {
    async fn start(
        auth_enabled: bool,
        bearer_token: Option<String>,
        rate_limit_enabled: bool,
    ) -> Result<Self> {
        // Create temp SQLite database
        let temp_db = tempfile::NamedTempFile::new()?;
        let db_path = temp_db.path().to_str().unwrap();
        let dsn = format!("sqlite://{}", db_path);

        // Connect and apply migrations (use larger pool for stress testing)
        let store = Arc::new(
            SqliteStore::connect_with_pool_size(&dsn, 20)
                .await
                .context("failed to connect to sqlite")?,
        );
        store
            .apply_embedded_migrations()
            .await
            .context("failed to apply migrations")?;

        // Create runtime components
        let pdp = Arc::new(StaticPdpEngine);
        let cap = Arc::new(InMemoryCapabilityService::default());

        let mut registry = AdapterRegistry::default();
        registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
        let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

        let bridge: Arc<dyn RuntimeBridge> = Arc::new(FakeBridge) as _;
        let runtime = GatewayRuntime::new(
            pdp,
            cap,
            rollback,
            store as Arc<dyn StoreFacade>,
            vec![bridge],
        );

        let (rate_limit_per_second, rate_limit_burst) = if rate_limit_enabled {
            (2, 50)
        } else {
            // Keep the limiter effectively out of the way without violating
            // ServerConfig validation.
            (1_000_000, 10_000)
        };

        // Build router
        let router = if auth_enabled {
            let config = ServerConfig {
                auth_mode: AuthMode::Bearer,
                bearer_token,
                allow_insecure_nonlocal_bind: true,
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                store_dsn: dsn,
                log_filter: "warn".to_string(),
                log_format: ferrum_gateway::LogFormat::Text,
                store_synchronous: None,
                store_wal_autocheckpoint: None,
                rate_limit_per_second,
                rate_limit_burst,
                write_queue_threshold: 100,
                pg_max_connections: 10,
                pg_min_idle: 2,
                pg_acquire_timeout_secs: 5,
                pg_statement_timeout_ms: 5000,
                pg_idle_in_transaction_timeout_ms: 10000,
                fs_workdir: None,
                git_repo_roots: Vec::new(),
                sqlite_db_roots: Vec::new(),
                #[cfg(feature = "s3")]
                s3_config: None,
                oidc_config: None,
                agent_clock_skew_secs: 30,
            };
            ferrum_gateway::build_router_with_auth(runtime, config)
        } else {
            // Use build_router_with_auth with auth disabled instead of build_router.
            // build_router is test-only and gated behind cfg(test) or test-utils feature.
            let config = ServerConfig {
                auth_mode: AuthMode::Disabled,
                bearer_token: None,
                allow_insecure_nonlocal_bind: true,
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                store_dsn: dsn,
                log_filter: "warn".to_string(),
                log_format: ferrum_gateway::LogFormat::Text,
                store_synchronous: None,
                store_wal_autocheckpoint: None,
                rate_limit_per_second,
                rate_limit_burst,
                write_queue_threshold: 100,
                pg_max_connections: 10,
                pg_min_idle: 2,
                pg_acquire_timeout_secs: 5,
                pg_statement_timeout_ms: 5000,
                pg_idle_in_transaction_timeout_ms: 10000,
                fs_workdir: None,
                git_repo_roots: Vec::new(),
                sqlite_db_roots: Vec::new(),
                #[cfg(feature = "s3")]
                s3_config: None,
                oidc_config: None,
                agent_clock_skew_secs: 30,
            };
            ferrum_gateway::build_router_with_auth(runtime, config)
        };

        // Bind to random port
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let base_url = format!("http://127.0.0.1:{}", port);

        // Spawn server
        let task = tokio::spawn(async move {
            axum::serve(listener, router.into_make_service())
                .await
                .expect("server error");
        });

        Ok(Self {
            base_url,
            _task: task,
            _temp_db: temp_db,
        })
    }
}

impl Drop for StressServer {
    fn drop(&mut self) {
        self._task.abort();
    }
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 1.0 {
        format!("{:.3} ms", ms)
    } else if ms < 10.0 {
        format!("{:.2} ms", ms)
    } else {
        format!("{:.1} ms", ms)
    }
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    result
}

fn print_separator() {
    if !TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    println!("═══════════════════════════════════════════════════════════════");
}

fn print_scenario_header(name: &str, workers: usize, duration: u64) {
    if !TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    print_separator();
    println!("  SCENARIO: {}  ({} workers, {}s)", name, workers, duration);
    print_separator();
}

fn print_stats_report(report: &StatsReport) {
    if !TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let error_pct = if report.total_requests > 0 {
        report.errors as f64 / report.total_requests as f64 * 100.0
    } else {
        0.0
    };

    println!(
        "  Requests:     {} total",
        format_number(report.total_requests)
    );
    println!(
        "  Errors:       {} ({:.2}%)",
        format_number(report.errors),
        error_pct
    );
    println!("  Throughput:   {:.1} req/s", report.req_per_sec);
    println!();
    println!("  Latency:");
    println!("    min:    {}", format_duration(report.min));
    println!("    p50:    {}", format_duration(report.p50));
    println!("    p90:    {}", format_duration(report.p90));
    println!("    p95:    {}", format_duration(report.p95));
    println!("    p99:    {}", format_duration(report.p99));
    println!("    max:    {}", format_duration(report.max));
    println!("    mean:   {}", format_duration(report.mean));
    println!();
    println!("  Status Codes:");
    for (code, count) in &report.status_histogram {
        println!("    {}:  {}", code, format_number(*count));
    }
    print_separator();
    println!();
}

#[derive(Debug, Serialize)]
struct ScenarioSummary {
    scenario: String,
    concurrency: usize,
    duration_secs: u64,
    total_requests: u64,
    errors: u64,
    error_rate: f64,
    req_per_sec: f64,
    p95_ms: f64,
    p99_ms: f64,
    status_histogram: std::collections::HashMap<u16, u64>,
}

#[derive(Debug, Serialize)]
struct StressSummary {
    target: String,
    auth_enabled: bool,
    rate_limit_enabled: bool,
    scenarios: Vec<ScenarioSummary>,
}

fn scenario_summary(
    scenario: &str,
    concurrency: usize,
    duration_secs: u64,
    stats: &Stats,
) -> ScenarioSummary {
    let mut report = stats.report();
    report.req_per_sec = if duration_secs > 0 {
        report.total_requests as f64 / duration_secs as f64
    } else {
        0.0
    };
    let denominator = report.total_requests + report.errors;
    let error_rate = if denominator > 0 {
        report.errors as f64 / denominator as f64
    } else {
        0.0
    };
    ScenarioSummary {
        scenario: scenario.to_string(),
        concurrency,
        duration_secs,
        total_requests: report.total_requests,
        errors: report.errors,
        error_rate,
        req_per_sec: report.req_per_sec,
        p95_ms: report.p95.as_secs_f64() * 1000.0,
        p99_ms: report.p99.as_secs_f64() * 1000.0,
        status_histogram: report.status_histogram,
    }
}

fn check_thresholds(
    summaries: &[ScenarioSummary],
    max_error_rate: Option<f64>,
    max_p95_ms: Option<u64>,
) -> Result<()> {
    if let Some(max_error_rate) = max_error_rate
        && !(0.0..=1.0).contains(&max_error_rate)
    {
        bail!("--max-error-rate must be between 0.0 and 1.0");
    }
    for summary in summaries {
        if let Some(max_error_rate) = max_error_rate
            && summary.error_rate > max_error_rate
        {
            bail!(
                "scenario '{}' exceeded max error rate: {:.4} > {:.4}",
                summary.scenario,
                summary.error_rate,
                max_error_rate
            );
        }
        if let Some(max_p95_ms) = max_p95_ms
            && summary.p95_ms > max_p95_ms as f64
        {
            bail!(
                "scenario '{}' exceeded max p95 latency: {:.2}ms > {}ms",
                summary.scenario,
                summary.p95_ms,
                max_p95_ms
            );
        }
    }
    Ok(())
}

fn print_json_summary(summary: &StressSummary) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(summary)?);
    Ok(())
}

fn escape_xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn print_junit_summary(summary: &StressSummary) {
    println!(
        r#"<testsuite name="ferrum-stress" tests="{}">"#,
        summary.scenarios.len()
    );
    for scenario in &summary.scenarios {
        println!(
            r#"  <testcase classname="ferrum-stress" name="{}">"#,
            escape_xml_attr(&scenario.scenario)
        );
        println!(
            r#"    <system-out>requests={} errors={} error_rate={:.6} p95_ms={:.3}</system-out>"#,
            scenario.total_requests, scenario.errors, scenario.error_rate, scenario.p95_ms
        );
        println!("  </testcase>");
    }
    println!("</testsuite>");
}

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════════════════

async fn run_health_scenario(
    client: &Client,
    base_url: &str,
    _token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let start = Instant::now();
                    let resp = client.get(format!("{}/v1/healthz", base_url)).send().await;
                    let latency = start.elapsed();
                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("health", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_auth_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|worker_id| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let start = Instant::now();
                    // 90% valid, 10% invalid
                    let use_valid = worker_id % 10 != 0;
                    let auth_value = if use_valid {
                        format!(
                            "Bearer {}",
                            token.as_ref().unwrap_or(&"invalid".to_string())
                        )
                    } else {
                        "Bearer invalid-token".to_string()
                    };

                    let resp = client
                        .get(format!("{}/v1/approvals", base_url))
                        .header("Authorization", auth_value)
                        .send()
                        .await;

                    let latency = start.elapsed();
                    match resp {
                        Ok(r) => {
                            stats.record(latency, r.status().as_u16());
                        }
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("auth", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_provenance_query_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let start = Instant::now();
                    let mut req = client
                        .post(format!("{}/v1/provenance/query", base_url))
                        .json(&serde_json::json!({}));

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let latency = start.elapsed();
                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("provenance-query", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_intent_compile_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let start = Instant::now();
                    let body = IntentCompileRequest {
                        principal_id: PrincipalId::new(),
                        session_id: None,
                        channel_id: None,
                        title: "stress-test-intent".to_string(),
                        goal: "Performance test goal".to_string(),
                        agent_plan_summary: None,
                        trusted_context: JsonMap::new(),
                        raw_inputs: vec![],
                        requested_resource_scope: vec![],
                        requested_risk_tier: Some(RiskTier::Medium),
                        approval_mode: None,
                        metadata: JsonMap::new(),
                    };

                    let mut req = client
                        .post(format!("{}/v1/intents/compile", base_url))
                        .json(&body);

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let latency = start.elapsed();
                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("intent-compile", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_execution_pipeline_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let pipeline_start = Instant::now();

                    // Step 1: Compile intent
                    let intent_id = PrincipalId::new();
                    let intent_body = IntentCompileRequest {
                        principal_id: intent_id,
                        session_id: None,
                        channel_id: None,
                        title: "stress-test-intent".to_string(),
                        goal: "Performance test goal".to_string(),
                        agent_plan_summary: None,
                        trusted_context: JsonMap::new(),
                        raw_inputs: vec![],
                        requested_resource_scope: vec![],
                        requested_risk_tier: Some(RiskTier::Medium),
                        approval_mode: None,
                        metadata: JsonMap::new(),
                    };

                    let mut req = client
                        .post(format!("{}/v1/intents/compile", base_url))
                        .json(&intent_body);

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let Ok(resp) = resp else {
                        stats.record_error();
                        continue;
                    };
                    let status = resp.status();
                    if status != 200 {
                        let body = resp.text().await.unwrap_or_default();
                        let ec = ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                        if ec < MAX_ERROR_LOGS {
                            eprintln!("[WARN] execution-pipeline: step 1 (compile) failed: status={}, body={}", status, body);
                        }
                        stats.record_error();
                        continue;
                    }

                    let Ok(intent_resp) = resp.json::<IntentCompileResponse>().await else {
                        stats.record_error();
                        continue;
                    };

                    let intent_id = intent_resp.envelope.intent_id;
                    let proposal_id = ferrum_proto::ProposalId::new();

                    // Step 2: Evaluate proposal
                    let proposal_body = ActionProposal {
                        proposal_id,
                        intent_id,
                        step_index: 0,
                        title: "test proposal".to_string(),
                        tool_name: "test_tool".to_string(),
                        server_name: "test_server".to_string(),
                        raw_arguments: serde_json::json!({}),
                        expected_effect: "read".to_string(),
                        estimated_risk: RiskTier::Medium,
                        requested_rollback_class: RollbackClass::R0NativeReversible,
                        taint_inputs: vec![],
                        metadata: JsonMap::new(),
                        created_at: Utc::now(),
                    };

                    let mut req = client
                        .post(format!(
                            "{}/v1/proposals/{}/evaluate",
                            base_url, proposal_id
                        ))
                        .json(&proposal_body);

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let Ok(resp) = resp else {
                        stats.record_error();
                        continue;
                    };
                    let status = resp.status();
                    if status != 200 {
                        let body = resp.text().await.unwrap_or_default();
                        let ec = ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                        if ec < MAX_ERROR_LOGS {
                            eprintln!("[WARN] execution-pipeline: step 2 (evaluate) failed: status={}, body={}", status, body);
                        }
                        stats.record_error();
                        continue;
                    }

                    // Step 3: Mint capability (with retry)
                    let mint_body = CapabilityMintRequest {
                        intent_id,
                        proposal_id,
                        tool_binding: ferrum_proto::ToolBinding {
                            server_name: "test_server".to_string(),
                            tool_name: "test_tool".to_string(),
                            tool_version: None,
                        },
                        resource_bindings: vec![],
                        argument_constraints: vec![],
                        taint_budget: ferrum_proto::TaintBudget {
                            max_taint_score: 10,
                            allow_external_tool_output: true,
                            allow_external_metadata: true,
                            allow_untrusted_text: true,
                        },
                        approval_binding: None,
                        requested_ttl_secs: 60,
                        metadata: JsonMap::new(),
                    };

                    let mut mint_resp = None;
                    let mut mint_status = 0u16;
                    for attempt in 0..2u32 {
                        let mut req = client
                            .post(format!("{}/v1/capabilities/mint", base_url))
                            .json(&mint_body);

                        if let Some(t) = &token {
                            req = req.header("Authorization", format!("Bearer {}", t));
                        }

                        match req.send().await {
                            Ok(r) => {
                                mint_status = r.status().as_u16();
                                if mint_status == 200 {
                                    mint_resp = Some(r);
                                    break;
                                }
                                if attempt < 2 {
                                    tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                    continue;
                                }
                                mint_resp = Some(r);
                            }
                            Err(_) if attempt < 2 => {
                                tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                continue;
                            }
                            Err(_) => break,
                        }
                    }

                    let Some(resp) = mint_resp else {
                        stats.record_error();
                        continue;
                    };
                    if mint_status != 200 {
                        let body = resp.text().await.unwrap_or_default();
                        let err_count = ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                        if err_count < 5 {
                            eprintln!("[WARN] execution-pipeline: step 3 (mint) failed after retries: status={}, body={}", mint_status, body);
                        }
                        stats.record_error();
                        continue;
                    }

                    let Ok(mint_resp) = resp.json::<CapabilityMintResponse>().await else {
                        stats.record_error();
                        continue;
                    };

                    let capability_id = mint_resp.lease.capability_id;

                    // Step 4: Authorize execution (with retry)
                    let auth_body = AuthorizeExecutionRequest {
                        proposal_id,
                        capability_id,
                        dry_run: true,
                    };

                    let mut auth_resp_final = None;
                    let mut auth_status = 0u16;
                    for attempt in 0..2u32 {
                        let mut req = client
                            .post(format!("{}/v1/executions/authorize", base_url))
                            .json(&auth_body);

                        if let Some(t) = &token {
                            req = req.header("Authorization", format!("Bearer {}", t));
                        }

                        match req.send().await {
                            Ok(r) => {
                                auth_status = r.status().as_u16();
                                if auth_status == 200 {
                                    auth_resp_final = Some(r);
                                    break;
                                }
                                if attempt < 2 {
                                    tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                    continue;
                                }
                                auth_resp_final = Some(r);
                            }
                            Err(_) if attempt < 2 => {
                                tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                continue;
                            }
                            Err(_) => break,
                        }
                    }

                    let Some(resp) = auth_resp_final else {
                        stats.record_error();
                        continue;
                    };
                    if auth_status != 200 {
                        let body = resp.text().await.unwrap_or_default();
                        let ec = ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                        if ec < MAX_ERROR_LOGS {
                            eprintln!("[WARN] execution-pipeline: step 4 (authorize) failed after retries: status={}, body={}", auth_status, body);
                        }
                        stats.record_error();
                        continue;
                    }

                    let Ok(auth_resp) = resp.json::<AuthorizeExecutionResponse>().await else {
                        stats.record_error();
                        continue;
                    };

                    let execution_id = auth_resp.execution.execution_id;

                    // Step 5: Prepare execution (retry on 5xx)
                    let mut prepare_resp = None;
                    let mut prepare_status = 0u16;
                    for attempt in 0..2u32 {
                        let mut req = client
                            .post(format!(
                                "{}/v1/executions/{}/prepare",
                                base_url, execution_id
                            ))
                            .json(&serde_json::json!({}));

                        if let Some(t) = &token {
                            req = req.header("Authorization", format!("Bearer {}", t));
                        }

                        match req.send().await {
                            Ok(r) => {
                                prepare_status = r.status().as_u16();
                                if prepare_status == 200 {
                                    prepare_resp = Some(r);
                                    break;
                                }
                                if attempt < 2 {
                                    tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                    continue;
                                }
                                prepare_resp = Some(r);
                            }
                            Err(_) if attempt < 2 => {
                                tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                continue;
                            }
                            Err(_) => break,
                        }
                    }

                    let Some(resp) = prepare_resp else {
                        stats.record_error();
                        continue;
                    };
                    if prepare_status != 200 {
                        let body = resp.text().await.unwrap_or_default();
                        let ec = ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                        if ec < MAX_ERROR_LOGS {
                            eprintln!("[WARN] execution-pipeline: step 5 (prepare) failed after retries: status={}, body={}", prepare_status, body);
                        }
                        stats.record_error();
                        continue;
                    }

                    // Step 6: Evaluate outcome
                    let outcome_body = OutcomeReport {
                        execution_id,
                        actual_effect: EffectType::ReadOnlyAnalysis,
                        description: "test outcome".to_string(),
                        result_digest: None,
                        adapter_success: true,
                        adapter_metadata: JsonMap::new(),
                    };

                    let mut req = client
                        .post(format!(
                            "{}/v1/executions/{}/evaluate-outcome",
                            base_url, execution_id
                        ))
                        .json(&outcome_body);

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let latency = pipeline_start.elapsed();

                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("execution-pipeline", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_capability_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    // Step 1: Compile intent to get real intent_id
                    let intent_body = IntentCompileRequest {
                        principal_id: PrincipalId::new(),
                        session_id: None,
                        channel_id: None,
                        title: "stress-test-intent".to_string(),
                        goal: "Performance test goal".to_string(),
                        agent_plan_summary: None,
                        trusted_context: JsonMap::new(),
                        raw_inputs: vec![],
                        requested_resource_scope: vec![],
                        requested_risk_tier: Some(RiskTier::Medium),
                        approval_mode: None,
                        metadata: JsonMap::new(),
                    };

                    let mut req = client
                        .post(format!("{}/v1/intents/compile", base_url))
                        .json(&intent_body);

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let Ok(resp) = resp else {
                        stats.record_error();
                        continue;
                    };
                    if resp.status() != 200 {
                        stats.record_error();
                        continue;
                    }

                    let Ok(intent_resp) = resp.json::<IntentCompileResponse>().await else {
                        stats.record_error();
                        continue;
                    };

                    let intent_id = intent_resp.envelope.intent_id;
                    let proposal_id = ferrum_proto::ProposalId::new();

                    // Step 2: Evaluate proposal with the real intent_id
                    let proposal_body = ActionProposal {
                        proposal_id,
                        intent_id,
                        step_index: 0,
                        title: "test proposal".to_string(),
                        tool_name: "test_tool".to_string(),
                        server_name: "test_server".to_string(),
                        raw_arguments: serde_json::json!({}),
                        expected_effect: "read".to_string(),
                        estimated_risk: RiskTier::Medium,
                        requested_rollback_class: RollbackClass::R0NativeReversible,
                        taint_inputs: vec![],
                        metadata: JsonMap::new(),
                        created_at: Utc::now(),
                    };

                    let mut req = client
                        .post(format!(
                            "{}/v1/proposals/{}/evaluate",
                            base_url, proposal_id
                        ))
                        .json(&proposal_body);

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let Ok(resp) = resp else {
                        stats.record_error();
                        continue;
                    };
                    if resp.status() != 200 {
                        stats.record_error();
                        continue;
                    }

                    // Step 3: Mint capability with the real intent_id and proposal_id (with retry)
                    let mint_body = CapabilityMintRequest {
                        intent_id,
                        proposal_id,
                        tool_binding: ferrum_proto::ToolBinding {
                            server_name: "test_server".to_string(),
                            tool_name: "test_tool".to_string(),
                            tool_version: None,
                        },
                        resource_bindings: vec![],
                        argument_constraints: vec![],
                        taint_budget: ferrum_proto::TaintBudget {
                            max_taint_score: 10,
                            allow_external_tool_output: true,
                            allow_external_metadata: true,
                            allow_untrusted_text: true,
                        },
                        approval_binding: None,
                        requested_ttl_secs: 60,
                        metadata: JsonMap::new(),
                    };

                    let mut mint_resp = None;
                    let mut mint_status = 0u16;
                    for attempt in 0..2u32 {
                        let mut req = client
                            .post(format!("{}/v1/capabilities/mint", base_url))
                            .json(&mint_body);

                        if let Some(t) = &token {
                            req = req.header("Authorization", format!("Bearer {}", t));
                        }

                        match req.send().await {
                            Ok(r) => {
                                mint_status = r.status().as_u16();
                                if mint_status == 200 {
                                    mint_resp = Some(r);
                                    break;
                                }
                                if attempt < 2 {
                                    tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                    continue;
                                }
                                mint_resp = Some(r);
                            }
                            Err(_) if attempt < 2 => {
                                tokio_sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                                continue;
                            }
                            Err(_) => break,
                        }
                    }

                    let Some(resp) = mint_resp else {
                        stats.record_error();
                        continue;
                    };
                    if mint_status != 200 {
                        let err_count = ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                        if err_count < MAX_ERROR_LOGS {
                            let body = resp.text().await.unwrap_or_default();
                            eprintln!("[WARN] capability: step 3 (mint) failed after retries: status={}, body={}", mint_status, body);
                        }
                        stats.record_error();
                        continue;
                    }

                    let Ok(mint_resp) = resp.json::<CapabilityMintResponse>().await else {
                        stats.record_error();
                        continue;
                    };

                    let capability_id = mint_resp.lease.capability_id;

                    // Step 4: Revoke capability
                    let start = Instant::now();
                    let mut req = client
                        .post(format!(
                            "{}/v1/capabilities/{}/revoke",
                            base_url, capability_id
                        ))
                        .json(&serde_json::json!({}));

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let latency = start.elapsed();

                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("capability", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_sqlite_contention_scenario(
    client: &Client,
    base_url: &str,
    _token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let start = Instant::now();
                    let body = ProvenanceIngestRequest {
                        source_runtime_id: "stress://test".to_string(),
                        kind: ProvenanceEventKind::ExternalEventReceived,
                        description: "Concurrent write test event".to_string(),
                        execution_id: None,
                        intent_id: None,
                        trust_labels: vec![],
                        sensitivity_labels: vec![],
                        metadata: JsonMap::new(),
                    };

                    let resp = client
                        .post(format!("{}/v1/provenance/ingest", base_url))
                        .json(&body)
                        .send()
                        .await;
                    let latency = start.elapsed();

                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("sqlite-contention", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

async fn run_rate_limit_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    if TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
        println!();
        println!(
            "  This scenario sends burst traffic to a workload route to detect 429 responses."
        );
        println!();
    }

    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let start = Instant::now();
                    let mut req = client.get(format!("{}/v1/intents?limit=1", base_url));

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let latency = start.elapsed();
                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("rate-limit", concurrency, duration_secs);
    print_stats_report(&report);

    // Check for 429 responses
    if !TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
        return stats;
    }
    if let Some(count) = report.status_histogram.get(&429) {
        println!(
            "  Rate limit detected: {} requests got 429 (Too Many Requests)",
            format_number(*count)
        );
    } else {
        println!("  No 429 responses detected (rate limiting may not be enabled)");
    }

    stats
}

#[derive(Clone, Copy)]
enum MixedOp {
    Healthz,
    ProvenanceQuery,
    IntentCompile,
    ProvenanceIngest,
    Approvals,
    CapabilityMint,
}

impl MixedOp {
    fn random() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let r = COUNTER.fetch_add(1, Ordering::Relaxed) % 100;
        if r < 30 {
            MixedOp::Healthz
        } else if r < 50 {
            MixedOp::ProvenanceQuery
        } else if r < 65 {
            MixedOp::IntentCompile
        } else if r < 80 {
            MixedOp::ProvenanceIngest
        } else if r < 90 {
            MixedOp::Approvals
        } else {
            MixedOp::CapabilityMint
        }
    }
}

async fn run_mixed_scenario(
    client: &Client,
    base_url: &str,
    token: &Option<String>,
    concurrency: usize,
    duration_secs: u64,
) -> Arc<Stats> {
    let stats = Arc::new(Stats::new());
    let end_time = Instant::now() + Duration::from_secs(duration_secs);

    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let client = client.clone();
            let base_url = base_url.to_string();
            let token = token.clone();
            let stats = Arc::clone(&stats);

            tokio::spawn(async move {
                while Instant::now() < end_time {
                    let op = MixedOp::random();
                    let start = Instant::now();
                    let (method, path, body) = match op {
                        MixedOp::Healthz => ("GET", "/v1/healthz".to_string(), None),
                        MixedOp::ProvenanceQuery => (
                            "POST",
                            "/v1/provenance/query".to_string(),
                            Some(serde_json::json!({})),
                        ),
                        MixedOp::IntentCompile => {
                            let body = IntentCompileRequest {
                                principal_id: PrincipalId::new(),
                                session_id: None,
                                channel_id: None,
                                title: "stress-test-intent".to_string(),
                                goal: "Performance test goal".to_string(),
                                agent_plan_summary: None,
                                trusted_context: JsonMap::new(),
                                raw_inputs: vec![],
                                requested_resource_scope: vec![],
                                requested_risk_tier: Some(RiskTier::Medium),
                                approval_mode: None,
                                metadata: JsonMap::new(),
                            };
                            (
                                "POST",
                                "/v1/intents/compile".to_string(),
                                Some(serde_json::to_value(body).unwrap()),
                            )
                        }
                        MixedOp::ProvenanceIngest => (
                            "POST",
                            "/v1/provenance/ingest".to_string(),
                            Some(serde_json::json!({
                                "source_runtime_id": "stress://test",
                                "kind": "ExternalEventReceived",
                                "description": "Mixed workload test",
                                "trust_labels": [],
                                "sensitivity_labels": [],
                                "metadata": {}
                            })),
                        ),
                        MixedOp::Approvals => ("GET", "/v1/approvals".to_string(), None),
                        MixedOp::CapabilityMint => {
                            // Step 1: Compile intent to get real intent_id
                            let intent_body = IntentCompileRequest {
                                principal_id: PrincipalId::new(),
                                session_id: None,
                                channel_id: None,
                                title: "stress-test-intent".to_string(),
                                goal: "Performance test goal".to_string(),
                                agent_plan_summary: None,
                                trusted_context: JsonMap::new(),
                                raw_inputs: vec![],
                                requested_resource_scope: vec![],
                                requested_risk_tier: Some(RiskTier::Medium),
                                approval_mode: None,
                                metadata: JsonMap::new(),
                            };

                            let mut req = client
                                .post(format!("{}/v1/intents/compile", base_url))
                                .json(&intent_body);

                            if let Some(t) = &token {
                                req = req.header("Authorization", format!("Bearer {}", t));
                            }

                            let resp = req.send().await;
                            let Ok(resp) = resp else {
                                continue;
                            };
                            if resp.status() != 200 {
                                continue;
                            }

                            let Ok(intent_resp) = resp.json::<IntentCompileResponse>().await else {
                                continue;
                            };

                            let intent_id = intent_resp.envelope.intent_id;
                            let proposal_id = ferrum_proto::ProposalId::new();

                            // Step 2: Evaluate proposal with the real intent_id
                            let proposal_body = ActionProposal {
                                proposal_id,
                                intent_id,
                                step_index: 0,
                                title: "test proposal".to_string(),
                                tool_name: "test_tool".to_string(),
                                server_name: "test_server".to_string(),
                                raw_arguments: serde_json::json!({}),
                                expected_effect: "read".to_string(),
                                estimated_risk: RiskTier::Medium,
                                requested_rollback_class: RollbackClass::R0NativeReversible,
                                taint_inputs: vec![],
                                metadata: JsonMap::new(),
                                created_at: Utc::now(),
                            };

                            let mut req = client
                                .post(format!(
                                    "{}/v1/proposals/{}/evaluate",
                                    base_url, proposal_id
                                ))
                                .json(&proposal_body);

                            if let Some(t) = &token {
                                req = req.header("Authorization", format!("Bearer {}", t));
                            }

                            let resp = req.send().await;
                            let Ok(resp) = resp else {
                                continue;
                            };
                            if resp.status() != 200 {
                                continue;
                            }

                            // Step 3: Mint capability with real IDs
                            let body = CapabilityMintRequest {
                                intent_id,
                                proposal_id,
                                tool_binding: ferrum_proto::ToolBinding {
                                    server_name: "test_server".to_string(),
                                    tool_name: "test_tool".to_string(),
                                    tool_version: None,
                                },
                                resource_bindings: vec![],
                                argument_constraints: vec![],
                                taint_budget: ferrum_proto::TaintBudget {
                                    max_taint_score: 10,
                                    allow_external_tool_output: true,
                                    allow_external_metadata: true,
                                    allow_untrusted_text: true,
                                },
                                approval_binding: None,
                                requested_ttl_secs: 60,
                                metadata: JsonMap::new(),
                            };
                            (
                                "POST",
                                "/v1/capabilities/mint".to_string(),
                                Some(serde_json::to_value(body).unwrap()),
                            )
                        }
                    };

                    let mut req = match method {
                        "GET" => client.get(format!("{}{}", base_url, path)),
                        "POST" => {
                            let b = body.unwrap();
                            client.post(format!("{}{}", base_url, path)).json(&b)
                        }
                        _ => unreachable!(),
                    };

                    if let Some(t) = &token {
                        req = req.header("Authorization", format!("Bearer {}", t));
                    }

                    let resp = req.send().await;
                    let latency = start.elapsed();

                    match resp {
                        Ok(r) => stats.record(latency, r.status().as_u16()),
                        Err(_) => stats.record_error(),
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    let mut report = stats.report();
    report.req_per_sec = report.total_requests as f64 / duration_secs as f64;
    print_scenario_header("mixed", concurrency, duration_secs);
    print_stats_report(&report);

    stats
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.concurrency == 0 {
        bail!("--concurrency must be greater than 0");
    }
    if args.duration == 0 {
        bail!("--duration must be greater than 0");
    }
    if let Some(max_error_rate) = args.max_error_rate
        && !(0.0..=1.0).contains(&max_error_rate)
    {
        bail!("--max-error-rate must be between 0.0 and 1.0");
    }
    TEXT_OUTPUT_ENABLED.store(
        args.output_format == StressOutputFormat::Text,
        Ordering::Relaxed,
    );

    // Suppress all tracing output - stress test uses its own stats
    // axum response_failed ERROR logs are too noisy at 50 workers
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::ERROR)
        .with_target(false)
        .init();

    if TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
        println!();
        println!("╔═══════════════════════════════════════════════════════════════╗");
        println!("║           FERRUM-GATE STRESS TEST v0.1.0                     ║");
        println!("╚═══════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Configuration:");
        println!("    Scenario:    {}", args.scenario);
        println!("    Concurrency: {}", args.concurrency);
        println!("    Duration:    {}s", args.duration);
        println!(
            "    Target:      {}",
            args.server_url.as_deref().unwrap_or("in-process")
        );
        println!(
            "    Auth:        {}",
            if args.auth { "enabled" } else { "disabled" }
        );
        println!(
            "    Rate Limit:  {}",
            if args.rate_limit {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!();
    }

    // Start test server
    let token = if args.auth {
        Some(args.token.clone())
    } else {
        None
    };

    let (base_url, _server) = if let Some(server_url) = args.server_url.clone() {
        (server_url.trim_end_matches('/').to_string(), None)
    } else {
        if TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
            println!("Starting test server...");
        }
        let server = StressServer::start(args.auth, token.clone(), args.rate_limit).await?;
        let base_url = server.base_url.clone();
        if TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
            println!("Server started at: {}", base_url);
            println!();
        }
        (base_url, Some(server))
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to build HTTP client")?;

    let scenario = args.scenario.to_lowercase();
    let mut summaries = Vec::new();
    macro_rules! run_and_record {
        ($name:literal, $concurrency:expr, $future:expr) => {{
            let effective_concurrency = $concurrency;
            let stats = $future.await;
            summaries.push(scenario_summary(
                $name,
                effective_concurrency,
                args.duration,
                &stats,
            ));
        }};
    }
    match scenario.as_str() {
        "health" => {
            run_and_record!(
                "health",
                args.concurrency,
                run_health_scenario(&client, &base_url, &token, args.concurrency, args.duration,)
            );
        }
        "auth" => {
            run_and_record!(
                "auth",
                args.concurrency,
                run_auth_scenario(&client, &base_url, &token, args.concurrency, args.duration,)
            );
        }
        "provenance-query" => {
            run_and_record!(
                "provenance-query",
                args.concurrency,
                run_provenance_query_scenario(
                    &client,
                    &base_url,
                    &token,
                    args.concurrency,
                    args.duration,
                )
            );
        }
        "intent-compile" => {
            let effective_concurrency = std::cmp::min(args.concurrency, 5);
            run_and_record!(
                "intent-compile",
                effective_concurrency,
                run_intent_compile_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
        }
        "execution-pipeline" => {
            let effective_concurrency = std::cmp::min(args.concurrency, 5);
            run_and_record!(
                "execution-pipeline",
                effective_concurrency,
                run_execution_pipeline_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
        }
        "capability" => {
            let effective_concurrency = std::cmp::min(args.concurrency, 5);
            run_and_record!(
                "capability",
                effective_concurrency,
                run_capability_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
        }
        "sqlite-contention" => {
            run_and_record!(
                "sqlite-contention",
                args.concurrency,
                run_sqlite_contention_scenario(
                    &client,
                    &base_url,
                    &token,
                    args.concurrency,
                    args.duration,
                )
            );
        }
        "rate-limit" => {
            run_and_record!(
                "rate-limit",
                args.concurrency,
                run_rate_limit_scenario(
                    &client,
                    &base_url,
                    &token,
                    args.concurrency,
                    args.duration,
                )
            );
        }
        "mixed" => {
            let effective_concurrency = std::cmp::min(args.concurrency, 5);
            run_and_record!(
                "mixed",
                effective_concurrency,
                run_mixed_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
        }
        "all" => {
            // Run all scenarios sequentially
            if TEXT_OUTPUT_ENABLED.load(Ordering::Relaxed) {
                println!("Running ALL scenarios sequentially...\n");
            }

            run_and_record!(
                "health",
                args.concurrency,
                run_health_scenario(&client, &base_url, &token, args.concurrency, args.duration,)
            );
            run_and_record!(
                "auth",
                args.concurrency,
                run_auth_scenario(&client, &base_url, &token, args.concurrency, args.duration,)
            );
            run_and_record!(
                "provenance-query",
                args.concurrency,
                run_provenance_query_scenario(
                    &client,
                    &base_url,
                    &token,
                    args.concurrency,
                    args.duration,
                )
            );
            let effective_concurrency = std::cmp::min(args.concurrency, 5);
            run_and_record!(
                "intent-compile",
                effective_concurrency,
                run_intent_compile_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
            run_and_record!(
                "execution-pipeline",
                effective_concurrency,
                run_execution_pipeline_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
            run_and_record!(
                "capability",
                effective_concurrency,
                run_capability_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
            run_and_record!(
                "sqlite-contention",
                args.concurrency,
                run_sqlite_contention_scenario(
                    &client,
                    &base_url,
                    &token,
                    args.concurrency,
                    args.duration,
                )
            );
            run_and_record!(
                "rate-limit",
                args.concurrency,
                run_rate_limit_scenario(
                    &client,
                    &base_url,
                    &token,
                    args.concurrency,
                    args.duration,
                )
            );
            run_and_record!(
                "mixed",
                effective_concurrency,
                run_mixed_scenario(
                    &client,
                    &base_url,
                    &token,
                    effective_concurrency, // cap at 5: SQLite single-writer bottleneck
                    args.duration,
                )
            );
        }
        _ => {
            eprintln!("Unknown scenario: {}", scenario);
            eprintln!(
                "Available scenarios: health, auth, provenance-query, intent-compile, \
                 execution-pipeline, capability, sqlite-contention, rate-limit, mixed, all"
            );
            std::process::exit(1);
        }
    }

    let summary = StressSummary {
        target: base_url,
        auth_enabled: args.auth,
        rate_limit_enabled: args.rate_limit,
        scenarios: summaries,
    };
    check_thresholds(&summary.scenarios, args.max_error_rate, args.max_p95_ms)?;

    match args.output_format {
        StressOutputFormat::Text => {
            println!();
            println!("Stress test completed.");
        }
        StressOutputFormat::Json => print_json_summary(&summary)?,
        StressOutputFormat::Junit => print_junit_summary(&summary),
    }

    // Force exit: reqwest connection pool and axum server spawn background
    // tasks that keep the tokio runtime alive. For a stress-test tool,
    // a clean forced exit is the pragmatic choice.
    std::process::exit(0);
}
