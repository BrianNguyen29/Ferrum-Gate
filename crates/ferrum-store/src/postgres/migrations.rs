//! PostgreSQL embedded migration parity.
//!
//! `INIT_MIGRATION` is the single source of truth for the PostgreSQL schema.
//! `CURRENT_SCHEMA_VERSION` remains **1** because the initial migration already
//! includes every table required by the P3 repos (intents, executions, proposals,
//! capabilities, rollback_contracts, approvals, provenance, ledger, policy_bundles).
//!
//! # SQLite-only migrations intentionally skipped
//!
//! The SQLite migration sequence contains extra files that are **not** ported to
//! PostgreSQL:
//!
//! | SQLite file | Table | PG status |
//! |-------------|-------|-----------|
//! | `002_add_leader_tips.sql` | `leader_tips` | **Skipped** — sync-only, SQLite-specific. |
//! | `003_add_sync_state.sql` | `sync_state` | **Skipped** — sync-only, SQLite-specific. |
//! | `004_add_leader_allowlist.sql` | `leader_allowlist` | **Skipped** — sync-only, SQLite-specific. |
//! | `005_add_policy_bundles.sql` | `policy_bundles` | **Already present** in `001_initial.sql`. |
//!
//! There is no need for incremental PG migration files until a *new* schema change
//! is introduced that affects PostgreSQL. At that point the versioned runner in
//! [`PostgresStore::apply_embedded_migrations`](super::PostgresStore::apply_embedded_migrations)
//! can be extended to apply `002_*.sql` (or a combined bump) and increment this
//! constant.

pub const INIT_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/postgres/001_initial.sql"
));

/// Current schema version for the PostgreSQL embedded migration.
///
/// Remains `1` until a PostgreSQL-specific schema change requires a new
/// migration file. See module-level docs for parity details.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;
