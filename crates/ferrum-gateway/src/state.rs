use ferrum_adapter_http::HttpEgressConfig;
use ferrum_cap::CapabilityService;
use ferrum_firewall::TaintScoringFirewall;
use ferrum_pdp::PdpEngine;
use ferrum_rollback::RollbackService;
use ferrum_store::{LifecycleReconciliationReport, StoreFacade};
use ferrum_sync::RuntimeBridge;
use ipnet::IpNet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::metrics::Metrics;

#[cfg(test)]
use crate::behavioral::build_profiler;
#[cfg(test)]
use ferrum_store::InMemoryNonceCache;

/// Canonical shared state for gateway handlers and background tasks.
/// Includes the runtime, server config, metrics, behavioral profiler, OIDC JWKS cache,
/// and nonce cache for Agent auth replay protection.
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) runtime: GatewayRuntime,
    pub(crate) server_config: ServerConfig,
    pub(crate) metrics: Arc<Metrics>,
    pub(crate) profiler: Arc<dyn crate::behavioral::BehavioralProfiler>,
    pub(crate) jwks_cache: Option<Arc<OidcJwksCache>>,
    /// Nonce cache for Agent auth replay protection.
    pub(crate) nonce_cache: Arc<dyn ferrum_store::NonceCache>,
}

#[cfg(test)]
impl AppState {
    /// Test-only constructor that builds an AppState from a runtime and config.
    pub(crate) fn test_new(runtime: GatewayRuntime, server_config: ServerConfig) -> Arc<AppState> {
        Arc::new(AppState {
            runtime,
            server_config: server_config.clone(),
            metrics: Arc::new(Metrics::new()),
            profiler: build_profiler(&server_config),
            jwks_cache: None,
            nonce_cache: Arc::new(InMemoryNonceCache::new(
                server_config.nonce_cache_max_entries,
            )),
        })
    }
}

#[cfg(feature = "worm-sink")]
pub use crate::worm_sink::WormSinkConfig;

#[cfg(feature = "worm-sink")]
pub use crate::worm_sink::WormSinkConfig;

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

/// Re-export the nonce cache backend selector used by server configuration.
pub use ferrum_store::NonceCacheBackend;

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

/// JWT token profile for OIDC authentication.
///
/// Controls whether the gateway enforces a strict `typ` header on incoming
/// OIDC/JWT tokens. The default is `LegacyJwt`, which preserves the existing
/// behavior of accepting any signed JWT (including tokens with a missing or
/// non-standard `typ`). The `Rfc9068AccessToken` profile opts into RFC 9068
/// access-token validation and requires the `typ` header to be exactly one of
/// the well-known values before any key or signature work is performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OidcTokenProfile {
    /// Legacy behavior: accept any signed JWT `typ` (or none). No typ enforcement.
    #[default]
    LegacyJwt,
    /// RFC 9068 access-token profile: require `typ` to be exactly `at+jwt` or
    /// `application/at+jwt`.
    Rfc9068AccessToken,
}

impl std::str::FromStr for OidcTokenProfile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "legacy_jwt" => Ok(Self::LegacyJwt),
            "rfc9068_access_token" => Ok(Self::Rfc9068AccessToken),
            _ => Err(format!("unknown OIDC token profile: {s}")),
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
    /// JWT token profile governing `typ` header validation.
    /// Default: `LegacyJwt` (no `typ` enforcement).
    pub token_profile: OidcTokenProfile,
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
            token_profile: OidcTokenProfile::default(),
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

/// PDP evaluation mode.
///
/// - `Static`: use the built-in static PDP engine only.
/// - `Bundles`: use active policy-bundle rules only; default Allow when no
///   active bundle matches.
/// - `Dual`: evaluate active bundles first, then fall back to the static PDP
///   engine (default; preserves existing behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdpMode {
    Static,
    Bundles,
    #[default]
    Dual,
}

impl std::fmt::Display for PdpMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PdpMode::Static => write!(f, "static"),
            PdpMode::Bundles => write!(f, "bundles"),
            PdpMode::Dual => write!(f, "dual"),
        }
    }
}

