use async_trait::async_trait;
use ferrum_proto::{
    CheckSpec, CheckType, HttpMethod, JsonMap, RollbackContract, RollbackPrepareRequest,
    RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, AdapterRegistry, ExecuteReceipt, PrepareReceipt, RecoveryReceipt,
    RollbackAdapter, VerifyReceipt,
};
use reqwest::{Client, Url};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ADAPTER_KEY: &str = "http";

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

/// HttpRollbackAdapter provides HTTP request capture and status-only verification.
///
/// Supported operations:
/// - `prepare`: captures request metadata (method, url, digest) for idempotent requests
/// - `execute`: performs a minimal HTTP GET request for status verification
/// - `verify`: validates expected HTTP status from verify_checks
/// - `rollback`: conservative no-op for this slice (GET requests are inherently safe)
/// - `compensate`: alias for rollback in this slice
///
/// URL scope semantics:
/// - `RollbackTarget::HttpRequest.url` represents the BOUND URL scope/prefix (base_url + path_prefix)
/// - Execute-time payload may contain a concrete URL within that scope
/// - Adapter validates fail-closed that actual URL stays within bound scope prefix
/// - Method must match the bound method (only GET for this slice)
///
/// Metadata keys (clearer naming to distinguish bound vs executed):
/// - `bound_url`: the allowed URL scope prefix from prepare
/// - `bound_method`: the allowed method from prepare
/// - `executed_url`: the concrete URL actually executed (from payload or bound default)
/// - `executed_method`: the method actually executed (from payload or bound default)
/// - `executed_request_digest`: SHA256(method:executed_url) - digest of what was actually executed
///
/// This slice is intentionally conservative:
/// - Only HTTP GET is supported in execute (read-only, idempotent)
/// - rollback/compensate are no-ops since GET has no side effects
/// - Response bodies are not captured or compared
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

    /// Compute a SHA256 digest from method and URL for request identification.
    /// Used for executed request digest (based on actual executed URL/method).
    fn compute_request_digest(method: &HttpMethod, url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{:?}:{}", method, url).as_bytes());
        format!("{:x}", hasher.finalize())
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

    /// Parse URL and method from execute payload if present.
    /// Returns (payload_url, payload_method) where either or both may be None if not in payload.
    /// Stay fail-closed if the payload tries to provide malformed or unsupported values.
    fn parse_payload_url_method(
        payload: &serde_json::Value,
    ) -> Result<(Option<String>, Option<HttpMethod>), HttpAdapterError> {
        let obj = match payload.as_object() {
            Some(o) => o,
            None => return Ok((None, None)),
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

        Ok((url, method))
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
        let (method, url, _) =
            Self::extract_http_target(&request.target).map_err(AdapterError::from)?;

        // Validate URL is well-formed
        if url.is_empty() {
            return Err(AdapterError::Validation("URL cannot be empty".to_string()).into());
        }

        let request_digest = Self::compute_request_digest(&method, &url);

        // Store clearer metadata with "bound_" prefix to indicate scope/prefix semantics
        let mut metadata = JsonMap::new();
        metadata.insert(
            "bound_method".to_string(),
            serde_json::json!(format!("{:?}", method)),
        );
        metadata.insert("bound_url".to_string(), serde_json::json!(url));
        metadata.insert(
            "bound_request_digest".to_string(),
            serde_json::json!(request_digest),
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

        // Only support GET for this conservative first slice
        if bound_method != HttpMethod::Get {
            return Err(AdapterError::Unsupported(format!(
                "execute only supports HTTP GET in this slice; got {:?}",
                bound_method
            ))
            .into());
        }

        // Parse URL and method from payload (execute-time concrete request)
        let (payload_url, payload_method) =
            Self::parse_payload_url_method(payload).map_err(AdapterError::from)?;

        // Resolve actual executed URL/method: payload overrides bound, else use bound
        let executed_url = payload_url.unwrap_or_else(|| bound_url.clone());
        let executed_method = payload_method.unwrap_or(bound_method.clone());

        // Fail-closed: validate executed URL stays within bound scope prefix
        if let Err(scope_err) = Self::validate_url_within_scope(&executed_url, &bound_url) {
            return Err(AdapterError::Validation(scope_err).into());
        }

        // Fail-closed: validate method matches bound method
        if executed_method != bound_method {
            return Err(AdapterError::Validation(format!(
                "executed method {:?} does not match bound method {:?}",
                executed_method, bound_method
            ))
            .into());
        }

        // Execute the HTTP GET request using the resolved (possibly payload-derived) URL
        let response =
            self.client.get(&executed_url).send().await.map_err(|e| {
                HttpAdapterError::RequestFailed(format!("GET request failed: {}", e))
            })?;

        let status = response.status().as_u16();

        // Compute digest based on actual executed URL/method
        let executed_request_digest = Self::compute_request_digest(&executed_method, &executed_url);

        // Store clearer metadata distinguishing bound scope vs executed concrete request
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

        // Get expected status from verify_checks
        let expected_status = Self::extract_expected_status(&contract.verify_checks);

        // Fall back to execute-time status metadata if no explicit check is configured.
        // This enables verify to succeed when the execute phase captured the HTTP status,
        // allowing end-to-end coverage without requiring explicit HttpStatusExpected checks.
        // Stay fail-closed: if no status metadata exists, verify fails.
        let expected_status = match expected_status {
            Some(s) => s,
            None => {
                // Try to get execute-time status from contract metadata
                if let Some(execute_status) = contract.metadata.get("status") {
                    if let Some(status_val) = execute_status.as_u64() {
                        status_val as u16
                    } else {
                        return Ok(VerifyReceipt {
                            verified: false,
                            adapter_metadata: {
                                let mut m = JsonMap::new();
                                // Use executed_url from metadata if available, else fall back to bound_url
                                let verify_url = contract
                                    .metadata
                                    .get("executed_url")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&bound_url);
                                m.insert("url".to_string(), serde_json::json!(verify_url));
                                m.insert(
                                    "reason".to_string(),
                                    serde_json::json!("execute-time status is not a valid number"),
                                );
                                m
                            },
                        });
                    }
                } else {
                    return Ok(VerifyReceipt {
                        verified: false,
                        adapter_metadata: {
                            let mut m = JsonMap::new();
                            // Use executed_url from metadata if available, else fall back to bound_url
                            let verify_url = contract
                                .metadata
                                .get("executed_url")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&bound_url);
                            m.insert("url".to_string(), serde_json::json!(verify_url));
                            m.insert(
                                "reason".to_string(),
                                serde_json::json!("no HttpStatusExpected check configured and no execute-time status in metadata"),
                            );
                            m
                        },
                    });
                }
            }
        };

        // Only support GET for verification in this slice
        if bound_method != HttpMethod::Get {
            return Err(AdapterError::Unsupported(format!(
                "verify only supports HTTP GET in this slice; got {:?}",
                bound_method
            ))
            .into());
        }

        // Determine URL to verify against: prefer executed_url from metadata, fall back to bound_url
        // This ensures verify uses the concrete URL that was actually executed, not just the scope prefix
        let verify_url = contract
            .metadata
            .get("executed_url")
            .and_then(|v| v.as_str())
            .unwrap_or(&bound_url)
            .to_string();

        // For the conservative GET/no-op slice: when using execute-time status metadata
        // fallback (i.e., no explicit verify_checks), trust the metadata without
        // re-executing the HTTP request. This is safe because:
        // 1. GET requests are idempotent - executing twice doesn't change state
        // 2. The execute phase already captured the actual status
        // 3. The alternative (making another HTTP call) could fail if the server
        //    has shut down or the endpoint is no longer available
        let verify_checks_has_explicit_expectation =
            Self::extract_expected_status(&contract.verify_checks).is_some();

        let (verified, actual_status) = if verify_checks_has_explicit_expectation {
            // Explicit check configured: re-execute HTTP using the executed URL from metadata
            let response = self.client.get(&verify_url).send().await.map_err(|e| {
                HttpAdapterError::RequestFailed(format!("GET request failed: {}", e))
            })?;
            let actual = response.status().as_u16();
            (actual == expected_status, Some(actual))
        } else {
            // No explicit check: trust execute-time status from metadata ONLY if successful.
            // Stay fail-closed: non-success status codes (4xx, 5xx) do NOT auto-verify.
            // This prevents a failed HTTP execution (e.g., 500) from incorrectly auto-committing.
            let is_success = (200..300).contains(&expected_status);
            (is_success, None)
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
            metadata.insert("verify_url".to_string(), serde_json::json!(verify_url));
        } else {
            metadata.insert(
                "verified_via".to_string(),
                serde_json::json!("execute-time metadata fallback"),
            );
            metadata.insert("verify_url".to_string(), serde_json::json!(verify_url));
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

        // Conservative no-op for this slice:
        // HTTP GET requests have no side effects and cannot be "rolled back"
        // This aligns with fail-closed semantics where we don't attempt
        // destructive operations that we cannot guarantee are safe

        let mut metadata = JsonMap::new();
        metadata.insert(
            "bound_request_digest".to_string(),
            serde_json::json!(bound_request_digest),
        );
        metadata.insert("rollback".to_string(), serde_json::json!("no-op"));
        metadata.insert(
            "reason".to_string(),
            serde_json::json!("HTTP GET has no side effects; rollback not applicable"),
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
            metadata: JsonMap::new(),
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
    async fn test_execute_rejects_non_get_methods() {
        let adapter = HttpRollbackAdapter::new();
        let target = make_http_target(HttpMethod::Post, "https://example.com/api");
        let contract = make_contract(target, JsonMap::new(), vec![]);

        let payload = serde_json::json!({});
        let err = adapter.execute(&contract, &payload).await.unwrap_err();
        match err {
            AdapterError::Unsupported(msg) => {
                assert!(msg.contains("GET"));
            }
            _ => panic!("expected unsupported error, got {:?}", err),
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
        let contract = make_contract(target, JsonMap::new(), vec![]);

        // Payload URL is a sub-path within the bound scope
        let payload_url = format!("http://127.0.0.1:{}/api/users", port);
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
        let _ = handle.join();
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
}
