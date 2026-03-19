use async_trait::async_trait;
use ferrum_proto::{
    CheckSpec, CheckType, HttpMethod, JsonMap, RollbackContract, RollbackPrepareRequest,
    RollbackTarget,
};
use ferrum_rollback::{
    AdapterError, AdapterRegistry, ExecuteReceipt, PrepareReceipt, RecoveryReceipt,
    RollbackAdapter, VerifyReceipt,
};
use reqwest::Client;
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

        let mut metadata = JsonMap::new();
        metadata.insert(
            "method".to_string(),
            serde_json::json!(format!("{:?}", method)),
        );
        metadata.insert("url".to_string(), serde_json::json!(url));
        metadata.insert(
            "request_digest".to_string(),
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
        _payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        let (method, url, _) =
            Self::extract_http_target(&contract.target).map_err(AdapterError::from)?;

        // Only support GET for this conservative first slice
        if method != HttpMethod::Get {
            return Err(AdapterError::Unsupported(format!(
                "execute only supports HTTP GET in this slice; got {:?}",
                method
            ))
            .into());
        }

        // Execute the HTTP GET request
        let response =
            self.client.get(&url).send().await.map_err(|e| {
                HttpAdapterError::RequestFailed(format!("GET request failed: {}", e))
            })?;

        let status = response.status().as_u16();

        let mut metadata = JsonMap::new();
        metadata.insert("method".to_string(), serde_json::json!("Get"));
        metadata.insert("url".to_string(), serde_json::json!(url));
        metadata.insert("status".to_string(), serde_json::json!(status));
        metadata.insert("executed".to_string(), serde_json::json!(true));

        Ok(ExecuteReceipt {
            external_id: None,
            result_digest: Some(status.to_string()),
            adapter_metadata: metadata,
        })
    }

    async fn verify(&self, contract: &RollbackContract) -> Result<VerifyReceipt, AdapterError> {
        let (method, url, _) =
            Self::extract_http_target(&contract.target).map_err(AdapterError::from)?;

        // Get expected status from verify_checks
        let expected_status = Self::extract_expected_status(&contract.verify_checks);

        // If no expected status check is configured, fail closed
        let expected_status = match expected_status {
            Some(s) => s,
            None => {
                return Ok(VerifyReceipt {
                    verified: false,
                    adapter_metadata: {
                        let mut m = JsonMap::new();
                        m.insert("url".to_string(), serde_json::json!(url));
                        m.insert(
                            "reason".to_string(),
                            serde_json::json!("no HttpStatusExpected check configured"),
                        );
                        m
                    },
                });
            }
        };

        // Only support GET for verification in this slice
        if method != HttpMethod::Get {
            return Err(AdapterError::Unsupported(format!(
                "verify only supports HTTP GET in this slice; got {:?}",
                method
            ))
            .into());
        }

        // Execute GET and check status
        let response =
            self.client.get(&url).send().await.map_err(|e| {
                HttpAdapterError::RequestFailed(format!("GET request failed: {}", e))
            })?;

        let actual_status = response.status().as_u16();
        let verified = actual_status == expected_status;

        let mut metadata = JsonMap::new();
        metadata.insert("method".to_string(), serde_json::json!("Get"));
        metadata.insert("url".to_string(), serde_json::json!(url));
        metadata.insert(
            "expected_status".to_string(),
            serde_json::json!(expected_status),
        );
        metadata.insert(
            "actual_status".to_string(),
            serde_json::json!(actual_status),
        );
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
        let (_method, _url, request_digest) =
            Self::extract_http_target(&contract.target).map_err(AdapterError::from)?;

        // Conservative no-op for this slice:
        // HTTP GET requests have no side effects and cannot be "rolled back"
        // This aligns with fail-closed semantics where we don't attempt
        // destructive operations that we cannot guarantee are safe

        let mut metadata = JsonMap::new();
        metadata.insert(
            "request_digest".to_string(),
            serde_json::json!(request_digest),
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
        assert_eq!(meta.get("method").unwrap().as_str().unwrap(), "Get");
        assert_eq!(
            meta.get("url").unwrap().as_str().unwrap(),
            "https://example.com/api"
        );
        assert!(meta.get("request_digest").unwrap().is_string());
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
}
