#![cfg(feature = "postgres")]

//! PostgreSQL sustained-insert benchmark for P4.3 validation.
//!
//! **Local Docker PostgreSQL only — NOT a production benchmark.**
//! This test measures insert throughput against a local Docker PostgreSQL
//! instance and gates the documented >= 1000 writes/s target.
//!
//! When the database is unreachable the test skips gracefully.

use ferrum_proto::{IntentEnvelope, IntentId, IntentStatus};
use ferrum_store::postgres::PostgresStore;
use std::time::{Duration, Instant};

const TEST_DSN: &str =
    "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test";

static PG_TEST_MUTEX: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

fn pg_lock() -> &'static tokio::sync::Mutex<()> {
    PG_TEST_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn make_test_intent(intent_id: IntentId, status: IntentStatus) -> IntentEnvelope {
    let now = chrono::Utc::now();
    IntentEnvelope {
        intent_id,
        principal_id: ferrum_proto::PrincipalId::new(),
        session_id: None,
        channel_id: None,
        title: "bench intent".to_string(),
        goal: "benchmark goal".to_string(),
        normalized_goal: "benchmark goal".to_string(),
        allowed_outcomes: vec![],
        forbidden_outcomes: vec![],
        resource_scope: vec![],
        risk_tier: ferrum_proto::RiskTier::Low,
        approval_mode: ferrum_proto::ApprovalMode::None,
        default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
        time_budget: ferrum_proto::TimeBudget {
            max_duration_ms: 30_000,
            max_steps: 8,
            max_retries_per_step: 1,
        },
        trust_context: ferrum_proto::TrustContextSummary {
            input_labels: vec![],
            sensitivity_labels: vec![],
            taint_score: 0,
            contains_external_metadata: false,
            contains_tool_output: false,
            contains_untrusted_text: false,
        },
        derived_from_event_ids: vec![],
        tags: vec![],
        metadata: ferrum_proto::JsonMap::new(),
        status,
        created_at: now,
        expires_at: now + chrono::Duration::minutes(15),
    }
}

/// Attempt to connect to the local Postgres and bootstrap the schema.
/// Returns `None` if the database is unreachable so tests can skip.
async fn setup() -> Option<(PostgresStore, tokio::sync::MutexGuard<'static, ()>)> {
    let guard = pg_lock().lock().await;

    let store = match PostgresStore::connect(TEST_DSN).await {
        Ok(s) => s,
        Err(_) => return None,
    };

    // Clean slate for benchmark
    let _ = sqlx::query("DROP TABLE IF EXISTS executions CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS proposals CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS capabilities CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS rollback_contracts CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS approvals CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS provenance_edges CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS provenance_events CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS ledger_entries CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS policy_bundles CASCADE")
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DROP TABLE IF EXISTS intents CASCADE")
        .execute(store.pool())
        .await;

    if let Err(e) = store.apply_embedded_migrations().await {
        eprintln!("apply_embedded_migrations failed: {}", e);
        return None;
    }

    Some((store, guard))
}

#[tokio::test]
async fn postgres_sustained_insert_throughput() {
    if cfg!(debug_assertions) {
        eprintln!(
            "Skipping postgres bench test: run in release mode for meaningful throughput measurement"
        );
        return;
    }

    let (store, _guard) = match setup().await {
        Some(s) => s,
        None => {
            eprintln!("Skipping postgres bench test: database not reachable");
            return;
        }
    };

    let repo = store.intents();

    // Batch size chosen to amortize round-trip cost while staying well
    // within PostgreSQL parameter limits (500 rows * 10 cols = 5000 params).
    const BATCH_SIZE: usize = 500;
    const BENCH_DURATION: Duration = Duration::from_secs(3);

    let start = Instant::now();
    let mut count: usize = 0;

    while start.elapsed() < BENCH_DURATION {
        let batch: Vec<IntentEnvelope> = (0..BATCH_SIZE)
            .map(|_| make_test_intent(IntentId::new(), IntentStatus::Active))
            .collect();
        repo.insert_many(&batch)
            .await
            .expect("batch insert should succeed");
        count += BATCH_SIZE;
    }

    let elapsed = start.elapsed();
    let writes_per_sec = count as f64 / elapsed.as_secs_f64();

    println!(
        "postgres_bench: inserted {} intents in {:?} => {:.1} writes/s",
        count, elapsed, writes_per_sec
    );

    assert!(
        writes_per_sec >= 1000.0,
        "sustained insert throughput {:.1} writes/s is below target 1000 writes/s",
        writes_per_sec
    );
}
