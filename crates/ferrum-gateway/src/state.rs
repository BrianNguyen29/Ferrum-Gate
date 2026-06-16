use ferrum_cap::CapabilityService;
use ferrum_firewall::TaintScoringFirewall;
use ferrum_pdp::PdpEngine;
use ferrum_rollback::RollbackService;
use ferrum_store::{LifecycleReconciliationReport, StoreFacade};
use ferrum_sync::RuntimeBridge;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Re-export the canonical `AppState` so that extracted handler modules
/// (e.g. `capabilities`, `monitoring`, `policy_eval`) can reference it via
/// `crate::state::AppState` instead of taking a direct dependency on
/// `crate::server`.
pub(crate) use crate::server::AppState;

#[derive(Clone)]
pub struct GatewayRuntime {
    pub pdp: Arc<dyn PdpEngine>,
    pub cap: Arc<dyn CapabilityService>,
    pub rollback: Arc<RollbackService>,
    pub store: Arc<dyn StoreFacade>,
    pub bridges: Vec<Arc<dyn RuntimeBridge>>,
    pub firewall: Arc<TaintScoringFirewall>,
    pub lifecycle_reconciliation_report: Option<LifecycleReconciliationReport>,
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
            lifecycle_reconciliation_report: None,
        }
    }

    pub fn with_lifecycle_reconciliation_report(
        mut self,
        report: LifecycleReconciliationReport,
    ) -> Self {
        self.lifecycle_reconciliation_report = Some(report);
        self
    }
}

/// Re-export canonical `AuthMode` from `ferrum-proto` to eliminate drift.
pub use ferrum_proto::token::{AuthMode, TokenRole};

/// Static key material for offline JWT validation (Phase 4.3).
///
/// Production deployments should use asymmetric algorithms (RSA/EC/Ed)
/// and load keys from config files or environment. HS256 is supported
/// for tests only and must be explicitly enabled.
#[derive(Clone, Debug)]
pub enum KeyMaterial {
    /// HMAC secret (test-only; explicitly opt-in).
    Hmac(Vec<u8>),
    /// RSA public key PEM.
    Rsa(Vec<u8>),
    /// ECDSA public key PEM.
    Ecdsa(Vec<u8>),
    /// Ed25519 public key PEM.
    Ed(Vec<u8>),
    /// RSA public key from JWKS (base64url-encoded modulus and exponent).
    ///
    /// Supported for live JWKS fetch (Phase 4.4). Other JWK key types
    /// (EC, Ed, oct) are explicitly unsupported and will be skipped
    /// with a warning during JWKS fetch.
    RsaJwk { n: String, e: String },
}

impl KeyMaterial {
    /// Build a `jsonwebtoken::DecodingKey` from this key material.
    pub fn to_decoding_key(
        &self,
    ) -> Result<jsonwebtoken::DecodingKey, jsonwebtoken::errors::Error> {
        match self {
            KeyMaterial::Hmac(bytes) => Ok(jsonwebtoken::DecodingKey::from_secret(bytes)),
            KeyMaterial::Rsa(pem) => jsonwebtoken::DecodingKey::from_rsa_pem(pem),
            KeyMaterial::Ecdsa(pem) => jsonwebtoken::DecodingKey::from_ec_pem(pem),
            KeyMaterial::Ed(pem) => jsonwebtoken::DecodingKey::from_ed_pem(pem),
            KeyMaterial::RsaJwk { n, e } => jsonwebtoken::DecodingKey::from_rsa_components(n, e),
        }
    }
}

/// OIDC configuration for JWT validation (Phase 4.3 + 4.4).
///
/// Supports both static keys (offline validation) and live JWKS fetch
/// with lazy cache. Static keys take precedence over fetched JWKS keys.
#[derive(Clone, Debug)]
pub struct OidcConfig {
    /// Expected JWT issuer (`iss`). Must match exactly.
    pub issuer: String,
    /// Allowed audiences (`aud`). At least one must match.
    pub audiences: Vec<String>,
    /// Clock skew / leeway in seconds. Default: 30.
    pub clock_skew_secs: i64,
    /// Claim name for actor_id. Default: "sub".
    pub actor_id_claim: String,
    /// Claim name for role/group membership. Default: "groups".
    pub role_source_claim: String,
    /// Mapping from IdP group/role name to FerrumGate `TokenRole`.
    /// Unmapped roles are deny-by-default.
    pub role_mappings: HashMap<String, TokenRole>,
    /// Allowed signature algorithms. Production should restrict this to
    /// asymmetric algorithms only (e.g., RS256, ES256, EdDSA).
    pub allowed_algorithms: Vec<jsonwebtoken::Algorithm>,
    /// Static decoding keys keyed by JWT `kid`.
    /// For JWTs without `kid`, use an empty string as the key.
    pub static_keys: HashMap<String, KeyMaterial>,
    /// If true, require `email_verified` claim to be true.
    pub require_email_verified: bool,
    /// URL to fetch JWKS from. When set, static_keys may be empty.
    pub jwks_url: Option<String>,
    /// JWKS cache TTL in seconds. Default: 300.
    pub jwks_cache_ttl_secs: u64,
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            audiences: Vec::new(),
            clock_skew_secs: 30,
            actor_id_claim: "sub".to_string(),
            role_source_claim: "groups".to_string(),
            role_mappings: HashMap::new(),
            allowed_algorithms: vec![
                jsonwebtoken::Algorithm::RS256,
                jsonwebtoken::Algorithm::RS384,
                jsonwebtoken::Algorithm::RS512,
                jsonwebtoken::Algorithm::ES256,
                jsonwebtoken::Algorithm::ES384,
                jsonwebtoken::Algorithm::EdDSA,
            ],
            static_keys: HashMap::new(),
            require_email_verified: true,
            jwks_url: None,
            jwks_cache_ttl_secs: 300,
        }
    }
}

