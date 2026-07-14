use super::*;
use ferrum_gateway::NonceCacheBackend;
use std::fs;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn clear_test_env() {
    for key in [
        "FERRUMD_CONFIG",
        "FERRUMD_BIND_ADDR",
        "FERRUMD_STORE_DSN",
        "FERRUMD_AUTH_MODE",
        "FERRUMD_BEARER_TOKEN",
        "FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND",
        "FERRUMD_LOG_FILTER",
        "FERRUMD_RATE_LIMIT_PER_SECOND",
        "FERRUMD_RATE_LIMIT_BURST",
        "FERRUMD_TRUSTED_PROXY_CIDRS",
        "FERRUMD_PRE_AUTH_RATE_LIMIT_PER_SECOND",
        "FERRUMD_PRE_AUTH_RATE_LIMIT_BURST",
        "FERRUMD_LOG_FORMAT",
        "FERRUMD_WRITE_QUEUE_THRESHOLD",
        "FERRUMD_PG_MAX_CONNECTIONS",
        "FERRUMD_PG_MIN_IDLE",
        "FERRUMD_PG_ACQUIRE_TIMEOUT_SECS",
        "FERRUMD_PG_STATEMENT_TIMEOUT_MS",
        "FERRUMD_PG_IDLE_IN_TRANSACTION_TIMEOUT_MS",
        "FERRUMD_FS_WORKDIR",
        "FERRUMD_GIT_REPO_ROOTS",
        "FERRUMD_SQLITE_DB_ROOTS",
        "FERRUMD_OIDC_ISSUER",
        "FERRUMD_OIDC_AUDIENCES",
        "FERRUMD_OIDC_JWKS_URL",
        "FERRUMD_OIDC_JWKS_CACHE_TTL_SECS",
        "FERRUMD_OIDC_ACTOR_ID_CLAIM",
        "FERRUMD_OIDC_ROLE_SOURCE_CLAIM",
        "FERRUMD_OIDC_REQUIRE_EMAIL_VERIFIED",
        "FERRUMD_OIDC_ALLOWED_ALGORITHMS",
        "FERRUMD_OIDC_ROLE_MAPPINGS",
        "FERRUMD_OIDC_TOKEN_PROFILE",
        "FERRUMD_LIFECYCLE_RECONCILIATION_ENABLED",
        "FERRUMD_LIFECYCLE_RECONCILIATION_INTERVAL_SECS",
        "FERRUMD_LIFECYCLE_RECONCILIATION_BATCH_LIMIT",
        "FERRUMD_APPROVAL_TIMEOUT_ENABLED",
        "FERRUMD_APPROVAL_TIMEOUT_SECONDS",
        "FERRUMD_APPROVAL_RECONCILIATION_INTERVAL_SECS",
        "FERRUMD_HA_RECONCILER_ENABLED",
        "FERRUMD_HA_RECONCILER_INTERVAL_SECS",
        "FERRUMD_HA_RECONCILER_STALE_THRESHOLD_SECS",
        "FERRUMD_HA_RECONCILER_BATCH_SIZE",
        "FERRUMD_AUDIT_FAIL_CLOSED",
        "FERRUMD_APPROVAL_MFA_REQUIRED",
        "FERRUMD_PDP_MODE",
        "FERRUMD_MFA_SECRET_KEY",
        "FERRUMD_MFA_TOTP_ISSUER",
        "FERRUMD_MFA_LOCKOUT_MAX_ATTEMPTS",
        "FERRUMD_MFA_LOCKOUT_DURATION_SECS",
        "FERRUMD_NONCE_CACHE_BACKEND",
        "FERRUMD_NONCE_CACHE_TTL_SECS",
        "FERRUMD_NONCE_CACHE_MAX_ENTRIES",
        "FERRUMD_BEHAVIORAL_ANOMALY_ENABLED",
        "FERRUMD_BEHAVIORAL_ANOMALY_WINDOW_SECS",
        "FERRUMD_BEHAVIORAL_ANOMALY_WARNING_THRESHOLD",
        "FERRUMD_BEHAVIORAL_ANOMALY_CRITICAL_THRESHOLD",
        "FERRUMD_BEHAVIORAL_ANOMALY_MAX_ACTORS",
        "FERRUMD_AUDIT_WORM_SINK_ENABLED",
        "FERRUMD_AUDIT_WORM_SINK_BUCKET",
        "FERRUMD_AUDIT_WORM_SINK_PREFIX",
        "FERRUMD_AUDIT_WORM_SINK_OBJECT_LOCK_MODE",
        "FERRUMD_AUDIT_WORM_SINK_RETENTION_DAYS",
        "FERRUMD_AUDIT_WORM_SINK_LEGAL_HOLD",
        "FERRUMD_AUDIT_WORM_SINK_EXPORT_INTERVAL_SECS",
        "FERRUMD_AUDIT_WORM_SINK_BATCH_LIMIT",
        "FERRUMD_AUDIT_WORM_SINK_LIVE",
        "FERRUMD_AUDIT_WORM_SINK_ENDPOINT_URL",
        "FERRUMD_AUDIT_WORM_SINK_REGION",
        "FERRUMD_AUDIT_WORM_SINK_ACCESS_KEY_ID",
        "FERRUMD_AUDIT_WORM_SINK_SECRET_ACCESS_KEY",
        "FERRUMD_GCS_ALLOWED_BUCKET",
        "FERRUMD_GCS_ENDPOINT_URL",
        "FERRUMD_GCS_PROJECT_ID",
        "FERRUMD_GCS_CREDENTIALS_PATH",
        "FERRUMD_GCS_LIVE",
        "FERRUMD_HTTP_EGRESS_ALLOWED_HOSTS",
    ] {
        unsafe { std::env::remove_var(key) };
    }
}

