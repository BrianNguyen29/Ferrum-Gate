use anyhow::{Context, Result, anyhow};
use chrono::{Duration, Utc};
use ferrum_adapter_sqlite::SqliteRollbackAdapter;
use ferrum_proto::{
    ActionProposal, ApprovalMode, CapabilityId, CapabilityLease, CapabilityStatus, Decision,
    EffectType, ExecutionRecord, ExecutionState, IntentEnvelope, IntentId, IntentStatus,
    OutcomeClause, PolicyBundleId, PrincipalId, ProposalId, ResourceBinding, ResourceMode,
    RiskTier, RollbackClass, RollbackContract, RollbackState, RollbackTarget, TaintBudget,
    TimeBudget, ToolBinding, TrustContextSummary,
};
use ferrum_rollback::RollbackAdapter;
use ferrum_store::{
    CapabilityRepo, ExecutionRepo, IntentRepo, ProposalRepo, RollbackRepo, SqliteStore,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use tempfile::TempDir;
use tokio::task::JoinSet;

const DEFAULT_CONCURRENCY: usize = 10;
const DEFAULT_ITERATIONS: usize = 100;

#[derive(Clone, Debug)]
struct BenchConfig {
    concurrency: usize,
    iterations: usize,
}

#[derive(Clone, Debug)]
struct ScenarioResult {
    scenario: &'static str,
    surface: &'static str,
    operations: usize,
    total_duration: StdDuration,
    throughput_ops_per_sec: f64,
    avg_latency_ms: f64,
    min_latency_ms: f64,
    max_latency_ms: f64,
    error_count: usize,
}

#[derive(Default)]
struct ScenarioMeasurements {
    durations: Mutex<Vec<StdDuration>>,
    errors: Mutex<Vec<String>>,
}

impl ScenarioMeasurements {
    fn record_success(&self, duration: StdDuration) {
        self.durations
            .lock()
            .expect("durations mutex poisoned")
            .push(duration);
    }

    fn record_error(&self, error: anyhow::Error) {
        self.errors
            .lock()
            .expect("errors mutex poisoned")
            .push(error.to_string());
    }

    fn finish(
        self,
        scenario: &'static str,
        surface: &'static str,
        started_at: Instant,
    ) -> Result<ScenarioResult> {
        let durations = self
            .durations
            .into_inner()
            .expect("durations mutex poisoned");
        let errors = self.errors.into_inner().expect("errors mutex poisoned");
        if durations.is_empty() {
            return Err(anyhow!(
                "scenario {scenario} produced no successful measurements"
            ));
        }

        let total_duration = started_at.elapsed();
        let total_ops = durations.len();
        let total_secs = total_duration.as_secs_f64();
        let throughput_ops_per_sec = if total_secs > 0.0 {
            total_ops as f64 / total_secs
        } else {
            total_ops as f64
        };

        let latencies_ms: Vec<f64> = durations.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
        let sum_latency_ms: f64 = latencies_ms.iter().sum();
        let min_latency_ms = latencies_ms.iter().copied().fold(f64::INFINITY, f64::min);
        let max_latency_ms = latencies_ms
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let avg_latency_ms = sum_latency_ms / total_ops as f64;

        Ok(ScenarioResult {
            scenario,
            surface,
            operations: total_ops,
            total_duration,
            throughput_ops_per_sec,
            avg_latency_ms,
            min_latency_ms,
            max_latency_ms,
            error_count: errors.len(),
        })
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let config = BenchConfig::from_env()?;
    let results = run_suite(&config).await?;
    print_results(&config, &results);
    Ok(())
}

impl BenchConfig {
    fn from_env() -> Result<Self> {
        let mut concurrency = DEFAULT_CONCURRENCY;
        let mut iterations = DEFAULT_ITERATIONS;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--concurrency" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --concurrency"))?;
                    concurrency = value
                        .parse::<usize>()
                        .with_context(|| format!("invalid concurrency value: {value}"))?;
                }
                "--iterations" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("missing value for --iterations"))?;
                    iterations = value
                        .parse::<usize>()
                        .with_context(|| format!("invalid iterations value: {value}"))?;
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => return Err(anyhow!("unknown argument: {other}")),
            }
        }

        if concurrency == 0 {
            return Err(anyhow!("--concurrency must be >= 1"));
        }
        if iterations == 0 {
            return Err(anyhow!("--iterations must be >= 1"));
        }

        Ok(Self {
            concurrency,
            iterations,
        })
    }
}

