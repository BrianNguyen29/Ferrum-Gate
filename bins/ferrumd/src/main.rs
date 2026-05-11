use anyhow::{Context, Result};
use clap::Parser;
use ferrum_adapter_fs::{PlannableFsAdapter, register_fs_adapter};
use ferrum_adapter_git::register_git_adapter;
use ferrum_cap::InMemoryCapabilityService;
use ferrum_gateway::{AuthMode, GatewayRuntime, ServerConfig, run_http_server};
use ferrum_pdp::StaticPdpEngine;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
#[cfg(feature = "postgres")]
use ferrum_store::postgres::PostgresStore;
use ferrum_store::{SqliteStore, SqliteWalTuning, StoreFacade};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_STORE_DSN: &str = "sqlite::memory:";
const DEFAULT_LOG_FILTER: &str = "info";
const AUTO_CONFIG_FILE: &str = "configs/ferrumgate.dev.toml";

#[derive(Debug, Parser)]
#[command(name = "ferrumd")]
#[command(about = "FerrumGate daemon")]
struct Args {
    /// Path to configuration file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Bind address.
    #[arg(long)]
    bind_addr: Option<String>,

    /// Store DSN.
    #[arg(long)]
    store_dsn: Option<String>,

    /// Auth mode: "disabled" or "bearer".
    #[arg(long)]
    auth_mode: Option<String>,

    /// Bearer token for authentication.
    #[arg(long)]
    bearer_token: Option<String>,

    /// Allow binding to non-loopback addresses when auth is disabled.
    #[arg(long)]
    allow_insecure_nonlocal_bind: bool,

    /// Log filter.
    #[arg(long)]
    log_filter: Option<String>,

    /// SQLite synchronous pragma: "off", "normal", "full", "extra".
    #[arg(long)]
    store_synchronous: Option<String>,

    /// SQLite wal_autocheckpoint pragma: number of frames between checkpoints.
    #[arg(long)]
    store_wal_autocheckpoint: Option<u32>,

    /// Rate limit: sustained requests per second per IP (default 2).
    #[arg(long)]
    rate_limit_per_second: Option<u64>,

    /// Rate limit: burst size per IP (default 50).
    #[arg(long)]
    rate_limit_burst: Option<u32>,

    /// Log format: "text" or "json" (default "text").
    #[arg(long)]
    log_format: Option<String>,

    /// Write queue depth threshold for deep readiness probe (1..=10000).
    #[arg(long)]
    write_queue_threshold: Option<u64>,
}

fn get_env<T: std::str::FromStr>(key: &str) -> Option<T> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ConfigFile {
    #[serde(default)]
    server: Option<ServerSection>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ServerSection {
    #[serde(default)]
    bind_addr: Option<String>,
    #[serde(default)]
    store_dsn: Option<String>,
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default)]
    bearer_token: Option<String>,
    #[serde(default)]
    allow_insecure_nonlocal_bind: Option<bool>,
    #[serde(default)]
    log_filter: Option<String>,
    #[serde(default)]
    store_synchronous: Option<String>,
    #[serde(default)]
    store_wal_autocheckpoint: Option<u32>,
    #[serde(default)]
    rate_limit_per_second: Option<u64>,
    #[serde(default)]
    rate_limit_burst: Option<u32>,
    #[serde(default)]
    log_format: Option<String>,
    #[serde(default)]
    write_queue_threshold: Option<u64>,
}

fn load_config_file(path: &PathBuf) -> Result<ConfigFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    let config: ConfigFile = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file: {}", path.display()))?;
    Ok(config)
}

