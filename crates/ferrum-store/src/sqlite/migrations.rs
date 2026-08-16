//! SQLite embedded forward-only migrations.
//!
//! `MIGRATIONS` is the ordered list of schema changes. The runner applies every
//! migration whose `version` is greater than the value recorded in
//! `_schema_version`, so existing databases are upgraded without replaying
//! historical `ALTER` statements.
//!
//! `CURRENT_SCHEMA_VERSION` must match the highest `version` in [`MIGRATIONS`].

/// A single forward-only embedded migration.
pub struct EmbeddedMigration {
    /// Monotonically increasing version number.
    pub version: i64,
    /// Human-readable name for diagnostics.
    #[allow(dead_code)]
    pub name: &'static str,
    /// SQL to execute. Must be safe to run once at the target version.
    pub sql: &'static str,
}

/// Ordered list of SQLite forward-only migrations.
pub const MIGRATIONS: &[EmbeddedMigration] = &[
    EmbeddedMigration {
        version: 1,
        name: "001_initial",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/001_initial.sql"
        )),
    },
    EmbeddedMigration {
        version: 2,
        name: "002_add_leader_tips",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/002_add_leader_tips.sql"
        )),
    },
    EmbeddedMigration {
        version: 3,
        name: "003_add_sync_state",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/003_add_sync_state.sql"
        )),
    },
    EmbeddedMigration {
        version: 4,
        name: "004_add_leader_allowlist",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/004_add_leader_allowlist.sql"
        )),
    },
    EmbeddedMigration {
        version: 5,
        name: "005_add_policy_bundles",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/005_add_policy_bundles.sql"
        )),
    },
    EmbeddedMigration {
        version: 6,
        name: "006_add_policy_bundle_versions",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/006_add_policy_bundle_versions.sql"
        )),
    },
    EmbeddedMigration {
        version: 7,
        name: "007_add_tokens",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/007_add_tokens.sql"
        )),
    },
    EmbeddedMigration {
        version: 8,
        name: "008_add_audit_log",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/008_add_audit_log.sql"
        )),
    },
    EmbeddedMigration {
        version: 9,
        name: "009_add_audit_log_hash_chain",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/009_add_audit_log_hash_chain.sql"
        )),
    },
    EmbeddedMigration {
        version: 10,
        name: "010_add_agent_registry",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/010_add_agent_registry.sql"
        )),
    },
    EmbeddedMigration {
        version: 11,
        name: "011_add_audit_merkle_roots",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/011_add_audit_merkle_roots.sql"
        )),
    },
    EmbeddedMigration {
        version: 12,
        name: "012_add_audit_checkpoints",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/012_add_audit_checkpoints.sql"
        )),
    },
    EmbeddedMigration {
        version: 13,
        name: "013_add_lifecycle_outbox",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/013_add_lifecycle_outbox.sql"
        )),
    },
    EmbeddedMigration {
        version: 14,
        name: "014_add_lifecycle_outbox_fencing",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/014_add_lifecycle_outbox_fencing.sql"
        )),
    },
    EmbeddedMigration {
        version: 15,
        name: "015_add_mfa_credentials",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/015_add_mfa_credentials.sql"
        )),
    },
    EmbeddedMigration {
        version: 16,
        name: "016_add_mfa_credentials_active_index",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/016_add_mfa_credentials_active_index.sql"
        )),
    },
    EmbeddedMigration {
        version: 17,
        name: "017_add_mfa_lockout_columns",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/017_add_mfa_lockout_columns.sql"
        )),
    },
    EmbeddedMigration {
        version: 18,
        name: "018_add_quarantine_holds",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/018_add_quarantine_holds.sql"
        )),
    },
    EmbeddedMigration {
        version: 19,
        name: "019_add_mfa_agent_lockouts",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/019_add_mfa_agent_lockouts.sql"
        )),
    },
    EmbeddedMigration {
        version: 20,
        name: "020_add_owner_actor_id",
        sql: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/migrations/020_add_owner_actor_id.sql"
        )),
    },
];

/// Current schema version for the SQLite embedded migration.
///
/// Must match the highest `version` in [`MIGRATIONS`].
pub const CURRENT_SCHEMA_VERSION: i64 = 20;

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