fn write_temp_config(contents: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ferrumd-test-{}.toml", unique));
    fs::write(&path, contents).unwrap();
    path
}
#[test]
fn test_resolve_config_cli_over_env_over_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:1111"
store_dsn = "sqlite://from-file.db"
auth_mode = "disabled"
log_filter = "warn"
git_repo_roots = ["/from/file/repos"]
sqlite_db_roots = ["/from/file/databases"]
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_BIND_ADDR", "127.0.0.1:2222");
        std::env::set_var("FERRUMD_STORE_DSN", "sqlite://from-env.db");
        std::env::set_var("FERRUMD_LOG_FILTER", "debug");
        std::env::set_var(
            "FERRUMD_GIT_REPO_ROOTS",
            "/from/env/repos,/from/env/repos-2",
        );
        std::env::set_var("FERRUMD_SQLITE_DB_ROOTS", "/from/env/databases");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: Some("127.0.0.1:3333".to_string()),
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.bind_addr, "127.0.0.1:3333".parse().unwrap());
    assert_eq!(config.store_dsn, "sqlite://from-env.db");
    assert_eq!(config.log_filter, "debug");
    assert_eq!(
        config.git_repo_roots,
        vec![
            PathBuf::from("/from/env/repos"),
            PathBuf::from("/from/env/repos-2")
        ]
    );
    assert_eq!(
        config.sqlite_db_roots,
        vec![PathBuf::from("/from/env/databases")]
    );

    unsafe {
        std::env::set_var("FERRUMD_GIT_REPO_ROOTS", "");
        std::env::set_var("FERRUMD_SQLITE_DB_ROOTS", " , ");
    }
    let disabled = resolve_config(&args).unwrap();
    assert!(
        disabled.git_repo_roots.is_empty(),
        "an explicitly empty env value must disable the Git adapter"
    );
    assert!(
        disabled.sqlite_db_roots.is_empty(),
        "an explicitly empty env value must disable the SQLite adapter"
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_allows_nonlocal_bind_when_env_override_is_true() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "disabled"
allow_insecure_nonlocal_bind = false
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND", "true");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert!(config.allow_insecure_nonlocal_bind);
    assert_eq!(config.bind_addr.ip().to_string(), "0.0.0.0");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_bearer_mode_without_token() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "bearer"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(error.to_string().contains("bearer token cannot be empty"));

    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(feature = "postgres"))]
fn test_resolve_config_rejects_postgres_dsn_without_feature() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
store_dsn = "postgres://user:pass@localhost:5432/db"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("PostgreSQL support is not enabled"),
        "expected PostgreSQL not enabled error, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(feature = "postgres"))]
fn test_resolve_config_rejects_postgresql_dsn_without_feature() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
store_dsn = "postgresql://user:pass@localhost:5432/db"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("PostgreSQL support is not enabled"),
        "expected PostgreSQL not enabled error, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
#[cfg(feature = "postgres")]
fn test_resolve_config_accepts_postgres_dsn_with_feature() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
store_dsn = "postgres://user:pass@localhost:5432/db"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).expect("expected config to be accepted");
    assert_eq!(config.store_dsn, "postgres://user:pass@localhost:5432/db");

    let _ = fs::remove_file(path);
}

#[test]
#[cfg(feature = "postgres")]
fn test_resolve_config_accepts_postgresql_dsn_with_feature() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
store_dsn = "postgresql://user:pass@localhost:5432/db"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).expect("expected config to be accepted");
    assert_eq!(config.store_dsn, "postgresql://user:pass@localhost:5432/db");

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_mysql_dsn() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
store_dsn = "mysql://user:pass@localhost:3306/db"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("MySQL is not implemented"),
        "expected MySQL not implemented error, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rate_limit_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.rate_limit_per_second, 2);
    assert_eq!(config.rate_limit_burst, 50);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rate_limit_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_per_second = 5
rate_limit_burst = 100
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.rate_limit_per_second, 5);
    assert_eq!(config.rate_limit_burst, 100);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rate_limit_cli_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_per_second = 5
rate_limit_burst = 100
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: Some(10),
        rate_limit_burst: Some(200),
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.rate_limit_per_second, 10);
    assert_eq!(config.rate_limit_burst, 200);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rate_limit_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_per_second = 5
rate_limit_burst = 100
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_RATE_LIMIT_PER_SECOND", "15");
        std::env::set_var("FERRUMD_RATE_LIMIT_BURST", "300");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.rate_limit_per_second, 15);
    assert_eq!(config.rate_limit_burst, 300);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_zero_rate_limit_per_second() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_per_second = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("rate_limit_per_second must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_zero_rate_limit_burst() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_burst = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("rate_limit_burst must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_rate_limit_burst_too_large() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_burst = 20000
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("rate_limit_burst must be at most 10000")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_log_format_defaults_to_text() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.log_format, ferrum_gateway::LogFormat::Text);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_log_format_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
log_format = "json"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.log_format, ferrum_gateway::LogFormat::Json);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_log_format_cli_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
log_format = "text"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: Some("json".to_string()),
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.log_format, ferrum_gateway::LogFormat::Json);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_log_format_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
log_format = "text"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_LOG_FORMAT", "json");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.log_format, ferrum_gateway::LogFormat::Json);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_invalid_log_format() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
log_format = "invalid"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(error.to_string().contains("invalid log format"));

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_accepts_compact_as_text_format() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
log_format = "compact"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    // "compact" is accepted as alias for "text"
    assert_eq!(config.log_format, ferrum_gateway::LogFormat::Text);

    let _ = fs::remove_file(path);
}

// === write_queue_threshold tests ===

#[test]
fn test_resolve_config_write_queue_threshold_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.write_queue_threshold, 100);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_write_queue_threshold_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
write_queue_threshold = 500
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.write_queue_threshold, 500);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_write_queue_threshold_cli_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
write_queue_threshold = 500
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: Some(200),
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.write_queue_threshold, 200);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_write_queue_threshold_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
write_queue_threshold = 500
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_WRITE_QUEUE_THRESHOLD", "300");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();

    assert_eq!(config.write_queue_threshold, 300);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_zero_write_queue_threshold() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
write_queue_threshold = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("write_queue_threshold must be between 1 and 10000")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_write_queue_threshold_too_large() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
write_queue_threshold = 10001
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("write_queue_threshold must be between 1 and 10000")
    );

    let _ = fs::remove_file(path);
}

// === PostgreSQL pool config tests ===

#[test]
fn test_resolve_config_pg_pool_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_max_connections, 10);
    assert_eq!(config.pg_min_idle, 2);
    assert_eq!(config.pg_acquire_timeout_secs, 5);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pg_pool_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_max_connections = 20
pg_min_idle = 5
pg_acquire_timeout_secs = 10
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_max_connections, 20);
    assert_eq!(config.pg_min_idle, 5);
    assert_eq!(config.pg_acquire_timeout_secs, 10);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pg_pool_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_max_connections = 20
pg_min_idle = 5
pg_acquire_timeout_secs = 10
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_PG_MAX_CONNECTIONS", "30");
        std::env::set_var("FERRUMD_PG_MIN_IDLE", "8");
        std::env::set_var("FERRUMD_PG_ACQUIRE_TIMEOUT_SECS", "15");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_max_connections, 30);
    assert_eq!(config.pg_min_idle, 8);
    assert_eq!(config.pg_acquire_timeout_secs, 15);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_pg_pool_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_PG_MAX_CONNECTIONS", "30");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: Some(50),
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_max_connections, 50);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_zero_pg_max_connections() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_max_connections = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("pg_max_connections must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_zero_pg_acquire_timeout() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_acquire_timeout_secs = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("pg_acquire_timeout_secs must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pg_timeout_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_statement_timeout_ms, 5000);
    assert_eq!(config.pg_idle_in_transaction_timeout_ms, 10000);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pg_timeout_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_statement_timeout_ms = 3000
