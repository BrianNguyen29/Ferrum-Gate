use anyhow::{Context, Result};
use clap::Parser;
#[cfg(feature = "gcs")]
use ferrum_adapter_gcs::GcsConfig;
use ferrum_adapter_http::HttpEgressConfig;
#[cfg(feature = "s3")]
use ferrum_adapter_s3::S3Config;
use ferrum_gateway::{AuthMode, ServerConfig};
use ipnet::IpNet;
use std::net::SocketAddr;
use std::path::PathBuf;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_STORE_DSN: &str = "sqlite::memory:";
const DEFAULT_LOG_FILTER: &str = "info";
const AUTO_CONFIG_FILE: &str = "configs/ferrumgate.dev.toml";

#[derive(Debug, Default, Parser)]
#[command(name = "ferrumd")]
#[command(about = "FerrumGate daemon")]
pub struct Args {
    /// Path to configuration file.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Bind address.
    #[arg(long)]
    bind_addr: Option<String>,

    /// Store DSN.
    #[arg(long)]
    store_dsn: Option<String>,

    /// Auth mode: "disabled", "bearer", "scoped", "oidc", or "agent".
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

    /// Trusted proxy CIDR ranges (comma-separated) whose single `X-Real-IP`
    /// header may be honored. `X-Forwarded-For` is ignored. Defaults to empty
    /// (trust-none). Universal CIDRs (0.0.0.0/0, ::/0) are rejected.
    #[arg(long)]
    trusted_proxy_cidrs: Option<String>,

    /// Pre-auth rate limit: sustained requests per second per source.
    /// When omitted, inherits rate_limit_per_second.
    #[arg(long)]
    pre_auth_rate_limit_per_second: Option<u64>,

    /// Pre-auth rate limit: burst size per source.
    /// When omitted, inherits rate_limit_burst.
    #[arg(long)]
    pre_auth_rate_limit_burst: Option<u32>,

    /// Log format: "text" or "json" (default "text").
    #[arg(long)]
    log_format: Option<String>,

    /// PDP mode: "static", "bundles", or "dual" (default "dual").
    #[arg(long)]
    pdp_mode: Option<String>,

    /// Write queue depth threshold for deep readiness probe (1..=10000).
    #[arg(long)]
    write_queue_threshold: Option<u64>,

    /// PostgreSQL pool max_connections (conservative default: 10).
    #[arg(long)]
    pg_max_connections: Option<u32>,

    /// PostgreSQL pool min_idle (conservative default: 2).
    #[arg(long)]
    pg_min_idle: Option<u32>,

    /// PostgreSQL pool acquire_timeout in seconds (conservative default: 5).
    #[arg(long)]
    pg_acquire_timeout_secs: Option<u64>,

    /// PostgreSQL session statement timeout in milliseconds (0 disables, default: 5000).
    #[arg(long)]
    pg_statement_timeout_ms: Option<u64>,

    /// PostgreSQL session idle-in-transaction timeout in milliseconds (0 disables, default: 10000).
    #[arg(long)]
    pg_idle_in_transaction_timeout_ms: Option<u64>,

    /// Enable periodic background lifecycle outbox reconciliation (default: false).
    #[arg(long)]
    lifecycle_reconciliation_enabled: bool,

    /// Interval between periodic reconciliation runs in seconds (default: 60).
    #[arg(long)]
    lifecycle_reconciliation_interval_secs: Option<u64>,

    /// Maximum outbox records to reconcile per periodic batch (default: 1000).
    #[arg(long)]
    lifecycle_reconciliation_batch_limit: Option<u32>,

    /// Enable periodic background approval timeout reconciliation (default: false).
    #[arg(long)]
    approval_timeout_enabled: bool,

    /// Maximum age in seconds before a pending approval is expired by the
    /// background reconciler (default: 3600, min: 60, max: 86400).
    #[arg(long)]
    approval_timeout_seconds: Option<u64>,

    /// Interval in seconds between approval timeout reconciliation runs
    /// (default: 300, min: 5, max: 86400).
    #[arg(long)]
    approval_reconciliation_interval_secs: Option<u64>,

    /// Enable periodic background quarantine hold timeout reconciliation (default: false).
    #[arg(long)]
    quarantine_timeout_enabled: bool,

    /// Maximum age in seconds before a pending quarantine hold is expired by the
    /// background reconciler (default: 86400, min: 60, max: 604800).
    #[arg(long)]
    quarantine_timeout_seconds: Option<u64>,

    /// Interval in seconds between quarantine hold timeout reconciliation runs
    /// (default: 300, min: 5, max: 86400).
    #[arg(long)]
    quarantine_reconciliation_interval_secs: Option<u64>,

    /// Enable periodic background HA reconciler for stale in-flight executions
    /// (default: false).
    #[arg(long)]
    ha_reconciler_enabled: bool,

    /// Interval in seconds between HA reconciler runs
    /// (default: 60, min: 5, max: 3600).
    #[arg(long)]
    ha_reconciler_interval_secs: Option<u64>,

    /// Staleness threshold in seconds before an in-flight execution is
    /// reconciled by the HA reconciler
    /// (default: 1800, min: 60, max: 86400).
    #[arg(long)]
    ha_reconciler_stale_threshold_secs: Option<u64>,

    /// Maximum number of stale executions to reconcile per HA reconciler pass
    /// (default: 100, min: 1, max: 10000).
    #[arg(long)]
    ha_reconciler_batch_size: Option<u32>,

    /// When true, audit append failures block the action and return 503 (default: false).
    #[arg(long)]
    audit_fail_closed: bool,

    /// When true, approval resolve requires a second factor (MFA) (default: false).
    #[arg(long)]
    approval_mfa_required: bool,

    /// MFA secret key for encrypting TOTP secrets.
    /// When set, TOTP credential storage is enabled.
    #[arg(long)]
    mfa_secret_key: Option<String>,

    /// TOTP issuer name displayed in authenticator apps (default: "FerrumGate").
    #[arg(long)]
    mfa_totp_issuer: Option<String>,

    /// Maximum consecutive failed MFA attempts before locking a factor (default: 5).
    #[arg(long)]
    mfa_lockout_max_attempts: Option<u32>,

    /// Duration in seconds to lock a factor after exceeding max attempts (default: 900).
    #[arg(long)]
    mfa_lockout_duration_secs: Option<u64>,

    /// Nonce cache backend: "auto", "memory", or "postgres" (default: "auto").
    #[arg(long)]
    nonce_cache_backend: Option<String>,

    /// Nonce cache TTL in seconds. 0 derives from agent_clock_skew_secs*2 (min 60).
    #[arg(long)]
    nonce_cache_ttl_secs: Option<u64>,

    /// Maximum entries for the in-memory nonce cache (default: 10000).
    #[arg(long)]
    nonce_cache_max_entries: Option<usize>,

    /// Enable behavioral anomaly detection (Phase 1 V1, default: false).
    #[arg(long)]
    behavioral_anomaly_enabled: bool,

    /// Rolling window in seconds for behavioral anomaly detection (default: 60).
    #[arg(long)]
    behavioral_anomaly_window_secs: Option<u64>,

