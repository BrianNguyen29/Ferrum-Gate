use async_trait::async_trait;
use ferrum_proto::{
    CheckSpec, CheckType, HttpMethod, JsonMap, RollbackContract, RollbackPrepareRequest,
    RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, AdapterRegistry, ExecuteReceipt, PrepareReceipt, RecoveryReceipt,
    RollbackAdapter, VerifyReceipt,
};
use reqwest::{Client, Url, header::HeaderName};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

pub const ADAPTER_KEY: &str = "http";
const APPROVED_HTTP_REQUEST_METADATA_KEY: &str = "approved_http_request";

#[derive(Debug, Error)]
pub enum HttpAdapterError {
    #[error("unsupported action: {0}")]
    Unsupported(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("HTTP request failed: {0}")]
    RequestFailed(String),
}

impl From<HttpAdapterError> for AdapterError {
    fn from(e: HttpAdapterError) -> Self {
        match e {
            HttpAdapterError::Unsupported(s) => AdapterError::Unsupported(s),
            HttpAdapterError::Validation(s) => AdapterError::Validation(s),
            HttpAdapterError::Internal(s) => AdapterError::Internal(s),
            HttpAdapterError::RequestFailed(s) => AdapterError::Internal(s),
        }
    }
}

/// HttpRollbackAdapter provides conservative HTTP request capture and verification.
///
/// Supported operations:
/// - `prepare`: captures bound scope plus approved-request digest metadata
/// - `execute`: performs HTTP requests (GET/POST/PUT/PATCH/DELETE) with body handling
/// - `verify`: validates status - GET can re-request; mutations use execute-time metadata only
/// - `rollback`: conservative no-op (remote mutation recovery is not established)
/// - `compensate`: alias for rollback in this slice
///
/// URL scope semantics:
/// - `RollbackTarget::HttpRequest.url` represents the BOUND URL scope/prefix (base_url + path_prefix)
/// - Execute-time payload may contain a concrete URL within that scope
/// - Adapter validates fail-closed that actual URL stays within bound scope prefix
/// - Method must match the bound method
///
/// Approved request digest semantics:
/// - `prepare` may receive the approved HTTP request payload via transient metadata
/// - GET digest = SHA256(method:url[:headers])
/// - POST/PUT/PATCH/DELETE digest = SHA256(method:url:body[:headers]) where body is canonical JSON or empty
/// - Header names are canonicalized to lowercase before digesting
/// - The approved digest binds execute-time payloads to the concrete approved request without
///   broadening rollback/recovery guarantees for remote mutation methods
///
/// Verify semantics by method:
/// - GET: re-requests if explicit HttpStatusExpected check; otherwise uses execute-time metadata
/// - Mutations: ALWAYS uses execute-time metadata only (no replay). Fail-closed if metadata missing.
///
/// Metadata keys (clearer naming to distinguish bound vs executed):
/// - `bound_url`: the allowed URL scope prefix from prepare
/// - `bound_method`: the allowed method from prepare
/// - `bound_request_digest`: digest of the bound scope target
/// - `approved_url`: concrete approved URL resolved at prepare time
/// - `approved_method`: concrete approved method resolved at prepare time
/// - `approved_body_digest`: SHA256(canonical body) for the approved request body
/// - `approved_headers_digest`: SHA256(canonical lowercase header map) for approved headers
/// - `approved_request_digest`: digest of the approved concrete request
/// - `executed_url`: the concrete URL actually executed (from payload or bound default)
/// - `executed_method`: the method actually executed (from payload or bound default)
/// - `executed_body_digest`: SHA256(canonical body) for the execute-time request body
/// - `executed_headers_digest`: SHA256(canonical lowercase header map) for execute-time headers
/// - `executed_request_digest`: digest computed at execute time including actual body/header shape
///
/// This slice is conservative:
/// - rollback/compensate are no-ops since mutation recovery guarantees are not yet established
/// - Response bodies are not captured or compared
/// - Destructive remote mutation recovery remains an explicit R3 boundary
pub struct HttpRollbackAdapter {
    client: Client,
}

