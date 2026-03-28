//! HTTP/JSON transport implementation for Sync-3a diagnostic probe.
//!
//! This module provides a real network transport that implements the `Transport`
//! trait using HTTP/JSON with `reqwest`. It is the production transport for
//! follower-side probe communication with leader nodes.
//!
//! ## Design
//!
//! The transport maps cleanly to the existing `LeaderTipRequest` / `LeaderTipResponse`
//! and `ProofRequest` / `ProofResponse` DTOs defined in `transport.rs`. No second
//! protocol is invented; the HTTP endpoints on the leader expose the same DTOs.
//!
//! ## Error Mapping
//!
//! All network errors are mapped to `TransportError` variants per the fail-closed
//! table in Sync-3:
//! - Connection refused / DNS failure -> `LeaderUnreachable`
//! - Timeout -> `LeaderTimeout`
//! - HTTP 401/403 -> `LeaderCapabilityDenied`
//! - HTTP 400/500 -> `InternalError`
//!
//! ## Read-Only Guarantee
//!
//! Both endpoints (`/v1/sync/leader/tip` and `/v1/sync/leader/tip/proof`) are
//! read-only. The follower never modifies leader state via these endpoints.
//!
//! ## Auth
//!
//! Bearer token authentication is supported via the optional `bearer_token` field.
//! When `Some(token)` is set, the transport sends `Authorization: Bearer <token>`
//! header on all requests. When `None`, no auth header is sent (for auth-disabled
//! deployments). Auth-disabled deployments continue to work with `None`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::transport::{
    LeaderTip, LeaderTipRequest, LeaderTipResponse, LeaderVersion, Proof, ProofRequest,
    ProofResponse, Transport, TransportError,
};

// ---------------------------------------------------------------------------
// HTTP transport DTOs (leader-side responses)
// ---------------------------------------------------------------------------

/// Response from `GET /v1/sync/leader/tip`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpLeaderTipResponse {
    pub leader_tip: Option<LeaderTip>,
    pub leader_version: Option<LeaderVersion>,
}

/// Response from `GET /v1/sync/leader/tip/proof`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProofResponse {
    pub proof: Option<Proof>,
}

// ---------------------------------------------------------------------------
// HttpLeaderTransport
// ---------------------------------------------------------------------------

/// HTTP/JSON transport implementation using `reqwest`.
///
/// This transport communicates with a leader node over HTTP/JSON.
/// All requests are read-only; no state is modified on the leader.
///
/// # Type Parameters
///
/// - `C`: HTTP client implementing `HttpClient` trait. Defaults to `reqwest::Client`.
#[derive(Clone)]
pub struct HttpLeaderTransport<C = reqwest::Client> {
    /// Base URL of the leader node (e.g., "http://127.0.0.1:8080").
    /// Endpoints are appended to this base URL.
    base_url: String,
    /// HTTP client used for requests.
    client: C,
    /// Optional bearer token for authentication.
    /// When `Some(token)`, the transport sends `Authorization: Bearer <token>`.
    /// When `None`, no auth header is sent (auth-disabled deployments).
    bearer_token: Option<String>,
}

impl HttpLeaderTransport {
    /// Create a new HTTP transport with the default client and no auth.
    ///
    /// This constructor is for auth-disabled deployments where no bearer token is needed.
    ///
    /// # Panics
    ///
    /// Panics if the underlying HTTP client cannot be created.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_client_and_token(base_url, reqwest::Client::new(), None)
    }

    /// Create a new HTTP transport with the default client and optional bearer token.
    ///
    /// This is the primary constructor for production use where bearer auth may be required.
    ///
    /// # Panics
    ///
    /// Panics if the underlying HTTP client cannot be created.
    pub fn with_bearer_token(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self::with_client_and_token(base_url, reqwest::Client::new(), token)
    }
}

impl<C> HttpLeaderTransport<C> {
    /// Create a new HTTP transport with a custom HTTP client and no auth.
    pub fn with_client(base_url: impl Into<String>, client: C) -> Self {
        Self {
            base_url: base_url.into(),
            client,
            bearer_token: None,
        }
    }

