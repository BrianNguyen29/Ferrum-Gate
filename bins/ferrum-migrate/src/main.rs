use anyhow::{Result, bail};

#[cfg(feature = "postgres")]
use anyhow::Context;
use clap::Parser;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{ColumnIndex, Row};
#[cfg(feature = "postgres")]
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
#[cfg(feature = "postgres")]
use std::collections::BTreeSet;

#[cfg(feature = "postgres")]
use sqlx::PgPool;

fn parse_chunk_size(s: &str) -> Result<usize, String> {
    let val: usize = s
        .parse()
        .map_err(|_| format!("`{}` is not a valid chunk size", s))?;
    if val == 0 {
        return Err("chunk-size must be greater than 0".to_string());
    }
    if val > 10_000 {
        return Err("chunk-size must not exceed 10000".to_string());
    }
    Ok(val)
}

#[derive(Debug, Parser)]
#[command(name = "ferrum-migrate")]
#[command(about = "FerrumGate SQLite to PostgreSQL migration tool (P5e.4 streaming)")]
struct Args {
    /// Source SQLite DSN (e.g., sqlite://path/to/db or sqlite::memory:).
    #[arg(long)]
    from: String,

    /// Target PostgreSQL DSN (e.g., postgres://user:pass@localhost:5432/db).
    #[arg(long)]
    to: String,

    /// Apply migration to target. Without this flag, runs in dry-run mode.
    #[arg(long)]
    apply: bool,

    /// Output results as JSON.
    #[arg(long)]
    json: bool,

    /// Number of rows to process per chunk during migration.
    #[arg(long, default_value = "1000", value_parser = parse_chunk_size)]
    chunk_size: usize,

    /// Resume a previous migration using idempotent upsert semantics.
    /// Tables without a stable unique key cannot be resumed safely.
    #[arg(long)]
    resume: bool,
}

/// A single table migration result.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct TableResult {
    table: String,
    source_count: usize,
    target_count: usize,
    migrated_count: usize,
    id_match: bool,
    count_match: bool,
    #[serde(default)]
    hash_match: bool,
    source_content_hash: Option<String>,
    target_content_hash: Option<String>,
    errors: Vec<String>,
}

/// Overall migration report.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct MigrationReport {
    dry_run: bool,
    applied: bool,
    overall_success: bool,
    tables: Vec<TableResult>,
}

/// Migration configuration for a single table.
#[allow(dead_code)]
#[cfg(any(feature = "postgres", test))]
struct TableMigration<'a> {
    name: &'a str,
    /// Columns to select from SQLite (must match INSERT columns).
    select_columns: &'a str,
    /// INSERT statement for PostgreSQL with $N placeholders.
    insert_sql: &'a str,
    /// Column name containing the stable ID, if any.
    id_column: Option<&'a str>,
    /// Number of bind parameters in insert_sql.
    param_count: usize,
}