impl HttpRollbackAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Extract HTTP target from RollbackTarget::HttpRequest.
    /// Returns (bound_method, bound_url, bound_request_digest) representing the scope/prefix.
    fn extract_http_target(
        target: &RollbackTarget,
    ) -> Result<(HttpMethod, String, String), HttpAdapterError> {
        match target {
            RollbackTarget::HttpRequest {
                method,
                url,
                request_digest,
            } => Ok((method.clone(), url.clone(), request_digest.clone())),
            _ => Err(HttpAdapterError::Validation(format!(
                "expected HttpRequest target, got {:?}",
                target
            ))),
        }
    }

    /// Compute a SHA256 digest from method and URL for scope-level identification.
    #[cfg_attr(not(test), allow(dead_code))]
    fn compute_request_digest(method: &HttpMethod, url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}:{}", method, url).as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Canonicalize a query string by sorting key-value pairs.
    /// Returns (canonical_url, canonical_query_string_or_empty).
    /// Example: "https://example.com/api?b=2&a=1" -> ("https://example.com/api?a=1&b=2", "a=1&b=2")
    fn canonicalize_query_string(url: &str) -> (String, String) {
        let Ok(mut parsed) = Url::parse(url) else {
            return (url.to_string(), String::new());
        };

        let query = match parsed.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                parsed.set_query(None);
                return (parsed.to_string(), String::new());
            }
        };

        let mut params: Vec<(String, String)> = Self::parse_query_string(&query);
        params.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        let canonical_query = params
            .iter()
            .map(|(k, v): &(String, String)| {
                if v.is_empty() {
                    k.clone()
                } else {
                    format!("{}={}", k, v)
                }
            })
            .collect::<Vec<_>>()
            .join("&");

        parsed.set_query(Some(&canonical_query));

        (parsed.to_string(), canonical_query)
    }

    /// Parse a query string into key-value pairs, preserving values (including empty).
    fn parse_query_string(query: &str) -> Vec<(String, String)> {
        query
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?.to_string();
                let value = parts.next().unwrap_or("").to_string();
                Some((key, value))
            })
            .collect()
    }

    /// Compute a SHA256 digest for query string presence.
    /// Empty or absent query produces an empty string digest.
    fn compute_query_digest(query: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        match query {
            Some(q) if !q.is_empty() => {
                hasher.update(q.as_bytes());
            }
            _ => {}
        }
        format!("{:x}", hasher.finalize())
    }

    /// Extract canonical query string from a URL, returning None if no query is present.
    fn extract_query_from_url(url: &str) -> Option<String> {
        let (_, canonical_query) = Self::canonicalize_query_string(url);
        if canonical_query.is_empty() {
            None
        } else {
            Some(canonical_query)
        }
    }

    fn canonical_headers(
        headers: Option<&HashMap<String, String>>,
    ) -> Option<BTreeMap<String, String>> {
        let headers = headers?;
        if headers.is_empty() {
            return None;
        }

        Some(
            headers
                .iter()
                .map(|(key, value)| (key.to_lowercase(), value.clone()))
                .collect(),
        )
    }

    fn compute_headers_digest(headers: Option<&HashMap<String, String>>) -> String {
        let mut hasher = Sha256::new();
        let headers_str = match Self::canonical_headers(headers) {
            Some(headers) => serde_json::to_string(&headers).unwrap_or_default(),
            None => String::new(),
        };
        hasher.update(headers_str.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Compute a SHA256 digest for concrete HTTP request shape.
    /// - GET ignores body but includes canonicalized headers when present.
    /// - POST/PUT/PATCH/DELETE include canonical JSON body and canonicalized headers.
    /// - Query strings are canonicalized (sorted by key) before digesting to ensure
    ///   semantically identical query strings produce the same digest.
    fn compute_body_aware_digest(
        method: &HttpMethod,
        url: &str,
        body: &serde_json::Value,
        headers: Option<&HashMap<String, String>>,
    ) -> String {
        let mut hasher = Sha256::new();
        let headers_str = Self::canonical_headers(headers)
            .as_ref()
            .map(|headers| serde_json::to_string(headers).unwrap_or_default());

        // Canonicalize URL query string for consistent digest computation
        let (canonical_url, _canonical_query) = Self::canonicalize_query_string(url);

        if Self::is_mutation_method(method) && !body.is_null() {
            let body_str = serde_json::to_string(body).unwrap_or_default();
            hasher.update(format!("{:?}:{}:{}", method, canonical_url, body_str).as_bytes());
        } else {
            hasher.update(format!("{:?}:{}", method, canonical_url).as_bytes());
        }

        if let Some(headers_str) = headers_str {
            hasher.update(format!(":{}", headers_str).as_bytes());
        }

        format!("{:x}", hasher.finalize())
    }

    fn headers_present(headers: Option<&HashMap<String, String>>) -> bool {
        Self::canonical_headers(headers).is_some()
    }

    fn compute_body_digest(body: &serde_json::Value) -> String {
        let mut hasher = Sha256::new();
        let body_str = if body.is_null() {
            String::new()
        } else {
            serde_json::to_string(body).unwrap_or_default()
        };
        hasher.update(body_str.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Returns true if the HTTP method is mutation-capable (has side effects).
    fn is_mutation_method(method: &HttpMethod) -> bool {
        matches!(
            method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch | HttpMethod::Delete
        )
    }

    /// Extract expected status code from verify_checks if HttpStatusExpected is present.
    fn extract_expected_status(checks: &[CheckSpec]) -> Option<u16> {
        for check in checks {
            if matches!(check.check_type, CheckType::HttpStatusExpected) {
                if let Some(status) = check.config.get("status") {
                    return status.as_u64().map(|s| s as u16);
                }
            }
        }
        None
    }

    fn parse_http_method(raw: &str) -> Option<HttpMethod> {
        match raw.to_uppercase().as_str() {
            "GET" => Some(HttpMethod::Get),
            "POST" => Some(HttpMethod::Post),
            "PUT" => Some(HttpMethod::Put),
            "PATCH" => Some(HttpMethod::Patch),
            "DELETE" => Some(HttpMethod::Delete),
            _ => None,
        }
    }

    /// Parse URL, method, body, and headers from a request payload if present.
    /// Returns (payload_url, payload_method, payload_body, payload_headers) where any may be absent.
    /// Stay fail-closed if the payload tries to provide malformed or unsupported values.
    fn parse_request_parts(
        payload: &serde_json::Value,
    ) -> Result<
        (
            Option<String>,
            Option<HttpMethod>,
            serde_json::Value,
            Option<HashMap<String, String>>,
        ),
        HttpAdapterError,
    > {
        let obj = match payload.as_object() {
            Some(o) => o,
            None => return Ok((None, None, serde_json::Value::Null, None)),
        };

        let url = match obj.get("url") {
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| {
                        HttpAdapterError::Validation(
                            "payload url must be a string when provided".to_string(),
                        )
                    })?
                    .to_string(),
            ),
            None => None,
        };

        let method = match obj.get("method") {
            Some(value) => {
                let raw = value.as_str().ok_or_else(|| {
                    HttpAdapterError::Validation(
                        "payload method must be a string when provided".to_string(),
                    )
                })?;
                Some(Self::parse_http_method(raw).ok_or_else(|| {
                    HttpAdapterError::Validation(format!(
                        "unsupported HTTP method in payload: {}",
                        raw
                    ))
                })?)
            }
            None => None,
        };

        let body = obj.get("body").cloned().unwrap_or(serde_json::Value::Null);

        let headers = match obj.get("headers") {
            Some(value) => {
                let headers_obj = value.as_object().ok_or_else(|| {
                    HttpAdapterError::Validation(
                        "payload headers must be an object when provided".to_string(),
                    )
                })?;
                let mut headers_map = HashMap::new();
                for (k, v) in headers_obj {
                    let header_value = v.as_str().ok_or_else(|| {
                        HttpAdapterError::Validation(format!(
                            "header value for '{}' must be a string",
                            k
                        ))
                    })?;
                    headers_map.insert(k.to_lowercase(), header_value.to_string());
                }
                if headers_map.is_empty() {
                    None
                } else {
                    Some(headers_map)
                }
            }
            None => None,
        };

        Ok((url, method, body, headers))
    }

    fn resolve_request_parts(
        bound_method: &HttpMethod,
        bound_url: &str,
        payload: &serde_json::Value,
    ) -> Result<
        (
            String,
            HttpMethod,
            serde_json::Value,
            Option<HashMap<String, String>>,
        ),
        HttpAdapterError,
    > {
        let (payload_url, payload_method, payload_body, payload_headers) =
            Self::parse_request_parts(payload)?;
        let resolved_url = payload_url.unwrap_or_else(|| bound_url.to_string());
        let resolved_method = payload_method.unwrap_or(bound_method.clone());

        Self::validate_url_within_scope(&resolved_url, bound_url)
            .map_err(HttpAdapterError::Validation)?;

        if resolved_method != *bound_method {
            return Err(HttpAdapterError::Validation(format!(
                "executed method {:?} does not match bound method {:?}",
                resolved_method, bound_method
            )));
        }

        Ok((resolved_url, resolved_method, payload_body, payload_headers))
    }

    /// Validate that the executed URL stays within the bound URL scope.
    /// The executed URL must match scheme/host/port and stay within the bound path prefix.
    /// Returns Ok(()) if valid, Err(message) if fail-closed.
    fn validate_url_within_scope(executed_url: &str, bound_url: &str) -> Result<(), String> {
        let executed = Url::parse(executed_url).map_err(|e| {
            format!(
                "executed URL '{}' is not a valid absolute URL: {}",
                executed_url, e
            )
        })?;
        let bound = Url::parse(bound_url).map_err(|e| {
            format!(
                "bound URL '{}' is not a valid absolute URL: {}",
                bound_url, e
            )
        })?;

        let executed_host = executed.host_str().unwrap_or_default();
        let bound_host = bound.host_str().unwrap_or_default();
        if executed.scheme() != bound.scheme()
            || executed_host != bound_host
            || executed.port_or_known_default() != bound.port_or_known_default()
        {
            return Err(format!(
                "executed URL '{}' is not within bound scope '{}'",
                executed_url, bound_url
            ));
        }

        let executed_path = executed.path();
        let bound_path = bound.path();
        let path_allowed = if bound_path.ends_with('/') {
            executed_path.starts_with(bound_path)
        } else {
            executed_path == bound_path
                || executed_path.starts_with(&format!("{}/", bound_path.trim_end_matches('/')))
        };

        if !path_allowed {
            return Err(format!(
                "executed URL '{}' is not within bound scope '{}'",
                executed_url, bound_url
            ));
        }

        Ok(())
    }
}

impl Default for HttpRollbackAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RollbackAdapter for HttpRollbackAdapter {
    fn key(&self) -> &'static str {
        ADAPTER_KEY
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        let (method, url, bound_request_digest) =
            Self::extract_http_target(&request.target).map_err(AdapterError::from)?;

        // Validate URL is well-formed
        if url.is_empty() {
            return Err(AdapterError::Validation("URL cannot be empty".to_string()).into());
        }

        let approved_request_payload = request
            .metadata
            .get(APPROVED_HTTP_REQUEST_METADATA_KEY)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let (approved_url, approved_method, approved_body, approved_headers) =
            Self::resolve_request_parts(&method, &url, &approved_request_payload)
                .map_err(AdapterError::from)?;
        let approved_request_digest = Self::compute_body_aware_digest(
            &approved_method,
            &approved_url,
            &approved_body,
            approved_headers.as_ref(),
        );
        let approved_body_digest = Self::compute_body_digest(&approved_body);
        let approved_headers_digest = Self::compute_headers_digest(approved_headers.as_ref());
        let approved_query = Self::extract_query_from_url(&approved_url);
        let approved_query_digest = Self::compute_query_digest(approved_query.as_deref());
        let approved_query_present = approved_query.is_some();

        let mut metadata = JsonMap::new();
        metadata.insert(
            "bound_method".to_string(),
            serde_json::json!(format!("{:?}", method)),
        );
        metadata.insert("bound_url".to_string(), serde_json::json!(url));
        metadata.insert(
            "bound_request_digest".to_string(),
            serde_json::json!(bound_request_digest),
        );
        metadata.insert(
            "approved_method".to_string(),
            serde_json::json!(format!("{:?}", approved_method)),
        );
        metadata.insert("approved_url".to_string(), serde_json::json!(approved_url));
        metadata.insert(
            "approved_body_digest".to_string(),
            serde_json::json!(approved_body_digest),
        );
        metadata.insert(
            "approved_headers_digest".to_string(),
            serde_json::json!(approved_headers_digest),
        );
        metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(approved_request_digest),
        );
        metadata.insert(
            "approved_body_present".to_string(),
            serde_json::json!(!approved_body.is_null()),
        );
        metadata.insert(
            "approved_headers_present".to_string(),
            serde_json::json!(Self::headers_present(approved_headers.as_ref())),
        );
        metadata.insert(
            "approved_query_present".to_string(),
            serde_json::json!(approved_query_present),
        );
        metadata.insert(
            "approved_query_digest".to_string(),
            serde_json::json!(approved_query_digest),
        );

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
        let (bound_method, bound_url, _) =
            Self::extract_http_target(&contract.target).map_err(AdapterError::from)?;

        let (executed_url, executed_method, executed_body, executed_headers) =
            Self::resolve_request_parts(&bound_method, &bound_url, payload)
                .map_err(AdapterError::from)?;

        let executed_request_digest = Self::compute_body_aware_digest(
            &executed_method,
            &executed_url,
            &executed_body,
            executed_headers.as_ref(),
        );
        let executed_body_digest = Self::compute_body_digest(&executed_body);
        let executed_headers_digest = Self::compute_headers_digest(executed_headers.as_ref());
        let executed_query = Self::extract_query_from_url(&executed_url);
        let executed_query_digest = Self::compute_query_digest(executed_query.as_deref());
        let executed_query_present = executed_query.is_some();

        if let Some(approved_request_digest) = contract
            .metadata
            .get("approved_request_digest")
            .and_then(|value| value.as_str())
        {
            if approved_request_digest != executed_request_digest {
                return Err(AdapterError::Validation(format!(
                    "executed request digest does not match approved request digest: approved={} executed={}",
                    approved_request_digest, executed_request_digest
                ))
                .into());
            }
        }

        // Execute the HTTP request with the appropriate method.
        // For mutation-capable methods, body is sent as canonical JSON when present.
        let mut request = match executed_method {
            HttpMethod::Get => self.client.get(&executed_url),
            HttpMethod::Post => self.client.post(&executed_url),
            HttpMethod::Put => self.client.put(&executed_url),
            HttpMethod::Patch => self.client.patch(&executed_url),
            HttpMethod::Delete => self.client.delete(&executed_url),
        };
        if Self::is_mutation_method(&executed_method) && !executed_body.is_null() {
            request = request.json(&executed_body);
        }
        // Apply headers to the request if provided.
        // Header validation against allowlist is performed by the firewall before this adapter executes.
        if let Some(ref headers) = executed_headers {
            for (name, value) in headers {
                let header_name = HeaderName::try_from(name.as_str()).map_err(|e| {
                    HttpAdapterError::Validation(format!("invalid header name '{}': {}", name, e))
                })?;
                request = request.header(header_name, value);
            }
        }
        let response = request.send().await.map_err(|e| {
            HttpAdapterError::RequestFailed(format!("{:?} request failed: {}", executed_method, e))
        })?;
        let status = response.status().as_u16();

        let mut metadata = JsonMap::new();
        metadata.insert(
            "bound_method".to_string(),
            serde_json::json!(format!("{:?}", bound_method)),
        );
        metadata.insert("bound_url".to_string(), serde_json::json!(bound_url));
        metadata.insert(
            "executed_method".to_string(),
            serde_json::json!(format!("{:?}", executed_method)),
        );
        metadata.insert("executed_url".to_string(), serde_json::json!(executed_url));
        metadata.insert(
            "executed_body_digest".to_string(),
            serde_json::json!(executed_body_digest),
        );
        metadata.insert(
            "executed_headers_digest".to_string(),
            serde_json::json!(executed_headers_digest),
        );
        metadata.insert(
            "executed_body_present".to_string(),
            serde_json::json!(!executed_body.is_null()),
        );
        metadata.insert(
            "executed_headers_present".to_string(),
            serde_json::json!(Self::headers_present(executed_headers.as_ref())),
        );
        metadata.insert(
            "executed_query_present".to_string(),
            serde_json::json!(executed_query_present),
        );
        metadata.insert(
            "executed_query_digest".to_string(),
            serde_json::json!(executed_query_digest),
        );
        metadata.insert("status".to_string(), serde_json::json!(status));
        metadata.insert("executed".to_string(), serde_json::json!(true));
        metadata.insert(
            "executed_request_digest".to_string(),
            serde_json::json!(executed_request_digest),
        );

        Ok(ExecuteReceipt {
            external_id: None,
            result_digest: Some(status.to_string()),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let (bound_method, bound_url, _) =
            Self::extract_http_target(&contract.target).map_err(AdapterError::from)?;

        // For mutation-capable methods, verify MUST use execute-time metadata only.
        // Replaying mutating requests during verify is NOT safe - it would re-execute the side effect.
        // This is the key distinction from GET where re-requesting is safe.
        let is_mutation = Self::is_mutation_method(&bound_method);

        // Get expected status from verify_checks (explicit expectation)
        let explicit_expected_status = Self::extract_expected_status(&contract.verify_checks);

        // Get execute-time status from metadata (always required for mutations)
        let execute_status = contract
            .metadata
            .get("status")
            .and_then(|v| v.as_u64())
            .map(|s| s as u16);

        let verify_url = contract
            .metadata
            .get("executed_url")
            .and_then(|v| v.as_str())
            .unwrap_or(&bound_url)
            .to_string();

        // Fail-closed: mutation methods require execute-time status in metadata
        if is_mutation && execute_status.is_none() {
            return Ok(VerifyReceipt {
                verified: false,
                adapter_metadata: {
                    let mut m = JsonMap::new();
                    m.insert(
                        "bound_method".to_string(),
                        serde_json::json!(format!("{:?}", bound_method)),
                    );
                    m.insert("bound_url".to_string(), serde_json::json!(bound_url));
                    m.insert("verify_url".to_string(), serde_json::json!(verify_url));
                    m.insert(
                        "reason".to_string(),
                        serde_json::json!(
                            "mutation method verify requires execute-time status in metadata"
                        ),
                    );
                    m
                },
            });
        }

        // Determine expected status:
        // - Explicit check if provided
        // - Otherwise use execute-time metadata (required for mutations, optional for GET)
        let expected_status = match explicit_expected_status {
            Some(s) => s,
            None => {
                match execute_status {
                    Some(s) => s,
                    None => {
                        // GET with no status metadata - fail closed
                        return Ok(VerifyReceipt {
                            verified: false,
                            adapter_metadata: {
                                let mut m = JsonMap::new();
                                m.insert(
                                    "bound_method".to_string(),
                                    serde_json::json!(format!("{:?}", bound_method)),
                                );
                                m.insert("bound_url".to_string(), serde_json::json!(bound_url));
                                m.insert("verify_url".to_string(), serde_json::json!(verify_url));
                                m.insert(
                                    "reason".to_string(),
                                    serde_json::json!("no HttpStatusExpected check and no execute-time status in metadata"),
                                );
                                m
                            },
                        });
                    }
                }
            }
        };

        // For GET with explicit check: optionally re-request to verify current server state
        // For mutations: always use execute-time metadata (do not replay)
        // For GET without explicit check: only 2xx auto-verifies via execute-time metadata
        let (verified, actual_status) = if !is_mutation && explicit_expected_status.is_some() {
            // GET with explicit check: re-request to verify actual current state
            let response = self.client.get(&verify_url).send().await.map_err(|e| {
                HttpAdapterError::RequestFailed(format!("GET request failed: {}", e))
            })?;
            let actual = response.status().as_u16();
            (actual == expected_status, Some(actual))
        } else if !is_mutation {
            // GET without explicit check: auto-verify only 2xx via execute-time metadata
            let actual = execute_status.unwrap_or(expected_status);
            let verified = (200..300).contains(&actual);
            (verified, Some(actual))
        } else if explicit_expected_status.is_some() {
            // Mutation with explicit expectation: crosscheck execute-time status only (no replay)
            let actual = execute_status.unwrap();
            let verified = actual == expected_status;
            (verified, Some(actual))
        } else {
            // Mutation without explicit check: auto-verify only successful execute-time statuses.
            // Stay fail-closed for non-2xx outcomes like 4xx/5xx.
            let actual = execute_status.unwrap();
            let verified = (200..300).contains(&actual);
            (verified, Some(actual))
        };

        let mut metadata = JsonMap::new();
        metadata.insert(
            "bound_method".to_string(),
            serde_json::json!(format!("{:?}", bound_method)),
        );
        metadata.insert("bound_url".to_string(), serde_json::json!(bound_url));
        metadata.insert(
            "expected_status".to_string(),
            serde_json::json!(expected_status),
        );
        if let Some(actual) = actual_status {
            metadata.insert("actual_status".to_string(), serde_json::json!(actual));
        }
        metadata.insert("verify_url".to_string(), serde_json::json!(verify_url));
        if is_mutation || explicit_expected_status.is_none() {
            metadata.insert(
                "verified_via".to_string(),
                serde_json::json!("execute-time metadata"),
            );
        }
        metadata.insert("verified".to_string(), serde_json::json!(verified));

        Ok(VerifyReceipt {
            verified,
            adapter_metadata: metadata,
        })
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // Compensate is the same as rollback for this slice
        self.rollback(contract).await
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        let (_bound_method, _bound_url, bound_request_digest) =
            Self::extract_http_target(&contract.target).map_err(AdapterError::from)?;

        // Conservative no-op for this slice.
        // GET has no side effects and rollback is a no-op.
        // Mutating HTTP methods (POST/PUT/PATCH/DELETE) require explicit R3 boundary
        // at intent compile time; this adapter cannot recover remote mutations.

        let mut metadata = JsonMap::new();
        metadata.insert(
            "bound_request_digest".to_string(),
            serde_json::json!(bound_request_digest),
        );
        metadata.insert("rollback".to_string(), serde_json::json!("no-op"));
        metadata.insert(
            "reason".to_string(),
            serde_json::json!(
                "HTTP adapter rollback is conservative no-op; mutating HTTP methods require explicit R3 boundary at compile time"
            ),
        );

        Ok(RecoveryReceipt {
            recovered: true,
            adapter_metadata: metadata,
        })
    }
}

/// Register this adapter with a registry.
pub fn register_http_adapter(registry: &mut AdapterRegistry) {
    registry.register(std::sync::Arc::new(HttpRollbackAdapter::new()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_proto::{
        ActionType, CheckSpec, CheckType, RollbackClass, RollbackContract, RollbackState,
    };
    use std::io::{Read, Write};

    fn make_http_target(method: HttpMethod, url: &str) -> RollbackTarget {
        let request_digest = HttpRollbackAdapter::compute_request_digest(&method, url);
        RollbackTarget::HttpRequest {
            method,
            url: url.to_string(),
            request_digest,
        }
    }

    fn make_prepare_request(target: RollbackTarget) -> RollbackPrepareRequest {
        make_prepare_request_with_metadata(target, JsonMap::new())
    }

    fn make_prepare_request_with_metadata(
        target: RollbackTarget,
        metadata: JsonMap,
    ) -> RollbackPrepareRequest {
        RollbackPrepareRequest {
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id: ferrum_proto::ExecutionId::new(),
            action_type: ActionType::HttpMutation,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: ADAPTER_KEY.to_string(),
            target,
            prepare_checks: vec![],
            verify_checks: vec![],
            compensation_plan: vec![],
            auto_commit: false,
            metadata,
        }
    }

    fn make_contract(
        target: RollbackTarget,
        metadata: JsonMap,
        verify_checks: Vec<CheckSpec>,
    ) -> RollbackContract {
        RollbackContract {
            contract_id: ferrum_proto::RollbackContractId::new(),
            intent_id: ferrum_proto::IntentId::new(),
            proposal_id: ferrum_proto::ProposalId::new(),
            execution_id: ferrum_proto::ExecutionId::new(),
            action_type: ActionType::HttpMutation,
            rollback_class: RollbackClass::R1SnapshotRecoverable,
            adapter_key: ADAPTER_KEY.to_string(),
            target,
            prepare_checks: vec![],
            verify_checks,
            compensation_plan: vec![],
            auto_commit: false,
            state: RollbackState::Prepared,
            created_at: chrono::Utc::now(),
            expires_at: None,
            metadata,
        }
    }

    fn make_status_check(expected: u16) -> CheckSpec {
        CheckSpec {
            check_type: CheckType::HttpStatusExpected,
            config: {
                let mut m = JsonMap::new();
                m.insert("status".to_string(), serde_json::json!(expected));
                m
            },
        }
    }

    /// Start a local HTTP server on a random port and return (port, join_handle).
    /// The server handles exactly one request and exits.
    fn start_local_server(response_status: u16) -> (u16, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // Read request headers
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                // Send HTTP response with configurable status
                let status_line = format!(
                    "HTTP/1.1 {} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    response_status,
                    status_text(response_status)
                );
                let _ = stream.write_all(status_line.as_bytes());
                let _ = stream.flush();
            }
        });
        // Give server a moment to start
        std::thread::sleep(std::time::Duration::from_millis(10));
        (port, handle)
    }

    fn status_text(code: u16) -> &'static str {
        match code {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Unknown",
        }
    }

    #[tokio::test]
    async fn test_prepare_captures_request_metadata() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");
        let request = make_prepare_request(target);

        let receipt = adapter.prepare(&request).await.unwrap();

        assert!(receipt.accepted);
        let meta = receipt.adapter_metadata;
        // Updated keys to use "bound_" prefix to clarify scope semantics
        assert_eq!(meta.get("bound_method").unwrap().as_str().unwrap(), "Get");
        assert_eq!(
            meta.get("bound_url").unwrap().as_str().unwrap(),
            "https://example.com/api"
        );
        assert!(meta.get("bound_request_digest").unwrap().is_string());
    }

    #[tokio::test]
    async fn test_prepare_rejects_empty_url() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "");
        let request = make_prepare_request(target);

        let err = adapter.prepare(&request).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("empty"));
            }
            _ => panic!("expected validation error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_prepare_captures_approved_request_digest_for_mutation_payload() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");
        let mut metadata = JsonMap::new();
        metadata.insert(
            APPROVED_HTTP_REQUEST_METADATA_KEY.to_string(),
            serde_json::json!({
                "url": "https://example.com/api/users",
                "method": "POST",
                "body": {"name": "test"}
            }),
        );
        let request = make_prepare_request_with_metadata(target, metadata);

        let receipt = adapter.prepare(&request).await.unwrap();
        let meta = receipt.adapter_metadata;

        assert_eq!(
            meta.get("approved_method").unwrap().as_str().unwrap(),
            "Post"
        );
        assert_eq!(
            meta.get("approved_url").unwrap().as_str().unwrap(),
            "https://example.com/api/users"
        );
        assert_eq!(
            meta.get("approved_body_present").unwrap().as_bool(),
            Some(true)
        );
        assert!(meta.get("approved_body_digest").unwrap().is_string());
        assert!(meta.get("approved_request_digest").unwrap().is_string());
        assert_ne!(
            meta.get("bound_request_digest"),
            meta.get("approved_request_digest")
        );
    }

    #[tokio::test]
    async fn test_prepare_rejects_approved_payload_outside_scope() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");
        let mut metadata = JsonMap::new();
        metadata.insert(
            APPROVED_HTTP_REQUEST_METADATA_KEY.to_string(),
            serde_json::json!({
                "url": "https://example.com/other",
                "method": "POST",
                "body": {"name": "test"}
            }),
        );
        let request = make_prepare_request_with_metadata(target, metadata);

        let err = adapter.prepare(&request).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => assert!(msg.contains("not within bound scope")),
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_prepare_rejects_approved_payload_method_mismatch() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");
        let mut metadata = JsonMap::new();
        metadata.insert(
            APPROVED_HTTP_REQUEST_METADATA_KEY.to_string(),
            serde_json::json!({
                "url": "https://example.com/api/users",
                "method": "PUT",
                "body": {"name": "test"}
            }),
        );
        let request = make_prepare_request_with_metadata(target, metadata);

        let err = adapter.prepare(&request).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => assert!(msg.contains("does not match bound method")),
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_verify_fails_without_status_check() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(!receipt.verified);
        let meta = receipt.adapter_metadata;
        assert!(
            meta.get("reason")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("no HttpStatusExpected")
        );
    }

    #[tokio::test]
    async fn test_verify_returns_true_when_status_matches() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, &format!("http://127.0.0.1:{}/test", port));
        let check = make_status_check(200);
        let contract = make_contract(target, JsonMap::new(), vec![check]);

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(receipt.verified);
        assert_eq!(
            receipt
                .adapter_metadata
                .get("actual_status")
                .unwrap()
                .as_u64()
                .unwrap(),
            200
        );
        let _ = handle.join();
    }

    #[tokio::test]
    async fn test_verify_returns_false_when_status_differs() {
        let (port, handle) = start_local_server(404);
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, &format!("http://127.0.0.1:{}/test", port));
        let check = make_status_check(200); // Expect 200 but server returns 404
        let contract = make_contract(target, JsonMap::new(), vec![check]);

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(!receipt.verified);
        assert_eq!(
            receipt
                .adapter_metadata
                .get("actual_status")
                .unwrap()
                .as_u64()
                .unwrap(),
            404
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("expected_status")
                .unwrap()
                .as_u64()
                .unwrap(),
            200
        );
        let _ = handle.join();
    }

    #[tokio::test]
    async fn test_rollback_is_conservative_noop() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let receipt = adapter.rollback(&contract).await.unwrap();

        // Rollback succeeds but is a no-op
        assert!(receipt.recovered);
        let meta = receipt.adapter_metadata;
        assert_eq!(meta.get("rollback").unwrap().as_str().unwrap(), "no-op");
    }

    #[tokio::test]
    async fn test_compensate_same_as_rollback() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let compensate_receipt = adapter.compensate(&contract).await.unwrap();
        let rollback_receipt = adapter.rollback(&contract).await.unwrap();

        assert_eq!(compensate_receipt.recovered, rollback_receipt.recovered);
        assert_eq!(
            compensate_receipt.adapter_metadata.get("rollback"),
            rollback_receipt.adapter_metadata.get("rollback")
        );
    }

    #[tokio::test]
    async fn test_execute_returns_status_in_result_digest() {
        let (port, handle) = start_local_server(201);
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, &format!("http://127.0.0.1:{}/test", port));
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let receipt = adapter
            .execute(&contract, &serde_json::json!({}))
            .await
            .unwrap();

        assert!(receipt.result_digest.is_some());
        assert_eq!(receipt.result_digest.unwrap(), "201");
        let _ = handle.join();
    }

    /// Regression test: non-success execute-time HTTP status must NOT verify.
    /// Without explicit HttpStatusExpected, only 2xx statuses should auto-verify.
    /// A 500 status should cause verify to return verified=false (fail-closed).
    #[tokio::test]
    async fn test_verify_fails_for_non_success_execute_time_status() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");

        // Simulate execute-time metadata with a 500 Internal Server Error
        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(500));

        // No explicit verify_checks - relying on execute-time metadata fallback
        let contract = make_contract(target, metadata, vec![]);

        let receipt = adapter.verify(&contract).await.unwrap();

        // Must NOT verify - 500 is not a successful HTTP status (fail-closed)
        assert!(
            !receipt.verified,
            "500 status should NOT auto-verify; verify must be fail-closed. metadata={:?}",
            receipt.adapter_metadata
        );
    }

    /// Verify that 2xx execute-time status DOES verify (positive case for the fix).
    #[tokio::test]
    async fn test_verify_succeeds_for_success_execute_time_status() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");

        // Simulate execute-time metadata with a 200 OK
        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(200));

        // No explicit verify_checks - relying on execute-time metadata fallback
        let contract = make_contract(target, metadata, vec![]);

        let receipt = adapter.verify(&contract).await.unwrap();

        // 200 IS a successful status and should verify
        assert!(
            receipt.verified,
            "200 status should auto-verify; got {:?}",
            receipt.adapter_metadata
        );
    }

    /// Test that execute uses payload URL when provided (concrete URL within bound scope prefix).
    /// The bound URL is https://example.com/api but payload specifies https://example.com/api/users
    /// which is within scope, so it should succeed.
    #[tokio::test]
    async fn test_execute_uses_payload_url_within_scope() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        // Bound URL scope: base path only
        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);

        // Payload URL is a sub-path within the bound scope
        let payload_url = format!("http://127.0.0.1:{}/api/users", port);
        let mut metadata = JsonMap::new();
        metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(HttpRollbackAdapter::compute_body_aware_digest(
                &HttpMethod::Get,
                &payload_url,
                &serde_json::Value::Null,
                None,
            )),
        );
        let contract = make_contract(target, metadata, vec![]);
        let payload = serde_json::json!({
            "url": payload_url,
            "method": "GET"
        });

        let receipt = adapter.execute(&contract, &payload).await.unwrap();

        // Execute should succeed and use the payload URL
        assert!(receipt.result_digest.is_some());
        let meta = &receipt.adapter_metadata;
        // bound_url should be the scope prefix
        assert_eq!(meta.get("bound_url").unwrap().as_str().unwrap(), bound_url);
        // executed_url should be the concrete URL from payload
        assert_eq!(
            meta.get("executed_url").unwrap().as_str().unwrap(),
            payload_url
        );
        assert_eq!(
            meta.get("executed_request_digest"),
            contract.metadata.get("approved_request_digest")
        );
        let _ = handle.join();
    }

    #[tokio::test]
    async fn test_execute_rejects_request_digest_mismatch_against_approved_request() {
        let adapter = HttpRollbackAdapter::new();
        let bound_url = "https://example.com/api";
        let target = make_http_target(HttpMethod::Get, bound_url);
        let approved_url = "https://example.com/api/users";
        let executed_url = "https://example.com/api/projects";

        let mut metadata = JsonMap::new();
        metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(HttpRollbackAdapter::compute_body_aware_digest(
                &HttpMethod::Get,
                approved_url,
                &serde_json::Value::Null,
                None,
            )),
        );
        let contract = make_contract(target, metadata, vec![]);
        let payload = serde_json::json!({
            "url": executed_url,
            "method": "GET"
        });

        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("does not match approved request digest"));
            }
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    /// Test that execute fails closed when payload URL is outside the bound scope.
    /// The bound URL is https://example.com/api but payload specifies https://example.com/other
    /// which is NOT within scope, so it should fail with validation error.
    #[tokio::test]
    async fn test_execute_fails_closed_for_out_of_scope_payload_url() {
        let adapter = HttpRollbackAdapter::new();

        // Bound URL scope: /api prefix only
        let bound_url = "https://example.com/api";
        let target = make_http_target(HttpMethod::Get, bound_url);
        let contract = make_contract(target, JsonMap::new(), vec![]);

        // Payload URL is NOT within the bound scope (/other vs /api)
        let payload_url = "https://example.com/other";
        let payload = serde_json::json!({
            "url": payload_url,
            "method": "GET"
        });

        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(
                    msg.contains("not within bound scope"),
                    "expected scope validation error, got: {}",
                    msg
                );
            }
            other => panic!(
                "expected validation error for out-of-scope URL, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_execute_fails_closed_for_prefix_confusion_payload_url() {
        let adapter = HttpRollbackAdapter::new();

        let bound_url = "https://example.com/api";
        let target = make_http_target(HttpMethod::Get, bound_url);
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let payload = serde_json::json!({
            "url": "https://example.com/apix",
            "method": "GET"
        });

        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("not within bound scope"));
            }
            other => panic!("expected validation error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_rejects_invalid_payload_method() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let payload = serde_json::json!({
            "url": "https://example.com/api/users",
            "method": "TRACE"
        });

        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("unsupported HTTP method"));
            }
            other => panic!("expected validation error, got: {:?}", other),
        }
    }

    /// Test that execute defaults to bound URL when payload does not specify URL.
    #[tokio::test]
    async fn test_execute_uses_bound_url_when_payload_has_no_url() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        // Bound URL is the concrete URL
        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);
        let contract = make_contract(target, JsonMap::new(), vec![]);

        // Empty payload - no URL specified
        let payload = serde_json::json!({});

        let receipt = adapter.execute(&contract, &payload).await.unwrap();

        // Execute should succeed and use the bound URL
        assert!(receipt.result_digest.is_some());
        let meta = &receipt.adapter_metadata;
        // Both bound_url and executed_url should be the same since no payload override
        assert_eq!(meta.get("bound_url").unwrap().as_str().unwrap(), bound_url);
        assert_eq!(
            meta.get("executed_url").unwrap().as_str().unwrap(),
            bound_url
        );
        let _ = handle.join();
    }

    /// Test execute with POST method and body.
    #[tokio::test]
    async fn test_execute_post_with_body() {
        let (port, handle) = start_local_server(201);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Post, &bound_url);
        let mut metadata = JsonMap::new();
        metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(HttpRollbackAdapter::compute_body_aware_digest(
                &HttpMethod::Post,
                &bound_url,
                &serde_json::json!({"name": "test"}),
                None,
            )),
        );
        let contract = make_contract(target, metadata, vec![]);
        let payload = serde_json::json!({
            "body": {"name": "test"}
        });

        let receipt = adapter.execute(&contract, &payload).await.unwrap();

        assert!(receipt.result_digest.is_some());
        assert_eq!(receipt.result_digest.unwrap(), "201");
        let meta = &receipt.adapter_metadata;
        assert_eq!(
            meta.get("executed_method").unwrap().as_str().unwrap(),
            "Post"
        );
        assert!(
            meta.get("executed_body_present")
                .unwrap()
                .as_bool()
                .unwrap()
        );
        let _ = handle.join();
    }

    /// Test execute with PUT method.
    #[tokio::test]
    async fn test_execute_put_with_body() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api/users/1", port);
        let target = make_http_target(HttpMethod::Put, &bound_url);
        let mut metadata = JsonMap::new();
        metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(HttpRollbackAdapter::compute_body_aware_digest(
                &HttpMethod::Put,
                &bound_url,
                &serde_json::json!({"name": "updated"}),
                None,
            )),
        );
        let contract = make_contract(target, metadata, vec![]);
        let payload = serde_json::json!({
            "body": {"name": "updated"}
        });

        let receipt = adapter.execute(&contract, &payload).await.unwrap();

        assert!(receipt.result_digest.is_some());
        assert_eq!(receipt.result_digest.unwrap(), "200");
        let _ = handle.join();
    }

    /// Test execute with DELETE method.
    #[tokio::test]
    async fn test_execute_delete() {
        let (port, handle) = start_local_server(204);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api/users/1", port);
        let target = make_http_target(HttpMethod::Delete, &bound_url);
        let metadata = JsonMap::new();
        let contract = make_contract(target, metadata, vec![]);
        let payload = serde_json::json!({});

        let receipt = adapter.execute(&contract, &payload).await.unwrap();

        assert!(receipt.result_digest.is_some());
        assert_eq!(receipt.result_digest.unwrap(), "204");
        let _ = handle.join();
    }

    /// Test verify for mutation using execute-time metadata (positive case).
    #[tokio::test]
    async fn test_verify_mutation_uses_execute_time_metadata() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");

        // Simulate execute-time metadata with 201 Created
        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(201));
        metadata.insert(
            "executed_url".to_string(),
            serde_json::json!("https://example.com/api/users"),
        );
        metadata.insert("executed_method".to_string(), serde_json::json!("Post"));

        let contract = make_contract(target, metadata, vec![]);

        let receipt = adapter.verify(&contract).await.unwrap();

        // Should verify - 201 matches expected from execute-time metadata
        assert!(
            receipt.verified,
            "201 should verify via execute-time metadata"
        );
        let meta = &receipt.adapter_metadata;
        assert_eq!(
            meta.get("verified_via").unwrap().as_str().unwrap(),
            "execute-time metadata"
        );
    }

    /// Test verify for mutation with explicit status check acts as crosscheck.
    #[tokio::test]
    async fn test_verify_mutation_with_explicit_check_crosscheck() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");

        // Execute-time metadata says 201
        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(201));
        metadata.insert(
            "executed_url".to_string(),
            serde_json::json!("https://example.com/api/users"),
        );

        // Explicit check says 201
        let check = make_status_check(201);
        let contract = make_contract(target, metadata, vec![check]);

        let receipt = adapter.verify(&contract).await.unwrap();

        // Should verify - 201 matches both execute-time and explicit
        assert!(receipt.verified);
    }

    /// Test verify for mutation without explicit check only auto-verifies 2xx.
    #[tokio::test]
    async fn test_verify_mutation_without_explicit_check_rejects_non_success_status() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");

        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(500));
        metadata.insert(
            "executed_url".to_string(),
            serde_json::json!("https://example.com/api/users"),
        );

        let contract = make_contract(target, metadata, vec![]);
        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(
            !receipt.verified,
            "500 should not auto-verify for mutations"
        );
        assert_eq!(
            receipt
                .adapter_metadata
                .get("actual_status")
                .unwrap()
                .as_u64(),
            Some(500)
        );
    }

    /// Test verify for mutation fails when execute-time status doesn't match explicit check.
    #[tokio::test]
    async fn test_verify_mutation_fails_when_status_mismatch() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");

        // Execute-time metadata says 201
        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(201));
        metadata.insert(
            "executed_url".to_string(),
            serde_json::json!("https://example.com/api/users"),
        );

        // Explicit check says 200 - mismatch
        let check = make_status_check(200);
        let contract = make_contract(target, metadata, vec![check]);

        let receipt = adapter.verify(&contract).await.unwrap();

        // Should NOT verify - execute-time (201) doesn't match explicit (200)
        assert!(!receipt.verified, "201 != 200 should fail verify");
    }

    /// Test that verify uses executed_url from metadata when re-checking explicit status.
    /// This verifies the fix where verify re-executes against the actual executed URL, not the bound scope.
    #[tokio::test]
    async fn test_verify_uses_executed_url_from_metadata() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        // Bound URL scope is /api but executed was /api/users
        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let executed_url = format!("http://127.0.0.1:{}/api/users", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);

        // Contract metadata has executed_url from execute phase
        let mut metadata = JsonMap::new();
        metadata.insert("status".to_string(), serde_json::json!(200));
        metadata.insert(
            "executed_url".to_string(),
            serde_json::json!(executed_url.clone()),
        );

        let check = make_status_check(200);
        let contract = make_contract(target, metadata, vec![check]);

        let receipt = adapter.verify(&contract).await.unwrap();

        assert!(receipt.verified);
        // Verify should have used the executed_url, not bound_url
        assert_eq!(
            receipt
                .adapter_metadata
                .get("verify_url")
                .unwrap()
                .as_str()
                .unwrap(),
            executed_url
        );
        let _ = handle.join();
    }

    #[tokio::test]
    async fn test_verify_fails_closed_for_mutation_without_execute_metadata() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");
        let contract = make_contract(target, JsonMap::new(), vec![]);

        // Mutation without execute-time metadata should fail-closed (verified: false)
        let receipt = adapter.verify(&contract).await.unwrap();
        assert!(!receipt.verified);
        let meta = &receipt.adapter_metadata;
        assert_eq!(meta.get("bound_method").unwrap().as_str().unwrap(), "Post");
        assert!(
            meta.get("reason")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("mutation method verify requires execute-time status in metadata")
        );
    }

    /// Test that rollback is conservative no-op for all methods.
    #[tokio::test]
    async fn test_rollback_is_conservative_noop_for_all_methods() {
        let methods = vec![
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Put,
            HttpMethod::Patch,
            HttpMethod::Delete,
        ];

        for method in methods {
            let adapter = HttpRollbackAdapter::new();
            let target = make_http_target(method.clone(), "https://example.com/api");
            let contract = make_contract(target, JsonMap::new(), vec![]);

            let receipt = adapter.rollback(&contract).await.unwrap();

            // Rollback succeeds but is a no-op for all methods
            assert!(
                receipt.recovered,
                "rollback should succeed (no-op) for {:?}",
                method
            );
            let meta = receipt.adapter_metadata;
            assert_eq!(meta.get("rollback").unwrap().as_str().unwrap(), "no-op");
            assert!(
                meta.get("reason")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .contains("R3 boundary"),
                "reason should mention R3 boundary for {:?}",
                method
            );
        }
    }

    /// Test that body-aware digest is correctly computed for mutations.
    #[tokio::test]
    async fn test_body_aware_digest_computation() {
        let method = HttpMethod::Post;
        let url = "https://example.com/api";

        // Empty body
        let empty_body = serde_json::Value::Null;
        let digest_empty =
            HttpRollbackAdapter::compute_body_aware_digest(&method, url, &empty_body, None);

        // Object body
        let obj_body = serde_json::json!({"key": "value"});
        let digest_obj =
            HttpRollbackAdapter::compute_body_aware_digest(&method, url, &obj_body, None);

        // Array body
        let arr_body = serde_json::json!([1, 2, 3]);
        let digest_arr =
            HttpRollbackAdapter::compute_body_aware_digest(&method, url, &arr_body, None);

        // All digests should be different
        assert_ne!(
            digest_empty, digest_obj,
            "empty vs object body should differ"
        );
        assert_ne!(
            digest_empty, digest_arr,
            "empty vs array body should differ"
        );
        assert_ne!(digest_obj, digest_arr, "object vs array body should differ");

        // Same body should produce same digest
        let digest_obj2 =
            HttpRollbackAdapter::compute_body_aware_digest(&method, url, &obj_body, None);
        assert_eq!(
            digest_obj, digest_obj2,
            "same body should produce same digest"
        );
    }

    /// Test that GET digest does not include body.
    #[tokio::test]
    async fn test_get_digest_ignores_body() {
        let method = HttpMethod::Get;
        let url = "https://example.com/api";

        let digest_no_body = HttpRollbackAdapter::compute_body_aware_digest(
            &method,
            url,
            &serde_json::Value::Null,
            None,
        );
        let digest_with_body = HttpRollbackAdapter::compute_body_aware_digest(
            &method,
            url,
            &serde_json::json!({"key": "value"}),
            None,
        );

        // For GET, body should not affect digest
        assert_eq!(
            digest_no_body, digest_with_body,
            "GET digest should be same regardless of body"
        );
    }

    /// Test that parse_request_parts correctly extracts headers from payload.
    #[tokio::test]
    async fn test_parse_request_parts_extracts_headers() {
        let payload = serde_json::json!({
            "url": "https://example.com/api",
            "method": "POST",
            "body": {"name": "test"},
            "headers": {
                "content-type": "application/json",
                "x-custom-header": "custom-value"
            }
        });

        let result = HttpRollbackAdapter::parse_request_parts(&payload).unwrap();
        let (_url, _method, _body, headers) = result;

        assert!(headers.is_some());
        let headers = headers.unwrap();
        assert_eq!(
            headers.get("content-type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            headers.get("x-custom-header"),
            Some(&"custom-value".to_string())
        );
    }

    #[tokio::test]
    async fn test_parse_request_parts_normalizes_header_names_to_lowercase() {
        let payload = serde_json::json!({
            "headers": {
                "Authorization": "Bearer token123",
                "X-Custom-Header": "custom-value"
            }
        });

        let (_url, _method, _body, headers) =
            HttpRollbackAdapter::parse_request_parts(&payload).unwrap();
        let headers = headers.unwrap();

        assert_eq!(
            headers.get("authorization"),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(
            headers.get("x-custom-header"),
            Some(&"custom-value".to_string())
        );
        assert!(!headers.contains_key("Authorization"));
    }

    /// Test that parse_request_parts handles missing headers gracefully.
    #[tokio::test]
    async fn test_parse_request_parts_handles_missing_headers() {
        let payload = serde_json::json!({
            "url": "https://example.com/api",
            "method": "GET"
        });

        let result = HttpRollbackAdapter::parse_request_parts(&payload).unwrap();
        let (_url, _method, _body, headers) = result;

        assert!(headers.is_none());
    }

    /// Test that parse_request_parts rejects non-object headers.
    #[tokio::test]
    async fn test_parse_request_parts_rejects_non_object_headers() {
        let payload = serde_json::json!({
            "url": "https://example.com/api",
            "method": "GET",
            "headers": "not-an-object"
        });

        let err = HttpRollbackAdapter::parse_request_parts(&payload).unwrap_err();
        match err {
            HttpAdapterError::Validation(msg) => {
                assert!(msg.contains("must be an object"));
            }
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    /// Test that parse_request_parts rejects non-string header values.
    #[tokio::test]
    async fn test_parse_request_parts_rejects_non_string_header_values() {
        let payload = serde_json::json!({
            "url": "https://example.com/api",
            "method": "GET",
            "headers": {
                "content-type": 123
            }
        });

        let err = HttpRollbackAdapter::parse_request_parts(&payload).unwrap_err();
        match err {
            HttpAdapterError::Validation(msg) => {
                assert!(msg.contains("must be a string"));
            }
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    /// Test that execute succeeds when headers are provided.
    /// Header application is verified by integration tests that check server-side header receipt.
    #[tokio::test]
    async fn test_execute_succeeds_with_headers() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);
        let metadata = JsonMap::new();
        let contract = make_contract(target, metadata, vec![]);

        let payload = serde_json::json!({
            "headers": {
                "x-custom-header": "custom-value",
                "authorization": "Bearer token123"
            }
        });

        let receipt = adapter.execute(&contract, &payload).await.unwrap();

        // Execute should succeed with headers
        assert!(receipt.result_digest.is_some());
        assert_eq!(receipt.result_digest.unwrap(), "200");
        let _ = handle.join();
    }

    /// Test that execute works with empty headers object.
    #[tokio::test]
    async fn test_execute_with_empty_headers_object() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);
        let metadata = JsonMap::new();
        let contract = make_contract(target, metadata, vec![]);

        let payload = serde_json::json!({
            "headers": {}
        });

        let receipt = adapter.execute(&contract, &payload).await.unwrap();
        assert!(receipt.result_digest.is_some());
        let _ = handle.join();
    }

    /// Test that execute fails for invalid header names.
    #[tokio::test]
    async fn test_execute_fails_for_invalid_header_name() {
        let adapter = HttpRollbackAdapter::new();

        let bound_url = "https://example.com/api";
        let target = make_http_target(HttpMethod::Get, bound_url);
        let metadata = JsonMap::new();
        let contract = make_contract(target, metadata, vec![]);

        let payload = serde_json::json!({
            "headers": {
                "invalid header name": "value"
            }
        });

        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("invalid header name"));
            }
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_execute_rejects_request_digest_mismatch_when_headers_differ() {
        let adapter = HttpRollbackAdapter::new();
        let bound_url = "https://example.com/api";
        let target = make_http_target(HttpMethod::Get, bound_url);

        let approved_headers = HashMap::from([(
            "authorization".to_string(),
            "Bearer approved-token".to_string(),
        )]);
        let mut metadata = JsonMap::new();
        metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(HttpRollbackAdapter::compute_body_aware_digest(
                &HttpMethod::Get,
                bound_url,
                &serde_json::Value::Null,
                Some(&approved_headers),
            )),
        );
        let contract = make_contract(target, metadata, vec![]);
        let payload = serde_json::json!({
            "headers": {
                "authorization": "Bearer different-token"
            }
        });

        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Validation(msg) => {
                assert!(msg.contains("does not match approved request digest"));
            }
            other => panic!("expected validation error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_digest_changes_when_headers_change() {
        let method = HttpMethod::Get;
        let url = "https://example.com/api";
        let headers_a =
            HashMap::from([("authorization".to_string(), "Bearer token-a".to_string())]);
        let headers_b =
            HashMap::from([("authorization".to_string(), "Bearer token-b".to_string())]);

        let digest_a = HttpRollbackAdapter::compute_body_aware_digest(
            &method,
            url,
            &serde_json::Value::Null,
            Some(&headers_a),
        );
        let digest_b = HttpRollbackAdapter::compute_body_aware_digest(
            &method,
            url,
            &serde_json::Value::Null,
            Some(&headers_b),
        );

        assert_ne!(
            digest_a, digest_b,
            "header changes should affect GET request digest"
        );
    }

    // === Canonical Query String Tests ===

    #[test]
    fn test_canonicalize_query_string_sort_keys() {
        // Different key orders should canonicalize to same form
        let (url1, query1) =
            HttpRollbackAdapter::canonicalize_query_string("https://example.com/api?b=2&a=1");
        let (url2, query2) =
            HttpRollbackAdapter::canonicalize_query_string("https://example.com/api?a=1&b=2");

        assert_eq!(url1, url2);
        assert_eq!(query1, query2);
        assert_eq!(query1, "a=1&b=2");
    }

    #[test]
    fn test_canonicalize_query_string_preserves_values() {
        // Empty values should be preserved
        let (_url1, query1) =
            HttpRollbackAdapter::canonicalize_query_string("https://example.com/api?flag&a=1");
        assert_eq!(query1, "a=1&flag");

        // Values with special characters
        let (_url2, query2) = HttpRollbackAdapter::canonicalize_query_string(
            "https://example.com/api?b=hello%20world&a=test",
        );
        assert_eq!(query2, "a=test&b=hello%20world");
    }

    #[test]
    fn test_canonicalize_query_string_handles_no_query() {
        let (url, query) =
            HttpRollbackAdapter::canonicalize_query_string("https://example.com/api");
        assert_eq!(url, "https://example.com/api");
        assert!(query.is_empty());
    }

    #[test]
    fn test_canonicalize_query_string_handles_empty_query() {
        let (url, query) =
            HttpRollbackAdapter::canonicalize_query_string("https://example.com/api?");
        assert_eq!(url, "https://example.com/api");
        assert!(query.is_empty());
    }

    #[test]
    fn test_query_digest_same_for_semantically_identical_queries() {
        // Same query string in different order should produce same digest
        let digest1 = HttpRollbackAdapter::compute_body_aware_digest(
            &HttpMethod::Get,
            "https://example.com/api?b=2&a=1",
            &serde_json::Value::Null,
            None,
        );
        let digest2 = HttpRollbackAdapter::compute_body_aware_digest(
            &HttpMethod::Get,
            "https://example.com/api?a=1&b=2",
            &serde_json::Value::Null,
            None,
        );
        assert_eq!(
            digest1, digest2,
            "semantically identical query strings should produce same digest"
        );
    }

    #[test]
    fn test_query_digest_differs_for_different_queries() {
        // Different query strings should produce different digests
        let digest1 = HttpRollbackAdapter::compute_body_aware_digest(
            &HttpMethod::Get,
            "https://example.com/api?a=1",
            &serde_json::Value::Null,
            None,
        );
        let digest2 = HttpRollbackAdapter::compute_body_aware_digest(
            &HttpMethod::Get,
            "https://example.com/api?a=2",
            &serde_json::Value::Null,
            None,
        );
        assert_ne!(
            digest1, digest2,
            "different query strings should produce different digests"
        );
    }

    #[test]
    fn test_extract_query_from_url() {
        assert_eq!(
            HttpRollbackAdapter::extract_query_from_url("https://example.com/api?b=2&a=1"),
            Some("a=1&b=2".to_string())
        );
        assert_eq!(
            HttpRollbackAdapter::extract_query_from_url("https://example.com/api"),
            None
        );
        assert_eq!(
            HttpRollbackAdapter::extract_query_from_url("https://example.com/api?"),
            None
        );
    }

    #[test]
    fn test_query_digest_computation() {
        // No query
        let digest_none = HttpRollbackAdapter::compute_query_digest(None);
        // Empty query
        let digest_empty = HttpRollbackAdapter::compute_query_digest(Some(""));
        // With query
        let digest_a1 = HttpRollbackAdapter::compute_query_digest(Some("a=1"));

        assert_eq!(
            digest_none, digest_empty,
            "none and empty query should produce same digest"
        );
        assert_ne!(
            digest_none, digest_a1,
            "no query vs query should produce different digests"
        );
    }

    /// Test that prepare captures query metadata
    #[tokio::test]
    async fn test_prepare_captures_query_metadata() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Get, "https://example.com/api");

        // First query: ?b=2&a=1
        let mut metadata = JsonMap::new();
        metadata.insert(
            APPROVED_HTTP_REQUEST_METADATA_KEY.to_string(),
            serde_json::json!({
                "url": "https://example.com/api?b=2&a=1",
                "method": "GET"
            }),
        );
        let request = make_prepare_request_with_metadata(target.clone(), metadata);

        let receipt = adapter.prepare(&request).await.unwrap();
        let meta = receipt.adapter_metadata;

        // Query should be present
        assert_eq!(
            meta.get("approved_query_present").unwrap().as_bool(),
            Some(true),
            "approved query should be present"
        );
        // Query digest should be stored (not raw query)
        assert!(
            meta.get("approved_query_digest").unwrap().is_string(),
            "approved query digest should be stored"
        );

        let digest1 = meta
            .get("approved_request_digest")
            .unwrap()
            .as_str()
            .unwrap();

        // Second query: ?a=1&b=2 (same query, different order)
        let mut metadata2 = JsonMap::new();
        metadata2.insert(
            APPROVED_HTTP_REQUEST_METADATA_KEY.to_string(),
            serde_json::json!({
                "url": "https://example.com/api?a=1&b=2",
                "method": "GET"
            }),
        );
        let request2 = make_prepare_request_with_metadata(target.clone(), metadata2);
        let receipt2 = adapter.prepare(&request2).await.unwrap();
        let meta2 = receipt2.adapter_metadata;

        let digest2 = meta2
            .get("approved_request_digest")
            .unwrap()
            .as_str()
            .unwrap();

        // Digests should match for same query in different order
        assert_eq!(
            digest1, digest2,
            "approved_request_digest should match for semantically identical queries"
        );
        assert_eq!(
            meta.get("approved_query_digest").unwrap(),
            meta2.get("approved_query_digest").unwrap(),
            "same query in different order should have same digest"
        );
    }

    /// Test that execute captures query metadata
    #[tokio::test]
    async fn test_execute_captures_query_metadata() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);
        let metadata = JsonMap::new();
        let contract = make_contract(target, metadata, vec![]);

        let payload = serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api?b=2&a=1", port),
            "method": "GET"
        });

        let receipt = adapter.execute(&contract, &payload).await.unwrap();
        let meta = &receipt.adapter_metadata;

        // Query should be present
        assert_eq!(
            meta.get("executed_query_present").unwrap().as_bool(),
            Some(true),
            "executed query should be present"
        );
        // Query digest should be stored
        assert!(
            meta.get("executed_query_digest").unwrap().is_string(),
            "executed query digest should be stored"
        );
        let _ = handle.join();
    }

    /// Test that different query order produces same digest during execute
    #[tokio::test]
    async fn test_execute_same_query_different_order_succeeds() {
        let (port, handle) = start_local_server(200);
        let adapter = HttpRollbackAdapter::new();

        let bound_url = format!("http://127.0.0.1:{}/api", port);
        let target = make_http_target(HttpMethod::Get, &bound_url);

        // First prepare with query ?a=1&b=2
        let mut metadata = JsonMap::new();
        metadata.insert(
            APPROVED_HTTP_REQUEST_METADATA_KEY.to_string(),
            serde_json::json!({
                "url": format!("http://127.0.0.1:{}/api?a=1&b=2", port),
                "method": "GET"
            }),
        );
        let request = make_prepare_request_with_metadata(target.clone(), metadata);
        let receipt = adapter.prepare(&request).await.unwrap();
        let approved_digest = receipt
            .adapter_metadata
            .get("approved_request_digest")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        // Contract with approved digest
        let mut contract_metadata = JsonMap::new();
        contract_metadata.insert(
            "approved_request_digest".to_string(),
            serde_json::json!(approved_digest),
        );
        let contract = make_contract(target.clone(), contract_metadata, vec![]);

        // Execute with query ?b=2&a=1 (different order, same semantic)
        let payload = serde_json::json!({
            "url": format!("http://127.0.0.1:{}/api?b=2&a=1", port),
            "method": "GET"
        });

        // Should succeed because digests match
        let result = adapter.execute(&contract, &payload).await;
        assert!(
            result.is_ok(),
            "semantically identical query in different order should execute successfully"
        );
        let _ = handle.join();
    }
}