pg_idle_in_transaction_timeout_ms = 7000
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_statement_timeout_ms, 3000);
    assert_eq!(config.pg_idle_in_transaction_timeout_ms, 7000);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pg_timeout_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_statement_timeout_ms = 3000
pg_idle_in_transaction_timeout_ms = 7000
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_PG_STATEMENT_TIMEOUT_MS", "8000");
        std::env::set_var("FERRUMD_PG_IDLE_IN_TRANSACTION_TIMEOUT_MS", "15000");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_statement_timeout_ms, 8000);
    assert_eq!(config.pg_idle_in_transaction_timeout_ms, 15000);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_pg_timeout_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_PG_STATEMENT_TIMEOUT_MS", "8000");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: Some(2000),
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_statement_timeout_ms, 2000);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_accepts_zero_pg_timeout_as_disabled() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pg_statement_timeout_ms = 0
pg_idle_in_transaction_timeout_ms = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pg_statement_timeout_ms, 0);
    assert_eq!(config.pg_idle_in_transaction_timeout_ms, 0);

    let _ = fs::remove_file(path);
}

// ── OIDC Config Tests (Phase 4.4) ──

#[test]
fn test_resolve_config_oidc_from_toml_with_jwks_url() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"

[oidc]
issuer = "https://test-issuer.example.com"
audiences = ["ferrumgate-test"]
jwks_url = "https://test-issuer.example.com/jwks.json"
jwks_cache_ttl_secs = 600
actor_id_claim = "sub"
role_source_claim = "groups"
require_email_verified = true
allowed_algorithms = ["HS256"]

[oidc.role_mappings]
fg-admins = "admin"
fg-operators = "operator"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.auth_mode, AuthMode::Oidc);
    let oidc = config.oidc_config.as_ref().unwrap();
    assert_eq!(oidc.issuer, "https://test-issuer.example.com");
    assert_eq!(oidc.audiences, vec!["ferrumgate-test"]);
    assert_eq!(
        oidc.jwks_url.as_deref(),
        Some("https://test-issuer.example.com/jwks.json")
    );
    assert_eq!(oidc.jwks_cache_ttl_secs, 600);
    assert_eq!(oidc.actor_id_claim, "sub");
    assert_eq!(oidc.role_source_claim, "groups");
    assert!(oidc.require_email_verified);
    assert_eq!(
        oidc.allowed_algorithms,
        vec![jsonwebtoken::Algorithm::HS256]
    );
    assert_eq!(oidc.role_mappings.len(), 2);
    assert_eq!(
        oidc.role_mappings.get("fg-admins"),
        Some(&ferrum_proto::TokenRole::Admin)
    );
    assert_eq!(
        oidc.role_mappings.get("fg-operators"),
        Some(&ferrum_proto::TokenRole::Operator)
    );
    assert!(oidc.static_keys.is_empty());

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_oidc_from_env_overrides_toml() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"

[oidc]
issuer = "https://file-issuer.example.com"
audiences = ["file-aud"]
jwks_url = "https://file-issuer.example.com/jwks.json"

[oidc.role_mappings]
fg-admins = "admin"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_OIDC_ISSUER", "https://env-issuer.example.com");
        std::env::set_var("FERRUMD_OIDC_AUDIENCES", "env-aud1,env-aud2");
        std::env::set_var(
            "FERRUMD_OIDC_JWKS_URL",
            "https://env-issuer.example.com/jwks.json",
        );
        std::env::set_var("FERRUMD_OIDC_JWKS_CACHE_TTL_SECS", "120");
        std::env::set_var("FERRUMD_OIDC_ACTOR_ID_CLAIM", "email");
        std::env::set_var("FERRUMD_OIDC_ROLE_SOURCE_CLAIM", "roles");
        std::env::set_var("FERRUMD_OIDC_REQUIRE_EMAIL_VERIFIED", "false");
        std::env::set_var("FERRUMD_OIDC_ALLOWED_ALGORITHMS", "RS256,ES256");
        std::env::set_var(
            "FERRUMD_OIDC_ROLE_MAPPINGS",
            "env-admins=admin,env-operators=operator",
        );
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let oidc = config.oidc_config.as_ref().unwrap();
    assert_eq!(oidc.issuer, "https://env-issuer.example.com");
    assert_eq!(oidc.audiences, vec!["env-aud1", "env-aud2"]);
    assert_eq!(
        oidc.jwks_url.as_deref(),
        Some("https://env-issuer.example.com/jwks.json")
    );
    assert_eq!(oidc.jwks_cache_ttl_secs, 120);
    assert_eq!(oidc.actor_id_claim, "email");
    assert_eq!(oidc.role_source_claim, "roles");
    assert!(!oidc.require_email_verified);
    assert_eq!(
        oidc.allowed_algorithms,
        vec![
            jsonwebtoken::Algorithm::RS256,
            jsonwebtoken::Algorithm::ES256
        ]
    );
    assert_eq!(oidc.role_mappings.len(), 2);
    assert_eq!(
        oidc.role_mappings.get("env-admins"),
        Some(&ferrum_proto::TokenRole::Admin)
    );
    assert_eq!(
        oidc.role_mappings.get("env-operators"),
        Some(&ferrum_proto::TokenRole::Operator)
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_oidc_rejects_missing_issuer() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"

[oidc]
issuer = ""
audiences = ["ferrumgate-test"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let err = resolve_config(&args).expect_err("expected config error");
    assert!(
        err.to_string().contains("oidc issuer cannot be empty"),
        "got: {}",
        err
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_oidc_rejects_empty_static_keys_without_jwks_url() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"

[oidc]
issuer = "https://test-issuer.example.com"
audiences = ["ferrumgate-test"]

[oidc.role_mappings]
fg-admins = "admin"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let err = resolve_config(&args).expect_err("expected config error");
    assert!(
        err.to_string()
            .contains("static_keys cannot be empty when jwks_url is not configured"),
        "got: {}",
        err
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_oidc_allows_empty_static_keys_with_jwks_url() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"

[oidc]
issuer = "https://test-issuer.example.com"
audiences = ["ferrumgate-test"]
jwks_url = "https://test-issuer.example.com/jwks.json"

[oidc.role_mappings]
fg-admins = "admin"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let oidc = config.oidc_config.as_ref().unwrap();
    assert!(oidc.static_keys.is_empty());
    assert_eq!(
        oidc.jwks_url.as_deref(),
        Some("https://test-issuer.example.com/jwks.json")
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_oidc_static_key_hmac_from_toml() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let secret_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"test-secret");
    let toml = format!(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"

[oidc]
issuer = "https://test-issuer.example.com"
audiences = ["ferrumgate-test"]

[[oidc.static_keys]]
kid = "test-key-1"
type = "hmac"
secret = "{secret_b64}"

[oidc.role_mappings]
fg-admins = "admin"
"#
    );

    let path = write_temp_config(&toml);

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let oidc = config.oidc_config.as_ref().unwrap();
    assert_eq!(oidc.static_keys.len(), 1);
    let km = oidc.static_keys.get("test-key-1").unwrap();
    assert!(matches!(km, ferrum_gateway::KeyMaterial::Hmac(bytes) if bytes == b"test-secret"));

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_lifecycle_reconciliation_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.lifecycle_reconciliation_enabled);
    assert_eq!(config.lifecycle_reconciliation_interval_secs, 60);
    assert_eq!(config.lifecycle_reconciliation_batch_limit, 1000);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_lifecycle_reconciliation_env_overrides() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_LIFECYCLE_RECONCILIATION_ENABLED", "true");
        std::env::set_var("FERRUMD_LIFECYCLE_RECONCILIATION_INTERVAL_SECS", "120");
        std::env::set_var("FERRUMD_LIFECYCLE_RECONCILIATION_BATCH_LIMIT", "500");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.lifecycle_reconciliation_enabled);
    assert_eq!(config.lifecycle_reconciliation_interval_secs, 120);
    assert_eq!(config.lifecycle_reconciliation_batch_limit, 500);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_lifecycle_reconciliation_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_LIFECYCLE_RECONCILIATION_ENABLED", "true");
        std::env::set_var("FERRUMD_LIFECYCLE_RECONCILIATION_INTERVAL_SECS", "120");
        std::env::set_var("FERRUMD_LIFECYCLE_RECONCILIATION_BATCH_LIMIT", "500");
    }

    let args = Args {
        config: None,
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: true,
        lifecycle_reconciliation_interval_secs: Some(30),
        lifecycle_reconciliation_batch_limit: Some(2500),
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.lifecycle_reconciliation_enabled);
    assert_eq!(config.lifecycle_reconciliation_interval_secs, 30);
    assert_eq!(config.lifecycle_reconciliation_batch_limit, 2500);

    clear_test_env();
}