/// Core governance tables in dependency-safe order.
#[cfg(any(feature = "postgres", test))]
fn table_migrations() -> Vec<TableMigration<'static>> {
    vec![
        TableMigration {
            name: "intents",
            select_columns: "intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode, default_rollback_class, created_at, expires_at, raw_json",
            insert_sql: "INSERT INTO intents (intent_id, principal_id, normalized_goal, status, risk_tier, approval_mode, default_rollback_class, created_at, expires_at, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            id_column: Some("intent_id"),
            param_count: 10,
        },
        TableMigration {
            name: "proposals",
            select_columns: "proposal_id, intent_id, step_index, server_name, tool_name, estimated_risk, requested_rollback_class, created_at, raw_json",
            insert_sql: "INSERT INTO proposals (proposal_id, intent_id, step_index, server_name, tool_name, estimated_risk, requested_rollback_class, created_at, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            id_column: Some("proposal_id"),
            param_count: 9,
        },
        TableMigration {
            name: "capabilities",
            select_columns: "capability_id, intent_id, proposal_id, server_name, tool_name, status, issued_at, expires_at, revoked_at, raw_json",
            insert_sql: "INSERT INTO capabilities (capability_id, intent_id, proposal_id, server_name, tool_name, status, issued_at, expires_at, revoked_at, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            id_column: Some("capability_id"),
            param_count: 10,
        },
        TableMigration {
            name: "executions",
            select_columns: "execution_id, intent_id, proposal_id, capability_id, rollback_contract_id, decision, state, started_at, finished_at, result_digest, raw_json",
            insert_sql: "INSERT INTO executions (execution_id, intent_id, proposal_id, capability_id, rollback_contract_id, decision, state, started_at, finished_at, result_digest, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            id_column: Some("execution_id"),
            param_count: 11,
        },
        TableMigration {
            name: "rollback_contracts",
            select_columns: "contract_id, intent_id, proposal_id, execution_id, adapter_key, action_type, rollback_class, state, auto_commit, created_at, expires_at, raw_json",
            insert_sql: "INSERT INTO rollback_contracts (contract_id, intent_id, proposal_id, execution_id, adapter_key, action_type, rollback_class, state, auto_commit, created_at, expires_at, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            id_column: Some("contract_id"),
            param_count: 12,
        },
        TableMigration {
            name: "approvals",
            select_columns: "approval_id, intent_id, proposal_id, execution_id, action_digest, state, expires_at, created_at, raw_json",
            insert_sql: "INSERT INTO approvals (approval_id, intent_id, proposal_id, execution_id, action_digest, state, expires_at, created_at, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            id_column: Some("approval_id"),
            param_count: 9,
        },
        TableMigration {
            name: "provenance_events",
            select_columns: "event_id, kind, occurred_at, intent_id, proposal_id, execution_id, capability_id, rollback_contract_id, policy_bundle_id, raw_json",
            insert_sql: "INSERT INTO provenance_events (event_id, kind, occurred_at, intent_id, proposal_id, execution_id, capability_id, rollback_contract_id, policy_bundle_id, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            id_column: Some("event_id"),
            param_count: 10,
        },
        TableMigration {
            name: "provenance_edges",
            select_columns: "to_event_id, from_event_id, edge_type, summary",
            insert_sql: "INSERT INTO provenance_edges (to_event_id, from_event_id, edge_type, summary) VALUES ($1, $2, $3, $4)",
            id_column: None,
            param_count: 4,
        },
        TableMigration {
            name: "ledger_entries",
            select_columns: "entry_id, event_id, intent_id, execution_id, occurred_at, content_hash, previous_ledger_hash, raw_json",
            insert_sql: "INSERT INTO ledger_entries (entry_id, event_id, intent_id, execution_id, occurred_at, content_hash, previous_ledger_hash, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            id_column: Some("entry_id"),
            param_count: 8,
        },
        TableMigration {
            name: "policy_bundles",
            select_columns: "bundle_id, version, active, content_hash, created_at, updated_at, raw_json",
            insert_sql: "INSERT INTO policy_bundles (bundle_id, version, active, content_hash, created_at, updated_at, raw_json) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            id_column: Some("bundle_id"),
            param_count: 7,
        },
    ]
}

/// Connect to SQLite source.
#[cfg(feature = "postgres")]
async fn connect_sqlite(dsn: &str) -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(dsn)
        .await
        .with_context(|| format!("failed to connect to SQLite source: {}", dsn))?;
    Ok(pool)
}

#[cfg(feature = "postgres")]
async fn connect_postgres(dsn: &str) -> Result<PgPool> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(dsn)
        .await
        .with_context(|| format!("failed to connect to PostgreSQL target: {}", dsn))?;
    Ok(pool)
}

/// Check whether the target table is empty.
#[cfg(feature = "postgres")]
async fn is_target_empty(pg: &PgPool, table: &str) -> Result<bool> {
    let sql = format!("SELECT COUNT(*) FROM {}", table);
    let count: i64 = sqlx::query_scalar(&sql).fetch_one(pg).await?;
    Ok(count == 0)
}

