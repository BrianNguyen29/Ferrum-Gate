//! PostgreSQL embedded migration parity.
//!
//! `MIGRATIONS` is the ordered list of forward-only schema changes.
//! `CURRENT_SCHEMA_VERSION` is **2** after adding `policy_bundle_version`.
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
//! All DDL uses `IF NOT EXISTS`, making it safe to re-run when bumping the
//! schema version.

/// A single forward-only embedded migration.
pub struct EmbeddedMigration {
    /// Monotonically increasing version number.
    pub version: i64,
    /// Human-readable name for diagnostics.
    #[allow(dead_code)]
    pub name: &'static str,
    /// SQL to execute. Must be idempotent where possible.
    pub sql: &'static str,
}

/// Ordered list of PostgreSQL forward-only migrations.
///
/// The runner applies every migration whose `version` is greater than the
/// current value stored in `_schema_version`.
pub const MIGRATIONS: &[EmbeddedMigration] = &[
    EmbeddedMigration {
        version: 1,
        name: "001_initial",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/postgres/001_initial.sql"
        )),
    },
    EmbeddedMigration {
        version: 2,
        name: "002_add_policy_bundle_versions",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/postgres/002_add_policy_bundle_versions.sql"
        )),
    },
    EmbeddedMigration {
        version: 3,
        name: "003_add_audit_log_hash_chain",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/postgres/003_add_audit_log_hash_chain.sql"
        )),
    },
    EmbeddedMigration {
        version: 4,
        name: "004_add_agent_registry",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/postgres/004_add_agent_registry.sql"
        )),
    },
    EmbeddedMigration {
        version: 5,
        name: "005_add_audit_merkle_roots",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/postgres/005_add_audit_merkle_roots.sql"
        )),
    },
    EmbeddedMigration {
        version: 6,
        name: "006_add_audit_checkpoints",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/postgres/006_add_audit_checkpoints.sql"
        )),
    },
];

/// Current schema version for the PostgreSQL embedded migration.
///
/// Must match the highest `version` in [`MIGRATIONS`].
#[allow(dead_code)]
pub const CURRENT_SCHEMA_VERSION: i64 = 6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_sorted_and_non_empty() {
        assert!(!MIGRATIONS.is_empty(), "MIGRATIONS must not be empty");
        for window in MIGRATIONS.windows(2) {
            assert!(
                window[0].version < window[1].version,
                "MIGRATIONS must be strictly ascending: {} followed by {}",
                window[0].version,
                window[1].version
            );
        }
    }

    #[test]
    fn current_schema_version_matches_last_migration() {
        let last = MIGRATIONS.last().expect("MIGRATIONS is non-empty");
        assert_eq!(
            CURRENT_SCHEMA_VERSION, last.version,
            "CURRENT_SCHEMA_VERSION must match the last migration version"
        );
    }

    #[test]
    fn migration_versions_are_unique() {
        let mut versions: Vec<i64> = MIGRATIONS.iter().map(|m| m.version).collect();
        let original_len = versions.len();
        versions.sort_unstable();
        versions.dedup();
        assert_eq!(
            versions.len(),
            original_len,
            "Migration versions must be unique"
        );
    }
}