#[test]
fn test_resolve_config_lifecycle_reconciliation_file_overrides_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
lifecycle_reconciliation_enabled = true
lifecycle_reconciliation_interval_secs = 90
lifecycle_reconciliation_batch_limit = 200
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.lifecycle_reconciliation_enabled);
    assert_eq!(config.lifecycle_reconciliation_interval_secs, 90);
    assert_eq!(config.lifecycle_reconciliation_batch_limit, 200);

    let _ = fs::remove_file(path);
    clear_test_env();
}

// === approval timeout config tests ===

#[test]
fn test_resolve_config_approval_timeout_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.approval_timeout_seconds, 3600);
    assert_eq!(config.approval_reconciliation_interval_secs, 300);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_approval_timeout_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
approval_timeout_seconds = 7200
approval_reconciliation_interval_secs = 600
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.approval_timeout_seconds, 7200);
    assert_eq!(config.approval_reconciliation_interval_secs, 600);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_approval_timeout_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
approval_timeout_seconds = 7200
approval_reconciliation_interval_secs = 600
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_APPROVAL_TIMEOUT_SECONDS", "1800");
        std::env::set_var("FERRUMD_APPROVAL_RECONCILIATION_INTERVAL_SECS", "120");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.approval_timeout_seconds, 1800);
    assert_eq!(config.approval_reconciliation_interval_secs, 120);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_approval_timeout_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_APPROVAL_TIMEOUT_SECONDS", "1800");
        std::env::set_var("FERRUMD_APPROVAL_RECONCILIATION_INTERVAL_SECS", "120");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: Some(900),
        approval_reconciliation_interval_secs: Some(60),
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.approval_timeout_seconds, 900);
    assert_eq!(config.approval_reconciliation_interval_secs, 60);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_approval_timeout_too_small() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
approval_timeout_enabled = true
approval_timeout_seconds = 30
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("approval_timeout_seconds must be between 60 and 86400")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_approval_reconciliation_interval_too_small() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
approval_timeout_enabled = true
approval_reconciliation_interval_secs = 1
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("approval_reconciliation_interval_secs must be between 5 and 86400")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_approval_timeout_enabled_defaults_to_false() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.approval_timeout_enabled);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_approval_timeout_enabled_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
approval_timeout_enabled = true
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.approval_timeout_enabled);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_zero_reconciliation_interval() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
lifecycle_reconciliation_enabled = true
lifecycle_reconciliation_interval_secs = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("lifecycle_reconciliation_interval_secs must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_zero_reconciliation_batch_limit() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
lifecycle_reconciliation_enabled = true
lifecycle_reconciliation_batch_limit = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("lifecycle_reconciliation_batch_limit must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_approval_mfa_required_defaults_to_false() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.approval_mfa_required);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_approval_mfa_required_from_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
approval_mfa_required = false
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_APPROVAL_MFA_REQUIRED", "true");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.approval_mfa_required);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_approval_mfa_required_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_APPROVAL_MFA_REQUIRED", "false");
    }

    let args = Args {
        config: None,
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: true,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.approval_mfa_required);

    clear_test_env();
}