/// Canonicalize a database row into a deterministic string for hashing.
///
/// Format: `column1=value1;column2=value2;...` ordered by `select_columns`.
/// - NULL values are rendered as `NULL`.
/// - Boolean-like columns (`auto_commit`, `active`) are rendered as `true`/`false`.
/// - Integer columns (`step_index`, `entry_id`) are rendered as decimal strings.
/// - All other columns are rendered as their raw text value.
fn canonical_row<R: Row>(row: &R, select_columns: &str) -> Result<String>
where
    for<'r> &'r str: ColumnIndex<R>,
    Option<i64>: sqlx::Type<R::Database>,
    for<'r> Option<i64>: sqlx::Decode<'r, R::Database>,
    Option<String>: sqlx::Type<R::Database>,
    for<'r> Option<String>: sqlx::Decode<'r, R::Database>,
{
    let mut parts = Vec::new();
    for col_name in select_columns.split(',').map(|s| s.trim()) {
        let val = if col_name == "auto_commit" || col_name == "active" {
            match row.try_get::<Option<i64>, _>(col_name)? {
                Some(v) => (v != 0).to_string(),
                None => "NULL".to_string(),
            }
        } else if col_name == "step_index" || col_name == "entry_id" {
            match row.try_get::<Option<i64>, _>(col_name)? {
                Some(v) => v.to_string(),
                None => "NULL".to_string(),
            }
        } else {
            match row.try_get::<Option<String>, _>(col_name)? {
                Some(v) => v,
                None => "NULL".to_string(),
            }
        };
        parts.push(format!("{}={}", col_name, val));
    }
    Ok(parts.join(";"))
}

/// Aggregate a collection of per-row SHA-256 hashes into a single sorted hash.
fn aggregate_hash(mut hashes: Vec<String>) -> String {
    hashes.sort_unstable();
    let joined = hashes.join("\n");
    format!("{:x}", Sha256::digest(joined.as_bytes()))
}

/// Compute the aggregate content hash for a PostgreSQL target table.
#[cfg(feature = "postgres")]
async fn compute_target_hash(
    pg: &PgPool,
    table: &str,
    select_columns: &str,
    chunk_size: usize,
) -> Result<String> {
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", table))
        .fetch_one(pg)
        .await?;
    let count = count as usize;
    let mut hashes = Vec::new();
    for offset in (0..count).step_by(chunk_size) {
        let sql = format!(
            "SELECT {} FROM {} LIMIT {} OFFSET {}",
            select_columns, table, chunk_size, offset
        );
        let rows = sqlx::query(&sql).fetch_all(pg).await?;
        for row in &rows {
            let canonical = canonical_row(row, select_columns)?;
            let hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));
            hashes.push(hash);
        }
    }
    Ok(aggregate_hash(hashes))
}

/// Build the INSERT (or upsert) SQL for a table.
#[cfg(any(feature = "postgres", test))]
fn build_insert_sql(insert_sql: &str, id_column: Option<&str>, resume: bool) -> Result<String> {
    if !resume {
        return Ok(insert_sql.to_string());
    }
    match id_column {
        Some(id_col) => Ok(format!(
            "{} ON CONFLICT ({}) DO NOTHING",
            insert_sql, id_col
        )),
        None => bail!(
            "Table has no stable ID column and cannot be safely resumed. \
             Run without --resume or add a PRIMARY KEY/UNIQUE constraint."
        ),
    }
}