fn print_usage() {
    println!("Usage: cargo run -p ferrum-perf-baseline -- [--concurrency N] [--iterations M]");
}

async fn run_suite(config: &BenchConfig) -> Result<Vec<ScenarioResult>> {
    let mut results = Vec::with_capacity(4);
    results.push(run_intent_compile(config).await?);
    results.push(run_execution_pipeline(config).await?);
    results.push(run_capability_cycle(config).await?);
    results.push(run_sqlite_contention(config).await?);
    Ok(results)
}

async fn run_intent_compile(config: &BenchConfig) -> Result<ScenarioResult> {
    let fixture = StoreFixture::new("intent_compile").await?;
    let measurements = Arc::new(ScenarioMeasurements::default());
    let started_at = Instant::now();

    run_concurrent(config, {
        let store = fixture.store.clone();
        let measurements = measurements.clone();
        move |worker_idx, iteration_idx| {
            let store = store.clone();
            let measurements = measurements.clone();
            async move {
                let operation_started = Instant::now();
                match intent_compile_operation(&store, worker_idx, iteration_idx).await {
                    Ok(()) => measurements.record_success(operation_started.elapsed()),
                    Err(error) => measurements.record_error(error),
                }
                Ok(())
            }
        }
    })
    .await?;

    Arc::try_unwrap(measurements)
        .map_err(|_| anyhow!("intent-compile measurements still shared"))?
        .finish("intent-compile", "ferrum-store (pooled)", started_at)
}

async fn run_execution_pipeline(config: &BenchConfig) -> Result<ScenarioResult> {
    let fixture = StoreFixture::new("execution_pipeline").await?;
    let measurements = Arc::new(ScenarioMeasurements::default());
    let started_at = Instant::now();

    run_concurrent(config, {
        let store = fixture.store.clone();
        let measurements = measurements.clone();
        move |worker_idx, iteration_idx| {
            let store = store.clone();
            let measurements = measurements.clone();
            async move {
                let operation_started = Instant::now();
                match execution_pipeline_operation(&store, worker_idx, iteration_idx).await {
                    Ok(()) => measurements.record_success(operation_started.elapsed()),
                    Err(error) => measurements.record_error(error),
                }
                Ok(())
            }
        }
    })
    .await?;

    Arc::try_unwrap(measurements)
        .map_err(|_| anyhow!("execution-pipeline measurements still shared"))?
        .finish("execution-pipeline", "ferrum-store (pooled)", started_at)
}

async fn run_capability_cycle(config: &BenchConfig) -> Result<ScenarioResult> {
    let fixture = StoreFixture::new("capability_cycle").await?;
    let measurements = Arc::new(ScenarioMeasurements::default());
    let started_at = Instant::now();

    run_concurrent(config, {
        let store = fixture.store.clone();
        let measurements = measurements.clone();
        move |worker_idx, iteration_idx| {
            let store = store.clone();
            let measurements = measurements.clone();
            async move {
                let operation_started = Instant::now();
                match capability_cycle_operation(&store, worker_idx, iteration_idx).await {
                    Ok(()) => measurements.record_success(operation_started.elapsed()),
                    Err(error) => measurements.record_error(error),
                }
                Ok(())
            }
        }
    })
    .await?;

    Arc::try_unwrap(measurements)
        .map_err(|_| anyhow!("capability-cycle measurements still shared"))?
        .finish("capability-cycle", "ferrum-store (pooled)", started_at)
}

async fn run_sqlite_contention(config: &BenchConfig) -> Result<ScenarioResult> {
    let fixture = AdapterFixture::new("sqlite_contention").await?;
    let measurements = Arc::new(ScenarioMeasurements::default());
    let started_at = Instant::now();

    run_concurrent(config, {
        let database_url = fixture.database_url.clone();
        let measurements = measurements.clone();
        move |worker_idx, iteration_idx| {
            let database_url = database_url.clone();
            let measurements = measurements.clone();
            async move {
                let operation_started = Instant::now();
                match sqlite_contention_operation(&database_url, worker_idx, iteration_idx).await {
                    Ok(()) => measurements.record_success(operation_started.elapsed()),
                    Err(error) => measurements.record_error(error),
                }
                Ok(())
            }
        }
    })
    .await?;

    Arc::try_unwrap(measurements)
        .map_err(|_| anyhow!("sqlite-contention measurements still shared"))?
        .finish(
            "sqlite-contention",
            "ferrum-adapter-sqlite (non-pooled)",
            started_at,
        )
}

