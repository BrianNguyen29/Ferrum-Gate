use super::*;
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
        "FERRUMD_LIFECYCLE_RECONCILIATION_ENABLED",
        "FERRUMD_LIFECYCLE_RECONCILIATION_INTERVAL_SECS",
        "FERRUMD_LIFECYCLE_RECONCILIATION_BATCH_LIMIT",
        "FERRUMD_AUDIT_FAIL_CLOSED",
        "FERRUMD_APPROVAL_MFA_REQUIRED",
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: Some(200),
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: Some(50),
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: Some(2000),
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let err = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let err = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: true,
        lifecycle_reconciliation_interval_secs: Some(30),
        lifecycle_reconciliation_batch_limit: Some(2500),
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.lifecycle_reconciliation_enabled);
    assert_eq!(config.lifecycle_reconciliation_interval_secs, 90);
    assert_eq!(config.lifecycle_reconciliation_batch_limit, 200);

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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let error = resolve_config(&args).err().expect("expected config error");
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: true,
    };

    let config = resolve_config(&args).unwrap();
    assert!(config.approval_mfa_required);

    clear_test_env();
}

#[test]
fn test_validate_warns_but_allows_disabled_lifecycle_reconciliation_in_production() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "bearer"
bearer_token = "valid-test-token"
store_dsn = "sqlite:///tmp/ferrumgate/test.db"
fs_workdir = "/tmp/ferrumgate"
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.lifecycle_reconciliation_enabled);

    let _ = fs::remove_file(path);
    clear_test_env();
}

#[test]
fn test_validate_warns_but_allows_disabled_audit_fail_closed_in_production() {
    let _guard = env_lock().lock().unwrap();
    clear_test_env();

    let path = write_temp_config(
        r#"[server]
bind_addr = "0.0.0.0:8080"
auth_mode = "bearer"
bearer_token = "valid-test-token"
store_dsn = "sqlite:///tmp/ferrumgate/test.db"
fs_workdir = "/tmp/ferrumgate"
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
        write_queue_threshold: None,
        pg_max_connections: None,
        pg_min_idle: None,
        pg_acquire_timeout_secs: None,
        pg_statement_timeout_ms: None,
        pg_idle_in_transaction_timeout_ms: None,
        lifecycle_reconciliation_enabled: false,
        lifecycle_reconciliation_interval_secs: None,
        lifecycle_reconciliation_batch_limit: None,
        audit_fail_closed: false,
        approval_mfa_required: false,
    };

    let config = resolve_config(&args).unwrap();
    assert!(!config.audit_fail_closed);

    let _ = fs::remove_file(path);
    clear_test_env();
}