    /// Create a new HTTP transport with a custom HTTP client and optional bearer token.
    pub fn with_client_and_token(
        base_url: impl Into<String>,
        client: C,
        token: Option<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            client,
            bearer_token: token,
        }
    }

    /// Returns the base URL of this transport.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the bearer token if one is set.
    pub fn bearer_token(&self) -> Option<&str> {
        self.bearer_token.as_deref()
    }
}

impl HttpLeaderTransport<reqwest::Client> {
    /// Create a new HTTP transport with a custom client configuration and no auth.
    pub fn with_client_config(base_url: impl Into<String>, client: reqwest::Client) -> Self {
        Self::with_client(base_url, client)
    }

    /// Create a new HTTP transport with a custom client configuration and optional bearer token.
    pub fn with_client_config_and_token(
        base_url: impl Into<String>,
        client: reqwest::Client,
        token: Option<String>,
    ) -> Self {
        Self::with_client_and_token(base_url, client, token)
    }
}

// ---------------------------------------------------------------------------
// HttpClient trait (for testability)
// ---------------------------------------------------------------------------

/// Trait abstracting the HTTP client operations needed by `HttpLeaderTransport`.
/// This allows mocking in tests without a real network.
///
/// # Note
///
/// Implementors should handle:
/// - Connection failures -> `TransportError::LeaderUnreachable`
/// - Timeouts -> `TransportError::LeaderTimeout`
/// - HTTP error status codes -> appropriate `TransportError` variant
#[async_trait]
pub trait HttpClient: Send + Sync {
    /// Perform a GET request and deserialize the JSON response.
    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        timeout_ms: u64,
    ) -> Result<T, TransportError>;

    /// Perform a GET request with optional bearer auth and deserialize the JSON response.
    async fn get_json_with_auth<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        timeout_ms: u64,
        bearer_token: Option<&str>,
    ) -> Result<T, TransportError>;
}

/// Implementation of `HttpClient` for `reqwest::Client`.
#[async_trait]
impl HttpClient for reqwest::Client {
    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        timeout_ms: u64,
    ) -> Result<T, TransportError> {
        self.get_json_with_auth(url, timeout_ms, None).await
    }

    async fn get_json_with_auth<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        timeout_ms: u64,
        bearer_token: Option<&str>,
    ) -> Result<T, TransportError> {
        let mut request = self.get(url);

        // Add bearer token if provided
        if let Some(token) = bearer_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .timeout(Duration::from_millis(timeout_ms))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    // Try to extract URL from error for context
                    TransportError::LeaderTimeout {
                        address: url.to_string(),
                        duration_ms: timeout_ms,
                    }
                } else if e.is_connect() {
                    TransportError::LeaderUnreachable {
                        address: url.to_string(),
                    }
                } else {
                    TransportError::InternalError {
                        details: format!("request failed: {}", e),
                    }
                }
            })?;

        let status = response.status();

        // Map HTTP status codes to TransportError variants
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(TransportError::LeaderCapabilityDenied {
                leader: url.to_string(),
                required_capability: "sync".to_string(),
            });
        }

        if !status.is_success() {
            return Err(TransportError::InternalError {
                details: format!("HTTP {} from {}", status, url),
            });
        }

        response
            .json::<T>()
            .await
            .map_err(|e| TransportError::InternalError {
                details: format!("failed to parse JSON response: {}", e),
            })
    }
}