async fn run_concurrent<F, Fut>(config: &BenchConfig, worker: F) -> Result<()>
where
    F: Fn(usize, usize) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    let worker = Arc::new(worker);
    let mut join_set = JoinSet::new();
    for worker_idx in 0..config.concurrency {
        let worker = worker.clone();
        let iterations = config.iterations;
        join_set.spawn(async move {
            for iteration_idx in 0..iterations {
                worker(worker_idx, iteration_idx).await?;
            }
            Ok::<(), anyhow::Error>(())
        });
    }

    while let Some(result) = join_set.join_next().await {
        result.context("worker task join failure")??;
    }

    Ok(())
}

async fn intent_compile_operation(
    store: &SqliteStore,
    worker_idx: usize,
    iteration_idx: usize,
) -> Result<()> {
    let intent = sample_intent(worker_idx, iteration_idx);
    store.intents().insert(&intent).await?;

    let proposal = sample_proposal(intent.intent_id, worker_idx, iteration_idx);
    store.proposals().insert(&proposal).await?;
    store
        .intents()
        .update_status(intent.intent_id, IntentStatus::Closed)
        .await?;
    Ok(())
}

async fn execution_pipeline_operation(
    store: &SqliteStore,
    worker_idx: usize,
    iteration_idx: usize,
) -> Result<()> {
    let intent = sample_intent(worker_idx, iteration_idx);
    store.intents().insert(&intent).await?;

    let proposal = sample_proposal(intent.intent_id, worker_idx, iteration_idx);
    store.proposals().insert(&proposal).await?;

    let capability = sample_capability(
        intent.intent_id,
        proposal.proposal_id,
        worker_idx,
        iteration_idx,
    );
    store.capabilities().insert(&capability).await?;

    let execution = sample_execution(
        intent.intent_id,
        proposal.proposal_id,
        capability.capability_id,
        worker_idx,
        iteration_idx,
    );
    store.executions().insert(&execution).await?;

    let rollback = sample_rollback(
        intent.intent_id,
        proposal.proposal_id,
        execution.execution_id,
    );
    store.rollback_contracts().insert(&rollback).await?;
    store
        .executions()
        .update_state(execution.execution_id, ExecutionState::AwaitingVerification)
        .await?;
    Ok(())
}

async fn capability_cycle_operation(
    store: &SqliteStore,
    worker_idx: usize,
    iteration_idx: usize,
) -> Result<()> {
    let intent = sample_intent(worker_idx, iteration_idx);
    store.intents().insert(&intent).await?;

    let proposal = sample_proposal(intent.intent_id, worker_idx, iteration_idx);
    store.proposals().insert(&proposal).await?;

    let capability = sample_capability(
        intent.intent_id,
        proposal.proposal_id,
        worker_idx,
        iteration_idx,
    );
    let capability_id = capability.capability_id;
    store.capabilities().insert(&capability).await?;

    let marked_used = store
        .capabilities()
        .mark_used_if_active(capability_id)
        .await?;
    if !marked_used {
        return Err(anyhow!("capability should transition to used exactly once"));
    }
    Ok(())
}

async fn sqlite_contention_operation(
    database_url: &str,
    worker_idx: usize,
    iteration_idx: usize,
) -> Result<()> {
    let adapter = SqliteRollbackAdapter::new("sqlite");
    let execution_id = ferrum_proto::ExecutionId::new();
    let prepare_request = make_sqlite_prepare_request(database_url, execution_id);
    let prepare_receipt = adapter.prepare(&prepare_request).await?;
    if !prepare_receipt.accepted {
        return Err(anyhow!("sqlite benchmark prepare should be accepted"));
    }

    let contract = make_sqlite_contract(database_url, execution_id);
    let payload = serde_json::json!({
        "table": "bench_items",
        "row_id": format!("worker-{worker_idx}-iteration-{iteration_idx}"),
        "content": format!("payload-{worker_idx}-{iteration_idx}"),
    });

    adapter.execute(&contract, &payload).await?;

    let verify_receipt = adapter.verify(&contract).await?;
    if !verify_receipt.verified {
        return Err(anyhow!("sqlite benchmark verify returned false"));
    }

    adapter.rollback(&contract).await?;
    Ok(())
}