#[test]
fn test_validate_rejects_disabled_lifecycle_reconciliation_in_production() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "bearer"
bearer_token = "valid-test-token"
store_dsn = "sqlite:///tmp/ferrumgate/test.db"
fs_workdir = "/tmp/ferrumgate"
approval_timeout_enabled = true
audit_fail_closed = true
lifecycle_reconciliation_enabled = false
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: true,
        audit_fail_closed: true,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let err = resolve_config(&args).unwrap_err();
    assert!(err.to_string().contains("lifecycle_reconciliation_enabled"));

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_validate_rejects_disabled_approval_timeout_in_production() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "bearer"
bearer_token = "valid-test-token"
store_dsn = "sqlite:///tmp/ferrumgate/test.db"
fs_workdir = "/tmp/ferrumgate"
lifecycle_reconciliation_enabled = true
audit_fail_closed = true
approval_timeout_enabled = false
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: true,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: true,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let err = resolve_config(&args).unwrap_err();
    assert!(err.to_string().contains("approval_timeout_enabled"));

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_validate_rejects_disabled_audit_fail_closed_in_production() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "bearer"
bearer_token = "valid-test-token"
store_dsn = "sqlite:///tmp/ferrumgate/test.db"
fs_workdir = "/tmp/ferrumgate"
lifecycle_reconciliation_enabled = true
approval_timeout_enabled = true
audit_fail_closed = false
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: true,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: true,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let err = resolve_config(&args).unwrap_err();
    assert!(err.to_string().contains("audit_fail_closed"));

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_mfa_secret_key_defaults_to_none() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.mfa_secret_key.is_none());
    assert_eq!(config.mfa_totp_issuer, "FerrumGate");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_mfa_secret_key_from_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var(
            "FERRUMD_MFA_SECRET_KEY",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        std::env::set_var("FERRUMD_MFA_TOTP_ISSUER", "MyOrg");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(
        config.mfa_secret_key,
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
    );
    assert_eq!(config.mfa_totp_issuer, "MyOrg");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_mfa_secret_key_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var(
            "FERRUMD_MFA_SECRET_KEY",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: Some(
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
        ),
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(
        config.mfa_secret_key,
        Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string())
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_mfa_secret_key_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
mfa_secret_key = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
mfa_totp_issuer = "FileIssuer"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(
        config.mfa_secret_key,
        Some("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string())
    );
    assert_eq!(config.mfa_totp_issuer, "FileIssuer");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_mfa_secret_key_redacted_in_debug() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
mfa_secret_key = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let debug = format!("{:?}", config);
    assert!(
        !debug.contains("secret-value"),
        "mfa_secret_key must be redacted in debug output"
    );
    assert!(
        debug.contains("<redacted>"),
        "debug output should contain <redacted>"
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_zero_mfa_lockout_max_attempts() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
mfa_lockout_max_attempts = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("mfa_lockout_max_attempts must be at least 1")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_mfa_lockout_duration_secs_too_large() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
mfa_lockout_duration_secs = 90000
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("mfa_lockout_duration_secs must be between 1 and 86400")
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_mfa_lockout_from_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe { std::env::set_var("FERRUMD_MFA_LOCKOUT_MAX_ATTEMPTS", "3") };
    unsafe { std::env::set_var("FERRUMD_MFA_LOCKOUT_DURATION_SECS", "300") };

    let args = Args {
        config: None,
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.mfa_lockout_max_attempts, 3);
    assert_eq!(config.mfa_lockout_duration_secs, 300);

    clear_test_env();
}

// === PDP mode config tests ===

#[test]
fn test_resolve_config_pdp_mode_defaults_to_dual() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pdp_mode, ferrum_gateway::PdpMode::Dual);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pdp_mode_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pdp_mode = "static"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pdp_mode, ferrum_gateway::PdpMode::Static);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pdp_mode_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pdp_mode = "static"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_PDP_MODE", "bundles");
    }

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pdp_mode, ferrum_gateway::PdpMode::Bundles);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_pdp_mode_cli_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pdp_mode = "static"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: Some("dual".to_string()),
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pdp_mode, ferrum_gateway::PdpMode::Dual);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_invalid_pdp_mode() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pdp_mode = "unknown"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        bind_addr: None,
        store_dsn: None,
        auth_mode: None,
        bearer_token: None,
        allow_insecure_nonlocal_bind: false,
        log_filter: None,
        store_synchronous: None,
        store_wal_autocheckpoint: None,
        rate_limit_per_second: None,
        rate_limit_burst: None,
        log_format: None,
        pdp_mode: None,
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        approval_timeout_seconds: None,
        approval_reconciliation_interval_secs: None,
        approval_timeout_enabled: false,
        audit_fail_closed: false,
        approval_mfa_required: false,
        mfa_secret_key: None,
        mfa_totp_issuer: None,
        mfa_lockout_max_attempts: None,
        mfa_lockout_duration_secs: None,
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(error.to_string().contains("invalid pdp mode"));

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_nonce_cache_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let args = Args::default();
    let config = resolve_config(&args).unwrap();
    assert_eq!(config.nonce_cache_backend, NonceCacheBackend::Auto);
    assert_eq!(config.nonce_cache_ttl_secs, 0);
    assert_eq!(config.nonce_cache_max_entries, 10_000);
}

#[test]
fn test_resolve_config_nonce_cache_cli_overrides() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let args = Args {
        nonce_cache_backend: Some("memory".to_string()),
        nonce_cache_ttl_secs: Some(120),
        nonce_cache_max_entries: Some(500),
        ..Default::default()
    };
    let config = resolve_config(&args).unwrap();
    assert_eq!(config.nonce_cache_backend, NonceCacheBackend::Memory);
    assert_eq!(config.nonce_cache_ttl_secs, 120);
    assert_eq!(config.nonce_cache_max_entries, 500);
}

#[test]
fn test_resolve_config_nonce_cache_env_overrides_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_NONCE_CACHE_BACKEND", "memory");
        std::env::set_var("FERRUMD_NONCE_CACHE_TTL_SECS", "180");
        std::env::set_var("FERRUMD_NONCE_CACHE_MAX_ENTRIES", "250");
    }

    let args = Args::default();
    let config = resolve_config(&args).unwrap();
    assert_eq!(config.nonce_cache_backend, NonceCacheBackend::Memory);
    assert_eq!(config.nonce_cache_ttl_secs, 180);
    assert_eq!(config.nonce_cache_max_entries, 250);
}

#[test]
fn test_resolve_config_nonce_cache_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_NONCE_CACHE_BACKEND", "postgres");
        std::env::set_var("FERRUMD_NONCE_CACHE_TTL_SECS", "180");
        std::env::set_var("FERRUMD_NONCE_CACHE_MAX_ENTRIES", "250");
    }

    let args = Args {
        nonce_cache_backend: Some("memory".to_string()),
        nonce_cache_ttl_secs: Some(120),
        nonce_cache_max_entries: Some(500),
        ..Default::default()
    };
    let config = resolve_config(&args).unwrap();
    assert_eq!(config.nonce_cache_backend, NonceCacheBackend::Memory);
    assert_eq!(config.nonce_cache_ttl_secs, 120);
    assert_eq!(config.nonce_cache_max_entries, 500);
}