/// Migrate a single table from SQLite to PostgreSQL.
#[cfg(feature = "postgres")]
async fn migrate_table(
    sqlite: &SqlitePool,
    pg: &PgPool,
    tm: &TableMigration<'_>,
    apply: bool,
    chunk_size: usize,
    resume: bool,
) -> Result<TableResult> {
    let source_count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", tm.name))
        .fetch_one(sqlite)
        .await?;
    let source_count = source_count as usize;

    let mut result = TableResult {
        table: tm.name.to_string(),
        source_count,
        target_count: 0,
        migrated_count: 0,
        id_match: true,
        count_match: true,
        hash_match: true,
        source_content_hash: None,
        target_content_hash: None,
        errors: Vec::new(),
    };

    if apply {
        // Target-empty safety check (skipped when resuming)
        if !resume && !is_target_empty(pg, tm.name).await? {
            bail!(
                "target table '{}' is not empty; P4.4 MVP requires an empty target",
                tm.name
            );
        }

        let sql = build_insert_sql(tm.insert_sql, tm.id_column, resume)?;
        let mut source_ids = BTreeSet::new();
        let mut source_hashes = Vec::new();

        for offset in (0..source_count).step_by(chunk_size) {
            let select_sql = format!(
                "SELECT {} FROM {} LIMIT {} OFFSET {}",
                tm.select_columns, tm.name, chunk_size, offset
            );
            let rows = sqlx::query(&select_sql).fetch_all(sqlite).await?;

            if let Some(id_col) = tm.id_column {
                for row in &rows {
                    if let Ok(id) = row.try_get::<String, _>(id_col) {
                        source_ids.insert(id);
                    }
                }
            }

            // Compute source content hashes for this chunk
            for row in &rows {
                match canonical_row(row, tm.select_columns) {
                    Ok(canonical) => {
                        let hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));
                        source_hashes.push(hash);
                    }
                    Err(e) => {
                        result.errors.push(format!("canonicalization error: {}", e));
                    }
                }
            }

            // Attempt chunk-wide transaction for efficiency
            let mut txn = pg.begin().await?;
            let mut chunk_ok = true;
            let mut chunk_errors = Vec::new();
            for row in &rows {
                let mut query = sqlx::query(&sql);
                for i in 0..tm.param_count {
                    let col_name = tm.select_columns.split(',').nth(i).map(|s| s.trim());
                    let col_name = match col_name {
                        Some(c) => c,
                        None => {
                            chunk_errors
                                .push(format!("column mismatch at parameter index {}", i + 1));
                            chunk_ok = false;
                            continue;
                        }
                    };
                    if col_name == "auto_commit" || col_name == "active" {
                        let val: i64 = row.try_get(col_name)?;
                        query = query.bind(val != 0);
                    } else if col_name == "step_index" || col_name == "entry_id" {
                        let val: i64 = row.try_get(col_name)?;
                        query = query.bind(val);
                    } else {
                        let val: String = row.try_get(col_name)?;
                        query = query.bind(val);
                    }
                }
                if let Err(e) = query.execute(&mut *txn).await {
                    chunk_errors.push(format!("insert error: {}", e));
                    chunk_ok = false;
                }
            }

            if chunk_ok {
                txn.commit().await?;
                result.migrated_count += rows.len();
            } else {
                txn.rollback().await?;
                // Fallback to row-by-row to preserve per-row error semantics
                for row in &rows {
                    let mut query = sqlx::query(&sql);
                    for i in 0..tm.param_count {
                        let col_name = tm.select_columns.split(',').nth(i).map(|s| s.trim());
                        let col_name = match col_name {
                            Some(c) => c,
                            None => {
                                result
                                    .errors
                                    .push(format!("column mismatch at parameter index {}", i + 1));
                                continue;
                            }
                        };
                        if col_name == "auto_commit" || col_name == "active" {
                            let val: i64 = row.try_get(col_name)?;
                            query = query.bind(val != 0);
                        } else if col_name == "step_index" || col_name == "entry_id" {
                            let val: i64 = row.try_get(col_name)?;
                            query = query.bind(val);
                        } else {
                            let val: String = row.try_get(col_name)?;
                            query = query.bind(val);
                        }
                    }
                    if let Err(e) = query.execute(pg).await {
                        result.errors.push(format!("insert error: {}", e));
                    } else {
                        result.migrated_count += 1;
                    }
                }
                result.errors.extend(chunk_errors);
            }
        }

        result.source_content_hash = Some(aggregate_hash(source_hashes));

        // Validate: count
        let count_sql = format!("SELECT COUNT(*) FROM {}", tm.name);
        let target_count: i64 = sqlx::query_scalar(&count_sql).fetch_one(pg).await?;
        result.target_count = target_count as usize;
        result.count_match = result.target_count == result.source_count;

        // Validate: ID set
        if let Some(id_col) = tm.id_column {
            let id_sql = format!("SELECT {} FROM {}", id_col, tm.name);
            let target_rows = sqlx::query(&id_sql).fetch_all(pg).await?;
            let target_ids: BTreeSet<String> = target_rows
                .iter()
                .filter_map(|row| row.try_get::<String, _>(id_col).ok())
                .collect();
            result.id_match = source_ids == target_ids;
        }

        // Validate: content hash
        match compute_target_hash(pg, tm.name, tm.select_columns, chunk_size).await {
            Ok(target_hash) => {
                result.target_content_hash = Some(target_hash.clone());
                result.hash_match = result.source_content_hash.as_ref().unwrap() == &target_hash;
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("target hash computation error: {}", e));
                result.hash_match = false;
            }
        }
    } else {
        // Dry-run: just report what would happen
        result.target_count = 0;
        result.migrated_count = 0;
    }

    Ok(result)
}