/// Lazy JWKS cache for live key fetching (Phase 4.4).
///
/// Fetches JWKS on key miss or when the cache is stale. Only RSA keys
/// are supported from JWKS; other key types are skipped with a warning.
/// Fail-closed: any fetch or parse error returns an error so that the
/// caller can reject the token.
pub struct OidcJwksCache {
    url: String,
    ttl: Duration,
    state: Mutex<JwksCacheState>,
}

struct JwksCacheState {
    keys: HashMap<String, KeyMaterial>,
    fetched_at: Option<Instant>,
}

impl OidcJwksCache {
    /// Create a new cache for the given JWKS URL and TTL.
    pub fn new(url: String, ttl_secs: u64) -> Self {
        Self {
            url,
            ttl: Duration::from_secs(ttl_secs),
            state: Mutex::new(JwksCacheState {
                keys: HashMap::new(),
                fetched_at: None,
            }),
        }
    }

    /// Look up a key by `kid`. Returns `Ok(Some(key))` on hit,
    /// `Ok(None)` if the key is not present after fetching,
    /// or `Err(String)` on fetch/parse failure.
    pub async fn get_key(&self, kid: &str) -> Result<Option<KeyMaterial>, String> {
        // Fast path: check cache while holding the lock
        {
            let state = self
                .state
                .lock()
                .map_err(|e| format!("jwks cache lock poisoned: {e}"))?;
            if let Some(key) = state.keys.get(kid) {
                if let Some(fetched_at) = state.fetched_at {
                    if fetched_at.elapsed() < self.ttl {
                        return Ok(Some(key.clone()));
                    }
                }
            }
        }

        // Cache miss or stale — fetch fresh JWKS (lock is released during I/O)
        self.fetch_and_cache().await?;

        // Re-check cache after fetch
        let state = self
            .state
            .lock()
            .map_err(|e| format!("jwks cache lock poisoned: {e}"))?;
        Ok(state.keys.get(kid).cloned())
    }

    /// Return the elapsed seconds since the last successful JWKS fetch.
    /// Returns `None` if the cache has never been populated.
    pub fn cache_age_seconds(&self) -> Option<u64> {
        let state = self.state.lock().ok()?;
        state.fetched_at.map(|t| t.elapsed().as_secs())
    }

    async fn fetch_and_cache(&self) -> Result<(), String> {
        let response = reqwest::get(&self.url)
            .await
            .map_err(|e| format!("jwks fetch failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("jwks fetch returned status {status}"));
        }

        let jwks: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("jwks parse failed: {e}"))?;

        let keys = jwks
            .get("keys")
            .and_then(|v| v.as_array())
            .ok_or("jwks response missing 'keys' array")?;

        let mut new_keys = HashMap::new();
        for key in keys {
            let kid = key
                .get("kid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            match jwk_to_key_material(key) {
                Ok(km) => {
                    new_keys.insert(kid.clone(), km);
                }
                Err(e) => {
                    tracing::warn!(error = %e, kid = %kid, "skipping unsupported jwk");
                }
            }
        }

        let mut state = self
            .state
            .lock()
            .map_err(|e| format!("jwks cache lock poisoned: {e}"))?;
        state.keys = new_keys;
        state.fetched_at = Some(Instant::now());
        tracing::debug!(key_count = state.keys.len(), "jwks cache refreshed");
        Ok(())
    }
}