#[test]
fn test_resolve_config_nonce_cache_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
nonce_cache_backend = "memory"
nonce_cache_ttl_secs = 240
nonce_cache_max_entries = 1000
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };
    let config = resolve_config(&args).unwrap();
    assert_eq!(config.nonce_cache_backend, NonceCacheBackend::Memory);
    assert_eq!(config.nonce_cache_ttl_secs, 240);
    assert_eq!(config.nonce_cache_max_entries, 1000);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_postgres_nonce_cache_with_sqlite_store() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let args = Args {
        nonce_cache_backend: Some("postgres".to_string()),
        store_dsn: Some("sqlite::memory:".to_string()),
        ..Default::default()
    };
    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("PostgreSQL store DSN"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_resolve_config_rejects_invalid_nonce_cache_backend() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let args = Args {
        nonce_cache_backend: Some("redis".to_string()),
        ..Default::default()
    };
    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("invalid nonce cache backend"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_resolve_config_ha_reconciler_defaults_disabled() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.ha_reconciler_enabled);
    assert_eq!(config.ha_reconciler_interval_secs, 60);
    assert_eq!(config.ha_reconciler_stale_threshold_secs, 1800);
    assert_eq!(config.ha_reconciler_batch_size, 100);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_ha_reconciler_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
ha_reconciler_enabled = true
ha_reconciler_interval_secs = 120
ha_reconciler_stale_threshold_secs = 600
ha_reconciler_batch_size = 50
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.ha_reconciler_enabled);
    assert_eq!(config.ha_reconciler_interval_secs, 120);
    assert_eq!(config.ha_reconciler_stale_threshold_secs, 600);
    assert_eq!(config.ha_reconciler_batch_size, 50);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_ha_reconciler_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
ha_reconciler_enabled = false
ha_reconciler_interval_secs = 120
ha_reconciler_stale_threshold_secs = 600
ha_reconciler_batch_size = 50
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_HA_RECONCILER_ENABLED", "true");
        std::env::set_var("FERRUMD_HA_RECONCILER_INTERVAL_SECS", "300");
        std::env::set_var("FERRUMD_HA_RECONCILER_STALE_THRESHOLD_SECS", "900");
        std::env::set_var("FERRUMD_HA_RECONCILER_BATCH_SIZE", "200");
    }

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.ha_reconciler_enabled);
    assert_eq!(config.ha_reconciler_interval_secs, 300);
    assert_eq!(config.ha_reconciler_stale_threshold_secs, 900);
    assert_eq!(config.ha_reconciler_batch_size, 200);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_ha_reconciler_cli_overrides_all() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
ha_reconciler_enabled = false
ha_reconciler_interval_secs = 120
ha_reconciler_stale_threshold_secs = 600
ha_reconciler_batch_size = 50
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_HA_RECONCILER_ENABLED", "false");
        std::env::set_var("FERRUMD_HA_RECONCILER_INTERVAL_SECS", "300");
    }

    let args = Args {
        config: Some(path.clone()),
        ha_reconciler_enabled: true,
        ha_reconciler_interval_secs: Some(10),
        ha_reconciler_stale_threshold_secs: Some(120),
        ha_reconciler_batch_size: Some(10),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.ha_reconciler_enabled);
    assert_eq!(config.ha_reconciler_interval_secs, 10);
    assert_eq!(config.ha_reconciler_stale_threshold_secs, 120);
    assert_eq!(config.ha_reconciler_batch_size, 10);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_ha_reconciler_validation_when_enabled() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
ha_reconciler_enabled = true
ha_reconciler_interval_secs = 3
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("ha_reconciler_interval_secs must be between 5 and 3600"),
        "unexpected error: {error}"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_ha_reconciler_invalid_values_ignored_when_disabled() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
ha_reconciler_enabled = false
ha_reconciler_interval_secs = 3
ha_reconciler_stale_threshold_secs = 30
ha_reconciler_batch_size = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.ha_reconciler_enabled);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_behavioral_anomaly_defaults_disabled() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.behavioral_anomaly_enabled);
    assert_eq!(config.behavioral_anomaly_window_secs, 60);
    assert_eq!(config.behavioral_anomaly_warning_threshold, 5);
    assert_eq!(config.behavioral_anomaly_critical_threshold, 10);
    assert_eq!(config.behavioral_anomaly_max_actors, 1000);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_behavioral_anomaly_env_over_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
behavioral_anomaly_enabled = true
behavioral_anomaly_window_secs = 120
behavioral_anomaly_warning_threshold = 3
behavioral_anomaly_critical_threshold = 7
behavioral_anomaly_max_actors = 500
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_BEHAVIORAL_ANOMALY_ENABLED", "false");
        std::env::set_var("FERRUMD_BEHAVIORAL_ANOMALY_WINDOW_SECS", "30");
    }

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.behavioral_anomaly_enabled);
    assert_eq!(config.behavioral_anomaly_window_secs, 30);
    assert_eq!(config.behavioral_anomaly_warning_threshold, 3);
    assert_eq!(config.behavioral_anomaly_critical_threshold, 7);
    assert_eq!(config.behavioral_anomaly_max_actors, 500);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_behavioral_anomaly_validation_rejects_invalid_thresholds() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
behavioral_anomaly_enabled = true
behavioral_anomaly_warning_threshold = 10
behavioral_anomaly_critical_threshold = 5
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let err = resolve_config(&args).expect_err("expected config validation error");
    assert!(
        err.to_string().contains(
            "behavioral_anomaly_critical_threshold must be >= behavioral_anomaly_warning_threshold"
        ),
        "unexpected error: {}",
        err
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "worm-sink")]
#[test]
fn test_resolve_config_rejects_worm_enabled_without_bucket() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
audit_worm_sink_enabled = true
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let err = resolve_config(&args).expect_err("expected config error");
    assert!(
        err.to_string()
            .contains("audit_worm_sink_enabled is true but audit_worm_sink_bucket is not set"),
        "unexpected error: {}",
        err
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "worm-sink")]
#[test]
fn test_resolve_config_worm_sink_accepts_file_config() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
audit_worm_sink_enabled = true

[server.audit_worm_sink]
bucket = "my-worm-bucket"
prefix = "audit"
object_lock_mode = "compliance"
retention_days = 7
export_interval_secs = 60
batch_limit = 1000
live = false
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.audit_worm_sink_enabled);
    let cfg = config.worm_sink_config.as_ref().unwrap();
    assert_eq!(cfg.bucket, "my-worm-bucket");
    assert_eq!(cfg.prefix, "audit");
    assert_eq!(
        cfg.object_lock_mode,
        ferrum_adapter_s3::ObjectLockMode::Compliance
    );
    assert_eq!(cfg.retention_days, 7);
    assert_eq!(cfg.export_interval_secs, 60);
    assert_eq!(cfg.batch_limit, 1000);
    assert!(!cfg.live);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "worm-sink")]
#[test]
fn test_resolve_config_worm_sink_rejects_invalid_object_lock_mode() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
audit_worm_sink_enabled = true

