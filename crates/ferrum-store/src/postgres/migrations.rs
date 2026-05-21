//! PostgreSQL embedded migration parity.
//!
//! `INIT_MIGRATION` is the single source of truth for the PostgreSQL schema.
//! `CURRENT_SCHEMA_VERSION` is **2** after adding the `audit_log` table.
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
//! All DDL in `001_initial.sql` uses `IF NOT EXISTS`, making it safe to re-run
//! when bumping the schema version.

pub const INIT_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/postgres/001_initial.sql"
));

/// Current schema version for the PostgreSQL embedded migration.
pub const CURRENT_SCHEMA_VERSION: i64 = 2;
