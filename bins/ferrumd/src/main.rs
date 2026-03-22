use anyhow::{Context, Result};
use clap::Parser;
use ferrum_adapter_fs::FsRollbackAdapter;
use ferrum_adapter_git::register_git_adapter;
use ferrum_adapter_http::register_http_adapter;
use ferrum_adapter_maildraft::MaildraftAdapter;
use ferrum_adapter_sqlite::SqliteRollbackAdapter;
use ferrum_cap::SqliteCapabilityService;
use ferrum_firewall::DefaultFirewall;
use ferrum_gateway::{AuthMode, GatewayConfig, GatewayRuntime, ServerConfig, run_http_server};
use ferrum_pdp::StaticPdpEngine;
use ferrum_rollback::{AdapterRegistry, NoopRollbackAdapter, RollbackService};
use ferrum_store::SqliteStore;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEV_CONFIG_PATH: &str = "configs/ferrumgate.dev.toml";
const DEFAULT_STORE_DSN: &str = "sqlite::memory:?cache=shared";

#[derive(Debug, Clone, Parser)]
#[command(name = "ferrumd")]
#[command(about = "FerrumGate daemon")]
struct Args {
    /// Path to the config file. If not provided, defaults to:
    /// - $FERRUMD_CONFIG env var value, if FERRUMD_CONFIG is set
    /// - configs/ferrumgate.dev.toml in repo cwd, if it exists
    /// - built-in defaults
    #[arg(long)]
    config: Option<PathBuf>,

    /// Override the bind address (host:port).
    #[arg(long)]
    bind: Option<String>,

    /// Override the store DSN (e.g. sqlite://path or sqlite::memory:?cache=shared).
    #[arg(long)]
    store_dsn: Option<String>,

    /// Auth mode: 'disabled' or 'bearer'.
    #[arg(long, value_parser = ["disabled", "bearer"])]
    auth_mode: Option<String>,

    /// Bearer token for control-plane auth (required when auth_mode is 'bearer').
    #[arg(long)]
    bearer_token: Option<String>,

    /// Allow binding to non-loopback addresses with auth disabled.
    /// Without this flag, binding to non-loopback with auth disabled will fail startup.
    #[arg(long)]
    allow_insecure_nonlocal: bool,

    /// Log filter override (e.g. debug,info,ferrum_gateway=debug).
    /// If not provided, falls back to RUST_LOG env var or config file value.
    #[arg(long)]
    log_filter: Option<String>,

    /// Print the resolved effective runtime config and exit.
    #[arg(long)]
    print_effective_config: bool,

