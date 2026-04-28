use anyhow::{Context, Result};
use clap::Parser;
use ferrum_adapter_fs::{PlannableFsAdapter, register_fs_adapter};
use ferrum_adapter_git::register_git_adapter;
use ferrum_cap::InMemoryCapabilityService;
use ferrum_gateway::{AuthMode, GatewayRuntime, ServerConfig, run_http_server};
use ferrum_pdp::StaticPdpEngine;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
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
        store_synchronous,
        store_wal_autocheckpoint,
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

    fmt().with_env_filter(env_filter).with_target(false).init();

    tracing::info!(
        "starting ferrumd with config: auth_mode={}, bind_addr={}, store_dsn={}",
        config.auth_mode,
        config.bind_addr,
        config.store_dsn
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

    let wal_tuning = SqliteWalTuning {
        synchronous: config.store_synchronous.clone(),
        wal_autocheckpoint: config.store_wal_autocheckpoint,
    };
    let store = Arc::new(
        SqliteStore::connect_with_tuning(&config.store_dsn, wal_tuning)
            .await
            .context("failed to connect to sqlite")?,
    );
    store
        .apply_embedded_migrations()
        .await
        .context("failed to apply migrations")?;

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store as Arc<dyn StoreFacade>, vec![]);
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
        };

        let error = resolve_config(&args).err().expect("expected config error");
        assert!(error.to_string().contains("bearer token cannot be empty"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_resolve_config_rejects_postgres_dsn() {
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
        };

        let error = resolve_config(&args).err().expect("expected config error");
        assert!(
            error.to_string().contains("PostgreSQL is not implemented"),
            "expected PostgreSQL not implemented error, got: {}",
            error
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_resolve_config_rejects_postgresql_dsn() {
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
        };

        let error = resolve_config(&args).err().expect("expected config error");
        assert!(
            error.to_string().contains("PostgreSQL is not implemented"),
            "expected PostgreSQL not implemented error, got: {}",
            error
        );

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
        };

        let error = resolve_config(&args).err().expect("expected config error");
        assert!(
            error.to_string().contains("MySQL is not implemented"),
            "expected MySQL not implemented error, got: {}",
            error
        );

        let _ = fs::remove_file(path);
    }
}
