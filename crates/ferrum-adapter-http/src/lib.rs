//! HTTP adapter for mutation and idempotency-aware compensation.
//!
//! This adapter implements the `RollbackAdapter` trait for HTTP operations,
//! supporting prepare→verify lifecycle with HttpStatusExpected checks.
//!
//! # HttpMutation Recovery Slice
//!
//! This adapter supports bounded HttpMutation execute for **request execution with receipt capture**:
//! - `prepare`: validates target/method/url shape and optionally validates prepare checks
//!   if they use `HttpStatusExpected` (making a request to check endpoint reachability)
//! - `execute`: sends real HTTP requests to the target URL using the specified method,
//!   captures request metadata (method, URL, request digest), response status, and bounded
//!   response digest/size. Fail-closed on connection/timeout/parse errors with phase-aware
//!   internal normalization. Supports payload body (string, number, bool, null, or object
//!   with "body" field).
//! - `verify`: supports explicit `HttpStatusExpected` checks against a local test server/endpoint
//!   and fail-closes on mismatch or request error
//!
//! # Limitations
//!
//! - Only `HttpStatusExpected` checks are supported; other check types return unsupported error.
//! - Rollback and compensate require a valid `http.replay_v1` compensation plan with idempotency key;
//!   unsupported shapes return `AdapterError::Unsupported` with structured reason codes.

use async_trait::async_trait;
use chrono::Utc;
use ferrum_proto::{
    ActionType, CheckType, HttpMethod, JsonMap, RollbackContract, RollbackPrepareRequest,
    RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, ExecuteReceipt, PrepareReceipt, RecoveryReceipt, RollbackAdapter, VerifyReceipt,
};
use reqwest::Url;
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::time::Duration;
use thiserror::Error;

pub mod planner;
pub use planner::PlannableHttpAdapter;

#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of concurrent connections.
    pub max_connections: usize,
    /// Connection timeout in milliseconds.
    pub connection_timeout_ms: u64,
    /// Pool idle timeout in milliseconds.
    pub pool_idle_timeout_ms: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            connection_timeout_ms: 5000,
            pool_idle_timeout_ms: 30000,
        }
    }
}

impl PoolConfig {
    /// Validates the pool configuration.
    /// Returns Ok if valid, or Err with validation message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("max_connections must be greater than 0".to_string());
        }
        if self.max_connections > 1000 {
            return Err("max_connections must be at most 1000".to_string());
        }
        if self.connection_timeout_ms == 0 {
            return Err("connection_timeout_ms must be greater than 0".to_string());
        }
        if self.connection_timeout_ms > 60000 {
            return Err("connection_timeout_ms must be at most 60000 (60 seconds)".to_string());
        }
        if self.pool_idle_timeout_ms > 3600000 {
            return Err("pool_idle_timeout_ms must be at most 3600000 (1 hour)".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial backoff delay in milliseconds.
    pub initial_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds.
    pub max_backoff_ms: u64,
    /// HTTP status codes that trigger retry.
    pub retryable_statuses: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            retryable_statuses: vec![429, 502, 503, 504],
        }
    }
}

impl RetryConfig {
    /// Validates the retry configuration.
    /// Returns Ok if valid, or Err with validation message if invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_retries > 10 {
            return Err("max_retries must be at most 10".to_string());
        }
        if self.initial_backoff_ms == 0 {
            return Err("initial_backoff_ms must be greater than 0".to_string());
        }
        if self.initial_backoff_ms > 60000 {
            return Err("initial_backoff_ms must be at most 60000 (60 seconds)".to_string());
        }
        if self.max_backoff_ms < self.initial_backoff_ms {
            return Err("max_backoff_ms must be >= initial_backoff_ms".to_string());
        }
        if self.max_backoff_ms > 300000 {
            return Err("max_backoff_ms must be at most 300000 (5 minutes)".to_string());
        }
        for status in &self.retryable_statuses {
            if *status < 100 || *status > 599 {
                return Err(format!(
                    "retryable_statuses contains invalid HTTP status {} (must be 100-599)",
                    status
                ));
            }
        }
        Ok(())
    }
}

/// Records a single retry attempt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttemptRecord {
    /// Zero-based attempt number.
    pub attempt_number: u32,
    /// HTTP status code received (0 if connection error).
    pub status_code: u16,
    /// Whether the attempt succeeded (status in expected list).
    pub succeeded: bool,
    /// Timestamp when attempt started (RFC3339).
    pub started_at: String,
    /// Timestamp when attempt completed (RFC3339).
    pub completed_at: String,
    /// Error message if attempt failed (connection error, etc).
    pub error_message: Option<String>,
}

/// Rollback metadata tracking all retry attempts.
/// This is included in error metadata when all retries are exhausted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetryRollbackMetadata {
    /// Version identifier.
    pub version: String,
    /// Total attempts made (including initial + retries).
    pub total_attempts: u32,
    /// All individual attempt records.
    pub attempts: Vec<AttemptRecord>,
    /// The final error message after all retries exhausted.
    pub final_error: String,
    /// Whether idempotency key was preserved across retries.
    pub idempotency_key_preserved: bool,
}

pub const ADAPTER_KIND: &str = "ferrum-adapter-http";

/// Phase context for error normalization.
const PHASE_PREPARE: &str = "prepare";
const PHASE_VERIFY: &str = "verify";
const PHASE_EXECUTE: &str = "execute";
const PHASE_COMPENSATE: &str = "compensate";
const PHASE_ROLLBACK: &str = "rollback";

/// The supported replay operation identifier.
const REPLAY_OPERATION: &str = "http.replay_v1";

/// Valid replay args keys and their types for strict schema validation.
const REPLAY_VALID_KEYS: &[&str] = &["method", "url", "payload", "expected_statuses"];

#[derive(Debug, Error)]
pub enum HttpAdapterError {
    #[error("invalid target: expected HttpRequest, got {0}")]
    InvalidTarget(String),
    #[error("unsupported action type: {0}")]
    UnsupportedAction(String),
    #[error("unsupported check type: {0}")]
    UnsupportedCheck(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("http request failed: {0}")]
    HttpRequestFailed(String),
    #[error("http status mismatch: expected {expected}, got {actual}")]
    HttpStatusMismatch { expected: u16, actual: u16 },
    #[error("malformed URL: {0}")]
    MalformedUrl(String),
    #[error("connection error: {0}")]
    ConnectionError(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("replay validation failed: {0}")]
    ReplayValidation(String),
    #[error("retry exhausted: {0}")]
    RetryExhausted(String),
}

impl From<HttpAdapterError> for AdapterError {
    fn from(err: HttpAdapterError) -> Self {
        match err {
            HttpAdapterError::InvalidTarget(msg) => AdapterError::Validation(msg),
            HttpAdapterError::UnsupportedAction(msg) => AdapterError::Unsupported(msg),
            HttpAdapterError::UnsupportedCheck(msg) => AdapterError::Unsupported(msg),
            HttpAdapterError::Validation(msg) => AdapterError::Validation(msg),
            HttpAdapterError::HttpRequestFailed(msg) => AdapterError::Internal(msg),
            HttpAdapterError::HttpStatusMismatch { .. } => {
                AdapterError::Validation(err.to_string())
            }
            HttpAdapterError::MalformedUrl(msg) => AdapterError::Validation(msg),
            HttpAdapterError::ConnectionError(msg) => AdapterError::Internal(msg),
            HttpAdapterError::Timeout(msg) => AdapterError::Internal(msg),
            HttpAdapterError::ReplayValidation(msg) => AdapterError::Validation(msg),
            HttpAdapterError::RetryExhausted(msg) => AdapterError::Internal(msg),
        }
    }
}