fn resolve_config(args: &Args) -> Result<ServerConfig> {
    // Try to load config file if specified
    let file_config = if let Some(ref config_path) = args.config {
        Some(load_config_file(config_path)?)
    } else if let Some(config_path) = get_env::<PathBuf>("FERRUMD_CONFIG") {
        Some(load_config_file(&config_path)?)
    } else {
        // Auto-load default config if present
        let auto_path = PathBuf::from(AUTO_CONFIG_FILE);
        if auto_path.exists() {
            Some(load_config_file(&auto_path)?)
        } else {
            None
        }
    };

    // Build config with precedence: CLI > env > config file > defaults
    let server = file_config.as_ref().and_then(|c| c.server.clone());

    let bind_addr = args
        .bind_addr
        .clone()
        .or_else(|| get_env("FERRUMD_BIND_ADDR"))
        .or_else(|| server.as_ref().and_then(|s| s.bind_addr.clone()))
        .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string());

    let store_dsn = args
        .store_dsn
        .clone()
        .or_else(|| get_env("FERRUMD_STORE_DSN"))
        .or_else(|| server.as_ref().and_then(|s| s.store_dsn.clone()))
        .unwrap_or_else(|| DEFAULT_STORE_DSN.to_string());

    let auth_mode = args
        .auth_mode
        .clone()
        .or_else(|| get_env("FERRUMD_AUTH_MODE"))
        .or_else(|| server.as_ref().and_then(|s| s.auth_mode.clone()))
        .unwrap_or_else(|| "disabled".to_string());

    let bearer_token = args
        .bearer_token
        .clone()
        .or_else(|| get_env("FERRUMD_BEARER_TOKEN"))
        .or_else(|| server.as_ref().and_then(|s| s.bearer_token.clone()));

    let allow_insecure_nonlocal_bind = args.allow_insecure_nonlocal_bind
        || get_env::<bool>("FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND").unwrap_or(false)
        || server
            .as_ref()
            .map(|s| s.allow_insecure_nonlocal_bind.unwrap_or(false))
            .unwrap_or(false);

    let log_filter = args
        .log_filter
        .clone()
        .or_else(|| get_env("FERRUMD_LOG_FILTER"))
        .or_else(|| server.as_ref().and_then(|s| s.log_filter.clone()))
        .unwrap_or_else(|| DEFAULT_LOG_FILTER.to_string());

    let log_format = args
        .log_format
        .clone()
        .or_else(|| get_env("FERRUMD_LOG_FORMAT"))
        .or_else(|| server.as_ref().and_then(|s| s.log_format.clone()))
        .unwrap_or_else(|| "text".to_string());

    let log_format_parsed: ferrum_gateway::LogFormat = log_format
        .parse()
        .map_err(|e: String| anyhow::anyhow!("invalid log format: {}", e))?;

    let store_synchronous = args
        .store_synchronous
        .clone()
        .or_else(|| get_env("FERRUMD_STORE_SYNCHRONOUS"))
        .or_else(|| server.as_ref().and_then(|s| s.store_synchronous.clone()));

    let store_wal_autocheckpoint = args
        .store_wal_autocheckpoint
        .or_else(|| {
            get_env::<String>("FERRUMD_STORE_WAL_AUTOCHECKPOINT")?
                .parse()
                .ok()
        })
        .or_else(|| server.as_ref().and_then(|s| s.store_wal_autocheckpoint));

    let rate_limit_per_second = args
        .rate_limit_per_second
        .or_else(|| get_env("FERRUMD_RATE_LIMIT_PER_SECOND"))
        .or_else(|| server.as_ref().and_then(|s| s.rate_limit_per_second))
        .unwrap_or(2);

    let rate_limit_burst = args
        .rate_limit_burst
        .or_else(|| get_env("FERRUMD_RATE_LIMIT_BURST"))
        .or_else(|| server.as_ref().and_then(|s| s.rate_limit_burst))
        .unwrap_or(50);

    let write_queue_threshold = args
        .write_queue_threshold
        .or_else(|| get_env("FERRUMD_WRITE_QUEUE_THRESHOLD"))
        .or_else(|| server.as_ref().and_then(|s| s.write_queue_threshold))
        .unwrap_or(100);

    let bind_addr_parsed: SocketAddr = bind_addr
        .parse()
        .with_context(|| format!("failed to parse bind address: {}", bind_addr))?;

    let auth_mode_parsed: AuthMode = auth_mode
        .parse()
        .map_err(|e: String| anyhow::anyhow!("invalid auth mode: {}", e))?;

    let config = ServerConfig {
        bind_addr: bind_addr_parsed,
        store_dsn,
        auth_mode: auth_mode_parsed,
        bearer_token,
        allow_insecure_nonlocal_bind,
        log_filter,
        log_format: log_format_parsed,
        store_synchronous,
        store_wal_autocheckpoint,
        rate_limit_per_second,
        rate_limit_burst,
        write_queue_threshold,
    };

    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("configuration error: {}", e))?;

    Ok(config)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Resolve configuration with precedence
    let config = resolve_config(&args)?;

    // Set up logging
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_filter));

    match config.log_format {
        ferrum_gateway::LogFormat::Json => {
            fmt()
                .with_env_filter(env_filter)
                .with_target(false)
                .json()
                .init();
        }
        ferrum_gateway::LogFormat::Text => {
            fmt().with_env_filter(env_filter).with_target(false).init();
        }
    }

    tracing::info!(
        "starting ferrumd with config: auth_mode={}, bind_addr={}, store_dsn={}, log_format={}",
        config.auth_mode,
        config.bind_addr,
        config.store_dsn,
        config.log_format
    );

    let pdp = Arc::new(StaticPdpEngine);
    let cap = Arc::new(InMemoryCapabilityService::default());

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    register_git_adapter(&mut registry);
    register_fs_adapter(&mut registry);
    let mut rollback_service = RollbackService::new(Arc::new(registry));
    rollback_service.register_planner(Arc::new(PlannableFsAdapter));
    let rollback = Arc::new(rollback_service);

    let store: Arc<dyn StoreFacade> = if config.store_dsn.to_lowercase().starts_with("postgres://")
        || config.store_dsn.to_lowercase().starts_with("postgresql://")
    {
        #[cfg(feature = "postgres")]
        {
            let pg_store = PostgresStore::connect(&config.store_dsn)
                .await
                .context("failed to connect to postgres")?;
            pg_store
                .apply_embedded_migrations()
                .await
                .context("failed to apply postgres migrations")?;
            Arc::new(pg_store) as Arc<dyn StoreFacade>
        }
        #[cfg(not(feature = "postgres"))]
        {
            // Unreachable in practice because validate_store_dsn rejects postgres DSNs
            // when the feature is not enabled.
            return Err(anyhow::anyhow!(
                "PostgreSQL support is not enabled. Build with --features postgres to enable it."
            ));
        }
    } else {
        let wal_tuning = SqliteWalTuning {
            synchronous: config.store_synchronous.clone(),
            wal_autocheckpoint: config.store_wal_autocheckpoint,
        };
        let sqlite_store = SqliteStore::connect_with_tuning(&config.store_dsn, wal_tuning)
            .await
            .context("failed to connect to sqlite")?;
        sqlite_store
            .apply_embedded_migrations()
            .await
            .context("failed to apply migrations")?;
        Arc::new(sqlite_store) as Arc<dyn StoreFacade>
    };

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store, vec![]);
    run_http_server(config, runtime).await
}

#[cfg(test)]
mod tests {
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
"#,
        );

        unsafe {
            std::env::set_var("FERRUMD_BIND_ADDR", "127.0.0.1:2222");
            std::env::set_var("FERRUMD_STORE_DSN", "sqlite://from-env.db");
            std::env::set_var("FERRUMD_LOG_FILTER", "debug");
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
        };

        let config = resolve_config(&args).unwrap();

        assert_eq!(config.bind_addr, "127.0.0.1:3333".parse().unwrap());
        assert_eq!(config.store_dsn, "sqlite://from-env.db");
        assert_eq!(config.log_filter, "debug");

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
        };

        let error = resolve_config(&args).err().expect("expected config error");
        assert!(
            error
                .to_string()
                .contains("write_queue_threshold must be between 1 and 10000")
        );

        let _ = fs::remove_file(path);
    }
}