/// Print report in human-readable format.
fn print_human(report: &MigrationReport) {
    if report.dry_run {
        println!("=== FerrumGate P4.4 Migration (DRY-RUN) ===");
        println!();
        println!("No data was written. Use --apply to execute migration.");
    } else if report.applied {
        println!("=== FerrumGate P4.4 Migration (APPLIED) ===");
    } else {
        println!("=== FerrumGate P4.4 Migration ===");
    }
    println!();

    let mut all_ok = true;
    for tr in &report.tables {
        let status = if tr.count_match && tr.id_match && tr.hash_match && tr.errors.is_empty() {
            "OK"
        } else {
            all_ok = false;
            "MISMATCH"
        };
        println!(
            "{:20} source={:4} target={:4} migrated={:4} ids={:5} hash={:5} [{}]",
            tr.table,
            tr.source_count,
            tr.target_count,
            tr.migrated_count,
            if tr.id_match { "match" } else { "diff" },
            if tr.hash_match { "match" } else { "diff" },
            status
        );
        for err in &tr.errors {
            println!("  ERROR: {}", err);
        }
    }
    println!();
    if report.dry_run {
        println!("Dry-run complete. Review the plan above before using --apply.");
    } else if all_ok {
        println!("Migration completed successfully.");
    } else {
        println!("Migration completed with validation issues. Review errors above.");
    }
}

/// Print report as JSON.
fn print_json(report: &MigrationReport) {
    println!("{}", serde_json::to_string_pretty(report).unwrap());
}

#[cfg(feature = "postgres")]
async fn run_migration(args: &Args) -> Result<MigrationReport> {
    let sqlite = connect_sqlite(&args.from).await?;
    let pg = connect_postgres(&args.to).await?;

    // Apply target schema migrations before migration
    if args.apply {
        let pg_store = ferrum_store::postgres::PostgresStore::connect(&args.to).await?;
        pg_store.apply_embedded_migrations().await?;
    }

    let migrations = table_migrations();
    let mut tables = Vec::with_capacity(migrations.len());
    let mut overall_success = true;

    for tm in &migrations {
        match migrate_table(&sqlite, &pg, tm, args.apply, args.chunk_size, args.resume).await {
            Ok(tr) => {
                if !tr.count_match || !tr.id_match || !tr.errors.is_empty() {
                    overall_success = false;
                }
                tables.push(tr);
            }
            Err(e) => {
                overall_success = false;
                tables.push(TableResult {
                    table: tm.name.to_string(),
                    source_count: 0,
                    target_count: 0,
                    migrated_count: 0,
                    id_match: false,
                    count_match: false,
                    hash_match: false,
                    source_content_hash: None,
                    target_content_hash: None,
                    errors: vec![e.to_string()],
                });
            }
        }
    }

    Ok(MigrationReport {
        dry_run: !args.apply,
        applied: args.apply,
        overall_success,
        tables,
    })
}

