use anyhow::{Context, Result};
use clap::Parser;
#[cfg(feature = "s3")]
use ferrum_adapter_s3::S3Config;
use ferrum_gateway::{AuthMode, ServerConfig};
use std::net::SocketAddr;
use std::path::PathBuf;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_STORE_DSN: &str = "sqlite::memory:";
const DEFAULT_LOG_FILTER: &str = "info";
const AUTO_CONFIG_FILE: &str = "configs/ferrumgate.dev.toml";

#[derive(Debug, Parser)]
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

    /// Log format: "text" or "json" (default "text").
    #[arg(long)]
    log_format: Option<String>,

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
    log_format: Option<String>,
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
    #[cfg(feature = "s3")]
    #[serde(default)]
    s3_config: Option<S3ConfigSection>,
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

#[cfg(feature = "s3")]
fn default_s3_max_object_size() -> u64 {
    100 * 1024 * 1024
}

#[cfg(feature = "s3")]
fn default_s3_require_versioning() -> bool {
    true
}

#[cfg(feature = "s3")]
fn default_s3_region() -> String {
    "us-east-1".to_string()
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

    let fs_workdir = get_env("FERRUMD_FS_WORKDIR")?
        .or_else(|| server.as_ref().and_then(|s| s.fs_workdir.clone()));
    let git_repo_roots = get_env_path_list("FERRUMD_GIT_REPO_ROOTS")?
        .or_else(|| server.as_ref().map(|s| s.git_repo_roots.clone()))
        .unwrap_or_default();
    let sqlite_db_roots = get_env_path_list("FERRUMD_SQLITE_DB_ROOTS")?
        .or_else(|| server.as_ref().map(|s| s.sqlite_db_roots.clone()))
        .unwrap_or_default();

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
        store_synchronous,
        store_wal_autocheckpoint,
        rate_limit_per_second,
        rate_limit_burst,
        write_queue_threshold,
        pg_max_connections,
        pg_min_idle,
        pg_acquire_timeout_secs,
        pg_statement_timeout_ms,
        pg_idle_in_transaction_timeout_ms,
        fs_workdir,
        git_repo_roots,
        sqlite_db_roots,
        #[cfg(feature = "s3")]
        s3_config,
        oidc_config,
        agent_clock_skew_secs: 30,
        lifecycle_reconciliation_enabled,
        lifecycle_reconciliation_interval_secs,
        lifecycle_reconciliation_batch_limit,
    };

    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("configuration error: {}", e))?;

    Ok(config)
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
        };

        let error = resolve_config(&args).err().expect("expected config error");
        assert!(
            error
                .to_string()
                .contains("lifecycle_reconciliation_batch_limit must be at least 1")
        );

        let _ = fs::remove_file(path);
    }
}
