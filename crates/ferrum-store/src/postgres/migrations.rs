pub const INIT_MIGRATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/migrations/postgres/001_initial.sql"
));

/// Current schema version for the PostgreSQL embedded migration.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;