[server.audit_worm_sink]
bucket = "my-worm-bucket"
object_lock_mode = "invalid-mode"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let err = resolve_config(&args).expect_err("expected config error");
    assert!(
        err.to_string()
            .contains("invalid audit_worm_sink object_lock_mode"),
        "unexpected error: {}",
        err
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "worm-sink")]
#[test]
fn test_resolve_config_worm_sink_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_AUDIT_WORM_SINK_BUCKET", "env-bucket");
        std::env::set_var("FERRUMD_AUDIT_WORM_SINK_PREFIX", "env-prefix");
        std::env::set_var("FERRUMD_AUDIT_WORM_SINK_OBJECT_LOCK_MODE", "compliance");
        std::env::set_var("FERRUMD_AUDIT_WORM_SINK_RETENTION_DAYS", "14");
    }

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
audit_worm_sink_enabled = true

[server.audit_worm_sink]
bucket = "file-bucket"
prefix = "file-prefix"
object_lock_mode = "governance"
retention_days = 7
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let cfg = config.worm_sink_config.as_ref().unwrap();
    assert_eq!(cfg.bucket, "env-bucket");
    assert_eq!(cfg.prefix, "env-prefix");
    assert_eq!(
        cfg.object_lock_mode,
        ferrum_adapter_s3::ObjectLockMode::Compliance
    );
    assert_eq!(cfg.retention_days, 14);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "worm-sink")]
#[test]
fn test_server_config_debug_redacts_worm_credentials() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
audit_worm_sink_enabled = true

[server.audit_worm_sink]
bucket = "my-worm-bucket"
access_key_id = "AKIAEXAMPLE"
secret_access_key = "w0rmdb33f/s3cr3t"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let debug = format!("{:?}", config);
    assert!(
        debug.contains("my-worm-bucket"),
        "debug output should contain non-sensitive bucket name"
    );
    assert!(
        !debug.contains("AKIAEXAMPLE"),
        "debug output must not contain WORM access key ID"
    );
    assert!(
        !debug.contains("w0rmdb33f/s3cr3t"),
        "debug output must not contain WORM secret access key"
    );
    assert!(
        debug.contains("<redacted>"),
        "debug output should show redaction placeholder"
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "gcs")]
#[test]
fn test_gcs_config_defaults_live_false() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.gcs_config]
allowed_bucket = "my-gcs-bucket"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let gcs = config
        .gcs_config
        .as_ref()
        .expect("gcs_config should be set");
    assert_eq!(gcs.allowed_bucket, "my-gcs-bucket");
    assert!(!gcs.live, "GCS live should default to false");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "gcs")]
#[test]
fn test_gcs_config_env_live_true() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_GCS_ALLOWED_BUCKET", "env-gcs-bucket");
        std::env::set_var("FERRUMD_GCS_LIVE", "true");
    }

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let gcs = config
        .gcs_config
        .as_ref()
        .expect("gcs_config should be set");
    assert_eq!(gcs.allowed_bucket, "env-gcs-bucket");
    assert!(gcs.live, "FERRUMD_GCS_LIVE=true should enable live mode");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[cfg(feature = "gcs")]
#[test]
fn test_gcs_config_cli_overrides_env_and_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_GCS_ALLOWED_BUCKET", "env-gcs-bucket");
        std::env::set_var("FERRUMD_GCS_LIVE", "true");
    }

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.gcs_config]
allowed_bucket = "file-gcs-bucket"
live = false
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        gcs_live: true,
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let gcs = config
        .gcs_config
        .as_ref()
        .expect("gcs_config should be set");
    assert_eq!(gcs.allowed_bucket, "env-gcs-bucket");
    assert!(gcs.live, "--gcs-live should override env and file");

    let _ = fs::remove_file(path);
    clear_test_env();
}

// === trusted_proxy_cidrs tests ===

#[test]
fn test_resolve_config_trusted_proxy_cidrs_defaults_to_trust_none() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(
        config.trusted_proxy_cidrs.is_empty(),
        "trusted_proxy_cidrs must default to empty (trust-none)"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_trusted_proxy_cidrs_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
trusted_proxy_cidrs = ["10.0.0.0/8", "192.168.0.0/16", "2001:db8::/32"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.trusted_proxy_cidrs.len(), 3);
    assert_eq!(config.trusted_proxy_cidrs[0].to_string(), "10.0.0.0/8");
    assert_eq!(config.trusted_proxy_cidrs[1].to_string(), "192.168.0.0/16");
    assert_eq!(config.trusted_proxy_cidrs[2].to_string(), "2001:db8::/32");

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_trusted_proxy_cidrs_cli_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
trusted_proxy_cidrs = ["10.0.0.0/8"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        trusted_proxy_cidrs: Some("172.16.0.0/12, 100.64.0.0/10".to_string()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.trusted_proxy_cidrs.len(), 2);
    assert_eq!(config.trusted_proxy_cidrs[0].to_string(), "172.16.0.0/12");
    assert_eq!(config.trusted_proxy_cidrs[1].to_string(), "100.64.0.0/10");

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_trusted_proxy_cidrs_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
trusted_proxy_cidrs = ["10.0.0.0/8"]
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_TRUSTED_PROXY_CIDRS", "192.0.2.0/24");
    }

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.trusted_proxy_cidrs.len(), 1);
    assert_eq!(config.trusted_proxy_cidrs[0].to_string(), "192.0.2.0/24");

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_universal_ipv4_trusted_proxy_cidr() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
trusted_proxy_cidrs = ["0.0.0.0/0"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("universal CIDR"),
        "expected universal CIDR rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_universal_ipv6_trusted_proxy_cidr() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
trusted_proxy_cidrs = ["::/0"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("universal CIDR"),
        "expected universal CIDR rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_invalid_trusted_proxy_cidr() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
trusted_proxy_cidrs = ["not-a-cidr"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("invalid trusted_proxy_cidrs entry"),
        "expected invalid CIDR rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

// === pre-auth rate limit tests ===

#[test]
fn test_resolve_config_pre_auth_rate_limit_inherits_defaults() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pre_auth_rate_limit_per_second, None);
    assert_eq!(config.pre_auth_rate_limit_burst, None);
    // Backwards-compatible: effective pre-auth values equal the inner defaults.
    assert_eq!(config.effective_pre_auth_rate_limit_per_second(), 2);
    assert_eq!(config.effective_pre_auth_rate_limit_burst(), 50);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pre_auth_rate_limit_inherits_custom_inner() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
rate_limit_per_second = 7
rate_limit_burst = 70
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pre_auth_rate_limit_per_second, None);
    assert_eq!(config.pre_auth_rate_limit_burst, None);
    assert_eq!(config.effective_pre_auth_rate_limit_per_second(), 7);
    assert_eq!(config.effective_pre_auth_rate_limit_burst(), 70);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pre_auth_rate_limit_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pre_auth_rate_limit_per_second = 3
pre_auth_rate_limit_burst = 25
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pre_auth_rate_limit_per_second, Some(3));
    assert_eq!(config.pre_auth_rate_limit_burst, Some(25));
    assert_eq!(config.effective_pre_auth_rate_limit_per_second(), 3);
    assert_eq!(config.effective_pre_auth_rate_limit_burst(), 25);

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pre_auth_rate_limit_cli_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pre_auth_rate_limit_per_second = 3
pre_auth_rate_limit_burst = 25
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        pre_auth_rate_limit_per_second: Some(9),
        pre_auth_rate_limit_burst: Some(90),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pre_auth_rate_limit_per_second, Some(9));
    assert_eq!(config.pre_auth_rate_limit_burst, Some(90));

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_pre_auth_rate_limit_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pre_auth_rate_limit_per_second = 3
pre_auth_rate_limit_burst = 25
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_PRE_AUTH_RATE_LIMIT_PER_SECOND", "11");
        std::env::set_var("FERRUMD_PRE_AUTH_RATE_LIMIT_BURST", "110");
    }

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert_eq!(config.pre_auth_rate_limit_per_second, Some(11));
    assert_eq!(config.pre_auth_rate_limit_burst, Some(110));

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_zero_pre_auth_rate_limit_per_second() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pre_auth_rate_limit_per_second = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("pre_auth_rate_limit_per_second must be at least 1"),
        "expected zero pre-auth per-second rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_zero_pre_auth_rate_limit_burst() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pre_auth_rate_limit_burst = 0
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("pre_auth_rate_limit_burst must be at least 1"),
        "expected zero pre-auth burst rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_rejects_pre_auth_rate_limit_burst_too_large() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