    /// Validate startup guard and exit without starting the server.
    #[arg(long)]
    check_startup_guard: bool,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    bind_addr: SocketAddr,
    store_dsn: String,
    auth_mode: AuthMode,
    bearer_token: Option<String>,
    allow_insecure_nonlocal: bool,
    log_filter: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ConfigSource {
    Cli,
    Env,
    File,
    Default,
    AutoDevConfig,
}

#[derive(Debug, Clone, Serialize)]
struct StartupGuardDiagnostic {
    ok: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct EffectiveRuntimeConfig {
    config_path: Option<String>,
    config_path_source: Option<ConfigSource>,
    bind_addr: String,
    bind_addr_source: ConfigSource,
    loopback_bind: bool,
    store_dsn_display: String,
    store_dsn_source: ConfigSource,
    store_is_persistent: bool,
    auth_mode: String,
    auth_mode_source: ConfigSource,
    bearer_token_present: bool,
    bearer_token_source: Option<ConfigSource>,
    allow_insecure_nonlocal: bool,
    allow_insecure_nonlocal_source: ConfigSource,
    log_filter: Option<String>,
    log_filter_source: Option<ConfigSource>,
    startup_guard: StartupGuardDiagnostic,
}

#[derive(Debug, Clone)]
struct ResolvedRuntimeConfig {
    runtime: RuntimeConfig,
    effective: EffectiveRuntimeConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    server: Option<ServerSection>,
    #[serde(default)]
    store: Option<StoreSection>,
    #[serde(default)]
    auth: Option<AuthSection>,
    #[serde(default)]
    log_filter: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ServerSection {
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    allow_insecure_nonlocal: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct StoreSection {
    #[serde(default)]
    dsn: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct AuthSection {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    bearer_token: Option<String>,
}

fn resolve_config_from_args(args: &Args) -> Result<ResolvedRuntimeConfig> {
    // Determine config file path
    let (config_path, config_path_source) = if let Some(p) = &args.config {
        (Some(p.clone()), Some(ConfigSource::Cli))
    } else if let Ok(env_path) = std::env::var("FERRUMD_CONFIG") {
        (Some(PathBuf::from(env_path)), Some(ConfigSource::Env))
    } else {
        // Auto-load dev config if it exists in repo cwd
        let dev_path = PathBuf::from(DEV_CONFIG_PATH);
        if dev_path.exists() {
            (Some(dev_path), Some(ConfigSource::AutoDevConfig))
        } else {
            (None, None)
        }
    };

    // Load config file if found
    let file_config: FileConfig = if let Some(path) = &config_path {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse config file {}", path.display()))?
    } else {
        FileConfig::default()
    };

    // Build server config from file sections (use owned values to avoid borrow issues)
    let server: ServerSection = file_config.server.clone().unwrap_or_default();
    let auth: AuthSection = file_config.auth.clone().unwrap_or_default();

    let env_bind = std::env::var("FERRUMD_BIND_ADDR").ok();
    let env_store_dsn = std::env::var("FERRUMD_STORE_DSN").ok();
    let env_auth_mode = std::env::var("FERRUMD_AUTH_MODE").ok();
    let env_bearer_token = std::env::var("FERRUMD_BEARER_TOKEN").ok();
    let env_allow_insecure_nonlocal = std::env::var("FERRUMD_ALLOW_INSECURE_NONLOCAL")
        .ok()
        .map(|value| parse_env_bool("FERRUMD_ALLOW_INSECURE_NONLOCAL", &value))
        .transpose()?;
    let env_log_filter = std::env::var("FERRUMD_LOG_FILTER").ok();

    // Resolve bind address: CLI > env > file > default
    let (bind_addr, bind_addr_source) = if let Some(bind_str) = &args.bind {
        (
            bind_str
                .parse::<SocketAddr>()
                .context("failed to parse bind address")?,
            ConfigSource::Cli,
        )
    } else if let Some(bind_str) = env_bind {
        (
            bind_str
                .parse::<SocketAddr>()
                .context("failed to parse bind address from FERRUMD_BIND_ADDR")?,
            ConfigSource::Env,
        )
    } else if let (Some(host), Some(port)) = (&server.host, server.port) {
        let addr_str = format!("{}:{}", host, port);
        (
            addr_str
                .parse::<SocketAddr>()
                .context("failed to parse bind address from config")?,
            ConfigSource::File,
        )
    } else {
        (
            DEFAULT_BIND_ADDR
                .parse::<SocketAddr>()
                .context("failed to parse default bind address")?,
            ConfigSource::Default,
        )
    };

    // Resolve store DSN: CLI > env > file > default (memory)
    let (store_dsn, store_dsn_source) = if let Some(value) = args.store_dsn.clone() {
        (value, ConfigSource::Cli)
    } else if let Some(value) = env_store_dsn {
        (value, ConfigSource::Env)
    } else if let Some(value) = file_config.store.as_ref().and_then(|s| s.dsn.clone()) {
        (value, ConfigSource::File)
    } else {
        (DEFAULT_STORE_DSN.to_string(), ConfigSource::Default)
    };

    // Resolve auth mode: CLI > env > file > default (disabled)
    let (auth_mode_raw, auth_mode_source) = if let Some(value) = args.auth_mode.clone() {
        (value, ConfigSource::Cli)
    } else if let Some(value) = env_auth_mode {
        (value, ConfigSource::Env)
    } else if let Some(value) = auth.mode.clone() {
        (value, ConfigSource::File)
    } else {
        ("disabled".to_string(), ConfigSource::Default)
    };
    let auth_mode: AuthMode = auth_mode_raw.parse::<AuthMode>().map_err(|_| {
        anyhow::anyhow!(
            "invalid auth_mode '{}', must be 'disabled' or 'bearer'",
            auth_mode_raw
        )
    })?;

    // Resolve bearer token: CLI > env > file > default (none)
    let (bearer_token, bearer_token_source) = if let Some(value) = args.bearer_token.clone() {
        (Some(value), Some(ConfigSource::Cli))
    } else if let Some(value) = env_bearer_token {
        (Some(value), Some(ConfigSource::Env))
    } else if let Some(value) = auth.bearer_token.clone() {
        (Some(value), Some(ConfigSource::File))
    } else {
        (None, None)
    };

    // Resolve allow_insecure_nonlocal: CLI > env > file > default (false)
    let (allow_insecure_nonlocal, allow_insecure_nonlocal_source) = if args.allow_insecure_nonlocal
    {
        (true, ConfigSource::Cli)
    } else if let Some(value) = env_allow_insecure_nonlocal {
        (value, ConfigSource::Env)
    } else if let Some(value) = server.allow_insecure_nonlocal {
        (value, ConfigSource::File)
    } else {
        (false, ConfigSource::Default)
    };

    // Resolve log filter: CLI > env > file > default
    let (log_filter, log_filter_source) = if let Some(value) = args.log_filter.clone() {
        (Some(value), Some(ConfigSource::Cli))
    } else if let Some(value) = env_log_filter {
        (Some(value), Some(ConfigSource::Env))
    } else if let Some(value) = file_config.log_filter.clone() {
        (Some(value), Some(ConfigSource::File))
    } else {
        (None, None)
    };

    let runtime = RuntimeConfig {
        bind_addr,
        store_dsn,
        auth_mode,
        bearer_token,
        allow_insecure_nonlocal,
        log_filter,
    };
    let startup_guard = startup_guard_diagnostic(&runtime);

    Ok(ResolvedRuntimeConfig {
        effective: EffectiveRuntimeConfig {
            config_path: config_path.as_ref().map(|path| path.display().to_string()),
            config_path_source,
            bind_addr: runtime.bind_addr.to_string(),
            bind_addr_source,
            loopback_bind: runtime.bind_addr.ip().is_loopback(),
            store_dsn_display: display_store_dsn(&runtime.store_dsn),
            store_dsn_source,
            store_is_persistent: !runtime.store_dsn.contains(":memory:"),
            auth_mode: match runtime.auth_mode {
                AuthMode::Disabled => "disabled".to_string(),
                AuthMode::Bearer => "bearer".to_string(),
            },
            auth_mode_source,
            bearer_token_present: runtime
                .bearer_token
                .as_deref()
                .map(|token| !token.is_empty())
                .unwrap_or(false),
            bearer_token_source,
            allow_insecure_nonlocal: runtime.allow_insecure_nonlocal,
            allow_insecure_nonlocal_source,
            log_filter: runtime.log_filter.clone(),
            log_filter_source,
            startup_guard,
        },
        runtime,
    })
}

fn display_store_dsn(store_dsn: &str) -> String {
    if store_dsn.starts_with("sqlite:") {
        store_dsn.to_string()
    } else {
        "[redacted-non-sqlite-dsn]".to_string()
    }
}

fn parse_env_bool(name: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!(
            "invalid boolean value '{}' for {} (expected true/false, 1/0, yes/no, on/off)",
            value,
            name
        ),
    }
}

fn apply_log_filter(config: &RuntimeConfig) {
    let filter = config
        .log_filter
        .clone()
        .unwrap_or_else(|| "info".to_string());

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&filter));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();
}

fn startup_guard_diagnostic(config: &RuntimeConfig) -> StartupGuardDiagnostic {
    let is_loopback = config.bind_addr.ip().is_loopback();

    if !is_loopback && !config.allow_insecure_nonlocal && config.auth_mode == AuthMode::Disabled {
        return StartupGuardDiagnostic {
            ok: false,
            message: format!(
                "refusing to bind to non-loopback address {} with auth disabled. Either use loopback (127.x.x.x), enable --allow-insecure-nonlocal, or set auth_mode to 'bearer'",
                config.bind_addr
            ),
        };
    }

    if config.auth_mode == AuthMode::Bearer {
        let token = config.bearer_token.as_deref().unwrap_or("");
        if token.is_empty() {
            return StartupGuardDiagnostic {
                ok: false,
                message: "auth_mode is 'bearer' but bearer_token is not set. Provide --bearer-token or set auth.bearer_token in config.".to_string(),
            };
        }
    }

    StartupGuardDiagnostic {
        ok: true,
        message: "startup guard passed".to_string(),
    }
}

fn validate_startup_guard(config: &RuntimeConfig) -> Result<()> {
    let diagnostic = startup_guard_diagnostic(config);
    if !diagnostic.ok {
        anyhow::bail!(diagnostic.message);
    }

    Ok(())
}

fn print_effective_config(config: &EffectiveRuntimeConfig) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(config)?);
    Ok(())
}