impl std::str::FromStr for PdpMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "static" => Ok(PdpMode::Static),
            "bundles" => Ok(PdpMode::Bundles),
            "dual" => Ok(PdpMode::Dual),
            _ => Err(format!(
                "invalid pdp mode: {} (expected 'static', 'bundles', or 'dual')",
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
    /// CIDR ranges of trusted reverse proxies / load balancers whose single
    /// `X-Real-IP` header may be honored. `X-Forwarded-For` is ignored.
    ///
    /// Parsed and validated at the config/startup boundary into typed
    /// [`IpNet`] values so the core consumer never deals with raw strings.
    /// Defaults to empty (trust-none): no proxy headers are honored unless a
    /// range is explicitly configured. Universal CIDRs (`0.0.0.0/0`, `::/0`)
    /// are rejected because they would silently trust every client.
    pub trusted_proxy_cidrs: Vec<IpNet>,
    /// Optional pre-auth (unauthenticated) rate limit: sustained requests per
    /// second per source. When `None`, inherits `rate_limit_per_second`.
    /// `Some(0)` is rejected; the pre-auth limit can never be disabled.
    pub pre_auth_rate_limit_per_second: Option<u64>,
    /// Optional pre-auth (unauthenticated) rate limit: burst size per source.
    /// When `None`, inherits `rate_limit_burst`. `Some(0)` is rejected.
    pub pre_auth_rate_limit_burst: Option<u32>,
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
    /// HTTP egress configuration. When present and non-empty, enables the HTTP adapter.
    pub http_egress: Option<HttpEgressConfig>,
    /// S3 adapter configuration. When present, enables the S3 adapter.
    #[cfg(feature = "s3")]
    pub s3_config: Option<ferrum_adapter_s3::S3Config>,
    /// GCS adapter configuration. When present, enables the GCS adapter.
    /// Requires the `gcs` feature; live SDK integration is a follow-up slice.
    #[cfg(feature = "gcs")]
    pub gcs_config: Option<ferrum_adapter_gcs::GcsConfig>,
    /// OIDC configuration. Required when `auth_mode` is `Oidc`.
    pub oidc_config: Option<OidcConfig>,
    /// Clock skew tolerance for Agent auth timestamps in seconds.
    /// Conservative default: 30.
    pub agent_clock_skew_secs: i64,
    /// Nonce cache backend selector.
    /// Default: Auto.
    pub nonce_cache_backend: NonceCacheBackend,
    /// Nonce cache TTL in seconds.
    /// When 0, derived from `agent_clock_skew_secs * 2` with a minimum of 60.
    /// Default: 0.
    pub nonce_cache_ttl_secs: u64,
    /// Maximum number of entries retained by the in-memory nonce cache.
    /// Default: 10_000.
    pub nonce_cache_max_entries: usize,
    /// Enable periodic background lifecycle outbox reconciliation.
    /// Default: false.
    pub lifecycle_reconciliation_enabled: bool,
    /// Interval between periodic lifecycle reconciliation runs in seconds.
    /// Default: 60.
    pub lifecycle_reconciliation_interval_secs: u64,
    /// PDP evaluation mode.
    /// Default: Dual.
    pub pdp_mode: PdpMode,
    /// Enable periodic background approval timeout reconciliation.
    /// Default: false.
    pub approval_timeout_enabled: bool,
    /// Maximum age in seconds before a pending approval is considered stale
    /// and transitioned to `Expired` by the background reconciler.
    /// Default: 3600. Valid range: 60..=86400.
    pub approval_timeout_seconds: u64,
    /// Interval between periodic approval timeout reconciliation runs in seconds.
    /// Default: 300. Valid range: 5..=86400.
    pub approval_reconciliation_interval_secs: u64,
    /// Enable periodic background quarantine hold timeout reconciliation.
    /// Default: false.
    pub quarantine_timeout_enabled: bool,
    /// Maximum age in seconds before a pending quarantine hold is considered stale
    /// and transitioned to `Expired` by the background reconciler.
    /// Default: 86400. Valid range: 60..=604800.
    pub quarantine_timeout_seconds: u64,
    /// Interval between periodic quarantine hold timeout reconciliation runs in seconds.
    /// Default: 300. Valid range: 5..=86400.
    pub quarantine_reconciliation_interval_secs: u64,
    /// Enable periodic background HA reconciler for stale in-flight executions.
    /// Default: false.
    pub ha_reconciler_enabled: bool,
    /// Interval between periodic HA reconciler runs in seconds.
    /// Default: 60. Valid range: 5..=3600.
    pub ha_reconciler_interval_secs: u64,
    /// Staleness threshold in seconds. An execution is considered stale when
    /// `started_at` is older than `now - threshold`.
    /// Default: 1800. Valid range: 60..=86400.
    pub ha_reconciler_stale_threshold_secs: u64,
    /// Maximum number of stale executions to reconcile per pass.
    /// Default: 100. Valid range: 1..=10000.
    pub ha_reconciler_batch_size: u32,
    /// Maximum number of outbox records to reconcile per periodic batch.
    /// Default: 1000.
    pub lifecycle_reconciliation_batch_limit: u32,
    /// When true, audit append failures block the action and return 503.
    /// Default: false (best-effort).
    pub audit_fail_closed: bool,
    /// When true, the WORM-compatible audit bundle sink is enabled.
    /// Default: false.
    #[cfg(feature = "worm-sink")]
    pub audit_worm_sink_enabled: bool,
    /// WORM sink configuration. Required when `audit_worm_sink_enabled` is true.
    #[cfg(feature = "worm-sink")]
    pub worm_sink_config: Option<WormSinkConfig>,
    /// When true, approval resolve requires a second factor (MFA).
    /// Default: false. No concrete verifier is wired yet; enabling this
    /// returns 403/mfa_required until client factor transport is implemented.
    pub approval_mfa_required: bool,
    /// MFA secret key for encrypting TOTP secrets.
    /// Loaded from `FERRUMD_MFA_SECRET_KEY` environment variable.
    /// Redacted in debug/display output.
    pub mfa_secret_key: Option<String>,
    /// TOTP issuer name displayed in authenticator apps.
    /// Default: "FerrumGate".
    pub mfa_totp_issuer: String,
    /// Maximum consecutive failed MFA attempts before locking a factor.
    /// Default: 5.
    pub mfa_lockout_max_attempts: u32,
    /// Duration in seconds to lock a factor after exceeding max attempts.
    /// Default: 900 (15 minutes).
    pub mfa_lockout_duration_secs: u64,
    /// Enable behavioral anomaly detection (Phase 1 V1).
    /// Default: false.
    pub behavioral_anomaly_enabled: bool,
    /// Rolling window in seconds for behavioral anomaly detection.
    /// Default: 60. Valid range: 1..=3600.
    pub behavioral_anomaly_window_secs: u64,
    /// Inclusive number of high-risk/R3 proposals in the window that triggers a warning.
    /// Default: 5. Valid range: 1..=1_000_000.
    pub behavioral_anomaly_warning_threshold: u32,
    /// Inclusive number of high-risk/R3 proposals in the window that triggers a critical finding.
    /// Must be greater than or equal to warning_threshold. Default: 10. Valid range: 1..=1_000_000.
    pub behavioral_anomaly_critical_threshold: u32,
    /// Maximum number of distinct principals tracked in memory by the behavioral profiler.
    /// Default: 1000. Valid range: 1..=100_000.
    pub behavioral_anomaly_max_actors: usize,
    /// Temporary compatibility deadline for owner-less legacy workflow objects in
    /// authenticated modes. When set, unbound capabilities and executions are
    /// accessible until the RFC3339 deadline; when unset or expired they are
    /// denied. Bearer/Disabled auth modes are unaffected.
    pub legacy_object_compat_allow_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("ServerConfig");
        d.field("bind_addr", &self.bind_addr);
        d.field("store_dsn", &self.store_dsn);
        d.field("auth_mode", &self.auth_mode);
        d.field(
            "bearer_token",
            &self.bearer_token.as_ref().map(|_| "<redacted>"),
        );
        d.field(
            "allow_insecure_nonlocal_bind",
            &self.allow_insecure_nonlocal_bind,
        );
        d.field("log_filter", &self.log_filter);
        d.field("log_format", &self.log_format);
        d.field("store_synchronous", &self.store_synchronous);
        d.field("store_wal_autocheckpoint", &self.store_wal_autocheckpoint);
        d.field("rate_limit_per_second", &self.rate_limit_per_second);
        d.field("rate_limit_burst", &self.rate_limit_burst);
        d.field("trusted_proxy_cidrs", &self.trusted_proxy_cidrs);
        d.field(
            "pre_auth_rate_limit_per_second",
            &self.pre_auth_rate_limit_per_second,
        );
        d.field("pre_auth_rate_limit_burst", &self.pre_auth_rate_limit_burst);
        d.field("write_queue_threshold", &self.write_queue_threshold);
        d.field("pg_max_connections", &self.pg_max_connections);
        d.field("pg_min_idle", &self.pg_min_idle);
        d.field("pg_acquire_timeout_secs", &self.pg_acquire_timeout_secs);
        d.field("pg_statement_timeout_ms", &self.pg_statement_timeout_ms);
        d.field(
            "pg_idle_in_transaction_timeout_ms",
            &self.pg_idle_in_transaction_timeout_ms,
        );
        d.field("fs_workdir", &self.fs_workdir);
        d.field("git_repo_roots", &self.git_repo_roots);
        d.field("sqlite_db_roots", &self.sqlite_db_roots);
        d.field("http_egress", &self.http_egress);
        #[cfg(feature = "s3")]
        d.field("s3_config", &self.s3_config);
        #[cfg(feature = "gcs")]
        d.field("gcs_config", &self.gcs_config);
        d.field("oidc_config", &self.oidc_config);
        d.field("agent_clock_skew_secs", &self.agent_clock_skew_secs);
        d.field("nonce_cache_backend", &self.nonce_cache_backend);
        d.field("nonce_cache_ttl_secs", &self.nonce_cache_ttl_secs);
        d.field("nonce_cache_max_entries", &self.nonce_cache_max_entries);
        d.field(
            "lifecycle_reconciliation_enabled",
            &self.lifecycle_reconciliation_enabled,
        );
        d.field(
            "lifecycle_reconciliation_interval_secs",
            &self.lifecycle_reconciliation_interval_secs,
        );
        d.field(
            "lifecycle_reconciliation_batch_limit",
            &self.lifecycle_reconciliation_batch_limit,
        );
        d.field("approval_timeout_enabled", &self.approval_timeout_enabled);
        d.field("approval_timeout_seconds", &self.approval_timeout_seconds);
        d.field(
            "approval_reconciliation_interval_secs",
            &self.approval_reconciliation_interval_secs,
        );
        d.field(
            "quarantine_timeout_enabled",
            &self.quarantine_timeout_enabled,
        );
        d.field(
            "quarantine_timeout_seconds",
            &self.quarantine_timeout_seconds,
        );
        d.field(
            "quarantine_reconciliation_interval_secs",
            &self.quarantine_reconciliation_interval_secs,
        );
        d.field("ha_reconciler_enabled", &self.ha_reconciler_enabled);
        d.field(
            "ha_reconciler_interval_secs",
            &self.ha_reconciler_interval_secs,
        );
        d.field(
            "ha_reconciler_stale_threshold_secs",
            &self.ha_reconciler_stale_threshold_secs,
        );
        d.field("ha_reconciler_batch_size", &self.ha_reconciler_batch_size);
        d.field("pdp_mode", &self.pdp_mode);
        d.field("audit_fail_closed", &self.audit_fail_closed);
        #[cfg(feature = "worm-sink")]
        d.field("audit_worm_sink_enabled", &self.audit_worm_sink_enabled);
        #[cfg(feature = "worm-sink")]
        d.field("worm_sink_config", &self.worm_sink_config);
        d.field("approval_mfa_required", &self.approval_mfa_required);
        d.field(
            "mfa_secret_key",
            &self.mfa_secret_key.as_ref().map(|_| "<redacted>"),
        );
        d.field("mfa_totp_issuer", &self.mfa_totp_issuer);
        d.field("mfa_lockout_max_attempts", &self.mfa_lockout_max_attempts);
        d.field("mfa_lockout_duration_secs", &self.mfa_lockout_duration_secs);
        d.field(
            "behavioral_anomaly_enabled",
            &self.behavioral_anomaly_enabled,
        );
        d.field(
            "behavioral_anomaly_window_secs",
            &self.behavioral_anomaly_window_secs,
        );
        d.field(
            "behavioral_anomaly_warning_threshold",
            &self.behavioral_anomaly_warning_threshold,
        );
        d.field(
            "behavioral_anomaly_critical_threshold",
            &self.behavioral_anomaly_critical_threshold,
        );
        d.field(
            "behavioral_anomaly_max_actors",
            &self.behavioral_anomaly_max_actors,
        );
        d.field(
            "legacy_object_compat_allow_until",
            &self.legacy_object_compat_allow_until,
        );
        d.finish()
    }
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
            trusted_proxy_cidrs: Vec::new(),
            pre_auth_rate_limit_per_second: None,
            pre_auth_rate_limit_burst: None,
            write_queue_threshold: 100,
            pg_max_connections: 10,
            pg_min_idle: 2,
            pg_acquire_timeout_secs: 5,
            pg_statement_timeout_ms: 5000,
            pg_idle_in_transaction_timeout_ms: 10000,
            fs_workdir: None,
            git_repo_roots: Vec::new(),
            sqlite_db_roots: Vec::new(),
            http_egress: None,
            #[cfg(feature = "s3")]
            s3_config: None,
            #[cfg(feature = "gcs")]
            gcs_config: None,
            oidc_config: None,
            agent_clock_skew_secs: 30,
            nonce_cache_backend: NonceCacheBackend::Auto,
            nonce_cache_ttl_secs: 0,
            nonce_cache_max_entries: 10_000,
            lifecycle_reconciliation_enabled: false,
            lifecycle_reconciliation_interval_secs: 60,
            lifecycle_reconciliation_batch_limit: 1000,
            pdp_mode: PdpMode::Dual,
            approval_timeout_enabled: false,
            approval_timeout_seconds: 3600,
            approval_reconciliation_interval_secs: 300,
            quarantine_timeout_enabled: false,
            quarantine_timeout_seconds: 86400,
            quarantine_reconciliation_interval_secs: 300,
            ha_reconciler_enabled: false,
            ha_reconciler_interval_secs: 60,
            ha_reconciler_stale_threshold_secs: 1800,
            ha_reconciler_batch_size: 100,
            audit_fail_closed: false,
            #[cfg(feature = "worm-sink")]
            audit_worm_sink_enabled: false,
            #[cfg(feature = "worm-sink")]
            worm_sink_config: None,
            approval_mfa_required: false,
            mfa_secret_key: None,
            mfa_totp_issuer: "FerrumGate".to_string(),
            mfa_lockout_max_attempts: 5,
            mfa_lockout_duration_secs: 900,
            behavioral_anomaly_enabled: false,
            behavioral_anomaly_window_secs: 60,
            behavioral_anomaly_warning_threshold: 5,
            behavioral_anomaly_critical_threshold: 10,
            behavioral_anomaly_max_actors: 1000,
            legacy_object_compat_allow_until: None,
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
        if let Some(http_egress) = &self.http_egress {
            http_egress
                .validate()
                .map_err(|e| format!("invalid http_egress.allowed_hosts: {e}"))?;
        }

        if production_like && !self.lifecycle_reconciliation_enabled {
            return Err(
                "lifecycle_reconciliation_enabled must be true for production-like non-loopback \
                 deployments"
                    .to_string(),
            );
        }
        if production_like && !self.approval_timeout_enabled {
            return Err(
                "approval_timeout_enabled must be true for production-like non-loopback \
                 deployments"
                    .to_string(),
            );
        }
        if production_like && !self.ha_reconciler_enabled {
            return Err(
                "ha_reconciler_enabled must be true for production-like non-loopback deployments; \
                 stale Running+Prepared side-effect pairs must be recoverable on restart"
                    .to_string(),
            );
        }
        if production_like && !self.approval_timeout_enabled {
            tracing::warn!(
                "approval_timeout_enabled is false in a production-like configuration; \
                 stale pending approvals will not be expired automatically"
            );
        }
        if production_like && !self.audit_fail_closed {
            return Err(
                "audit_fail_closed must be true for production-like non-loopback deployments"
                    .to_string(),
            );
        }

        #[cfg(feature = "worm-sink")]
        {
            if self.audit_worm_sink_enabled {
                let cfg = self
                    .worm_sink_config
                    .as_ref()
                    .ok_or("audit_worm_sink_enabled is true but worm_sink_config is missing")?;
                cfg.validate()
                    .map_err(|e| format!("audit_worm_sink configuration invalid: {e}"))?;
                if !cfg.live {
                    tracing::warn!(
                        "audit_worm_sink is enabled but live=false; the sink will not make live S3 calls"
                    );
                }
            }
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

        // Validate trusted proxy CIDRs. Universal CIDRs (`0.0.0.0/0`, `::/0`)
        // would match every client address and therefore silently trust any
        // `X-Forwarded-For` / `X-Real-IP` header, so they are rejected. Any
        // structurally invalid CIDR is already rejected at the parse boundary.
        for cidr in &self.trusted_proxy_cidrs {
            if cidr.prefix_len() == 0 {
                return Err(format!(
                    "trusted_proxy_cidrs must not contain a universal CIDR that matches all addresses: {cidr}"
                ));
            }
        }

        // Validate pre-auth rate limits when explicitly configured. When left
        // unset they inherit `rate_limit_*` (already validated above), so the
        // pre-auth limit is always enabled and can never be disabled.
        if let Some(per_second) = self.pre_auth_rate_limit_per_second {
            if per_second == 0 {
                return Err("pre_auth_rate_limit_per_second must be at least 1".to_string());
            }
        }
        if let Some(burst) = self.pre_auth_rate_limit_burst {
            if burst == 0 {
                return Err("pre_auth_rate_limit_burst must be at least 1".to_string());
            }
            if burst > 10_000 {
                return Err("pre_auth_rate_limit_burst must be at most 10000".to_string());
            }
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

        // Validate nonce cache backend compatibility.
        if self.nonce_cache_backend == NonceCacheBackend::Postgres
            && !is_postgres_dsn(&self.store_dsn)
        {
            return Err(
                "nonce_cache_backend='postgres' requires a PostgreSQL store DSN".to_string(),
            );
        }

        // Validate lifecycle reconciliation settings
        if self.lifecycle_reconciliation_enabled {
            if self.lifecycle_reconciliation_interval_secs == 0 {
                return Err("lifecycle_reconciliation_interval_secs must be at least 1".to_string());
            }
            if self.lifecycle_reconciliation_batch_limit == 0 {
                return Err("lifecycle_reconciliation_batch_limit must be at least 1".to_string());
            }
            if self.lifecycle_reconciliation_batch_limit > 10_000 {
                return Err(
                    "lifecycle_reconciliation_batch_limit must be at most 10000".to_string()
                );
            }
        }

        // Validate approval timeout settings only when the reconciler is enabled.
        if self.approval_timeout_enabled {
            if !(60..=86_400).contains(&self.approval_timeout_seconds) {
                return Err(format!(
                    "approval_timeout_seconds must be between 60 and 86400, got {}",
                    self.approval_timeout_seconds
                ));
            }
            if !(5..=86_400).contains(&self.approval_reconciliation_interval_secs) {
                return Err(format!(
                    "approval_reconciliation_interval_secs must be between 5 and 86400, got {}",
                    self.approval_reconciliation_interval_secs
                ));
            }
        }

        // Validate quarantine timeout settings only when the reconciler is enabled.
        if self.quarantine_timeout_enabled {
            if !(60..=604_800).contains(&self.quarantine_timeout_seconds) {
                return Err(format!(
                    "quarantine_timeout_seconds must be between 60 and 604800, got {}",
                    self.quarantine_timeout_seconds
                ));
            }
            if !(5..=86_400).contains(&self.quarantine_reconciliation_interval_secs) {
                return Err(format!(
                    "quarantine_reconciliation_interval_secs must be between 5 and 86400, got {}",
                    self.quarantine_reconciliation_interval_secs
                ));
            }
        }

        if production_like && !self.quarantine_timeout_enabled {
            tracing::warn!(
                "quarantine_timeout_enabled is false in a production-like configuration; \
                 stale pending quarantine holds will not be expired automatically"
            );
        }

        // Validate HA reconciler settings only when enabled.
        if self.ha_reconciler_enabled {
            if !(5..=3_600).contains(&self.ha_reconciler_interval_secs) {
                return Err(format!(
                    "ha_reconciler_interval_secs must be between 5 and 3600, got {}",
                    self.ha_reconciler_interval_secs
                ));
            }
            if !(60..=86_400).contains(&self.ha_reconciler_stale_threshold_secs) {
                return Err(format!(
                    "ha_reconciler_stale_threshold_secs must be between 60 and 86400, got {}",
                    self.ha_reconciler_stale_threshold_secs
                ));
            }
            if !(1..=10_000).contains(&self.ha_reconciler_batch_size) {
                return Err(format!(
                    "ha_reconciler_batch_size must be between 1 and 10000, got {}",
                    self.ha_reconciler_batch_size
                ));
            }
        }

        // Validate mfa_secret_key format if present: must be exactly 64 hex chars (32 bytes).
        if let Some(ref key) = self.mfa_secret_key {
            if key.len() != 64 {
                return Err(format!(
                    "mfa_secret_key must be exactly 64 hex characters (32 bytes), got {} characters",
                    key.len()
                ));
            }
            if hex::decode(key).is_err() {
                return Err("mfa_secret_key contains invalid hex characters".to_string());
            }
        }

        if self.mfa_lockout_max_attempts < 1 {
            return Err("mfa_lockout_max_attempts must be at least 1".to_string());
        }
        if !(1..=86400).contains(&self.mfa_lockout_duration_secs) {
            return Err(format!(
                "mfa_lockout_duration_secs must be between 1 and 86400, got {}",
                self.mfa_lockout_duration_secs
            ));
        }

        // Validate behavioral anomaly detection settings.
        if self.behavioral_anomaly_enabled {
            if !(1..=3600).contains(&self.behavioral_anomaly_window_secs) {
                return Err(format!(
                    "behavioral_anomaly_window_secs must be between 1 and 3600, got {}",
                    self.behavioral_anomaly_window_secs
                ));
            }
            if self.behavioral_anomaly_warning_threshold == 0 {
                return Err("behavioral_anomaly_warning_threshold must be at least 1".to_string());
            }
            if self.behavioral_anomaly_critical_threshold == 0 {
                return Err("behavioral_anomaly_critical_threshold must be at least 1".to_string());
            }
            if self.behavioral_anomaly_critical_threshold
                < self.behavioral_anomaly_warning_threshold
            {
                return Err(
                    "behavioral_anomaly_critical_threshold must be >= behavioral_anomaly_warning_threshold"
                        .to_string(),
                );
            }
            if !(1..=100_000).contains(&self.behavioral_anomaly_max_actors) {
                return Err(format!(
                    "behavioral_anomaly_max_actors must be between 1 and 100000, got {}",
                    self.behavioral_anomaly_max_actors
                ));
            }
        }

        // Validate legacy object compatibility deadline. It must be a finite,
        // explicit future timestamp; expired deadlines are denied at runtime but
        // rejected at config time because they are no-ops.
        if let Some(allow_until) = self.legacy_object_compat_allow_until {
            if allow_until <= chrono::Utc::now() {
                return Err(
                    "legacy_object_compat_allow_until must be a future RFC3339 timestamp"
                        .to_string(),
                );
            }
        }

        Ok(())
    }

    /// Effective pre-auth (unauthenticated) sustained rate limit in requests
    /// per second. Falls back to the authenticated `rate_limit_per_second`
    /// when no explicit pre-auth value is configured.
    pub fn effective_pre_auth_rate_limit_per_second(&self) -> u64 {
        self.pre_auth_rate_limit_per_second
            .unwrap_or(self.rate_limit_per_second)
    }

    /// Effective pre-auth (unauthenticated) burst size. Falls back to the
    /// authenticated `rate_limit_burst` when no explicit pre-auth value is
    /// configured.
    pub fn effective_pre_auth_rate_limit_burst(&self) -> u32 {
        self.pre_auth_rate_limit_burst
            .unwrap_or(self.rate_limit_burst)
    }
}

fn is_postgres_dsn(dsn: &str) -> bool {
    let dsn_lower = dsn.to_lowercase();
    dsn_lower.starts_with("postgres://") || dsn_lower.starts_with("postgresql://")
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
