use ferrum_cap::CapabilityService;
use ferrum_firewall::TaintScoringFirewall;
use ferrum_pdp::PdpEngine;
use ferrum_rollback::RollbackService;
use ferrum_store::StoreFacade;
use ferrum_sync::RuntimeBridge;
use std::sync::Arc;

#[derive(Clone)]
pub struct GatewayRuntime {
    pub pdp: Arc<dyn PdpEngine>,
    pub cap: Arc<dyn CapabilityService>,
    pub rollback: Arc<RollbackService>,
    pub store: Arc<dyn StoreFacade>,
    pub bridges: Vec<Arc<dyn RuntimeBridge>>,
    pub firewall: Arc<TaintScoringFirewall>,
}

impl GatewayRuntime {
    pub fn new(
        pdp: Arc<dyn PdpEngine>,
        cap: Arc<dyn CapabilityService>,
        rollback: Arc<RollbackService>,
        store: Arc<dyn StoreFacade>,
        bridges: Vec<Arc<dyn RuntimeBridge>>,
    ) -> Self {
        Self {
            pdp,
            cap,
            rollback,
            store,
            bridges,
            firewall: Arc::new(TaintScoringFirewall::new()),
        }
    }
}

/// Authentication mode for the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthMode {
    /// No authentication required.
    #[default]
    Disabled,
    /// Bearer token authentication required.
    Bearer,
}

impl std::fmt::Display for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMode::Disabled => write!(f, "disabled"),
            AuthMode::Bearer => write!(f, "bearer"),
        }
    }
}

impl std::str::FromStr for AuthMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "disabled" => Ok(AuthMode::Disabled),
            "bearer" => Ok(AuthMode::Bearer),
            _ => Err(format!("invalid auth mode: {}", s)),
        }
    }
}

/// Log format for the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Human-readable text format (default).
    #[default]
    Text,
    /// Structured JSON format.
    Json,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogFormat::Text => write!(f, "text"),
            LogFormat::Json => write!(f, "json"),
        }
    }
}

impl std::str::FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "text" | "compact" => Ok(LogFormat::Text),
            "json" => Ok(LogFormat::Json),
            _ => Err(format!(
                "invalid log format: {} (expected 'text' or 'json')",
                s
            )),
        }
    }
}

/// Server configuration for the gateway.
#[derive(Clone)]
pub struct ServerConfig {
    /// Socket address to bind to.
    pub bind_addr: std::net::SocketAddr,
    /// Store data source name (e.g., sqlite::memory:, sqlite://foo.db).
    pub store_dsn: String,
    /// Authentication mode.
    pub auth_mode: AuthMode,
    /// Bearer token for authentication (required when auth_mode is Bearer).
    pub bearer_token: Option<String>,
    /// Allow binding to non-loopback addresses when auth is disabled.
    pub allow_insecure_nonlocal_bind: bool,
    /// Log filter (e.g., debug, info, warn).
    pub log_filter: String,
    /// Log format: "text" (human-readable) or "json" (structured).
    pub log_format: LogFormat,
    /// SQLite synchronous pragma value (off, normal, full, extra).
    pub store_synchronous: Option<String>,
    /// SQLite wal_autocheckpoint pragma value (frames between checkpoints).
    pub store_wal_autocheckpoint: Option<u32>,
    /// Rate limit: sustained requests per second per IP.
    pub rate_limit_per_second: u64,
    /// Rate limit: burst size per IP.
    pub rate_limit_burst: u32,
    /// Write queue depth threshold for deep readiness probe.
    /// Valid range: 1..=10000. Default: 100.
    pub write_queue_threshold: u64,
    /// PostgreSQL pool max_connections.
    /// Conservative default: 10.
    pub pg_max_connections: u32,
    /// PostgreSQL pool min_idle.
    /// Conservative default: 2.
    pub pg_min_idle: u32,
    /// PostgreSQL pool acquire_timeout in seconds.
    /// Conservative default: 5.
    pub pg_acquire_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            store_dsn: "sqlite::memory:".to_string(),
            auth_mode: AuthMode::Disabled,
            bearer_token: None,
            allow_insecure_nonlocal_bind: false,
            log_filter: "info".to_string(),
            log_format: LogFormat::Text,
            store_synchronous: None,
            store_wal_autocheckpoint: None,
            rate_limit_per_second: 2,
            rate_limit_burst: 50,
            write_queue_threshold: 100,
            pg_max_connections: 10,
            pg_min_idle: 2,
            pg_acquire_timeout_secs: 5,
        }
    }
}