fn run_startup_guard_check(config: &EffectiveRuntimeConfig) -> Result<()> {
    if config.startup_guard.ok {
        println!("startup_guard: ok");
        return Ok(());
    }

    anyhow::bail!("startup_guard: {}", config.startup_guard.message)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let resolved = resolve_config_from_args(&args)?;

    if args.print_effective_config {
        print_effective_config(&resolved.effective)?;
        if !args.check_startup_guard {
            return Ok(());
        }
    }

    if args.check_startup_guard {
        return run_startup_guard_check(&resolved.effective);
    }

    let config = resolved.runtime;

    // Apply log filter before any tracing calls
    apply_log_filter(&config);

    // Validate startup guard
    validate_startup_guard(&config)?;

    let pdp = Arc::new(StaticPdpEngine);

    let mut registry = AdapterRegistry::default();
    registry.register(Arc::new(NoopRollbackAdapter::new("noop")));
    registry.register(Arc::new(FsRollbackAdapter::new("fs")));
    register_git_adapter(&mut registry);
    registry.register(Arc::new(SqliteRollbackAdapter::new("sqlite")));
    registry.register(Arc::new(MaildraftAdapter::new("maildraft")));
    register_http_adapter(&mut registry);
    let rollback = Arc::new(RollbackService::new(Arc::new(registry)));

    let store = Arc::new(SqliteStore::connect(&config.store_dsn).await?);
    store.apply_embedded_migrations().await?;

    // Reconcile legacy split-brain: any capability that is Active but already has
    // execution history is transitioned to Used. Fail-closed if reconciliation errors.
    let reconciled = store
        .reconcile_capabilities_with_executions()
        .await
        .context("capability reconciliation failed")?;
    if reconciled > 0 {
        tracing::info!(
            "reconciled {} capability rows with split-brain state",
            reconciled
        );
    }

    let cap: Arc<dyn ferrum_cap::CapabilityService> =
        Arc::new(SqliteCapabilityService::new(Arc::new(store.capabilities())));

    let firewall = Arc::new(DefaultFirewall::new());

    let runtime = GatewayRuntime::new(pdp, cap, rollback, store, firewall);

    // Build server config for HTTP server
    let gateway_config = GatewayConfig {
        bind_addr: config.bind_addr,
    };

    let server_config = ServerConfig {
        auth_mode: config.auth_mode,
        bearer_token: config.bearer_token,
    };

    tracing::info!(
        "ferrumd starting: bind={}, store={}, auth={:?}",
        config.bind_addr,
        config.store_dsn,
        config.auth_mode
    );

    run_http_server(gateway_config, runtime, server_config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(bind_addr: &str, auth_mode: AuthMode) -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: bind_addr.parse().unwrap(),
            store_dsn: DEFAULT_STORE_DSN.to_string(),
            auth_mode,
            bearer_token: None,
            allow_insecure_nonlocal: false,
            log_filter: None,
        }
    }

    #[test]
    fn parse_env_bool_accepts_common_true_values() {
        for value in ["1", "true", "TRUE", "yes", "on"] {
            assert!(parse_env_bool("TEST_BOOL", value).unwrap());
        }
    }

    #[test]
    fn parse_env_bool_accepts_common_false_values() {
        for value in ["0", "false", "FALSE", "no", "off"] {
            assert!(!parse_env_bool("TEST_BOOL", value).unwrap());
        }
    }

    #[test]
    fn validate_startup_guard_allows_loopback_without_auth() {
        let config = make_config("127.0.0.1:8080", AuthMode::Disabled);
        validate_startup_guard(&config).unwrap();
    }

    #[test]
    fn validate_startup_guard_rejects_nonloopback_without_auth() {
        let config = make_config("0.0.0.0:8080", AuthMode::Disabled);
        let error = validate_startup_guard(&config).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("refusing to bind to non-loopback address")
        );
    }

    #[test]
    fn validate_startup_guard_allows_nonloopback_with_override() {
        let mut config = make_config("0.0.0.0:8080", AuthMode::Disabled);
        config.allow_insecure_nonlocal = true;
        validate_startup_guard(&config).unwrap();
    }

    #[test]
    fn validate_startup_guard_requires_bearer_token() {
        let config = make_config("127.0.0.1:8080", AuthMode::Bearer);
        let error = validate_startup_guard(&config).unwrap_err();
        assert!(error.to_string().contains("bearer_token is not set"));
    }

    #[test]
    fn startup_guard_diagnostic_reports_failure_reason() {
        let config = make_config("0.0.0.0:8080", AuthMode::Disabled);
        let diagnostic = startup_guard_diagnostic(&config);

        assert!(!diagnostic.ok);
        assert!(
            diagnostic
                .message
                .contains("refusing to bind to non-loopback address")
        );
    }

    #[test]
    fn display_store_dsn_redacts_non_sqlite_values() {
        assert_eq!(
            display_store_dsn("postgres://user:pass@example.com/db"),
            "[redacted-non-sqlite-dsn]"
        );
        assert_eq!(
            display_store_dsn("sqlite://./ferrumgate.db"),
            "sqlite://./ferrumgate.db"
        );
    }
}