    /// High-risk/R3 proposals in the window that trigger a warning (default: 5).
    #[arg(long)]
    behavioral_anomaly_warning_threshold: Option<u32>,

    /// High-risk/R3 proposals in the window that trigger a critical finding (default: 10).
    #[arg(long)]
    behavioral_anomaly_critical_threshold: Option<u32>,

    /// Maximum number of principals tracked in memory by the behavioral profiler (default: 1000).
    #[arg(long)]
    behavioral_anomaly_max_actors: Option<usize>,

    /// Enable the WORM-compatible audit bundle sink (default: false).
    #[arg(long)]
    audit_worm_sink_enabled: bool,

    /// WORM sink target bucket.
    #[arg(long)]
    audit_worm_sink_bucket: Option<String>,

    /// WORM sink key prefix (default: "audit-worm").
    #[arg(long)]
    audit_worm_sink_prefix: Option<String>,

    /// WORM sink Object Lock mode: "governance" or "compliance" (default: "governance").
    #[arg(long)]
    audit_worm_sink_object_lock_mode: Option<String>,

    /// WORM sink retention period in days (default: 30).
    #[arg(long)]
    audit_worm_sink_retention_days: Option<u32>,

    /// Apply a legal hold to WORM sink objects (default: false).
    #[arg(long)]
    audit_worm_sink_legal_hold: bool,

    /// Interval between WORM sink export attempts in seconds (default: 300, min: 60).
    #[arg(long)]
    audit_worm_sink_export_interval_secs: Option<u64>,

    /// Maximum audit entries per WORM sink bundle (default: 1000, range: 1..=100000).
    #[arg(long)]
    audit_worm_sink_batch_limit: Option<u32>,

    /// Enable live S3 SDK calls for the WORM sink (default: false).
    #[arg(long)]
    audit_worm_sink_live: bool,

    /// Optional custom S3 endpoint for the WORM sink (e.g., http://localhost:9000).
    #[arg(long)]
    audit_worm_sink_endpoint_url: Option<String>,

    /// AWS region for the WORM sink (default: "us-east-1").
    #[arg(long)]
    audit_worm_sink_region: Option<String>,

    /// Optional static access key ID for the WORM sink.
    #[arg(long)]
    audit_worm_sink_access_key_id: Option<String>,

    /// Optional static secret access key for the WORM sink.
    #[arg(long)]
    audit_worm_sink_secret_access_key: Option<String>,

    /// Comma-separated list of exact allowed HTTP egress host names.
    /// When empty or omitted, the HTTP adapter and planner are not registered.
    #[arg(long)]
    http_egress_allowed_hosts: Option<String>,

    /// Enable live GCS SDK calls for the GCS adapter (default: false).
    #[arg(long)]
    gcs_live: bool,

    /// OIDC token profile: "legacy_jwt" (default) or "rfc9068_access_token".
    /// "legacy_jwt" accepts any signed JWT typ. "rfc9068_access_token" requires
    /// the typ header to be exactly "at+jwt" or "application/at+jwt".
    #[arg(long)]
    oidc_token_profile: Option<String>,
}

pub fn get_env<T>(key: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(value) => value
            .parse()
            .map(Some)
            .map_err(|e| anyhow::anyhow!("invalid {key}: {e}")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("failed to read {key}: {e}")),
    }
}

pub fn get_env_path_list(key: &str) -> Result<Option<Vec<PathBuf>>> {
    let value = match std::env::var(key) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(e) => return Err(anyhow::anyhow!("failed to read {key}: {e}")),
    };
    let paths = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    Ok(Some(paths))
}

/// Parse a comma-separated list of CIDR ranges into typed [`IpNet`] values.
/// Empty entries are ignored; any structurally invalid CIDR is rejected here at
/// the config/startup boundary so the core consumer stays fully typed.
fn parse_cidr_list(raw: &str) -> Result<Vec<IpNet>> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.parse::<IpNet>()
                .map_err(|e| anyhow::anyhow!("invalid CIDR '{item}': {e}"))
        })
        .collect()
}

fn parse_allowed_hosts(raw: &str) -> Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| Ok(item.to_string()))
        .collect()
}