impl ServerConfig {
    /// Validates the configuration and returns an error if invalid.
    pub fn validate(&self) -> Result<(), String> {
        // Check that bearer mode has a non-empty token
        if self.auth_mode == AuthMode::Bearer {
            let token = self.bearer_token.as_deref().unwrap_or("");
            if token.is_empty() {
                return Err("bearer token cannot be empty when auth mode is bearer".to_string());
            }
        }

        // Check that non-loopback bind is allowed when auth is disabled
        if !self.allow_insecure_nonlocal_bind
            && self.auth_mode == AuthMode::Disabled
            && !self.bind_addr.ip().is_loopback()
        {
            return Err(
                "binding to non-loopback address requires --allow-insecure-nonlocal-bind \
                 when auth is disabled"
                    .to_string(),
            );
        }

        // Validate store DSN is SQLite (PostgreSQL and MySQL not implemented)
        validate_store_dsn(&self.store_dsn)?;

        // Validate rate limit settings
        if self.rate_limit_per_second == 0 {
            return Err("rate_limit_per_second must be at least 1".to_string());
        }
        if self.rate_limit_burst == 0 {
            return Err("rate_limit_burst must be at least 1".to_string());
        }
        if self.rate_limit_burst > 10_000 {
            return Err("rate_limit_burst must be at most 10000".to_string());
        }

        // Validate write_queue_threshold range
        if !(1..=10000).contains(&self.write_queue_threshold) {
            return Err(format!(
                "write_queue_threshold must be between 1 and 10000, got {}",
                self.write_queue_threshold
            ));
        }

        // Validate PostgreSQL pool settings
        if self.pg_max_connections == 0 {
            return Err("pg_max_connections must be at least 1".to_string());
        }
        if self.pg_acquire_timeout_secs == 0 {
            return Err("pg_acquire_timeout_secs must be at least 1".to_string());
        }

        Ok(())
    }
}

/// Validates the store DSN.
///
/// PostgreSQL is accepted only when the `postgres` feature is enabled.
/// MySQL is explicitly not implemented.
/// See ADR-50 for the phased implementation plan.
fn validate_store_dsn(dsn: &str) -> Result<(), String> {
    let dsn_lower = dsn.to_lowercase();

    // Check for postgres:// or postgresql://
    #[cfg(not(feature = "postgres"))]
    if dsn_lower.starts_with("postgres://") || dsn_lower.starts_with("postgresql://") {
        return Err(
            "PostgreSQL support is not enabled. Build with --features postgres to enable it. \
             Use sqlite:// or sqlite::memory: for local development."
                .to_string(),
        );
    }

    // Check for mysql://
    if dsn_lower.starts_with("mysql://") {
        return Err(
            "MySQL is not implemented. See ADR-50 for the phased implementation plan. \
             Use sqlite:// or sqlite::memory: for local development."
                .to_string(),
        );
    }

    // Accept sqlite://, sqlite::memory:, or other SQLite variants
    // Accept postgres:// and postgresql:// only when the postgres feature is enabled
    Ok(())
}

/// Legacy gateway config for backward compatibility.
#[derive(Clone)]
pub struct GatewayConfig {
    pub bind_addr: std::net::SocketAddr,
}