#[cfg(not(feature = "postgres"))]
async fn run_migration(_args: &Args) -> Result<MigrationReport> {
    bail!("PostgreSQL support is not enabled. Build with --features postgres to enable migration.");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let report = run_migration(&args).await?;

    if args.json {
        print_json(&report);
    } else {
        print_human(&report);
    }

    if !report.overall_success && args.apply {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_defaults() {
        let args = Args::parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite::memory:",
            "--to",
            "postgres://localhost/db",
        ]);
        assert_eq!(args.from, "sqlite::memory:");
        assert_eq!(args.to, "postgres://localhost/db");
        assert!(!args.apply);
        assert!(!args.json);
        assert_eq!(args.chunk_size, 1000);
    }

    #[test]
    fn test_cli_chunk_size_custom() {
        let args = Args::parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite::memory:",
            "--to",
            "postgres://localhost/db",
            "--chunk-size",
            "5000",
        ]);
        assert_eq!(args.chunk_size, 5000);
    }

    #[test]
    fn test_cli_chunk_size_max_enforced() {
        let result = Args::try_parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite::memory:",
            "--to",
            "postgres://localhost/db",
            "--chunk-size",
            "10001",
        ]);
        assert!(result.is_err(), "chunk-size above 10000 should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("10000"),
            "error should mention max 10000: {}",
            err
        );
    }

    #[test]
    fn test_cli_chunk_size_zero_rejected() {
        let result = Args::try_parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite::memory:",
            "--to",
            "postgres://localhost/db",
            "--chunk-size",
            "0",
        ]);
        assert!(result.is_err(), "chunk-size of 0 should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("greater") || err.contains("0"),
            "error should mention invalid zero: {}",
            err
        );
    }

    #[test]
    fn test_cli_parsing_with_flags() {
        let args = Args::parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite://dev.db",
            "--to",
            "postgres://u:p@host/db",
            "--apply",
            "--json",
        ]);
        assert_eq!(args.from, "sqlite://dev.db");
        assert_eq!(args.to, "postgres://u:p@host/db");
        assert!(args.apply);
        assert!(args.json);
        assert!(!args.resume);
    }

    #[test]
    fn test_cli_resume_flag_parsing() {
        let args = Args::parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite::memory:",
            "--to",
            "postgres://localhost/db",
            "--resume",
        ]);
        assert!(args.resume);
        assert!(!args.apply);
    }

    #[test]
    fn test_cli_resume_with_apply() {
        let args = Args::parse_from([
            "ferrum-migrate",
            "--from",
            "sqlite::memory:",
            "--to",
            "postgres://localhost/db",
            "--apply",
            "--resume",
        ]);
        assert!(args.apply);
        assert!(args.resume);
    }

    #[test]
    fn test_build_insert_sql_non_resume() {
        let sql = build_insert_sql("INSERT INTO t (a) VALUES ($1)", Some("a"), false).unwrap();
        assert_eq!(sql, "INSERT INTO t (a) VALUES ($1)");
    }

    #[test]
    fn test_build_insert_sql_resume_with_id() {
        let sql = build_insert_sql("INSERT INTO t (a) VALUES ($1)", Some("a"), true).unwrap();
        assert_eq!(
            sql,
            "INSERT INTO t (a) VALUES ($1) ON CONFLICT (a) DO NOTHING"
        );
    }

    #[test]
    fn test_build_insert_sql_resume_without_id_fails() {
        let result = build_insert_sql("INSERT INTO t (a) VALUES ($1)", None, true);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no stable ID column"),
            "error should explain missing ID: {}",
            err
        );
    }

    #[test]
    fn test_table_migrations_have_conflict_target_except_edges() {
        for tm in table_migrations() {
            if tm.name == "provenance_edges" {
                assert!(
                    tm.id_column.is_none(),
                    "provenance_edges is expected to lack a safe conflict target"
                );
            } else {
                assert!(
                    tm.id_column.is_some(),
                    "table '{}' must have an id_column to support resume",
                    tm.name
                );
            }
        }
    }

    #[test]
    fn test_migration_report_serialization() {
        let report = MigrationReport {
            dry_run: true,
            applied: false,
            overall_success: true,
            tables: vec![TableResult {
                table: "intents".to_string(),
                source_count: 3,
                target_count: 0,
                migrated_count: 0,
                id_match: true,
                count_match: true,
                hash_match: true,
                source_content_hash: None,
                target_content_hash: None,
                errors: vec![],
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"dry_run\":true"));
        assert!(json.contains("\"table\":\"intents\""));
        assert!(json.contains("\"source_count\":3"));
    }

    #[test]
    fn test_table_migrations_order_has_intents_first() {
        let migs = table_migrations();
        assert_eq!(migs.first().unwrap().name, "intents");
    }

    #[test]
    fn test_table_migrations_includes_all_core_tables() {
        let names: Vec<_> = table_migrations().iter().map(|t| t.name).collect();
        assert!(names.contains(&"intents"));
        assert!(names.contains(&"proposals"));
        assert!(names.contains(&"capabilities"));
        assert!(names.contains(&"executions"));
        assert!(names.contains(&"rollback_contracts"));
        assert!(names.contains(&"approvals"));
        assert!(names.contains(&"provenance_events"));
        assert!(names.contains(&"provenance_edges"));
        assert!(names.contains(&"ledger_entries"));
        assert!(names.contains(&"policy_bundles"));
    }

    #[test]
    fn test_print_human_dry_run_header() {
        let report = MigrationReport {
            dry_run: true,
            applied: false,
            overall_success: true,
            tables: vec![],
        };
        // Just verify it doesn't panic
        print_human(&report);
    }

    #[test]
    fn test_print_human_applied_header() {
        let report = MigrationReport {
            dry_run: false,
            applied: true,
            overall_success: false,
            tables: vec![TableResult {
                table: "intents".to_string(),
                source_count: 1,
                target_count: 0,
                migrated_count: 0,
                id_match: false,
                count_match: false,
                hash_match: false,
                source_content_hash: None,
                target_content_hash: None,
                errors: vec!["demo error".to_string()],
            }],
        };
        print_human(&report);
    }

    #[test]
    fn test_print_json_roundtrip() {
        let report = MigrationReport {
            dry_run: false,
            applied: true,
            overall_success: true,
            tables: vec![TableResult {
                table: "proposals".to_string(),
                source_count: 2,
                target_count: 2,
                migrated_count: 2,
                id_match: true,
                count_match: true,
                hash_match: true,
                source_content_hash: None,
                target_content_hash: None,
                errors: vec![],
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: MigrationReport = serde_json::from_str(&json).unwrap();
        assert!(parsed.overall_success);
        assert_eq!(parsed.tables[0].source_count, 2);
    }

    #[tokio::test]
    async fn test_canonical_row_determinism_and_format() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE t (auto_commit INTEGER, step_index INTEGER, name TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (auto_commit, step_index, name) VALUES (1, 42, 'hello')")
            .execute(&pool)
            .await
            .unwrap();
        let row = sqlx::query("SELECT auto_commit, step_index, name FROM t")
            .fetch_one(&pool)
            .await
            .unwrap();
        let canonical1 = canonical_row(&row, "auto_commit, step_index, name").unwrap();
        let canonical2 = canonical_row(&row, "auto_commit, step_index, name").unwrap();
        assert_eq!(canonical1, canonical2);
        assert_eq!(canonical1, "auto_commit=true;step_index=42;name=hello");
    }

    #[tokio::test]
    async fn test_canonical_row_null_handling() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE t (auto_commit INTEGER, step_index INTEGER, name TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (auto_commit, step_index, name) VALUES (NULL, NULL, NULL)")
            .execute(&pool)
            .await
            .unwrap();
        let row = sqlx::query("SELECT auto_commit, step_index, name FROM t")
            .fetch_one(&pool)
            .await
            .unwrap();
        let canonical = canonical_row(&row, "auto_commit, step_index, name").unwrap();
        assert_eq!(canonical, "auto_commit=NULL;step_index=NULL;name=NULL");
    }

    #[test]
    fn test_aggregate_hash_determinism_and_order_independence() {
        let h1 = "aaa".to_string();
        let h2 = "bbb".to_string();
        let agg1 = aggregate_hash(vec![h1.clone(), h2.clone()]);
        let agg2 = aggregate_hash(vec![h2, h1]);
        assert_eq!(agg1, agg2, "aggregate hash must be order-independent");
    }

    #[tokio::test]
    async fn test_canonical_row_different_rows_different_hashes() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE t (auto_commit INTEGER, step_index INTEGER, name TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (auto_commit, step_index, name) VALUES (1, 1, 'a')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t (auto_commit, step_index, name) VALUES (0, 1, 'a')")
            .execute(&pool)
            .await
            .unwrap();
        let rows = sqlx::query("SELECT auto_commit, step_index, name FROM t ORDER BY rowid")
            .fetch_all(&pool)
            .await
            .unwrap();
        let c1 = canonical_row(&rows[0], "auto_commit, step_index, name").unwrap();
        let c2 = canonical_row(&rows[1], "auto_commit, step_index, name").unwrap();
        assert_ne!(
            c1, c2,
            "different rows must produce different canonical strings"
        );
    }

    /// Integration test: migrate a real SQLite database to PostgreSQL.
    /// Skips if PostgreSQL is not reachable.
    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_migrate_intents_integration() {
        use ferrum_store::IntentRepo;

        let pg_dsn = std::env::var("FERRUM_MIGRATE_TEST_PG_DSN").unwrap_or_else(|_| {
            "postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test"
                .to_string()
        });

        let pg = match connect_postgres(&pg_dsn).await {
            Ok(p) => p,
            Err(_) => {
                eprintln!("Skipping integration test: PostgreSQL not reachable");
                return;
            }
        };

        // Set up target schema
        let pg_store = ferrum_store::postgres::PostgresStore::connect(&pg_dsn)
            .await
            .unwrap();
        pg_store.apply_embedded_migrations().await.unwrap();

        // Clear target intents table for idempotency
        sqlx::query("DELETE FROM intents")
            .execute(&pg)
            .await
            .unwrap();

        // Set up source SQLite with schema and one intent
        let sqlite_store = ferrum_store::SqliteStore::connect("sqlite::memory:")
            .await
            .unwrap();
        sqlite_store.apply_embedded_migrations().await.unwrap();

        let intent_id = ferrum_proto::IntentId::new();
        let principal_id = ferrum_proto::PrincipalId::new();
        let intent = ferrum_proto::IntentEnvelope {
            intent_id,
            principal_id,
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30000,
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
            status: ferrum_proto::IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };
        sqlite_store.intents().insert(&intent).await.unwrap();

        // Migrate intents table
        let tm = table_migrations()
            .into_iter()
            .find(|t| t.name == "intents")
            .unwrap();
        let result = migrate_table(sqlite_store.pool(), &pg, &tm, true, 1, false)
            .await
            .unwrap();

        assert_eq!(result.source_count, 1, "expected 1 source intent");
        assert_eq!(result.target_count, 1, "expected 1 target intent");
        assert_eq!(result.migrated_count, 1, "expected 1 migrated intent");
        assert!(result.count_match, "count should match");
        assert!(result.id_match, "ids should match");
        assert!(result.errors.is_empty(), "expected no errors");

        // Verify the intent is readable via Postgres repo
        let pg_store = ferrum_store::postgres::PostgresStore::connect(&pg_dsn)
            .await
            .unwrap();
        let fetched = pg_store.intents().get(intent_id).await.unwrap();
        assert!(fetched.is_some(), "intent should be readable from postgres");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.intent_id, intent_id);
        assert_eq!(fetched.normalized_goal, "test goal");
    }
}