pub fn redact_dsn_for_log(dsn: &str) -> String {
    let Some((scheme, rest)) = dsn.split_once("://") else {
        return dsn.to_string();
    };
    let mut rest = rest.to_string();
    if let Some(at) = rest.find('@') {
        rest.replace_range(..at, "<redacted>");
    }
    if let Some(query) = rest.find('?') {
        rest.replace_range(query + 1.., "<redacted>");
    }
    format!("{scheme}://{rest}")
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ConfigFile {
    #[serde(default)]
    server: Option<ServerSection>,
    #[serde(default)]
    oidc: Option<OidcSection>,
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
    trusted_proxy_cidrs: Vec<String>,
    #[serde(default)]
    pre_auth_rate_limit_per_second: Option<u64>,
    #[serde(default)]
    pre_auth_rate_limit_burst: Option<u32>,
    #[serde(default)]
    log_format: Option<String>,
    #[serde(default)]
    pdp_mode: Option<String>,
    #[serde(default)]
    write_queue_threshold: Option<u64>,
    #[serde(default)]
    pg_max_connections: Option<u32>,
    #[serde(default)]
    pg_min_idle: Option<u32>,
    #[serde(default)]
    pg_acquire_timeout_secs: Option<u64>,
    #[serde(default)]
    pg_statement_timeout_ms: Option<u64>,
    #[serde(default)]
    pg_idle_in_transaction_timeout_ms: Option<u64>,
    #[serde(default)]
    fs_workdir: Option<PathBuf>,
    #[serde(default)]
    git_repo_roots: Vec<PathBuf>,
    #[serde(default)]
    sqlite_db_roots: Vec<PathBuf>,
    #[serde(default)]
    lifecycle_reconciliation_enabled: Option<bool>,
    #[serde(default)]
    lifecycle_reconciliation_interval_secs: Option<u64>,
    #[serde(default)]
    lifecycle_reconciliation_batch_limit: Option<u32>,
    #[serde(default)]
    approval_timeout_enabled: Option<bool>,
    #[serde(default)]
    approval_timeout_seconds: Option<u64>,
    #[serde(default)]
    approval_reconciliation_interval_secs: Option<u64>,
    #[serde(default)]
    quarantine_timeout_enabled: Option<bool>,
    #[serde(default)]
    quarantine_timeout_seconds: Option<u64>,
    #[serde(default)]
    quarantine_reconciliation_interval_secs: Option<u64>,
    #[serde(default)]
    ha_reconciler_enabled: Option<bool>,
    #[serde(default)]
    ha_reconciler_interval_secs: Option<u64>,
    #[serde(default)]
    ha_reconciler_stale_threshold_secs: Option<u64>,
    #[serde(default)]
    ha_reconciler_batch_size: Option<u32>,
    #[serde(default)]
    audit_fail_closed: Option<bool>,
    #[serde(default)]
    approval_mfa_required: Option<bool>,
    #[serde(default)]
    mfa_secret_key: Option<String>,
    #[serde(default = "default_mfa_totp_issuer")]
    mfa_totp_issuer: String,
    #[serde(default = "default_mfa_lockout_max_attempts")]
    mfa_lockout_max_attempts: u32,
    #[serde(default = "default_mfa_lockout_duration_secs")]
    mfa_lockout_duration_secs: u64,
    #[serde(default)]
    nonce_cache_backend: Option<String>,
    #[serde(default)]
    nonce_cache_ttl_secs: Option<u64>,
    #[serde(default)]
    nonce_cache_max_entries: Option<usize>,
    #[serde(default)]
    behavioral_anomaly_enabled: Option<bool>,
    #[serde(default)]
    behavioral_anomaly_window_secs: Option<u64>,
    #[serde(default)]
    behavioral_anomaly_warning_threshold: Option<u32>,
    #[serde(default)]
    behavioral_anomaly_critical_threshold: Option<u32>,
    #[serde(default)]
    behavioral_anomaly_max_actors: Option<usize>,
    #[cfg(feature = "s3")]
    #[serde(default)]
    s3_config: Option<S3ConfigSection>,
    #[cfg(feature = "gcs")]
    #[serde(default)]
    gcs_config: Option<GcsConfigSection>,
    #[serde(default)]
    http_egress: Option<HttpEgressSection>,
    #[cfg(feature = "worm-sink")]
    #[serde(default)]
    audit_worm_sink_enabled: Option<bool>,
    #[cfg(feature = "worm-sink")]
    #[serde(default)]
    audit_worm_sink: Option<WormSinkSection>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HttpEgressSection {
    #[serde(default)]
    allowed_hosts: Vec<String>,
}

#[cfg(feature = "worm-sink")]
#[derive(Debug, Clone, serde::Deserialize)]
struct WormSinkSection {
    #[serde(default)]
    #[allow(dead_code)]
    enabled: bool,
    #[serde(default)]
    bucket: Option<String>,
    #[serde(default = "default_worm_sink_prefix")]
    prefix: String,
    #[serde(default = "default_worm_sink_object_lock_mode")]
    object_lock_mode: String,
    #[serde(default = "default_worm_sink_retention_days")]
    retention_days: u32,
    #[serde(default)]
    legal_hold: bool,
    #[serde(default = "default_worm_sink_export_interval_secs")]
    export_interval_secs: u64,
    #[serde(default = "default_worm_sink_batch_limit")]
    batch_limit: u32,
    #[serde(default)]
    live: bool,
    #[serde(default)]
    endpoint_url: Option<String>,
    #[serde(default = "default_s3_region")]
    region: String,
    #[serde(default)]
    access_key_id: Option<String>,
    #[serde(default)]
    secret_access_key: Option<String>,
}

#[cfg(feature = "worm-sink")]
fn default_worm_sink_prefix() -> String {
    "audit-worm".to_string()
}

#[cfg(feature = "worm-sink")]
fn default_worm_sink_object_lock_mode() -> String {
    "governance".to_string()
}

#[cfg(feature = "worm-sink")]
fn default_worm_sink_retention_days() -> u32 {
    30
}

#[cfg(feature = "worm-sink")]
fn default_worm_sink_export_interval_secs() -> u64 {
    300
}

#[cfg(feature = "worm-sink")]
fn default_worm_sink_batch_limit() -> u32 {
    1000
}

#[cfg(feature = "s3")]
#[derive(Debug, Clone, serde::Deserialize)]
struct S3ConfigSection {
    allowed_bucket: String,
    #[serde(default = "default_s3_max_object_size")]
    max_object_size: u64,
    #[serde(default = "default_s3_require_versioning")]
    require_versioning: bool,
    #[serde(default)]
    endpoint_url: Option<String>,
    #[serde(default = "default_s3_region")]
    region: String,
    #[serde(default)]
    access_key_id: Option<String>,
    #[serde(default)]
    secret_access_key: Option<String>,
}

#[cfg(feature = "gcs")]
#[derive(Debug, Clone, serde::Deserialize)]
struct GcsConfigSection {
    allowed_bucket: String,
    #[serde(default = "default_gcs_max_object_size")]
    max_object_size: u64,
    #[serde(default)]
    live: bool,
    #[serde(default)]
    endpoint_url: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    credentials_path: Option<String>,
}

#[cfg(feature = "gcs")]
fn default_gcs_max_object_size() -> u64 {
    100 * 1024 * 1024
}

#[cfg(feature = "s3")]
fn default_s3_max_object_size() -> u64 {
    100 * 1024 * 1024
}

#[cfg(feature = "s3")]
fn default_s3_require_versioning() -> bool {
    true
}

#[cfg(any(feature = "s3", feature = "worm-sink"))]
fn default_s3_region() -> String {
    "us-east-1".to_string()
}

fn default_mfa_totp_issuer() -> String {
    "FerrumGate".to_string()
}

fn default_mfa_lockout_max_attempts() -> u32 {
    5
}

fn default_mfa_lockout_duration_secs() -> u64 {
    900
}

fn default_nonce_cache_ttl_secs() -> u64 {
    0
}

fn default_nonce_cache_max_entries() -> usize {
    10_000
}

#[derive(Debug, Clone, serde::Deserialize)]
struct OidcSection {
    issuer: String,
    audiences: Vec<String>,
    #[serde(default = "default_jwks_cache_ttl")]
    jwks_cache_ttl_secs: u64,
    #[serde(default = "default_actor_id_claim")]
    actor_id_claim: String,
    #[serde(default = "default_role_source_claim")]
    role_source_claim: String,
    #[serde(default)]
    require_email_verified: bool,
    #[serde(default)]
    allowed_algorithms: Vec<String>,
    #[serde(default)]
    role_mappings: std::collections::HashMap<String, String>,
    #[serde(default)]
    jwks_url: Option<String>,
    #[serde(default)]
    static_keys: Vec<StaticKeyEntry>,
    #[serde(default = "default_oidc_token_profile")]
    token_profile: String,
}

fn default_jwks_cache_ttl() -> u64 {
    300
}

fn default_actor_id_claim() -> String {
    "sub".to_string()
}

fn default_role_source_claim() -> String {
    "groups".to_string()
}

fn default_oidc_token_profile() -> String {
    "legacy_jwt".to_string()
}

#[derive(Debug, Clone, serde::Deserialize)]
struct StaticKeyEntry {
    kid: String,
    #[serde(rename = "type")]
    key_type: String,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    pem: Option<String>,
}

fn parse_oidc_algorithm(s: &str) -> Result<jsonwebtoken::Algorithm, String> {
    match s.to_ascii_uppercase().as_str() {
        "HS256" => Ok(jsonwebtoken::Algorithm::HS256),
        "HS384" => Ok(jsonwebtoken::Algorithm::HS384),
        "HS512" => Ok(jsonwebtoken::Algorithm::HS512),
        "RS256" => Ok(jsonwebtoken::Algorithm::RS256),
        "RS384" => Ok(jsonwebtoken::Algorithm::RS384),
        "RS512" => Ok(jsonwebtoken::Algorithm::RS512),
        "ES256" => Ok(jsonwebtoken::Algorithm::ES256),
        "ES384" => Ok(jsonwebtoken::Algorithm::ES384),
        "EDDSA" | "ED25519" => Ok(jsonwebtoken::Algorithm::EdDSA),
        _ => Err(format!("unknown jwt algorithm: {s}")),
    }
}

fn load_config_file(path: &PathBuf) -> Result<ConfigFile> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    let config: ConfigFile = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file: {}", path.display()))?;
    Ok(config)
}

pub fn resolve_config(args: &Args) -> Result<ServerConfig> {
    // Try to load config file if specified
    let file_config = if let Some(ref config_path) = args.config {
        Some(load_config_file(config_path)?)
    } else if let Some(config_path) = get_env::<PathBuf>("FERRUMD_CONFIG")? {
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
        .or(get_env("FERRUMD_BIND_ADDR")?)
        .or_else(|| server.as_ref().and_then(|s| s.bind_addr.clone()))
        .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string());

    let store_dsn = args
        .store_dsn
        .clone()
        .or(get_env("FERRUMD_STORE_DSN")?)
        .or_else(|| server.as_ref().and_then(|s| s.store_dsn.clone()))
        .unwrap_or_else(|| DEFAULT_STORE_DSN.to_string());

    let auth_mode = args
        .auth_mode
        .clone()
        .or(get_env("FERRUMD_AUTH_MODE")?)
        .or_else(|| server.as_ref().and_then(|s| s.auth_mode.clone()))
        .unwrap_or_else(|| "disabled".to_string());

    let bearer_token = args
        .bearer_token
        .clone()
        .or(get_env("FERRUMD_BEARER_TOKEN")?)
        .or_else(|| server.as_ref().and_then(|s| s.bearer_token.clone()));

    let allow_insecure_nonlocal_bind = if args.allow_insecure_nonlocal_bind {
        true
    } else {
        get_env::<bool>("FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND")?
            .or_else(|| server.as_ref().and_then(|s| s.allow_insecure_nonlocal_bind))
            .unwrap_or(false)
    };

    let log_filter = args
        .log_filter
        .clone()
        .or(get_env("FERRUMD_LOG_FILTER")?)
        .or_else(|| server.as_ref().and_then(|s| s.log_filter.clone()))
        .unwrap_or_else(|| DEFAULT_LOG_FILTER.to_string());

    let log_format = args
        .log_format
        .clone()
        .or(get_env("FERRUMD_LOG_FORMAT")?)
        .or_else(|| server.as_ref().and_then(|s| s.log_format.clone()))
        .unwrap_or_else(|| "text".to_string());

    let log_format_parsed: ferrum_gateway::LogFormat = log_format
        .parse()
        .map_err(|e: String| anyhow::anyhow!("invalid log format: {}", e))?;

    let pdp_mode = args
        .pdp_mode
        .clone()
        .or(get_env("FERRUMD_PDP_MODE")?)
        .or_else(|| server.as_ref().and_then(|s| s.pdp_mode.clone()))
        .unwrap_or_else(|| "dual".to_string());

    let pdp_mode_parsed: ferrum_gateway::PdpMode = pdp_mode
        .parse()
        .map_err(|e: String| anyhow::anyhow!("invalid pdp mode: {}", e))?;

    let store_synchronous = args
        .store_synchronous
        .clone()
        .or(get_env("FERRUMD_STORE_SYNCHRONOUS")?)
        .or_else(|| server.as_ref().and_then(|s| s.store_synchronous.clone()));

    let store_wal_autocheckpoint = args
        .store_wal_autocheckpoint
        .or(get_env("FERRUMD_STORE_WAL_AUTOCHECKPOINT")?)
        .or_else(|| server.as_ref().and_then(|s| s.store_wal_autocheckpoint));

    let rate_limit_per_second = args
        .rate_limit_per_second
        .or(get_env("FERRUMD_RATE_LIMIT_PER_SECOND")?)
        .or_else(|| server.as_ref().and_then(|s| s.rate_limit_per_second))
        .unwrap_or(2);

    let rate_limit_burst = args
        .rate_limit_burst
        .or(get_env("FERRUMD_RATE_LIMIT_BURST")?)
        .or_else(|| server.as_ref().and_then(|s| s.rate_limit_burst))
        .unwrap_or(50);

    // Trusted proxy CIDRs: CLI > env > config file > default (empty/trust-none).
    // Parsed and validated into typed `IpNet` values at this startup boundary.
    let trusted_proxy_cidrs: Vec<IpNet> = if let Some(cli) = args.trusted_proxy_cidrs.as_deref() {
        parse_cidr_list(cli)?
    } else if let Some(env) = get_env::<String>("FERRUMD_TRUSTED_PROXY_CIDRS")? {
        parse_cidr_list(&env)?
    } else if let Some(file) = server.as_ref() {
        file.trusted_proxy_cidrs
            .iter()
            .map(|cidr| {
                cidr.parse::<IpNet>()
                    .map_err(|e| anyhow::anyhow!("invalid trusted_proxy_cidrs entry '{cidr}': {e}"))
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    // Pre-auth rate limits stay optional so that, when omitted, the effective
    // values inherit `rate_limit_*` (see ServerConfig accessors).
    let pre_auth_rate_limit_per_second = args
        .pre_auth_rate_limit_per_second
        .or(get_env("FERRUMD_PRE_AUTH_RATE_LIMIT_PER_SECOND")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.pre_auth_rate_limit_per_second)
        });

    let pre_auth_rate_limit_burst = args
        .pre_auth_rate_limit_burst
        .or(get_env("FERRUMD_PRE_AUTH_RATE_LIMIT_BURST")?)
        .or_else(|| server.as_ref().and_then(|s| s.pre_auth_rate_limit_burst));

    let write_queue_threshold = args
        .write_queue_threshold
        .or(get_env("FERRUMD_WRITE_QUEUE_THRESHOLD")?)
        .or_else(|| server.as_ref().and_then(|s| s.write_queue_threshold))
        .unwrap_or(100);

    let pg_max_connections = args
        .pg_max_connections
        .or(get_env("FERRUMD_PG_MAX_CONNECTIONS")?)
        .or_else(|| server.as_ref().and_then(|s| s.pg_max_connections))
        .unwrap_or(10);

    let pg_min_idle = args
        .pg_min_idle
        .or(get_env("FERRUMD_PG_MIN_IDLE")?)
        .or_else(|| server.as_ref().and_then(|s| s.pg_min_idle))
        .unwrap_or(2);

    let pg_acquire_timeout_secs = args
        .pg_acquire_timeout_secs
        .or(get_env("FERRUMD_PG_ACQUIRE_TIMEOUT_SECS")?)
        .or_else(|| server.as_ref().and_then(|s| s.pg_acquire_timeout_secs))
        .unwrap_or(5);

    let pg_statement_timeout_ms = args
        .pg_statement_timeout_ms
        .or(get_env("FERRUMD_PG_STATEMENT_TIMEOUT_MS")?)
        .or_else(|| server.as_ref().and_then(|s| s.pg_statement_timeout_ms))
        .unwrap_or(5000);

    let pg_idle_in_transaction_timeout_ms = args
        .pg_idle_in_transaction_timeout_ms
        .or(get_env("FERRUMD_PG_IDLE_IN_TRANSACTION_TIMEOUT_MS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.pg_idle_in_transaction_timeout_ms)
        })
        .unwrap_or(10000);

    let lifecycle_reconciliation_enabled = if args.lifecycle_reconciliation_enabled {
        true
    } else {
        get_env::<bool>("FERRUMD_LIFECYCLE_RECONCILIATION_ENABLED")?
            .or_else(|| {
                server
                    .as_ref()
                    .and_then(|s| s.lifecycle_reconciliation_enabled)
            })
            .unwrap_or(false)
    };

    let lifecycle_reconciliation_interval_secs = args
        .lifecycle_reconciliation_interval_secs
        .or(get_env("FERRUMD_LIFECYCLE_RECONCILIATION_INTERVAL_SECS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.lifecycle_reconciliation_interval_secs)
        })
        .unwrap_or(60);

    let lifecycle_reconciliation_batch_limit = args
        .lifecycle_reconciliation_batch_limit
        .or(get_env("FERRUMD_LIFECYCLE_RECONCILIATION_BATCH_LIMIT")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.lifecycle_reconciliation_batch_limit)
        })
        .unwrap_or(1000);

    let approval_timeout_enabled = if args.approval_timeout_enabled {
        true
    } else {
        get_env::<bool>("FERRUMD_APPROVAL_TIMEOUT_ENABLED")?
            .or_else(|| server.as_ref().and_then(|s| s.approval_timeout_enabled))
            .unwrap_or(false)
    };

    let approval_timeout_seconds = args
        .approval_timeout_seconds
        .or(get_env("FERRUMD_APPROVAL_TIMEOUT_SECONDS")?)
        .or_else(|| server.as_ref().and_then(|s| s.approval_timeout_seconds))
        .unwrap_or(3600);

    let approval_reconciliation_interval_secs = args
        .approval_reconciliation_interval_secs
        .or(get_env("FERRUMD_APPROVAL_RECONCILIATION_INTERVAL_SECS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.approval_reconciliation_interval_secs)
        })
        .unwrap_or(300);

    let quarantine_timeout_enabled = if args.quarantine_timeout_enabled {
        true
    } else {
        get_env::<bool>("FERRUMD_QUARANTINE_TIMEOUT_ENABLED")?
            .or_else(|| server.as_ref().and_then(|s| s.quarantine_timeout_enabled))
            .unwrap_or(false)
    };

    let quarantine_timeout_seconds = args
        .quarantine_timeout_seconds
        .or(get_env("FERRUMD_QUARANTINE_TIMEOUT_SECONDS")?)
        .or_else(|| server.as_ref().and_then(|s| s.quarantine_timeout_seconds))
        .unwrap_or(86400);

    let quarantine_reconciliation_interval_secs = args
        .quarantine_reconciliation_interval_secs
        .or(get_env("FERRUMD_QUARANTINE_RECONCILIATION_INTERVAL_SECS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.quarantine_reconciliation_interval_secs)
        })
        .unwrap_or(300);

    let ha_reconciler_enabled = if args.ha_reconciler_enabled {
        true
    } else {
        get_env::<bool>("FERRUMD_HA_RECONCILER_ENABLED")?
            .or_else(|| server.as_ref().and_then(|s| s.ha_reconciler_enabled))
            .unwrap_or(false)
    };

    let ha_reconciler_interval_secs = args
        .ha_reconciler_interval_secs
        .or(get_env("FERRUMD_HA_RECONCILER_INTERVAL_SECS")?)
        .or_else(|| server.as_ref().and_then(|s| s.ha_reconciler_interval_secs))
        .unwrap_or(60);

    let ha_reconciler_stale_threshold_secs = args
        .ha_reconciler_stale_threshold_secs
        .or(get_env("FERRUMD_HA_RECONCILER_STALE_THRESHOLD_SECS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.ha_reconciler_stale_threshold_secs)
        })
        .unwrap_or(1800);

    let ha_reconciler_batch_size = args
        .ha_reconciler_batch_size
        .or(get_env("FERRUMD_HA_RECONCILER_BATCH_SIZE")?)
        .or_else(|| server.as_ref().and_then(|s| s.ha_reconciler_batch_size))
        .unwrap_or(100);

    let audit_fail_closed = if args.audit_fail_closed {
        true
    } else {
        get_env::<bool>("FERRUMD_AUDIT_FAIL_CLOSED")?
            .or_else(|| server.as_ref().and_then(|s| s.audit_fail_closed))
            .unwrap_or(false)
    };

    let approval_mfa_required = if args.approval_mfa_required {
        true
    } else {
        get_env::<bool>("FERRUMD_APPROVAL_MFA_REQUIRED")?
            .or_else(|| server.as_ref().and_then(|s| s.approval_mfa_required))
            .unwrap_or(false)
    };

    let mfa_secret_key = args
        .mfa_secret_key
        .clone()
        .or(get_env("FERRUMD_MFA_SECRET_KEY")?)
        .or_else(|| server.as_ref().and_then(|s| s.mfa_secret_key.clone()));

    let mfa_totp_issuer = args
        .mfa_totp_issuer
        .clone()
        .or(get_env("FERRUMD_MFA_TOTP_ISSUER")?)
        .or_else(|| server.as_ref().map(|s| s.mfa_totp_issuer.clone()))
        .unwrap_or_else(default_mfa_totp_issuer);

    let mfa_lockout_max_attempts = args
        .mfa_lockout_max_attempts
        .or(get_env("FERRUMD_MFA_LOCKOUT_MAX_ATTEMPTS")?)
        .or_else(|| server.as_ref().map(|s| s.mfa_lockout_max_attempts))
        .unwrap_or_else(default_mfa_lockout_max_attempts);

    let mfa_lockout_duration_secs = args
        .mfa_lockout_duration_secs
        .or(get_env("FERRUMD_MFA_LOCKOUT_DURATION_SECS")?)
        .or_else(|| server.as_ref().map(|s| s.mfa_lockout_duration_secs))
        .unwrap_or_else(default_mfa_lockout_duration_secs);

    let nonce_cache_backend = args
        .nonce_cache_backend
        .clone()
        .or(get_env("FERRUMD_NONCE_CACHE_BACKEND")?)
        .or_else(|| server.as_ref().and_then(|s| s.nonce_cache_backend.clone()))
        .unwrap_or_else(|| "auto".to_string());

    let nonce_cache_backend_parsed: ferrum_gateway::NonceCacheBackend = nonce_cache_backend
        .parse()
        .map_err(|e: String| anyhow::anyhow!("invalid nonce cache backend: {}", e))?;

    let nonce_cache_ttl_secs = args
        .nonce_cache_ttl_secs
        .or(get_env("FERRUMD_NONCE_CACHE_TTL_SECS")?)
        .or_else(|| server.as_ref().and_then(|s| s.nonce_cache_ttl_secs))
        .unwrap_or_else(default_nonce_cache_ttl_secs);

    let nonce_cache_max_entries = args
        .nonce_cache_max_entries
        .or(get_env("FERRUMD_NONCE_CACHE_MAX_ENTRIES")?)
        .or_else(|| server.as_ref().and_then(|s| s.nonce_cache_max_entries))
        .unwrap_or_else(default_nonce_cache_max_entries);

    let behavioral_anomaly_enabled = if args.behavioral_anomaly_enabled {
        true
    } else {
        get_env::<bool>("FERRUMD_BEHAVIORAL_ANOMALY_ENABLED")?
            .or_else(|| server.as_ref().and_then(|s| s.behavioral_anomaly_enabled))
            .unwrap_or(false)
    };

    let behavioral_anomaly_window_secs = args
        .behavioral_anomaly_window_secs
        .or(get_env("FERRUMD_BEHAVIORAL_ANOMALY_WINDOW_SECS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.behavioral_anomaly_window_secs)
        })
        .unwrap_or(60);

    let behavioral_anomaly_warning_threshold = args
        .behavioral_anomaly_warning_threshold
        .or(get_env("FERRUMD_BEHAVIORAL_ANOMALY_WARNING_THRESHOLD")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.behavioral_anomaly_warning_threshold)
        })
        .unwrap_or(5);

    let behavioral_anomaly_critical_threshold = args
        .behavioral_anomaly_critical_threshold
        .or(get_env("FERRUMD_BEHAVIORAL_ANOMALY_CRITICAL_THRESHOLD")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.behavioral_anomaly_critical_threshold)
        })
        .unwrap_or(10);

    let behavioral_anomaly_max_actors = args
        .behavioral_anomaly_max_actors
        .or(get_env("FERRUMD_BEHAVIORAL_ANOMALY_MAX_ACTORS")?)
        .or_else(|| {
            server
                .as_ref()
                .and_then(|s| s.behavioral_anomaly_max_actors)
        })
        .unwrap_or(1000);

    #[cfg(feature = "worm-sink")]
    let (audit_worm_sink_enabled, worm_sink_config) = {
        let enabled = if args.audit_worm_sink_enabled {
            true
        } else {
            get_env::<bool>("FERRUMD_AUDIT_WORM_SINK_ENABLED")?
                .or_else(|| server.as_ref().and_then(|s| s.audit_worm_sink_enabled))
                .unwrap_or(false)
        };

        let file_worm = server.as_ref().and_then(|s| s.audit_worm_sink.as_ref());
        let bucket = args
            .audit_worm_sink_bucket
            .clone()
            .or(get_env("FERRUMD_AUDIT_WORM_SINK_BUCKET")?)
            .or_else(|| file_worm.and_then(|w| w.bucket.clone()));

        if enabled && bucket.is_none() {
            return Err(anyhow::anyhow!(
                "audit_worm_sink_enabled is true but audit_worm_sink_bucket is not set"
            ));
        }

        let file_worm = server.as_ref().and_then(|s| s.audit_worm_sink.as_ref());

        let env_prefix = get_env::<String>("FERRUMD_AUDIT_WORM_SINK_PREFIX")?;
        let env_object_lock_mode = get_env::<String>("FERRUMD_AUDIT_WORM_SINK_OBJECT_LOCK_MODE")?;
        let raw_object_lock_mode = args
            .audit_worm_sink_object_lock_mode
            .clone()
            .or(env_object_lock_mode)
            .or_else(|| file_worm.map(|w| w.object_lock_mode.clone()))
            .unwrap_or_else(default_worm_sink_object_lock_mode);
        let parsed_object_lock_mode = raw_object_lock_mode
            .parse::<ferrum_adapter_s3::ObjectLockMode>()
            .map_err(|e| anyhow::anyhow!("invalid audit_worm_sink object_lock_mode: {e}"))?;
        let env_retention_days = get_env::<u32>("FERRUMD_AUDIT_WORM_SINK_RETENTION_DAYS")?;
        let env_legal_hold = get_env::<bool>("FERRUMD_AUDIT_WORM_SINK_LEGAL_HOLD")?;
        let env_export_interval_secs =
            get_env::<u64>("FERRUMD_AUDIT_WORM_SINK_EXPORT_INTERVAL_SECS")?;
        let env_batch_limit = get_env::<u32>("FERRUMD_AUDIT_WORM_SINK_BATCH_LIMIT")?;
        let env_live = get_env::<bool>("FERRUMD_AUDIT_WORM_SINK_LIVE")?;
        let env_endpoint_url = get_env::<String>("FERRUMD_AUDIT_WORM_SINK_ENDPOINT_URL")?;
        let env_region = get_env::<String>("FERRUMD_AUDIT_WORM_SINK_REGION")?;
        let env_access_key_id = get_env::<String>("FERRUMD_AUDIT_WORM_SINK_ACCESS_KEY_ID")?;
        let env_secret_access_key = get_env::<String>("FERRUMD_AUDIT_WORM_SINK_SECRET_ACCESS_KEY")?;

        let cfg = bucket.map(|bucket| ferrum_gateway::WormSinkConfig {
            bucket,
            prefix: args
                .audit_worm_sink_prefix
                .clone()
                .or(env_prefix)
                .or_else(|| file_worm.map(|w| w.prefix.clone()))
                .unwrap_or_else(default_worm_sink_prefix),
            object_lock_mode: parsed_object_lock_mode,
            retention_days: args
                .audit_worm_sink_retention_days
                .or(env_retention_days)
                .or_else(|| file_worm.map(|w| w.retention_days))
                .unwrap_or_else(default_worm_sink_retention_days),
            legal_hold: if args.audit_worm_sink_legal_hold {
                true
            } else {
                env_legal_hold
                    .or_else(|| file_worm.map(|w| w.legal_hold))
                    .unwrap_or(false)
            },
            export_interval_secs: args
                .audit_worm_sink_export_interval_secs
                .or(env_export_interval_secs)
                .or_else(|| file_worm.map(|w| w.export_interval_secs))
                .unwrap_or_else(default_worm_sink_export_interval_secs),
            batch_limit: args
                .audit_worm_sink_batch_limit
                .or(env_batch_limit)
                .or_else(|| file_worm.map(|w| w.batch_limit))
                .unwrap_or_else(default_worm_sink_batch_limit),
            live: if args.audit_worm_sink_live {
                true
            } else {
                env_live
                    .or_else(|| file_worm.map(|w| w.live))
                    .unwrap_or(false)
            },
            endpoint_url: args
                .audit_worm_sink_endpoint_url
                .clone()
                .or(env_endpoint_url)
                .or_else(|| file_worm.and_then(|w| w.endpoint_url.clone())),
            region: args
                .audit_worm_sink_region
                .clone()
                .or(env_region)
                .or_else(|| file_worm.map(|w| w.region.clone()))
                .unwrap_or_else(default_s3_region),
            access_key_id: args
                .audit_worm_sink_access_key_id
                .clone()
                .or(env_access_key_id)
                .or_else(|| file_worm.and_then(|w| w.access_key_id.clone())),
            secret_access_key: args
                .audit_worm_sink_secret_access_key
                .clone()
                .or(env_secret_access_key)
                .or_else(|| file_worm.and_then(|w| w.secret_access_key.clone())),
        });

        (enabled, cfg)
    };

    let fs_workdir = get_env("FERRUMD_FS_WORKDIR")?
        .or_else(|| server.as_ref().and_then(|s| s.fs_workdir.clone()));
    let git_repo_roots = get_env_path_list("FERRUMD_GIT_REPO_ROOTS")?
        .or_else(|| server.as_ref().map(|s| s.git_repo_roots.clone()))
        .unwrap_or_default();
    let sqlite_db_roots = get_env_path_list("FERRUMD_SQLITE_DB_ROOTS")?
        .or_else(|| server.as_ref().map(|s| s.sqlite_db_roots.clone()))
        .unwrap_or_default();

    let http_egress_allowed_hosts = if let Some(cli) = args.http_egress_allowed_hosts.as_deref() {
        Some(parse_allowed_hosts(cli)?)
    } else if let Some(env) = get_env::<String>("FERRUMD_HTTP_EGRESS_ALLOWED_HOSTS")? {
        let hosts = parse_allowed_hosts(&env)?;
        if hosts.is_empty() { None } else { Some(hosts) }
    } else if let Some(file) = server.as_ref().and_then(|s| s.http_egress.as_ref()) {
        let hosts = file.allowed_hosts.clone();
        if hosts.is_empty() { None } else { Some(hosts) }
    } else {
        None
    };

    let http_egress = if let Some(hosts) = http_egress_allowed_hosts {
        let cfg = HttpEgressConfig::from_hosts(hosts)
            .map_err(|e| anyhow::anyhow!("invalid http_egress.allowed_hosts: {e}"))?;
        if cfg.is_empty() { None } else { Some(cfg) }
    } else {
        None
    };

    #[cfg(feature = "s3")]
    let s3_config = {
        let file_s3 = server.as_ref().and_then(|s| s.s3_config.as_ref());
        let allowed_bucket = get_env::<String>("FERRUMD_S3_ALLOWED_BUCKET")?
            .or_else(|| file_s3.map(|c| c.allowed_bucket.clone()));
        if let Some(bucket) = allowed_bucket {
            let endpoint_url = get_env::<String>("FERRUMD_S3_ENDPOINT_URL")?
                .or_else(|| file_s3.and_then(|c| c.endpoint_url.clone()));
            let region = get_env::<String>("FERRUMD_S3_REGION")?
                .or_else(|| file_s3.map(|c| c.region.clone()))
                .unwrap_or_else(|| "us-east-1".to_string());
            let access_key_id = get_env::<String>("FERRUMD_S3_ACCESS_KEY_ID")?
                .or_else(|| file_s3.and_then(|c| c.access_key_id.clone()));
            let secret_access_key = get_env::<String>("FERRUMD_S3_SECRET_ACCESS_KEY")?
                .or_else(|| file_s3.and_then(|c| c.secret_access_key.clone()));
            let max_object_size = file_s3
                .map(|c| c.max_object_size)
                .unwrap_or(100 * 1024 * 1024);
            let require_versioning = file_s3.map(|c| c.require_versioning).unwrap_or(true);
            Some(S3Config {
                allowed_bucket: bucket,
                max_object_size,
                require_versioning,
                endpoint_url,
                region,
                live: true,
                access_key_id,
                secret_access_key,
            })
        } else {
            None
        }
    };
    #[cfg(not(feature = "s3"))]
    let _s3_config: Option<ferrum_adapter_s3::S3Config> = None;

    #[cfg(feature = "gcs")]
    let gcs_config = {
        let file_gcs = server.as_ref().and_then(|s| s.gcs_config.as_ref());
        let allowed_bucket = get_env::<String>("FERRUMD_GCS_ALLOWED_BUCKET")?
            .or_else(|| file_gcs.map(|c| c.allowed_bucket.clone()));
        if let Some(bucket) = allowed_bucket {
            let endpoint_url = get_env::<String>("FERRUMD_GCS_ENDPOINT_URL")?
                .or_else(|| file_gcs.and_then(|c| c.endpoint_url.clone()));
            let project_id = get_env::<String>("FERRUMD_GCS_PROJECT_ID")?
                .or_else(|| file_gcs.and_then(|c| c.project_id.clone()));
            let credentials_path = get_env::<String>("FERRUMD_GCS_CREDENTIALS_PATH")?
                .or_else(|| file_gcs.and_then(|c| c.credentials_path.clone()));
            let max_object_size = file_gcs
                .map(|c| c.max_object_size)
                .unwrap_or(100 * 1024 * 1024);
            let live = if args.gcs_live {
                true
            } else {
                get_env::<bool>("FERRUMD_GCS_LIVE")?
                    .or_else(|| file_gcs.map(|c| c.live))
                    .unwrap_or(false)
            };
            Some(GcsConfig {
                allowed_bucket: bucket,
                max_object_size,
                live,
                endpoint_url,
                project_id,
                credentials_path,
            })
        } else {
            None
        }
    };

    let bind_addr_parsed: SocketAddr = bind_addr
        .parse()
        .with_context(|| format!("failed to parse bind address: {}", bind_addr))?;

    let auth_mode_parsed: AuthMode = auth_mode
        .parse()
        .map_err(|e: String| anyhow::anyhow!("invalid auth mode: {}", e))?;

    // Build OIDC config from file + env overrides
    let oidc_config = if auth_mode_parsed == AuthMode::Oidc {
        let file_oidc = file_config.as_ref().and_then(|c| c.oidc.as_ref());

        let issuer = get_env::<String>("FERRUMD_OIDC_ISSUER")?
            .or_else(|| file_oidc.map(|o| o.issuer.clone()))
            .unwrap_or_default();

        let audiences_env = get_env::<String>("FERRUMD_OIDC_AUDIENCES")?;
        let audiences = if let Some(aud_str) = audiences_env {
            aud_str.split(',').map(|s| s.trim().to_string()).collect()
        } else {
            file_oidc.map(|o| o.audiences.clone()).unwrap_or_default()
        };

        let jwks_url = get_env::<String>("FERRUMD_OIDC_JWKS_URL")?
            .or_else(|| file_oidc.and_then(|o| o.jwks_url.clone()));

        let jwks_cache_ttl_secs = get_env::<u64>("FERRUMD_OIDC_JWKS_CACHE_TTL_SECS")?
            .or_else(|| file_oidc.map(|o| o.jwks_cache_ttl_secs))
            .unwrap_or(300);

        let token_profile = args
            .oidc_token_profile
            .clone()
            .or(get_env::<String>("FERRUMD_OIDC_TOKEN_PROFILE")?)
            .or_else(|| file_oidc.map(|o| o.token_profile.clone()))
            .unwrap_or_else(|| "legacy_jwt".to_string());
        let token_profile_parsed: ferrum_gateway::OidcTokenProfile = token_profile
            .parse()
            .map_err(|e: String| anyhow::anyhow!("invalid OIDC token profile: {e}"))?;

        let actor_id_claim = get_env::<String>("FERRUMD_OIDC_ACTOR_ID_CLAIM")?
            .or_else(|| file_oidc.map(|o| o.actor_id_claim.clone()))
            .unwrap_or_else(|| "sub".to_string());

        let role_source_claim = get_env::<String>("FERRUMD_OIDC_ROLE_SOURCE_CLAIM")?
            .or_else(|| file_oidc.map(|o| o.role_source_claim.clone()))
            .unwrap_or_else(|| "groups".to_string());

        let require_email_verified = get_env::<bool>("FERRUMD_OIDC_REQUIRE_EMAIL_VERIFIED")?
            .or_else(|| file_oidc.map(|o| o.require_email_verified))
            .unwrap_or(true);

        let algorithms_env = get_env::<String>("FERRUMD_OIDC_ALLOWED_ALGORITHMS")?;
        let allowed_algorithms: Vec<jsonwebtoken::Algorithm> = if let Some(alg_str) = algorithms_env
        {
            alg_str
                .split(',')
                .map(|s| parse_oidc_algorithm(s.trim()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow::anyhow!("invalid FERRUMD_OIDC_ALLOWED_ALGORITHMS: {e}"))?
        } else if let Some(oidc) = file_oidc {
            if oidc.allowed_algorithms.is_empty() {
                vec![
                    jsonwebtoken::Algorithm::RS256,
                    jsonwebtoken::Algorithm::RS384,
                    jsonwebtoken::Algorithm::RS512,
                    jsonwebtoken::Algorithm::ES256,
                    jsonwebtoken::Algorithm::ES384,
                    jsonwebtoken::Algorithm::EdDSA,
                ]
            } else {
                oidc.allowed_algorithms
                    .iter()
                    .map(|s| parse_oidc_algorithm(s))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| anyhow::anyhow!("invalid allowed_algorithms in config: {e}"))?
            }
        } else {
            vec![
                jsonwebtoken::Algorithm::RS256,
                jsonwebtoken::Algorithm::RS384,
                jsonwebtoken::Algorithm::RS512,
                jsonwebtoken::Algorithm::ES256,
                jsonwebtoken::Algorithm::ES384,
                jsonwebtoken::Algorithm::EdDSA,
            ]
        };

        let role_mappings_env = get_env::<String>("FERRUMD_OIDC_ROLE_MAPPINGS")?;
        let role_mappings: std::collections::HashMap<String, ferrum_proto::TokenRole> =
            if let Some(rm_str) = role_mappings_env {
                let mut map = std::collections::HashMap::new();
                for pair in rm_str.split(',') {
                    let pair = pair.trim();
                    if pair.is_empty() {
                        continue;
                    }
                    let (name, role_str) = pair.split_once('=').ok_or_else(|| {
                        anyhow::anyhow!("invalid role mapping '{pair}': expected name=role")
                    })?;
                    let role = role_str
                        .trim()
                        .parse::<ferrum_proto::TokenRole>()
                        .map_err(|e| anyhow::anyhow!("invalid role in mapping '{pair}': {e}"))?;
                    map.insert(name.trim().to_string(), role);
                }
                map
            } else if let Some(oidc) = file_oidc {
                let mut map = std::collections::HashMap::new();
                for (name, role_str) in &oidc.role_mappings {
                    let role = role_str
                        .parse::<ferrum_proto::TokenRole>()
                        .map_err(|e| anyhow::anyhow!("invalid role '{role_str}' in config: {e}"))?;
                    map.insert(name.clone(), role);
                }
                map
            } else {
                std::collections::HashMap::new()
            };

        let mut static_keys = std::collections::HashMap::new();
        if let Some(oidc) = file_oidc {
            for entry in &oidc.static_keys {
                let km = match entry.key_type.as_str() {
                    "hmac" => {
                        let secret_b64 = entry.secret.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("static key '{}' missing 'secret'", entry.kid)
                        })?;
                        let bytes = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            secret_b64,
                        )
                        .map_err(|e| {
                            anyhow::anyhow!("invalid base64 secret for '{}': {e}", entry.kid)
                        })?;
                        ferrum_gateway::KeyMaterial::Hmac(bytes)
                    }
                    "rsa" => {
                        let pem = entry.pem.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("static key '{}' missing 'pem'", entry.kid)
                        })?;
                        ferrum_gateway::KeyMaterial::Rsa(pem.as_bytes().to_vec())
                    }
                    "ecdsa" => {
                        let pem = entry.pem.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("static key '{}' missing 'pem'", entry.kid)
                        })?;
                        ferrum_gateway::KeyMaterial::Ecdsa(pem.as_bytes().to_vec())
                    }
                    "ed" => {
                        let pem = entry.pem.as_deref().ok_or_else(|| {
                            anyhow::anyhow!("static key '{}' missing 'pem'", entry.kid)
                        })?;
                        ferrum_gateway::KeyMaterial::Ed(pem.as_bytes().to_vec())
                    }
                    other => {
                        return Err(anyhow::anyhow!(
                            "static key '{}' has unsupported type '{other}'",
                            entry.kid
                        ));
                    }
                };
                static_keys.insert(entry.kid.clone(), km);
            }
        }

        Some(ferrum_gateway::OidcConfig {
            issuer,
            audiences,
            clock_skew_secs: 30,
            actor_id_claim,
            role_source_claim,
            role_mappings,
            allowed_algorithms,
            static_keys,
            require_email_verified,
            jwks_url,
            jwks_cache_ttl_secs,
            token_profile: token_profile_parsed,
        })
    } else {
        None
    };

    let config = ServerConfig {
        bind_addr: bind_addr_parsed,
        store_dsn,
        auth_mode: auth_mode_parsed,
        bearer_token,
        allow_insecure_nonlocal_bind,
        log_filter,
        log_format: log_format_parsed,
        pdp_mode: pdp_mode_parsed,
        store_synchronous,
        store_wal_autocheckpoint,
        rate_limit_per_second,
        rate_limit_burst,
        trusted_proxy_cidrs,
        pre_auth_rate_limit_per_second,
        pre_auth_rate_limit_burst,
        write_queue_threshold,
        pg_max_connections,
        pg_min_idle,
        pg_acquire_timeout_secs,
        pg_statement_timeout_ms,
        pg_idle_in_transaction_timeout_ms,
        fs_workdir,
        git_repo_roots,
        sqlite_db_roots,
        http_egress,
        #[cfg(feature = "s3")]
        s3_config,
        #[cfg(feature = "gcs")]
        gcs_config,
        #[cfg(feature = "worm-sink")]
        audit_worm_sink_enabled,
        #[cfg(feature = "worm-sink")]
        worm_sink_config,
        oidc_config,
        agent_clock_skew_secs: 30,
        nonce_cache_backend: nonce_cache_backend_parsed,
        nonce_cache_ttl_secs,
        nonce_cache_max_entries,
        lifecycle_reconciliation_enabled,
        lifecycle_reconciliation_interval_secs,
        lifecycle_reconciliation_batch_limit,
        approval_timeout_enabled,
        approval_timeout_seconds,
        approval_reconciliation_interval_secs,
        quarantine_timeout_enabled,
        quarantine_timeout_seconds,
        quarantine_reconciliation_interval_secs,
        ha_reconciler_enabled,
        ha_reconciler_interval_secs,
        ha_reconciler_stale_threshold_secs,
        ha_reconciler_batch_size,
        audit_fail_closed,
        approval_mfa_required,
        mfa_secret_key,
        mfa_totp_issuer,
        mfa_lockout_max_attempts,
        mfa_lockout_duration_secs,
        behavioral_anomaly_enabled,
        behavioral_anomaly_window_secs,
        behavioral_anomaly_warning_threshold,
        behavioral_anomaly_critical_threshold,
        behavioral_anomaly_max_actors,
    };

    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("configuration error: {}", e))?;

    Ok(config)
}

#[cfg(test)]
mod config_tests;