pre_auth_rate_limit_burst = 20000
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error
            .to_string()
            .contains("pre_auth_rate_limit_burst must be at most 10000"),
        "expected pre-auth burst upper-bound rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_http_egress_absent_is_none() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.http_egress.is_none());

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_http_egress_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.http_egress]
allowed_hosts = ["example.com", "api.example.com"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let http_egress = config.http_egress.expect("http_egress should be present");
    assert_eq!(
        http_egress.allowed_hosts,
        vec!["example.com".to_string(), "api.example.com".to_string()]
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_http_egress_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.http_egress]
allowed_hosts = ["example.com"]
"#,
    );

    unsafe {
        std::env::set_var(
            "FERRUMD_HTTP_EGRESS_ALLOWED_HOSTS",
            "env.example.com,other.com",
        );
    }

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let http_egress = config.http_egress.expect("http_egress should be present");
    assert_eq!(
        http_egress.allowed_hosts,
        vec!["env.example.com".to_string(), "other.com".to_string()]
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_http_egress_cli_overrides_env() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_HTTP_EGRESS_ALLOWED_HOSTS", "env.example.com");
    }

    let args = Args {
        config: Some(path.clone()),
        http_egress_allowed_hosts: Some("cli.example.com".to_string()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let http_egress = config.http_egress.expect("http_egress should be present");
    assert_eq!(
        http_egress.allowed_hosts,
        vec!["cli.example.com".to_string()]
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_http_egress_empty_env_disables() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.http_egress]
allowed_hosts = ["example.com"]
"#,
    );

    unsafe {
        std::env::set_var("FERRUMD_HTTP_EGRESS_ALLOWED_HOSTS", "   ");
    }

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.http_egress.is_none());

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_http_egress_rejects_ip_literal() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.http_egress]
allowed_hosts = ["127.0.0.1"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("IP address"),
        "expected IP address rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_http_egress_rejects_wildcard() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.http_egress]
allowed_hosts = ["*.example.com"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("wildcard"),
        "expected wildcard rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_http_egress_rejects_port() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "disabled"

[server.http_egress]
allowed_hosts = ["example.com:8080"]
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("port"),
        "expected port rejection, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}

// === OIDC token profile tests ===

#[test]
fn test_resolve_config_oidc_token_profile_defaults_to_legacy_jwt() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"
bearer_token = "unused"

[oidc]
issuer = "https://issuer.example.com"
audiences = ["ferrumgate"]
allowed_algorithms = ["HS256"]

[oidc.role_mappings]
fg-admins = "admin"

[[oidc.static_keys]]
kid = "k1"
type = "hmac"
secret = "c2VjcmV0"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let oidc = config.oidc_config.as_ref().unwrap();
    assert_eq!(
        oidc.token_profile,
        ferrum_gateway::OidcTokenProfile::LegacyJwt
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_oidc_token_profile_from_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"
bearer_token = "unused"

[oidc]
issuer = "https://issuer.example.com"
audiences = ["ferrumgate"]
token_profile = "rfc9068_access_token"
allowed_algorithms = ["HS256"]

[oidc.role_mappings]
fg-admins = "admin"

[[oidc.static_keys]]
kid = "k1"
type = "hmac"
secret = "c2VjcmV0"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let oidc = config.oidc_config.as_ref().unwrap();
    assert_eq!(
        oidc.token_profile,
        ferrum_gateway::OidcTokenProfile::Rfc9068AccessToken
    );

    let _ = fs::remove_file(path);
}

#[test]
fn test_resolve_config_oidc_token_profile_env_overrides_config_file() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    unsafe {
        std::env::set_var("FERRUMD_OIDC_TOKEN_PROFILE", "legacy_jwt");
    }

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"
bearer_token = "unused"

[oidc]
issuer = "https://issuer.example.com"
audiences = ["ferrumgate"]
token_profile = "rfc9068_access_token"
allowed_algorithms = ["HS256"]

[oidc.role_mappings]
fg-admins = "admin"

[[oidc.static_keys]]
kid = "k1"
type = "hmac"
secret = "c2VjcmV0"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let config = resolve_config(&args).unwrap();
    let oidc = config.oidc_config.as_ref().unwrap();
    assert_eq!(
        oidc.token_profile,
        ferrum_gateway::OidcTokenProfile::LegacyJwt
    );

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_resolve_config_rejects_invalid_oidc_token_profile() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "127.0.0.1:8080"
auth_mode = "oidc"
bearer_token = "unused"

[oidc]
issuer = "https://issuer.example.com"
audiences = ["ferrumgate"]
token_profile = "strict"
allowed_algorithms = ["HS256"]

[oidc.role_mappings]
fg-admins = "admin"

[[oidc.static_keys]]
kid = "k1"
type = "hmac"
secret = "c2VjcmV0"
"#,
    );

    let args = Args {
        config: Some(path.clone()),
        ..Default::default()
    };

    let error = resolve_config(&args).expect_err("expected config error");
    assert!(
        error.to_string().contains("invalid OIDC token profile"),
        "expected invalid OIDC token profile error, got: {}",
        error
    );

    let _ = fs::remove_file(path);
}