/// HTTP adapter implementing the RollbackAdapter trait.
///
/// Uses HTTP requests to provide bounded prepare→execute→verify behavior with status checks.
/// This first slice supports real request execution with receipt capture while keeping
/// rollback/compensate unsupported.
pub struct HttpAdapter {
    key: &'static str,
    allow_private_networks: bool,
}

/// Parsed and validated replay contract for the narrow http.replay_v1 slice.
struct ReplayContract {
    /// The idempotency key to use for replay.
    idempotency_key: String,
    /// Expected HTTP method (must be POST).
    method: HttpMethod,
    /// Target URL (must match target.url).
    url: String,
    /// Payload to replay.
    payload: serde_json::Value,
    /// Required expected status codes for validation (must be non-empty, values in 100..=599).
    expected_statuses: Vec<u16>,
}

impl HttpAdapter {
    /// Creates a new HttpAdapter with the given key.
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            allow_private_networks: false,
        }
    }

    #[cfg(test)]
    fn new_allow_private_networks_for_tests(key: &'static str) -> Self {
        Self {
            key,
            allow_private_networks: true,
        }
    }

    /// Computes exponential backoff delay with jitter cap.
    fn compute_backoff_delay(attempt: u32, config: &RetryConfig) -> Duration {
        // Exponential backoff: initial * 2^attempt
        let base_delay = config.initial_backoff_ms;
        let exponential_delay = base_delay * (2_u64.pow(attempt.min(10)));
        let capped_delay = exponential_delay.min(config.max_backoff_ms);
        Duration::from_millis(capped_delay)
    }

    /// Checks if a status code is retryable based on configuration.
    fn is_retryable_status(status: u16, config: &RetryConfig) -> bool {
        config.retryable_statuses.contains(&status)
    }

    /// Records an attempt and returns the attempt record.
    #[allow(dead_code)]
    fn record_attempt(
        attempt_number: u32,
        status_code: u16,
        succeeded: bool,
        started_at: chrono::DateTime<Utc>,
        completed_at: chrono::DateTime<Utc>,
        error_message: Option<String>,
    ) -> AttemptRecord {
        AttemptRecord {
            attempt_number,
            status_code,
            succeeded,
            started_at: started_at.to_rfc3339(),
            completed_at: completed_at.to_rfc3339(),
            error_message,
        }
    }

    /// Executes HTTP request with retry/backoff logic.
    /// Returns (status, response_body, attempt_history) on success.
    /// On final failure after all retries, returns error with full attempt history.
    #[allow(dead_code)]
    async fn execute_with_retry(
        method: HttpMethod,
        url: &str,
        body: Option<Vec<u8>>,
        idempotency_key: Option<&str>,
        retry_config: &RetryConfig,
        expected_statuses: &[u16],
    ) -> Result<(u16, Vec<u8>, Vec<AttemptRecord>), (AdapterError, Vec<AttemptRecord>)> {
        let mut attempts = Vec::new();
        let max_attempts = 1 + retry_config.max_retries; // initial + retries
        let method_clone = method.clone();
        let url_owned = url.to_string();
        let idempotency_key_owned = idempotency_key.map(|s| s.to_string());

        for attempt_num in 0..max_attempts {
            let started_at = Utc::now();

            // Execute the HTTP request
            let result = Self::execute_http_request(
                method_clone.clone(),
                &url_owned,
                body.clone(),
                idempotency_key_owned.as_deref(),
                false,
                PHASE_EXECUTE,
            )
            .await;

            let completed_at = Utc::now();

            match result {
                Ok((status, response_body)) => {
                    let succeeded = expected_statuses.contains(&status);
                    let record = Self::record_attempt(
                        attempt_num,
                        status,
                        succeeded,
                        started_at,
                        completed_at,
                        None,
                    );
                    attempts.push(record);

                    if succeeded || !Self::is_retryable_status(status, retry_config) {
                        // Success or non-retryable status - return
                        return Ok((status, response_body, attempts));
                    }

                    // Retryable failure - unless this is the last attempt
                    if attempt_num < max_attempts - 1 {
                        let delay = Self::compute_backoff_delay(attempt_num, retry_config);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    // Last attempt failed - fall through to error
                    let final_error = format!(
                        "HTTP request failed with status {} after {} attempts",
                        status, max_attempts
                    );
                    return Err((AdapterError::Internal(final_error), attempts));
                }
                Err(e) => {
                    // Connection error - record as attempt with status 0
                    let record = Self::record_attempt(
                        attempt_num,
                        0, // status 0 indicates connection error
                        false,
                        started_at,
                        completed_at,
                        Some(e.to_string()),
                    );
                    attempts.push(record);

                    // Unless this is the last attempt, retry
                    if attempt_num < max_attempts - 1 {
                        let delay = Self::compute_backoff_delay(attempt_num, retry_config);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    // Last attempt failed
                    let final_error =
                        format!("HTTP request failed after {} attempts: {}", max_attempts, e);
                    return Err((AdapterError::Internal(final_error), attempts));
                }
            }
        }

        // Should not reach here, but safety fallback
        let final_error = format!("HTTP request exhausted {} attempts", max_attempts);
        Err((AdapterError::Internal(final_error), attempts))
    }

    /// Extracts the HTTP request details from a RollbackTarget::HttpRequest variant.
    fn extract_http_target(target: &RollbackTarget) -> Result<(&HttpMethod, &str), AdapterError> {
        match target {
            RollbackTarget::HttpRequest { method, url, .. } => Ok((method, url)),
            _ => Err(AdapterError::Validation(format!(
                "invalid target: expected HttpRequest, got {:?}",
                target
            ))),
        }
    }

    /// Validates URL shape - must be http or https and parseable.
    fn validate_url_shape(url: &str) -> Result<(), AdapterError> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(HttpAdapterError::MalformedUrl(format!(
                "URL must start with http:// or https://, got: {}",
                url
            ))
            .into());
        }
        let parsed = Self::parse_url(url)?;
        match parsed.scheme() {
            "http" | "https" => {}
            scheme => {
                return Err(HttpAdapterError::MalformedUrl(format!(
                    "URL must use http:// or https://, got scheme: {}",
                    scheme
                ))
                .into());
            }
        }
        if parsed.host_str().is_none() {
            return Err(HttpAdapterError::MalformedUrl(format!("URL has no host: {}", url)).into());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(HttpAdapterError::MalformedUrl(
                "URL credentials are not allowed in adapter targets".to_string(),
            )
            .into());
        }
        Ok(())
    }

    fn parse_url(url: &str) -> Result<Url, AdapterError> {
        Url::parse(url).map_err(|_| {
            HttpAdapterError::MalformedUrl(format!("failed to parse URL: {}", url)).into()
        })
    }

    fn is_forbidden_destination_ip(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(addr) => {
                addr.is_loopback()
                    || addr.is_private()
                    || addr.is_link_local()
                    || addr.is_broadcast()
                    || addr.is_documentation()
                    || addr.is_unspecified()
                    || addr.is_multicast()
            }
            IpAddr::V6(addr) => {
                addr.is_loopback()
                    || addr.is_unspecified()
                    || addr.is_multicast()
                    || addr.is_unique_local()
                    || addr.is_unicast_link_local()
            }
        }
    }

    async fn validate_outbound_destination(
        url: &str,
        allow_private_networks: bool,
        phase: &'static str,
    ) -> Result<Url, AdapterError> {
        let parsed = Self::parse_url(url)?;
        Self::validate_url_shape(url)?;

        if allow_private_networks {
            return Ok(parsed);
        }

        let host = parsed.host_str().ok_or_else(|| {
            Self::phase_wrap_validation(phase, format!("URL has no host: {}", url))
        })?;
        let lower_host = host.trim_end_matches('.').to_ascii_lowercase();
        if lower_host == "localhost"
            || lower_host == "localhost.localdomain"
            || lower_host == "metadata.google.internal"
        {
            return Err(Self::phase_wrap_validation(
                phase,
                format!("forbidden private HTTP destination host: {}", host),
            ));
        }

        if let Ok(ip) = lower_host.parse::<IpAddr>() {
            if Self::is_forbidden_destination_ip(ip) {
                return Err(Self::phase_wrap_validation(
                    phase,
                    format!("forbidden private HTTP destination address: {}", ip),
                ));
            }
            return Ok(parsed);
        }

        let port = parsed.port_or_known_default().ok_or_else(|| {
            Self::phase_wrap_validation(
                phase,
                format!(
                    "URL has no known default port for scheme: {}",
                    parsed.scheme()
                ),
            )
        })?;
        let resolved = tokio::net::lookup_host((host, port)).await.map_err(|e| {
            Self::phase_wrap_internal(
                phase,
                format!(
                    "failed to resolve HTTP destination {}:{}: {}",
                    host, port, e
                ),
            )
        })?;

        for addr in resolved {
            let ip = addr.ip();
            if Self::is_forbidden_destination_ip(ip) {
                return Err(Self::phase_wrap_validation(
                    phase,
                    format!(
                        "forbidden private HTTP destination address: {} resolved from {}",
                        ip, host
                    ),
                ));
            }
        }

        Ok(parsed)
    }

    fn reqwest_method(method: HttpMethod) -> reqwest::Method {
        match method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
        }
    }

    fn http_client() -> Result<reqwest::Client, AdapterError> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .use_rustls_tls()
            .build()
            .map_err(|e| AdapterError::Internal(format!("failed to build HTTP client: {}", e)))
    }

    /// Normalizes a validation error with phase context.
    fn phase_wrap_validation(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Validation(format!("[{}] {}", phase, msg))
    }

    /// Normalizes an internal error with phase context.
    fn phase_wrap_internal(phase: &'static str, msg: String) -> AdapterError {
        AdapterError::Internal(format!("[{}] {}", phase, msg))
    }

    /// Parses a method string into HttpMethod.
    fn parse_method_string(method_str: &str) -> Result<HttpMethod, ()> {
        match method_str {
            "GET" | "get" | "Get" => Ok(HttpMethod::Get),
            "POST" | "post" | "Post" => Ok(HttpMethod::Post),
            "PUT" | "put" | "Put" => Ok(HttpMethod::Put),
            "PATCH" | "patch" | "Patch" => Ok(HttpMethod::Patch),
            "DELETE" | "delete" | "Delete" => Ok(HttpMethod::Delete),
            _ => Err(()),
        }
    }

    /// Runs an HTTP status check against the given URL using the specified method.
    async fn run_http_status_check(
        url: &str,
        method: HttpMethod,
        expected_statuses: &[u16],
        allow_private_networks: bool,
        phase: &'static str,
    ) -> Result<(), AdapterError> {
        let parsed =
            Self::validate_outbound_destination(url, allow_private_networks, phase).await?;
        let client = Self::http_client()?;
        let status = client
            .request(Self::reqwest_method(method), parsed)
            .send()
            .await
            .map_err(|e| {
                let kind = if e.is_connect() {
                    "connection error"
                } else {
                    "HTTP request failed"
                };
                Self::phase_wrap_internal(phase, format!("{}: {}", kind, e))
            })?
            .status()
            .as_u16();

        if !expected_statuses.contains(&status) {
            let expected_str = if expected_statuses.len() == 1 {
                format!("{}", expected_statuses[0])
            } else {
                format!("{:?}", expected_statuses)
            };
            return Err(AdapterError::Validation(format!(
                "[{}] HttpStatusExpected mismatch: expected {}, got {}",
                phase, expected_str, status
            )));
        }

        Ok(())
    }

    /// Runs a single check spec and returns an error if it fails.
    /// The `target_method` is used as the default for HTTP status checks.
    async fn run_check(
        check: &ferrum_proto::CheckSpec,
        url: &str,
        target_method: HttpMethod,
        allow_private_networks: bool,
        phase: &'static str,
    ) -> Result<(), AdapterError> {
        match check.check_type {
            CheckType::HttpStatusExpected => {
                // Validate 'url' field if present
                if let Some(serde_json::Value::String(check_url)) = check.config.get("url") {
                    if check_url != url {
                        return Err(AdapterError::Validation(format!(
                            "[{}] HttpStatusExpected check URL mismatch: check targets '{}', expected '{}'",
                            phase, check_url, url
                        )));
                    }
                }

                // Validate optional 'method' field if present - must match target method
                if let Some(serde_json::Value::String(method_str)) = check.config.get("method") {
                    let check_method = Self::parse_method_string(method_str).map_err(|_| {
                        AdapterError::Validation(format!(
                            "[{}] HttpStatusExpected check 'method' must be a valid HTTP method (GET, POST, PUT, PATCH, DELETE), got '{}'",
                            phase, method_str
                        ))
                    })?;
                    if check_method != target_method {
                        return Err(AdapterError::Validation(format!(
                            "[{}] HttpStatusExpected check method mismatch: check uses '{}', target expects {:?}",
                            phase, method_str, target_method
                        )));
                    }
                }

                // Parse expected statuses - support both single 'expected_status' and array 'expected_statuses'
                let expected_statuses = if let Some(arr) = check.config.get("expected_statuses") {
                    match arr {
                        serde_json::Value::Array(arr) => {
                            let mut statuses = Vec::new();
                            for (i, item) in arr.iter().enumerate() {
                                match item {
                                    serde_json::Value::Number(n) => {
                                        statuses.push(n.as_u64().unwrap_or(0) as u16);
                                    }
                                    serde_json::Value::String(s) => {
                                        let parsed = s.parse::<u16>().map_err(|_| {
                                            AdapterError::Validation(format!(
                                                "[{}] HttpStatusExpected check 'expected_statuses' element {} must be a number, got '{}'",
                                                phase, i, s
                                            ))
                                        })?;
                                        statuses.push(parsed);
                                    }
                                    v => {
                                        return Err(AdapterError::Validation(format!(
                                            "[{}] HttpStatusExpected check 'expected_statuses' element {} must be a number, got {}",
                                            phase, i, v
                                        )));
                                    }
                                }
                            }
                            if statuses.is_empty() {
                                return Err(AdapterError::Validation(format!(
                                    "[{}] HttpStatusExpected check 'expected_statuses' cannot be empty",
                                    phase
                                )));
                            }
                            statuses
                        }
                        v => {
                            return Err(AdapterError::Validation(format!(
                                "[{}] HttpStatusExpected check 'expected_statuses' must be an array of numbers, got {}",
                                phase, v
                            )));
                        }
                    }
                } else if let Some(val) = check.config.get("expected_status") {
                    match val {
                        serde_json::Value::Number(n) => {
                            vec![n.as_u64().unwrap_or(0) as u16]
                        }
                        serde_json::Value::String(s) => {
                            let parsed = s.parse::<u16>().map_err(|_| {
                                AdapterError::Validation(format!(
                                    "[{}] HttpStatusExpected check 'expected_status' must be a number, got '{}'",
                                    phase, s
                                ))
                            })?;
                            vec![parsed]
                        }
                        v => {
                            return Err(AdapterError::Validation(format!(
                                "[{}] HttpStatusExpected check 'expected_status' must be a number, got {}",
                                phase, v
                            )));
                        }
                    }
                } else {
                    return Err(AdapterError::Validation(format!(
                        "[{}] HttpStatusExpected check requires 'expected_status' or 'expected_statuses' config",
                        phase
                    )));
                };

                // Use target method for the check (either explicit in config or from target)
                // Note: method validation above ensures explicit method in config matches target
                let check_method = if let Some(serde_json::Value::String(method_str)) =
                    check.config.get("method")
                {
                    Self::parse_method_string(method_str).unwrap_or(target_method)
                } else {
                    target_method
                };

                Self::run_http_status_check(
                    url,
                    check_method,
                    &expected_statuses,
                    allow_private_networks,
                    phase,
                )
                .await
            }
            _ => Err(AdapterError::Unsupported(format!(
                "[{}] unsupported check type: {:?}",
                phase, check.check_type
            ))),
        }
    }

    /// Extracts body bytes from the payload for HTTP request body.
    /// Supports: string, number, bool, null, or object with "body" field.
    fn extract_body_bytes(payload: &serde_json::Value) -> Result<Option<Vec<u8>>, AdapterError> {
        match payload {
            serde_json::Value::String(s) => Ok(Some(s.as_bytes().to_vec())),
            serde_json::Value::Number(n) => Ok(Some(n.to_string().into_bytes())),
            serde_json::Value::Bool(b) => Ok(Some(b.to_string().into_bytes())),
            serde_json::Value::Null => Ok(None),
            serde_json::Value::Object(obj) => {
                // Support object with explicit "body" field
                if let Some(body_val) = obj.get("body") {
                    match body_val {
                        serde_json::Value::String(s) => Ok(Some(s.as_bytes().to_vec())),
                        serde_json::Value::Number(n) => Ok(Some(n.to_string().into_bytes())),
                        serde_json::Value::Bool(b) => Ok(Some(b.to_string().into_bytes())),
                        serde_json::Value::Null => Ok(None),
                        serde_json::Value::Array(arr) => {
                            // Serialize array to JSON bytes
                            Ok(Some(serde_json::to_vec(arr).map_err(|e| {
                                AdapterError::Validation(format!(
                                    "[execute] failed to serialize body array: {}",
                                    e
                                ))
                            })?))
                        }
                        serde_json::Value::Object(inner_obj) => {
                            Ok(Some(serde_json::to_vec(inner_obj).map_err(|e| {
                                AdapterError::Validation(format!(
                                    "[execute] failed to serialize body object: {}",
                                    e
                                ))
                            })?))
                        }
                    }
                } else {
                    // No "body" field: treat whole object as body
                    Ok(Some(serde_json::to_vec(obj).map_err(|e| {
                        AdapterError::Validation(format!(
                            "[execute] failed to serialize payload object: {}",
                            e
                        ))
                    })?))
                }
            }
            serde_json::Value::Array(arr) => Ok(Some(serde_json::to_vec(arr).map_err(|e| {
                AdapterError::Validation(format!(
                    "[execute] failed to serialize payload array: {}",
                    e
                ))
            })?)),
        }
    }

    /// Validates payload is a supported type for HTTP request body.
    fn validate_payload_shape(payload: &serde_json::Value) -> Result<(), AdapterError> {
        match payload {
            serde_json::Value::String(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::Bool(_)
            | serde_json::Value::Null => Ok(()),
            serde_json::Value::Object(obj) => {
                // Check if all values are simple types (or nested objects/arrays that we'll serialize)
                for (key, val) in obj {
                    match val {
                        serde_json::Value::String(_)
                        | serde_json::Value::Number(_)
                        | serde_json::Value::Bool(_)
                        | serde_json::Value::Null
                        | serde_json::Value::Array(_)
                        | serde_json::Value::Object(_) => {} // allowed
                    }
                    if matches!(
                        val,
                        serde_json::Value::String(_)
                            | serde_json::Value::Number(_)
                            | serde_json::Value::Bool(_)
                            | serde_json::Value::Null
                            | serde_json::Value::Array(_)
                            | serde_json::Value::Object(_)
                    ) {
                        continue;
                    }
                    return Err(AdapterError::Validation(format!(
                        "[execute] payload field '{}' has unsupported type: {}",
                        key,
                        serde_json::json!(val)
                            .as_object()
                            .map(|o| format!("{:?}", o))
                            .unwrap_or_default()
                    )));
                }
                Ok(())
            }
            serde_json::Value::Array(_) => Ok(()),
        }
    }

    /// Validates that an idempotency key is non-empty and header-safe (RFC-compliant token).
    /// Header-safe means it contains only visible ASCII characters except control chars and separators.
    fn validate_idempotency_key(key: &str) -> Result<(), AdapterError> {
        if key.is_empty() {
            return Err(HttpAdapterError::ReplayValidation(
                "idempotency_key must be non-empty".to_string(),
            )
            .into());
        }
        // RFC 7230 token characters: visible ASCII except control chars and separators
        for ch in key.chars() {
            let code = ch as u32;
            if !(0x21..=0x7e).contains(&code) {
                return Err(HttpAdapterError::ReplayValidation(format!(
                    "idempotency_key contains non-header-safe character: {:?}",
                    ch
                ))
                .into());
            }
        }
        Ok(())
    }

    /// Computes the request digest: SHA256(method + url + body).
    fn compute_request_digest(method: HttpMethod, url: &str, body: Option<&[u8]>) -> String {
        let mut d = Sha256::new();
        d.update(format!("{:?}", method).as_bytes());
        d.update(url.as_bytes());
        if let Some(b) = body {
            d.update(b);
        }
        format!("{:x}", d.finalize())
    }

    /// Parses and validates the compensation plan for a supported http.replay_v1 contract.
    /// Returns the validated replay contract or an error with structured reason codes.
    fn parse_replay_contract(
        contract: &RollbackContract,
        phase: &'static str,
    ) -> Result<ReplayContract, AdapterError> {
        // Must have exactly 1 compensation step
        if contract.compensation_plan.len() != 1 {
            return Err(HttpAdapterError::ReplayValidation(format!(
                "[{}] http.replay_v1 requires exactly 1 compensation step, got {}",
                phase,
                contract.compensation_plan.len()
            ))
            .into());
        }

        let step = &contract.compensation_plan[0];

        // Step operation must be exactly http.replay_v1
        if step.operation != REPLAY_OPERATION {
            return Err(HttpAdapterError::ReplayValidation(format!(
                "[{}] http.replay_v1 requires operation '{}', got '{}'",
                phase, REPLAY_OPERATION, step.operation
            ))
            .into());
        }

        // Validate idempotency_key
        Self::validate_idempotency_key(&step.idempotency_key)?;

        // Validate args schema: only method, url, payload, expected_statuses allowed
        let allowed_keys: std::collections::HashSet<&str> =
            REPLAY_VALID_KEYS.iter().cloned().collect();
        for key in step.args.keys() {
            if !allowed_keys.contains(key.as_str()) {
                return Err(HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 args contains unknown key '{}'; allowed: {:?}",
                    phase, key, REPLAY_VALID_KEYS
                ))
                .into());
            }
        }

        // Extract and validate method (must be POST)
        let method_str = step
            .args
            .get("method")
            .ok_or_else(|| {
                HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 args missing required 'method' field",
                    phase
                ))
            })?
            .as_str()
            .ok_or_else(|| {
                HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 args 'method' must be a string",
                    phase
                ))
            })?;

        let method = match method_str {
            "POST" | "post" | "Post" => HttpMethod::Post,
            "PUT" | "put" | "Put" => HttpMethod::Put,
            "PATCH" | "patch" | "Patch" => HttpMethod::Patch,
            _ => {
                return Err(HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 requires method POST/PUT/PATCH, got '{}'",
                    phase, method_str
                ))
                .into());
            }
        };

        // Extract and validate url (must exactly match target.url)
        let target_url = match &contract.target {
            RollbackTarget::HttpRequest { url: u, .. } => u.clone(),
            _ => {
                return Err(HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 requires target to be HttpRequest",
                    phase
                ))
                .into());
            }
        };
        let args_url = step
            .args
            .get("url")
            .ok_or_else(|| {
                HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 args missing required 'url' field",
                    phase
                ))
            })?
            .as_str()
            .ok_or_else(|| {
                HttpAdapterError::ReplayValidation(format!(
                    "[{}] http.replay_v1 args 'url' must be a string",
                    phase
                ))
            })?;

        if args_url != target_url {
            return Err(HttpAdapterError::ReplayValidation(format!(
                "[{}] http.replay_v1 args url must equal target.url '{}', got '{}'",
                phase, target_url, args_url
            ))
            .into());
        }

        // Extract payload
        let payload = step
            .args
            .get("payload")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // Extract required expected_statuses
        let expected_statuses = step.args.get("expected_statuses").ok_or_else(|| {
            HttpAdapterError::ReplayValidation(format!(
                "[{}] http.replay_v1 args missing required 'expected_statuses' field",
                phase
            ))
        })?;
        let expected_statuses = Self::parse_expected_statuses_array(expected_statuses, phase)?;

        Ok(ReplayContract {
            idempotency_key: step.idempotency_key.clone(),
            method,
            url: args_url.to_string(),
            payload,
            expected_statuses,
        })
    }

    /// Parses an expected_statuses JSON value into a Vec<u16>.
    /// Validates that the array is non-empty and all values are in HTTP range 100..=599.
    fn parse_expected_statuses_array(
        val: &serde_json::Value,
        phase: &'static str,
    ) -> Result<Vec<u16>, AdapterError> {
        match val {
            serde_json::Value::Array(arr) => {
                if arr.is_empty() {
                    return Err(HttpAdapterError::ReplayValidation(format!(
                        "[{}] http.replay_v1 expected_statuses cannot be empty",
                        phase
                    ))
                    .into());
                }
                let mut statuses = Vec::new();
                for (i, item) in arr.iter().enumerate() {
                    match item {
                        serde_json::Value::Number(n) => {
                            let status = n.as_u64().unwrap_or(0) as u16;
                            if !Self::is_valid_http_status(status) {
                                return Err(HttpAdapterError::ReplayValidation(format!(
                                    "[{}] http.replay_v1 expected_statuses element {} is out of valid HTTP range (100..=599), got {}",
                                    phase, i, status
                                ))
                                .into());
                            }
                            statuses.push(status);
                        }
                        serde_json::Value::String(s) => {
                            let parsed = s.parse::<u16>().map_err(|_| {
                                HttpAdapterError::ReplayValidation(format!(
                                    "[{}] http.replay_v1 expected_statuses element {} must be a number, got '{}'",
                                    phase, i, s
                                ))
                            })?;
                            if !Self::is_valid_http_status(parsed) {
                                return Err(HttpAdapterError::ReplayValidation(format!(
                                    "[{}] http.replay_v1 expected_statuses element {} is out of valid HTTP range (100..=599), got {}",
                                    phase, i, parsed
                                ))
                                .into());
                            }
                            statuses.push(parsed);
                        }
                        v => {
                            return Err(HttpAdapterError::ReplayValidation(format!(
                                "[{}] http.replay_v1 expected_statuses element {} must be a number, got {}",
                                phase, i, v
                            ))
                            .into());
                        }
                    }
                }
                Ok(statuses)
            }
            _ => Err(HttpAdapterError::ReplayValidation(format!(
                "[{}] http.replay_v1 expected_statuses must be an array, got {}",
                phase, val
            ))
            .into()),
        }
    }

    /// Validates that a status code is in the valid HTTP range (100..=599).
    fn is_valid_http_status(status: u16) -> bool {
        (100..=599).contains(&status)
    }

    /// Async wrapper for HTTP execution.
    /// Optionally includes an Idempotency-Key header if provided.
    async fn execute_http_request(
        method: HttpMethod,
        url: &str,
        body: Option<Vec<u8>>,
        idempotency_key: Option<&str>,
        allow_private_networks: bool,
        phase: &'static str,
    ) -> Result<(u16, Vec<u8>), AdapterError> {
        let parsed =
            Self::validate_outbound_destination(url, allow_private_networks, phase).await?;
        let client = Self::http_client()?;
        let mut request = client.request(Self::reqwest_method(method), parsed);

        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }

        if let Some(body_bytes) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body_bytes);
        }

        let response = request.send().await.map_err(|e| {
            let kind = if e.is_connect() {
                "connection error"
            } else {
                "HTTP request failed"
            };
            Self::phase_wrap_internal(phase, format!("{}: {}", kind, e))
        })?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|e| {
            Self::phase_wrap_internal(phase, format!("failed to read HTTP response body: {}", e))
        })?;

        Ok((status, body.to_vec()))
    }

    /// Helper to build structured reason codes for unsupported compensation.
    fn build_unsupported_reason_codes(contract: &RollbackContract) -> Vec<&'static str> {
        let compensation_steps_present = !contract.compensation_plan.is_empty();
        let idempotency_key_present_in_plan = contract
            .compensation_plan
            .iter()
            .any(|step| !step.idempotency_key.is_empty());

        let mut reason_codes = Vec::new();
        reason_codes.push("RECOVERY_SLICE_NARROW_HTTP_REPLAY_V1");
        if !compensation_steps_present {
            reason_codes.push("NO_COMPENSATION_PLAN");
        } else if !idempotency_key_present_in_plan {
            reason_codes.push("NO_IDEMPOTENCY_KEY_IN_COMPENSATION");
        } else {
            reason_codes.push("COMPENSATION_PLAN_UNSUPPORTED_FOR_HTTP");
        }
        reason_codes.push("NO_OUTBOUND_RECOVERY_PATH");
        reason_codes.push("NO_PERSISTED_EXECUTE_EVIDENCE");
        reason_codes
    }
}