struct StoreFixture {
    _temp_dir: TempDir,
    store: SqliteStore,
}

impl StoreFixture {
    async fn new(prefix: &str) -> Result<Self> {
        let temp_dir = tempfile::tempdir().context("create store temp dir")?;
        let database_url = sqlite_database_url(temp_dir.path(), &format!("{prefix}.sqlite"))?;
        let store = SqliteStore::connect(&database_url).await?;
        store.apply_embedded_migrations().await?;
        Ok(Self {
            _temp_dir: temp_dir,
            store,
        })
    }
}

struct AdapterFixture {
    _temp_dir: TempDir,
    database_url: String,
}

impl AdapterFixture {
    async fn new(prefix: &str) -> Result<Self> {
        let temp_dir = tempfile::tempdir().context("create adapter temp dir")?;
        let database_url = sqlite_database_url(temp_dir.path(), &format!("{prefix}.sqlite"))?;
        Ok(Self {
            _temp_dir: temp_dir,
            database_url,
        })
    }
}

fn sqlite_database_url(base: &Path, file_name: &str) -> Result<String> {
    let db_path: PathBuf = base.join(file_name);
    std::fs::File::create(&db_path)
        .with_context(|| format!("create sqlite file at {}", db_path.display()))?;
    Ok(format!("sqlite://{}", db_path.display()))
}

fn sample_intent(worker_idx: usize, iteration_idx: usize) -> IntentEnvelope {
    IntentEnvelope {
        intent_id: IntentId::new(),
        principal_id: PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: format!("Benchmark intent {worker_idx}-{iteration_idx}"),
        goal: "Establish performance baseline".to_string(),
        normalized_goal: "establish performance baseline".to_string(),
        allowed_outcomes: vec![OutcomeClause {
            id: "primary".to_string(),
            description: "baseline operation".to_string(),
            effect_type: EffectType::ReadOnlyAnalysis,
            required: true,
            selectors: None,
        }],
        forbidden_outcomes: Vec::new(),
        resource_scope: Vec::new(),
        risk_tier: RiskTier::Medium,
        approval_mode: ApprovalMode::None,
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
        tags: vec!["benchmark".to_string()],
        metadata: ferrum_proto::JsonMap::new(),
        status: IntentStatus::Active,
        policy_bundle_fingerprint: None,
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::minutes(15),
    }
}

fn sample_proposal(intent_id: IntentId, worker_idx: usize, iteration_idx: usize) -> ActionProposal {
    ActionProposal {
        proposal_id: ProposalId::new(),
        intent_id,
        step_index: 1,
        title: format!("Benchmark proposal {worker_idx}-{iteration_idx}"),
        tool_name: "benchmark.tool".to_string(),
        server_name: "bench".to_string(),
        raw_arguments: serde_json::json!({"worker": worker_idx, "iteration": iteration_idx}),
        expected_effect: "measure sqlite baseline".to_string(),
        estimated_risk: RiskTier::Low,
        requested_rollback_class: RollbackClass::R0NativeReversible,
        decision: None,
        taint_inputs: Vec::new(),
        metadata: ferrum_proto::JsonMap::new(),
        created_at: Utc::now(),
    }
}