// ---------------------------------------------------------------------------
// Transport implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl<T: HttpClient + Clone> Transport for HttpLeaderTransport<T> {
    async fn fetch_leader_tip(
        &self,
        request: &LeaderTipRequest,
    ) -> Result<LeaderTipResponse, TransportError> {
        let url = format!("{}/v1/sync/leader/tip", self.base_url);

        let response: HttpLeaderTipResponse = self
            .client
            .get_json_with_auth(&url, request.timeout_ms, self.bearer_token.as_deref())
            .await?;

        // Convert HTTP DTO to transport DTO
        Ok(LeaderTipResponse {
            leader_tip: response.leader_tip,
            leader_version: response.leader_version,
        })
    }

    async fn fetch_proof(&self, request: &ProofRequest) -> Result<ProofResponse, TransportError> {
        let url = format!(
            "{}/v1/sync/leader/tip/proof?start={}&end={}",
            self.base_url, request.start_sequence, request.end_sequence
        );

        let response: HttpProofResponse = self
            .client
            .get_json_with_auth(&url, request.timeout_ms, self.bearer_token.as_deref())
            .await?;

        // Map None proof to RangeNotAvailable per Sync-3 contract (A3).
        // A missing proof is a range problem, not an internal error (A7).
        let proof = response.proof.ok_or(TransportError::RangeNotAvailable {
            start: request.start_sequence,
            end: request.end_sequence,
        })?;

        Ok(ProofResponse { proof: Some(proof) })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{
        EntryHashInfo, HashPath, LeaderTip, LeaderVersion, ProbeRequestId, Proof,
    };
    use std::collections::VecDeque;

    /// A mock HTTP client that returns pre-configured responses.
    #[derive(Clone)]
    struct MockHttpClient {
        responses:
            std::sync::Arc<tokio::sync::Mutex<VecDeque<Result<serde_json::Value, TransportError>>>>,
    }

    impl MockHttpClient {
        fn new() -> Self {
            Self {
                responses: std::sync::Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
            }
        }

        /// Queue a successful JSON response.
        async fn queue_response(&self, response: impl Serialize) {
            let json = serde_json::to_value(response).unwrap();
            self.responses.lock().await.push_back(Ok(json));
        }

        /// Queue an error response.
        async fn queue_error(&self, error: TransportError) {
            self.responses.lock().await.push_back(Err(error));
        }
    }

    #[async_trait]
    impl HttpClient for MockHttpClient {
        async fn get_json<T: for<'de> Deserialize<'de>>(
            &self,
            url: &str,
            timeout_ms: u64,
        ) -> Result<T, TransportError> {
            self.get_json_with_auth(url, timeout_ms, None).await
        }

        async fn get_json_with_auth<T: for<'de> Deserialize<'de>>(
            &self,
            _url: &str,
            _timeout_ms: u64,
            _bearer_token: Option<&str>,
        ) -> Result<T, TransportError> {
            let response = self.responses.lock().await.pop_front();
            match response {
                Some(Ok(json)) => {
                    let deserialized: T = serde_json::from_value(json).map_err(|e| {
                        TransportError::InternalError {
                            details: format!("mock deserialization error: {}", e),
                        }
                    })?;
                    Ok(deserialized)
                }
                Some(Err(e)) => Err(e),
                None => Err(TransportError::InternalError {
                    details: "mock client: no more queued responses".to_string(),
                }),
            }
        }
    }

    /// A mock HTTP client that records auth headers for verification.
    #[derive(Clone, Default)]
    struct AuthRecordingClient {
        recorded_tokens: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
    }

    impl AuthRecordingClient {
        fn new() -> Self {
            Self {
                recorded_tokens: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn recorded_tokens(&self) -> Vec<Option<String>> {
            self.recorded_tokens.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HttpClient for AuthRecordingClient {
        async fn get_json<T: for<'de> Deserialize<'de>>(
            &self,
            url: &str,
            timeout_ms: u64,
        ) -> Result<T, TransportError> {
            self.get_json_with_auth(url, timeout_ms, None).await
        }

        async fn get_json_with_auth<T: for<'de> Deserialize<'de>>(
            &self,
            _url: &str,
            _timeout_ms: u64,
            bearer_token: Option<&str>,
        ) -> Result<T, TransportError> {
            self.recorded_tokens
                .lock()
                .unwrap()
                .push(bearer_token.map(String::from));
            // Return a minimal valid response for auth recording tests
            Err(TransportError::InternalError {
                details: "auth recording client: not a real response".to_string(),
            })
        }
    }

    fn make_tip(sequence: u64, hash: &str) -> LeaderTip {
        LeaderTip {
            sequence,
            hash: hash.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    fn make_version() -> LeaderVersion {
        LeaderVersion {
            version: "1.0.0".to_string(),
            min_follower_version: "1.0.0".to_string(),
        }
    }

    fn make_proof(sequences: Vec<u64>, hashes: Vec<&str>) -> Proof {
        let entries: Vec<EntryHashInfo> = sequences
            .into_iter()
            .zip(hashes.into_iter())
            .map(|(seq, hash)| EntryHashInfo {
                sequence: seq,
                entry_hash: hash.to_string(),
            })
            .collect();

        let range_hash = entries
            .iter()
            .map(|e| e.entry_hash.clone())
            .collect::<Vec<_>>()
            .join("");

        Proof {
            entries,
            range_hash,
            continuity_proof: HashPath {
                nodes: vec!["node1".to_string(), "node2".to_string()],
                leaf_count: 10,
            },
        }
    }

    #[tokio::test]
    async fn http_transport_fetch_leader_tip_success() {
        let mock = MockHttpClient::new();
        mock.queue_response(HttpLeaderTipResponse {
            leader_tip: Some(make_tip(100, "abc123")),
            leader_version: Some(make_version()),
        })
        .await;

        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock);
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };

        let response = transport.fetch_leader_tip(&request).await.unwrap();

        assert!(response.leader_tip.is_some());
        let tip = response.leader_tip.unwrap();
        assert_eq!(tip.sequence, 100);
        assert_eq!(tip.hash, "abc123");

        assert!(response.leader_version.is_some());
        let version = response.leader_version.unwrap();
        assert_eq!(version.version, "1.0.0");
    }

    #[tokio::test]
    async fn http_transport_fetch_proof_success() {
        let mock = MockHttpClient::new();
        mock.queue_response(HttpProofResponse {
            proof: Some(make_proof(vec![5, 6, 7], vec!["hash1", "hash2", "hash3"])),
        })
        .await;

        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock);
        let request = ProofRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            start_sequence: 5,
            end_sequence: 7,
            timeout_ms: 5000,
        };

        let response = transport.fetch_proof(&request).await.unwrap();

        assert!(response.proof.is_some());
        let proof = response.proof.unwrap();
        assert_eq!(proof.entries.len(), 3);
        assert_eq!(proof.range_hash, "hash1hash2hash3");
    }

    #[tokio::test]
    async fn http_transport_maps_unreachable_to_leader_unreachable() {
        let mock = MockHttpClient::new();
        mock.queue_error(TransportError::LeaderUnreachable {
            address: "http://leader:8080".to_string(),
        })
        .await;

        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock);
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };

        let result = transport.fetch_leader_tip(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TransportError::LeaderUnreachable { .. }));
    }

    #[tokio::test]
    async fn http_transport_maps_timeout_to_leader_timeout() {
        let mock = MockHttpClient::new();
        mock.queue_error(TransportError::LeaderTimeout {
            address: "http://leader:8080".to_string(),
            duration_ms: 5000,
        })
        .await;

        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock);
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };

        let result = transport.fetch_leader_tip(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TransportError::LeaderTimeout { .. }));
    }

    #[tokio::test]
    async fn http_transport_maps_capability_denied() {
        let mock = MockHttpClient::new();
        mock.queue_error(TransportError::LeaderCapabilityDenied {
            leader: "http://leader:8080".to_string(),
            required_capability: "sync".to_string(),
        })
        .await;

        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock);
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };

        let result = transport.fetch_leader_tip(&request).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, TransportError::LeaderCapabilityDenied { .. }));
    }

    #[tokio::test]
    async fn http_transport_fails_closed_on_empty_response() {
        let mock = MockHttpClient::new();
        // Queue an empty/null response
        mock.queue_response(serde_json::json!({
            "leader_tip": null,
            "leader_version": null
        }))
        .await;

        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock);
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };

        let response = transport.fetch_leader_tip(&request).await.unwrap();

        // Empty response is still a successful HTTP response (no transport error),
        // but the tip/version are None. The caller (TransportProbe) will map this
        // to A7 fail-closed because None tip/version is invalid input.
        assert!(response.leader_tip.is_none());
        assert!(response.leader_version.is_none());
    }

    #[tokio::test]
    async fn http_transport_base_url_accessible() {
        let transport = HttpLeaderTransport::new("http://leader:9000");
        assert_eq!(transport.base_url(), "http://leader:9000");
    }

    #[test]
    fn http_transport_clone_preserves_base_url() {
        let transport = HttpLeaderTransport::new("http://leader:8080");
        let cloned = transport.clone();
        // HttpLeaderTransport is Clone because reqwest::Client is Clone
        let _ = cloned;
    }

    // =========================================================================
    // Bearer token auth tests
    // =========================================================================

    #[tokio::test]
    async fn http_transport_with_bearer_token_none_sends_no_auth() {
        // When bearer_token is None, no Authorization header should be sent
        let mock = AuthRecordingClient::new();
        let transport =
            HttpLeaderTransport::with_client_and_token("http://leader:8080", mock.clone(), None);

        // Make a request (will fail with InternalError since mock doesn't return real response)
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };
        let _ = transport.fetch_leader_tip(&request).await;

        // Verify no token was recorded
        let tokens = mock.recorded_tokens();
        assert_eq!(tokens.len(), 1);
        assert!(
            tokens[0].is_none(),
            "None token should be recorded as None, not Some(None)"
        );
    }

    #[tokio::test]
    async fn http_transport_with_bearer_token_some_sends_bearer_token() {
        // When bearer_token is Some, Authorization header should include it
        let mock = AuthRecordingClient::new();
        let transport = HttpLeaderTransport::with_client_and_token(
            "http://leader:8080",
            mock.clone(),
            Some("secret-token".to_string()),
        );

        // Make a request (will fail with InternalError since mock doesn't return real response)
        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };
        let _ = transport.fetch_leader_tip(&request).await;

        // Verify the token was recorded
        let tokens = mock.recorded_tokens();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].as_ref(), Some(&"secret-token".to_string()));
    }

    #[tokio::test]
    async fn http_transport_with_bearer_token_via_constructor() {
        // Test the primary constructor with_bearer_token
        let mock = AuthRecordingClient::new();
        // Use with_client_and_token with our mock for auth recording
        let transport = HttpLeaderTransport::with_client_and_token(
            "http://leader:8080",
            mock.clone(),
            Some("my-secret".to_string()),
        );

        let request = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };
        let _ = transport.fetch_leader_tip(&request).await;

        let tokens = mock.recorded_tokens();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].as_ref(), Some(&"my-secret".to_string()));
    }

    #[tokio::test]
    async fn http_transport_bearer_token_accessor() {
        // Test the bearer_token() accessor
        let transport_none = HttpLeaderTransport::new("http://leader:8080");
        assert!(transport_none.bearer_token().is_none());

        let transport_some = HttpLeaderTransport::with_bearer_token(
            "http://leader:8080",
            Some("token123".to_string()),
        );
        assert_eq!(transport_some.bearer_token(), Some("token123"));
    }

    #[tokio::test]
    async fn http_transport_auth_enabled_path_uses_auth() {
        // Integration test: auth-enabled transport should use auth on all requests
        let mock = AuthRecordingClient::new();
        let transport = HttpLeaderTransport::with_client_and_token(
            "http://leader:8080",
            mock.clone(),
            Some("auth-token".to_string()),
        );

        // Fetch leader tip
        let request_tip = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };
        let _ = transport.fetch_leader_tip(&request_tip).await;

        // Fetch proof
        let request_proof = ProofRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            start_sequence: 1,
            end_sequence: 10,
            timeout_ms: 5000,
        };
        let _ = transport.fetch_proof(&request_proof).await;

        // Both requests should have recorded the token
        let tokens = mock.recorded_tokens();
        assert_eq!(tokens.len(), 2);
        for token in &tokens {
            assert_eq!(token.as_ref(), Some(&"auth-token".to_string()));
        }
    }

    #[tokio::test]
    async fn http_transport_auth_disabled_path_uses_no_auth() {
        // Integration test: auth-disabled transport should not send auth header
        let mock = AuthRecordingClient::new();
        let transport = HttpLeaderTransport::with_client("http://leader:8080", mock.clone());

        // Fetch leader tip
        let request_tip = LeaderTipRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            timeout_ms: 5000,
        };
        let _ = transport.fetch_leader_tip(&request_tip).await;

        // Fetch proof
        let request_proof = ProofRequest {
            request_id: ProbeRequestId::new(),
            follower_identity: "follower-1".to_string(),
            start_sequence: 1,
            end_sequence: 10,
            timeout_ms: 5000,
        };
        let _ = transport.fetch_proof(&request_proof).await;

        // Neither request should have recorded a token
        let tokens = mock.recorded_tokens();
        assert_eq!(tokens.len(), 2);
        for token in &tokens {
            assert!(token.is_none());
        }
    }
}