/// Register the HttpAdapter with the given registry using "http" as the adapter key.
/// This allows the adapter to be used for HttpMutation operations via the rollback service.
pub fn register_http_adapter(registry: &mut ferrum_rollback::AdapterRegistry) {
    registry.register(std::sync::Arc::new(HttpAdapter::new("http")));
}

#[async_trait]
impl RollbackAdapter for HttpAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate that target is HttpRequest
        let (method, url) = Self::extract_http_target(&request.target)?;

        // Validate that action_type is HttpMutation
        match request.action_type {
            ActionType::HttpMutation => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "unsupported action type: {:?}",
                    request.action_type
                )));
            }
        }

        // Validate URL shape
        Self::validate_url_shape(url)?;

        // Run prepare_checks if present (fail-closed on check failure)
        for check in &request.prepare_checks {
            Self::run_check(
                check,
                url,
                (*method).clone(),
                self.allow_private_networks,
                PHASE_PREPARE,
            )
            .await?;
        }

        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "prepared_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "target_url".to_string(),
            serde_json::Value::String(url.to_string()),
        );
        metadata.insert(
            "target_method".to_string(),
            serde_json::Value::String(format!("{:?}", method)),
        );

        // Minimal rollback groundwork marker for prepare phase.
        // NOTE: This is lightweight groundwork only - do not assume prepare metadata
        // persists in current orchestration if the service drops it.
        let rollback_groundwork = serde_json::json!({
            "version": "rollback_groundwork_v1",
            "phase": "prepare",
            "rollback_supported": false,
            "compensate_supported": false,
            "groundwork_mode": true,
        });
        metadata.insert("rollback_groundwork".to_string(), rollback_groundwork);

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: metadata,
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        // Validate target is HttpRequest
        let (method, url) = Self::extract_http_target(&contract.target)?;

        // Validate action type is HttpMutation
        match contract.action_type {
            ActionType::HttpMutation => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "[execute] unsupported action type: {:?}",
                    contract.action_type
                )));
            }
        }

        // Validate URL shape
        Self::validate_url_shape(url)?;

        // Validate payload shape
        Self::validate_payload_shape(payload)?;

        // Extract body bytes from payload
        let body_bytes = Self::extract_body_bytes(payload)?;

        // Compute request body size for metadata (before moving into HTTP call)
        let request_body_size = body_bytes.as_ref().map(|b| b.len()).unwrap_or(0);

        // Compute request digest: SHA256(method + url + body)
        let request_content = match &body_bytes {
            Some(body) => {
                let mut d = Sha256::new();
                d.update(format!("{:?}", method).as_bytes());
                d.update(url.as_bytes());
                d.update(body);
                format!("{:x}", d.finalize())
            }
            None => {
                let mut d = Sha256::new();
                d.update(format!("{:?}", method).as_bytes());
                d.update(url.as_bytes());
                format!("{:x}", d.finalize())
            }
        };

        // Try to parse a valid http.replay_v1 contract from compensation plan
        // If valid, we'll include the idempotency key in the request
        let parsed_replay = Self::parse_replay_contract(contract, PHASE_EXECUTE).ok();
        let idempotency_key_for_request =
            parsed_replay.as_ref().map(|r| r.idempotency_key.as_str());

        // Extract target request_digest via pattern matching
        let target_request_digest = match &contract.target {
            RollbackTarget::HttpRequest {
                request_digest: rd, ..
            } => rd.clone(),
            _ => {
                return Err(HttpAdapterError::ReplayValidation(
                    "[execute] http.replay_v1 requires HttpRequest target".to_string(),
                )
                .into());
            }
        };

        // If we have a valid replay contract, verify digest matches
        if let Some(ref replay) = parsed_replay {
            // Compute the expected digest from replay payload
            let replay_body_bytes = Self::extract_body_bytes(&replay.payload)?;
            let expected_digest = Self::compute_request_digest(
                replay.method.clone(),
                &replay.url,
                replay_body_bytes.as_deref(),
            );
            if expected_digest != target_request_digest {
                return Err(HttpAdapterError::ReplayValidation(format!(
                    "[execute] http.replay_v1 digest mismatch: computed '{}' != target.request_digest '{}'",
                    expected_digest, target_request_digest
                ))
                .into());
            }
        }

        // Execute HTTP request (with idempotency key if replay contract is valid)
        let (status, response_body) = Self::execute_http_request(
            (*method).clone(),
            url,
            body_bytes,
            idempotency_key_for_request,
            self.allow_private_networks,
            PHASE_EXECUTE,
        )
        .await?;

        // Compute response digest: SHA256(status + bounded body)
        // Bound response body to first 64KB to avoid memory issues
        const MAX_RESPONSE_DIGEST_BYTES: usize = 64 * 1024;
        let response_body_for_digest = if response_body.len() > MAX_RESPONSE_DIGEST_BYTES {
            &response_body[..MAX_RESPONSE_DIGEST_BYTES]
        } else {
            &response_body[..]
        };
        let response_digest = {
            let mut d = Sha256::new();
            d.update(status.to_string().as_bytes());
            d.update(response_body_for_digest);
            format!("{:x}", d.finalize())
        };

        // Build receipt metadata
        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "executed_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "target_method".to_string(),
            serde_json::Value::String(format!("{:?}", method)),
        );
        metadata.insert(
            "target_url".to_string(),
            serde_json::Value::String(url.to_string()),
        );
        metadata.insert(
            "request_digest".to_string(),
            serde_json::Value::String(request_content),
        );
        metadata.insert(
            "response_status".to_string(),
            serde_json::Value::Number(status.into()),
        );
        metadata.insert(
            "response_body_size".to_string(),
            serde_json::Value::Number(response_body.len().into()),
        );
        metadata.insert(
            "response_digest".to_string(),
            serde_json::Value::String(response_digest.clone()),
        );
        metadata.insert(
            "response_body_truncated".to_string(),
            serde_json::Value::Bool(response_body.len() > MAX_RESPONSE_DIGEST_BYTES),
        );

        // Build rollback groundwork v1 metadata block (digest-only, no raw bodies)
        let rollback_groundwork_v1 = serde_json::json!({
            "version": "rollback_groundwork_v1",
            "request": {
                "digest_algorithm": "SHA-256",
                "digest_window": "full",
                "digest_input_bytes": request_body_size,
                "body_size_bytes": request_body_size,
                "truncated": false,
                "content_type_hint": payload.as_object()
                    .and_then(|o| o.get("body"))
                    .map(|_| "application/json")
                    .unwrap_or_else(|| match payload {
                        serde_json::Value::String(_) => "text/plain",
                        serde_json::Value::Number(_) => "text/plain",
                        serde_json::Value::Bool(_) => "text/plain",
                        _ => "application/json",
                    }),
                "rollback_supported": false,
                "compensate_supported": false,
                "replay_confidence": "none",
            },
            "response": {
                "digest_algorithm": "SHA-256",
                "digest_window": if response_body.len() > MAX_RESPONSE_DIGEST_BYTES {
                    format!("first{}B", MAX_RESPONSE_DIGEST_BYTES)
                } else {
                    "full".to_string()
                },
                "digest_input_bytes": response_body_for_digest.len(),
                "body_size_bytes": response_body.len(),
                "truncated": response_body.len() > MAX_RESPONSE_DIGEST_BYTES,
                "content_type_hint": "application/octet-stream", // no header inspection
                "rollback_supported": false,
                "compensate_supported": false,
                "replay_confidence": "none",
            }
        });
        metadata.insert("rollback_groundwork_v1".to_string(), rollback_groundwork_v1);

        // Build http_recovery_readiness_v1 metadata block (classification for recovery path planning)
        // This uses ONLY current contract surface: target method, compensation plan, idempotency_key presence.
        // It does NOT guess transport headers that do not exist.
        let compensation_steps_present = !contract.compensation_plan.is_empty();
        let idempotency_key_present_in_plan = contract
            .compensation_plan
            .iter()
            .any(|step| !step.idempotency_key.is_empty());

        // Classify replayability based on method and compensation plan presence
        // - Safe methods (GET, DELETE) with idempotency key in plan = potentially_replayable
        // - Unsafe methods (POST, PUT, PATCH) need explicit idempotency key = conditional
        // - No compensation plan = not_replayable
        // - Valid http.replay_v1 contract = replay_ready
        let has_valid_replay_contract = parsed_replay.is_some();
        let replayable_classification = if has_valid_replay_contract {
            "replay_ready"
        } else {
            match (
                method,
                idempotency_key_present_in_plan,
                compensation_steps_present,
            ) {
                (HttpMethod::Get, _, true) => "potentially_replayable",
                (HttpMethod::Delete, _, true) => "potentially_replayable",
                (HttpMethod::Post, true, true) => "conditional_replayable",
                (HttpMethod::Put, true, true) => "conditional_replayable",
                (HttpMethod::Patch, true, true) => "conditional_replayable",
                (_, false, true) => "requires_idempotency_key",
                _ => "not_replayable",
            }
        };

        // Build reason codes explaining why recovery is not currently executable
        let mut reason_codes = Vec::new();
        if !compensation_steps_present {
            reason_codes.push("NO_COMPENSATION_PLAN");
        }
        if compensation_steps_present && !idempotency_key_present_in_plan {
            reason_codes.push("NO_IDEMPOTENCY_KEY_IN_COMPENSATION");
        }
        if compensation_steps_present && !has_valid_replay_contract {
            reason_codes.push("INVALID_REPLAY_CONTRACT");
        }
        if !has_valid_replay_contract {
            reason_codes.push("NO_REPLAY_CONTRACT");
        }
        reason_codes.push("NO_PERSISTED_EXECUTE_EVIDENCE");
        reason_codes.push("NO_OUTBOUND_RECOVERY_PATH");

        let http_recovery_readiness_v1 = serde_json::json!({
            "version": "http_recovery_readiness_v1",
            "replayable_classification": replayable_classification,
            "idempotency_key_present_in_plan": idempotency_key_present_in_plan,
            "compensation_steps_present": compensation_steps_present,
            "has_valid_replay_contract": has_valid_replay_contract,
            "rollback_supported": has_valid_replay_contract,
            "compensate_supported": has_valid_replay_contract,
            "reason_codes": reason_codes,
        });
        metadata.insert(
            "http_recovery_readiness_v1".to_string(),
            http_recovery_readiness_v1,
        );

        Ok(ExecuteReceipt {
            external_id: None,
            result_digest: Some(format!("http-{}", status)),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        // Validate target
        let (method, url) = Self::extract_http_target(&contract.target)?;

        // Validate URL shape
        Self::validate_url_shape(url)?;

        // If no verify_checks are provided, fail-closed with a clear reason.
        // Without explicit checks, we cannot verify the HTTP mutation succeeded.
        if contract.verify_checks.is_empty() {
            return Err(Self::phase_wrap_validation(
                PHASE_VERIFY,
                "no verify_checks provided and no default verification available for HttpMutation. \
                 HttpMutation verify requires explicit HttpStatusExpected checks to confirm \
                 the mutation had the expected effect. Provide verify_checks with HttpStatusExpected \
                 to specify the expected HTTP status code."
                    .to_string(),
            ));
        }

        // Run explicit verify_checks (fail-closed on mismatch or error)
        for check in &contract.verify_checks {
            Self::run_check(
                check,
                url,
                (*method).clone(),
                self.allow_private_networks,
                PHASE_VERIFY,
            )
            .await?;
        }

        // All checks passed
        Ok(VerifyReceipt {
            verified: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // Validate target is HttpRequest
        let (_method, url) = Self::extract_http_target(&contract.target)?;

        // Validate action type is HttpMutation
        match contract.action_type {
            ActionType::HttpMutation => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "[compensate] unsupported action type: {:?}",
                    contract.action_type
                )));
            }
        }

        // Validate URL shape
        Self::validate_url_shape(url)?;

        // Try to parse a valid http.replay_v1 contract
        let replay = match Self::parse_replay_contract(contract, PHASE_COMPENSATE) {
            Ok(r) => r,
            Err(e) => {
                // Fail closed with structured reason codes for unsupported shapes
                let reason_codes = Self::build_unsupported_reason_codes(contract);
                return Err(AdapterError::Unsupported(format!(
                    "[compensate] http.replay_v1 contract validation failed: {}. Reason codes: [{}]",
                    e,
                    reason_codes.join(", ")
                )));
            }
        };

        // Validate method is POST/PUT/PATCH (already enforced by parse_replay_contract, but double-check)
        match replay.method {
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {}
            _ => {
                return Err(AdapterError::Validation(format!(
                    "[compensate] http.replay_v1 requires POST/PUT/PATCH method, got {:?}",
                    replay.method
                )));
            }
        }

        // Validate URL matches target (already enforced by parse_replay_contract)
        // Validate digest matches target.request_digest
        let target_request_digest = match &contract.target {
            RollbackTarget::HttpRequest {
                request_digest: rd, ..
            } => rd.clone(),
            _ => {
                return Err(HttpAdapterError::ReplayValidation(
                    "[compensate] http.replay_v1 requires HttpRequest target".to_string(),
                )
                .into());
            }
        };

        // Extract body bytes from replay payload and compute digest
        let body_bytes = Self::extract_body_bytes(&replay.payload)?;
        let computed_digest =
            Self::compute_request_digest(replay.method.clone(), &replay.url, body_bytes.as_deref());
        if computed_digest != target_request_digest {
            return Err(HttpAdapterError::ReplayValidation(format!(
                "[compensate] http.replay_v1 digest mismatch: computed '{}' != target.request_digest '{}'",
                computed_digest, target_request_digest
            ))
            .into());
        }

        // Compute replay request digest BEFORE the HTTP call (need body_bytes before it's moved)
        // Also save these values before they're moved into execute_http_request
        let replay_request_digest =
            Self::compute_request_digest(replay.method.clone(), &replay.url, body_bytes.as_deref());
        let replay_method_str = format!("{:?}", replay.method);
        let replay_url = replay.url.clone();
        let replay_idempotency_key = replay.idempotency_key.clone();
        let replay_expected_statuses = replay.expected_statuses.clone();

        // Execute the replay request with idempotency key
        let (status, response_body) = Self::execute_http_request(
            replay.method,
            &replay.url,
            body_bytes,
            Some(&replay.idempotency_key),
            self.allow_private_networks,
            PHASE_COMPENSATE,
        )
        .await?;

        // Compute response digest: SHA256(status + bounded body)
        const MAX_RESPONSE_DIGEST_BYTES: usize = 64 * 1024;
        let response_body_for_digest = if response_body.len() > MAX_RESPONSE_DIGEST_BYTES {
            &response_body[..MAX_RESPONSE_DIGEST_BYTES]
        } else {
            &response_body[..]
        };
        let response_digest = {
            let mut d = Sha256::new();
            d.update(status.to_string().as_bytes());
            d.update(response_body_for_digest);
            format!("{:x}", d.finalize())
        };
        let response_body_truncated = response_body.len() > MAX_RESPONSE_DIGEST_BYTES;

        // Check expected_statuses (now required)
        if !replay_expected_statuses.contains(&status) {
            return Err(AdapterError::Validation(format!(
                "[compensate] http.replay_v1 status mismatch: expected {:?}, got {}",
                replay_expected_statuses, status
            )));
        }

        // Build recovery receipt metadata
        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "compensated_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "replay_operation".to_string(),
            serde_json::Value::String(REPLAY_OPERATION.to_string()),
        );
        metadata.insert(
            "idempotency_key".to_string(),
            serde_json::Value::String(replay_idempotency_key),
        );
        metadata.insert(
            "response_status".to_string(),
            serde_json::Value::Number(status.into()),
        );
        metadata.insert(
            "response_body_size".to_string(),
            serde_json::Value::Number(response_body.len().into()),
        );
        // Enrichment: replay audit metadata
        metadata.insert(
            "replay_target_url".to_string(),
            serde_json::Value::String(replay_url),
        );
        metadata.insert(
            "replay_method".to_string(),
            serde_json::Value::String(replay_method_str),
        );
        metadata.insert(
            "replay_request_digest".to_string(),
            serde_json::Value::String(replay_request_digest),
        );
        metadata.insert(
            "replay_response_digest".to_string(),
            serde_json::Value::String(response_digest),
        );
        metadata.insert(
            "replay_response_body_truncated".to_string(),
            serde_json::Value::Bool(response_body_truncated),
        );
        metadata.insert(
            "expected_statuses_checked".to_string(),
            serde_json::Value::Array(
                replay_expected_statuses
                    .iter()
                    .map(|&s| serde_json::Value::Number(s.into()))
                    .collect(),
            ),
        );

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: metadata,
        })
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // Validate target is HttpRequest
        let (_method, url) = Self::extract_http_target(&contract.target)?;

        // Validate action type is HttpMutation
        match contract.action_type {
            ActionType::HttpMutation => {}
            _ => {
                return Err(AdapterError::Unsupported(format!(
                    "[rollback] unsupported action type: {:?}",
                    contract.action_type
                )));
            }
        }

        // Validate URL shape
        Self::validate_url_shape(url)?;

        // Try to parse a valid http.replay_v1 contract
        let replay = match Self::parse_replay_contract(contract, PHASE_ROLLBACK) {
            Ok(r) => r,
            Err(e) => {
                // Fail closed with structured reason codes for unsupported shapes
                let reason_codes = Self::build_unsupported_reason_codes(contract);
                return Err(AdapterError::Unsupported(format!(
                    "[rollback] http.replay_v1 contract validation failed: {}. Reason codes: [{}]",
                    e,
                    reason_codes.join(", ")
                )));
            }
        };

        // Validate method is POST/PUT/PATCH (already enforced by parse_replay_contract, but double-check)
        match replay.method {
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {}
            _ => {
                return Err(AdapterError::Validation(format!(
                    "[rollback] http.replay_v1 requires POST/PUT/PATCH method, got {:?}",
                    replay.method
                )));
            }
        }

        // Validate digest matches target.request_digest
        let target_request_digest = match &contract.target {
            RollbackTarget::HttpRequest {
                request_digest: rd, ..
            } => rd.clone(),
            _ => {
                return Err(HttpAdapterError::ReplayValidation(
                    "[rollback] http.replay_v1 requires HttpRequest target".to_string(),
                )
                .into());
            }
        };

        // Extract body bytes from replay payload and compute digest
        let body_bytes = Self::extract_body_bytes(&replay.payload)?;
        let computed_digest =
            Self::compute_request_digest(replay.method.clone(), &replay.url, body_bytes.as_deref());
        if computed_digest != target_request_digest {
            return Err(HttpAdapterError::ReplayValidation(format!(
                "[rollback] http.replay_v1 digest mismatch: computed '{}' != target.request_digest '{}'",
                computed_digest, target_request_digest
            ))
            .into());
        }

        // Save these values before they're moved into execute_http_request
        let replay_method_str = format!("{:?}", replay.method);
        let replay_url = replay.url.clone();
        let replay_idempotency_key = replay.idempotency_key.clone();
        let replay_expected_statuses = replay.expected_statuses.clone();

        // Execute the replay request with idempotency key
        let (status, response_body) = Self::execute_http_request(
            replay.method,
            &replay.url,
            body_bytes,
            Some(&replay.idempotency_key),
            self.allow_private_networks,
            PHASE_ROLLBACK,
        )
        .await?;

        // Compute response digest: SHA256(status + bounded body)
        const MAX_RESPONSE_DIGEST_BYTES: usize = 64 * 1024;
        let response_body_for_digest = if response_body.len() > MAX_RESPONSE_DIGEST_BYTES {
            &response_body[..MAX_RESPONSE_DIGEST_BYTES]
        } else {
            &response_body[..]
        };
        let response_digest = {
            let mut d = Sha256::new();
            d.update(status.to_string().as_bytes());
            d.update(response_body_for_digest);
            format!("{:x}", d.finalize())
        };
        let response_body_truncated = response_body.len() > MAX_RESPONSE_DIGEST_BYTES;

        // Check expected_statuses (now required)
        if !replay_expected_statuses.contains(&status) {
            return Err(AdapterError::Validation(format!(
                "[rollback] http.replay_v1 status mismatch: expected {:?}, got {}",
                replay_expected_statuses, status
            )));
        }

        // Build recovery receipt metadata
        let mut metadata = JsonMap::new();
        metadata.insert(
            "adapter_kind".to_string(),
            serde_json::Value::String(ADAPTER_KIND.to_string()),
        );
        metadata.insert(
            "rolled_back_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        metadata.insert(
            "replay_operation".to_string(),
            serde_json::Value::String(REPLAY_OPERATION.to_string()),
        );
        metadata.insert(
            "idempotency_key".to_string(),
            serde_json::Value::String(replay_idempotency_key),
        );
        metadata.insert(
            "response_status".to_string(),
            serde_json::Value::Number(status.into()),
        );
        metadata.insert(
            "response_body_size".to_string(),
            serde_json::Value::Number(response_body.len().into()),
        );
        // Enrichment: replay audit metadata
        metadata.insert(
            "replay_target_url".to_string(),
            serde_json::Value::String(replay_url),
        );
        metadata.insert(
            "replay_method".to_string(),
            serde_json::Value::String(replay_method_str),
        );
        metadata.insert(
            "replay_request_digest".to_string(),
            serde_json::Value::String(computed_digest),
        );
        metadata.insert(
            "replay_response_digest".to_string(),
            serde_json::Value::String(response_digest),
        );
        metadata.insert(
            "replay_response_body_truncated".to_string(),
            serde_json::Value::Bool(response_body_truncated),
        );
        metadata.insert(
            "expected_statuses_checked".to_string(),
            serde_json::Value::Array(
                replay_expected_statuses
                    .iter()
                    .map(|&s| serde_json::Value::Number(s.into()))
                    .collect(),
            ),
        );

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: metadata,
        })
    }
}

#[cfg(test)]
mod tests;