/// Convert a single JWK entry to `KeyMaterial`.
///
/// Currently only RSA keys (`kty = "RSA"`) are supported.
/// Other key types return an explicit unsupported error.
pub(crate) fn jwk_to_key_material(jwk: &serde_json::Value) -> Result<KeyMaterial, String> {
    let kty = jwk
        .get("kty")
        .and_then(|v| v.as_str())
        .ok_or("jwk missing 'kty'")?;
    match kty {
        "RSA" => {
            let n = jwk
                .get("n")
                .and_then(|v| v.as_str())
                .ok_or("jwk missing 'n'")?;
            let e = jwk
                .get("e")
                .and_then(|v| v.as_str())
                .ok_or("jwk missing 'e'")?;
            // Validate that the components are well-formed by attempting to build a DecodingKey
            let _ = jsonwebtoken::DecodingKey::from_rsa_components(n, e)
                .map_err(|err| format!("invalid RSA JWK components: {err}"))?;
            Ok(KeyMaterial::RsaJwk {
                n: n.to_string(),
                e: e.to_string(),
            })
        }
        _ => Err(format!("unsupported jwk key type: {kty}")),
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
    /// PostgreSQL session statement timeout in milliseconds (`0` disables).
    /// Conservative default: 5000.
    pub pg_statement_timeout_ms: u64,
    /// PostgreSQL session idle-in-transaction timeout in milliseconds (`0` disables).
    /// Conservative default: 10000.
    pub pg_idle_in_transaction_timeout_ms: u64,
    /// Filesystem adapter workdir. Required for production-like non-loopback deployments.
    pub fs_workdir: Option<PathBuf>,
    /// Parent roots under which Git repositories may be mutated.
    pub git_repo_roots: Vec<PathBuf>,
    /// Parent roots under which SQLite database files may be mutated.
    pub sqlite_db_roots: Vec<PathBuf>,
    /// S3 adapter configuration. When present, enables the S3 adapter.
    #[cfg(feature = "s3")]
    pub s3_config: Option<ferrum_adapter_s3::S3Config>,
    /// OIDC configuration. Required when `auth_mode` is `Oidc`.
    pub oidc_config: Option<OidcConfig>,
    /// Clock skew tolerance for Agent auth timestamps in seconds.
    /// Conservative default: 30.
    pub agent_clock_skew_secs: i64,
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
            pg_statement_timeout_ms: 5000,
            pg_idle_in_transaction_timeout_ms: 10000,
            fs_workdir: None,
            git_repo_roots: Vec::new(),
            sqlite_db_roots: Vec::new(),
            #[cfg(feature = "s3")]
            s3_config: None,
            oidc_config: None,
            agent_clock_skew_secs: 30,
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
            if is_placeholder_bearer_token(token) {
                return Err(
                    "bearer token cannot use a documented placeholder value in bearer auth mode"
                        .to_string(),
                );
            }
        }

        // Scoped mode does not require a global bearer token; tokens are stored in the database.
        // However, we still validate that the store DSN is valid.

        // Check that OIDC mode has a valid configuration
        if self.auth_mode == AuthMode::Oidc {
            let oidc = self
                .oidc_config
                .as_ref()
                .ok_or("oidc config is required when auth mode is oidc".to_string())?;
            if oidc.issuer.is_empty() {
                return Err("oidc issuer cannot be empty".to_string());
            }
            if oidc.audiences.is_empty() {
                return Err("oidc audiences cannot be empty".to_string());
            }
            // Phase 4.4: static_keys may be empty if jwks_url is configured
            if oidc.static_keys.is_empty() && oidc.jwks_url.is_none() {
                return Err(
                    "oidc static_keys cannot be empty when jwks_url is not configured".to_string(),
                );
            }
            if oidc.role_mappings.is_empty() {
                return Err(
                    "oidc role_mappings cannot be empty (unmapped roles are denied by default)"
                        .to_string(),
                );
            }
            if oidc.allowed_algorithms.is_empty() {
                return Err("oidc allowed_algorithms cannot be empty".to_string());
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

        let production_like =
            !self.bind_addr.ip().is_loopback() && self.auth_mode != AuthMode::Disabled;
        if production_like
            && self
                .store_dsn
                .trim()
                .eq_ignore_ascii_case("sqlite::memory:")
        {
            return Err(
                "sqlite::memory: is not allowed for production-like non-loopback deployments"
                    .to_string(),
            );
        }
        if production_like && self.fs_workdir.is_none() {
            return Err(
                "fs_workdir is required for production-like non-loopback deployments".to_string(),
            );
        }
        if let Some(workdir) = &self.fs_workdir
            && !workdir.is_absolute()
        {
            return Err("fs_workdir must be an absolute path".to_string());
        }
        if self.git_repo_roots.iter().any(|root| !root.is_absolute()) {
            return Err("all git_repo_roots must be absolute paths".to_string());
        }
        if self.sqlite_db_roots.iter().any(|root| !root.is_absolute()) {
            return Err("all sqlite_db_roots must be absolute paths".to_string());
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

        // Validate agent clock skew
        if self.auth_mode == AuthMode::Agent && self.agent_clock_skew_secs <= 0 {
            return Err("agent_clock_skew_secs must be positive".to_string());
        }

        Ok(())
    }
}

fn is_placeholder_bearer_token(token: &str) -> bool {
    let normalized = token.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "change_me_to_a_secure_token"
            | "change_me"
            | "changeme"
            | "replace_me"
            | "replace-with-secure-token"
            | "example"
            | "example-token"
            | "test"
            | "token"
    ) || normalized.contains("change_me")
        || normalized.contains("changeme")
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