fn sample_capability(
    intent_id: IntentId,
    proposal_id: ProposalId,
    worker_idx: usize,
    iteration_idx: usize,
) -> CapabilityLease {
    let now = Utc::now();
    CapabilityLease {
        capability_id: CapabilityId::new(),
        intent_id,
        proposal_id,
        tool_binding: ToolBinding {
            server_name: "bench".to_string(),
            tool_name: "sqlite.measure".to_string(),
            tool_version: None,
        },
        resource_bindings: vec![ResourceBinding::Sqlite {
            db_path: format!("bench-{worker_idx}-{iteration_idx}.sqlite"),
            tables: vec!["bench_items".to_string()],
            mode: ResourceMode::ReadWrite,
        }],
        argument_constraints: Vec::new(),
        taint_budget: TaintBudget {
            max_taint_score: 20,
            allow_external_tool_output: false,
            allow_external_metadata: false,
            allow_untrusted_text: false,
        },
        approval_binding: None,
        issued_by: "benchmark".to_string(),
        policy_bundle_id: PolicyBundleId::new(),
        tool_manifest_id: None,
        manifest_hash: None,
        status: CapabilityStatus::Active,
        issued_at: now,
        expires_at: now + Duration::minutes(5),
        revoked_at: None,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn sample_execution(
    intent_id: IntentId,
    proposal_id: ProposalId,
    capability_id: CapabilityId,
    worker_idx: usize,
    iteration_idx: usize,
) -> ExecutionRecord {
    let mut metadata = ferrum_proto::JsonMap::new();
    metadata.insert("worker".to_string(), serde_json::json!(worker_idx));
    metadata.insert("iteration".to_string(), serde_json::json!(iteration_idx));

    ExecutionRecord {
        execution_id: ferrum_proto::ExecutionId::new(),
        proposal_id,
        intent_id,
        capability_id,
        rollback_contract_id: None,
        decision: Decision::Allow,
        state: ExecutionState::Authorized,
        started_at: Utc::now(),
        finished_at: None,
        result_digest: None,
        metadata,
    }
}

fn sample_rollback(
    intent_id: IntentId,
    proposal_id: ProposalId,
    execution_id: ferrum_proto::ExecutionId,
) -> RollbackContract {
    RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id,
        proposal_id,
        execution_id,
        action_type: ferrum_proto::ActionType::McpToolMutation,
        rollback_class: RollbackClass::R0NativeReversible,
        adapter_key: "benchmark".to_string(),
        target: RollbackTarget::Generic {
            namespace: "benchmark".to_string(),
            identifier: "execution-pipeline".to_string(),
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: Utc::now(),
        expires_at: None,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn make_sqlite_contract(
    database_url: &str,
    execution_id: ferrum_proto::ExecutionId,
) -> RollbackContract {
    RollbackContract {
        contract_id: ferrum_proto::RollbackContractId::new(),
        intent_id: ferrum_proto::IntentId::new(),
        proposal_id: ferrum_proto::ProposalId::new(),
        execution_id,
        action_type: ferrum_proto::ActionType::SqlMutation,
        rollback_class: RollbackClass::R2Compensatable,
        adapter_key: "sqlite".to_string(),
        target: RollbackTarget::SqliteTxn {
            db_path: database_url.to_string(),
            tx_id: "benchmark-tx".to_string(),
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        state: RollbackState::Prepared,
        created_at: Utc::now(),
        expires_at: None,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn make_sqlite_prepare_request(
    database_url: &str,
    execution_id: ferrum_proto::ExecutionId,
) -> ferrum_proto::RollbackPrepareRequest {
    ferrum_proto::RollbackPrepareRequest {
        intent_id: ferrum_proto::IntentId::new(),
        proposal_id: ferrum_proto::ProposalId::new(),
        execution_id,
        action_type: ferrum_proto::ActionType::SqlMutation,
        rollback_class: RollbackClass::R2Compensatable,
        adapter_key: "sqlite".to_string(),
        target: RollbackTarget::SqliteTxn {
            db_path: database_url.to_string(),
            tx_id: "benchmark-tx".to_string(),
        },
        prepare_checks: vec![],
        verify_checks: vec![],
        compensation_plan: vec![],
        auto_commit: false,
        metadata: ferrum_proto::JsonMap::new(),
    }
}

fn print_results(config: &BenchConfig, results: &[ScenarioResult]) {
    println!("# Ferrum-Gate G-E2 Performance Baseline");
    println!();
    println!("- concurrency: {}", config.concurrency);
    println!("- iterations per worker: {}", config.iterations);
    println!(
        "- total requested operations per scenario: {}",
        config.concurrency * config.iterations
    );
    println!();
    println!(
        "| Scenario | Surface | Operations | Total Seconds | Throughput (ops/sec) | Avg Latency (ms) | Min Latency (ms) | Max Latency (ms) | Errors |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|");
    for result in results {
        println!(
            "| {} | {} | {} | {:.3} | {:.2} | {:.2} | {:.2} | {:.2} | {} |",
            result.scenario,
            result.surface,
            result.operations,
            result.total_duration.as_secs_f64(),
            result.throughput_ops_per_sec,
            result.avg_latency_ms,
            result.min_latency_ms,
            result.max_latency_ms,
            result.error_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn benchmark_suite_smoke_test() {
        let config = BenchConfig {
            concurrency: 2,
            iterations: 2,
        };
        let results = run_suite(&config).await.expect("run benchmark suite");
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|result| result.operations >= 1));
    }
}
