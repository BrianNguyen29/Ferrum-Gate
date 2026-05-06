//! # HTTP Client for FerrumGate Gateway
//!
//! Phase D-0 read-only REST client that maps MCP tools to FerrumGate REST API endpoints.
//!
//! ## Security Notes
//!
//! - Bearer tokens are stored internally but never logged
//! - Error messages do not expose sensitive data
//! - All HTTP responses are parsed as JSON
//!
//! ## Error Codes
//!
//! - `-32002`: Authentication failed (401/403 from gateway)
//! - `-32003`: Gateway unreachable (connection refused, timeout)
//! - `-32004`: Gateway server error (4xx/5xx from gateway)

use std::time::Duration;

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Gateway errors mapped from HTTP responses or network failures.
/// These are converted to MCP JSON-RPC error responses.
#[derive(Debug, Clone)]
pub enum GatewayError {
    /// Authentication failed (401 or 403 from gateway).
    /// Contains a user-safe message without exposing token data.
    AuthError { message: String },
    /// Gateway is unreachable (connection refused, DNS failure, timeout).
    /// Contains a user-safe message without exposing connection details.
    Unreachable { message: String },
    /// Gateway returned an HTTP error (4xx or 5xx).
    /// Contains the status code and a user-safe message.
    ServerError { status: u16, message: String },
}

impl GatewayError {
    /// Create an auth error with a safe message (no token exposure).
    pub fn auth(message: &str) -> Self {
        Self::AuthError {
            message: message.to_string(),
        }
    }

    /// Create a gateway unreachable error with a safe message.
    pub fn unreachable(message: &str) -> Self {
        Self::Unreachable {
            message: message.to_string(),
        }
    }

    /// Create a server error with status code and safe message.
    pub fn server_error(status: u16, message: &str) -> Self {
        Self::ServerError {
            status,
            message: message.to_string(),
        }
    }

    /// Get the JSON-RPC error code for this error.
    pub fn code(&self) -> i32 {
        match self {
            GatewayError::AuthError { .. } => crate::error_codes::AUTH_FAILED,
            GatewayError::Unreachable { .. } => crate::error_codes::GATEWAY_UNREACHABLE,
            GatewayError::ServerError { .. } => crate::error_codes::GATEWAY_SERVER_ERROR,
        }
    }

    /// Get the error message for this error.
    pub fn message(&self) -> &str {
        match self {
            GatewayError::AuthError { message } => message,
            GatewayError::Unreachable { message } => message,
            GatewayError::ServerError { message, .. } => message,
        }
    }
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::AuthError { message } => write!(f, "Authentication failed: {}", message),
            GatewayError::Unreachable { message } => write!(f, "Gateway unreachable: {}", message),
            GatewayError::ServerError { status, message } => {
                write!(f, "Gateway error ({}): {}", status, message)
            }
        }
    }
}

impl std::error::Error for GatewayError {}

// ---------------------------------------------------------------------------
// HTTP Client
// ---------------------------------------------------------------------------

/// Configuration for the FerrumGate gateway client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the FerrumGate gateway (e.g., "http://127.0.0.1:8080").
    pub base_url: String,
    /// Bearer token for authentication (if required by endpoint).
    /// Stored but never logged.
    pub bearer_token: Option<String>,
    /// Request timeout.
    pub timeout: Duration,
}

impl ClientConfig {
    /// Create a new client config with default settings.
    pub fn new() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".to_string(),
            bearer_token: None,
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the base URL.
    pub fn base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    /// Set the bearer token.
    /// The token is stored but never logged.
    pub fn bearer_token(mut self, token: &str) -> Self {
        self.bearer_token = Some(token.to_string());
        self
    }

    /// Set the request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Read the base URL from the FERRUM_GATEWAY_URL environment variable.
    /// Falls back to default if not set.
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("FERRUM_GATEWAY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
            bearer_token: std::env::var("FERRUM_GATEWAY_BEARER_TOKEN").ok(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP client for FerrumGate gateway REST API.
///
/// Provides methods for all 9 read-only MCP tools.
/// Each method calls the corresponding REST endpoint and returns parsed JSON.
#[derive(Debug, Clone)]
pub struct FerrumGatewayClient {
    /// Base URL of the gateway.
    base_url: String,
    /// Optional bearer token for protected endpoints.
    bearer_token: Option<String>,
    /// HTTP client for making requests.
    client: reqwest::blocking::Client,
}

impl FerrumGatewayClient {
    /// Create a new client with the given configuration.
    pub fn new(config: &ClientConfig) -> Result<Self, reqwest::Error> {
        let client = reqwest::blocking::Client::builder()
            .timeout(config.timeout)
            .redirect(reqwest::redirect::Policy::none()) // Disable auto-redirect
            .build()?;

        Ok(Self {
            base_url: config.base_url.clone(),
            bearer_token: config.bearer_token.clone(),
            client,
        })
    }

    /// Create a new client from environment variables.
    /// Reads FERRUM_GATEWAY_URL and FERRUM_GATEWAY_BEARER_TOKEN.
    pub fn from_env() -> Result<Self, reqwest::Error> {
        let config = ClientConfig::from_env();
        Self::new(&config)
    }

    /// Get the configured base URL.
    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Build a request with optional auth header.
    fn build_request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> reqwest::blocking::RequestBuilder {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut builder = self.client.request(method, &url);

        if let Some(ref token) = self.bearer_token {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }

        builder
    }

    /// Execute a request and handle errors.
    /// Maps HTTP responses to GatewayError variants appropriately.
    fn execute(
        &self,
        request: reqwest::blocking::Request,
    ) -> Result<serde_json::Value, GatewayError> {
        let response = self.client.execute(request).map_err(|e| {
            // Network error - gateway unreachable
            let msg = match e.is_timeout() {
                true => "Gateway request timed out",
                false => "Gateway is unreachable",
            };
            GatewayError::unreachable(msg)
        })?;

        let status = response.status();

        if status.as_u16() == 401 || status.as_u16() == 403 {
            // Auth error - don't log the response body
            return Err(GatewayError::auth("Bearer token invalid or missing"));
        }

        if !status.is_success() {
            // Server error - try to parse error message but don't log sensitive data
            let msg = match response.json::<serde_json::Value>() {
                Ok(json) => json
                    .get("message")
                    .or(json.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Gateway returned an error")
                    .to_string(),
                Err(_) => "Gateway returned an error".to_string(),
            };
            return Err(GatewayError::server_error(status.as_u16(), &msg));
        }

        // Parse success response as JSON
        response
            .json::<serde_json::Value>()
            .map_err(|_e| GatewayError::server_error(status.as_u16(), "Failed to parse response"))
    }

    // -------------------------------------------------------------------------
    // Read-only tool endpoints
    // -------------------------------------------------------------------------

    /// Health probe: GET /v1/healthz
    /// No auth required.
    pub fn health(&self) -> Result<serde_json::Value, GatewayError> {
        let request = self
            .build_request(reqwest::Method::GET, "/v1/healthz")
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// Deep readiness probe: GET /v1/readyz/deep
    /// No auth required.
    pub fn readyz_deep(&self) -> Result<serde_json::Value, GatewayError> {
        let request = self
            .build_request(reqwest::Method::GET, "/v1/readyz/deep")
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// List intents: GET /v1/intents
    /// Auth required.
    pub fn list_intents(
        &self,
        intent_id: Option<&str>,
        state: Option<&str>,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, GatewayError> {
        let path = "/v1/intents".to_string();
        let mut query_params: Vec<(String, String)> = Vec::new();

        if let Some(id) = intent_id {
            query_params.push(("intent_id".to_string(), id.to_string()));
        }
        if let Some(s) = state {
            query_params.push(("state".to_string(), s.to_string()));
        }
        if let Some(c) = cursor {
            query_params.push(("cursor".to_string(), c.to_string()));
        }
        if let Some(l) = limit {
            query_params.push(("limit".to_string(), l.to_string()));
        }

        let request = self
            .build_request(reqwest::Method::GET, &path)
            .query(&query_params)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// Get execution: GET /v1/executions/{execution_id}
    /// Auth required.
    pub fn get_execution(&self, execution_id: &str) -> Result<serde_json::Value, GatewayError> {
        let path = format!("/v1/executions/{}", execution_id);
        let request = self
            .build_request(reqwest::Method::GET, &path)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// Query lineage: GET /v1/provenance/query
    /// Auth required.
    pub fn query_lineage(
        &self,
        execution_id: Option<&str>,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<serde_json::Value, GatewayError> {
        let path = "/v1/provenance/query".to_string();
        let mut query_params: Vec<(String, String)> = Vec::new();

        if let Some(id) = execution_id {
            query_params.push(("execution_id".to_string(), id.to_string()));
        }
        if let Some(c) = cursor {
            query_params.push(("cursor".to_string(), c.to_string()));
        }
        if let Some(l) = limit {
            query_params.push(("limit".to_string(), l.to_string()));
        }

        let request = self
            .build_request(reqwest::Method::GET, &path)
            .query(&query_params)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// List approvals: GET /v1/approvals
    /// Auth required.
    pub fn list_approvals(&self) -> Result<serde_json::Value, GatewayError> {
        let request = self
            .build_request(reqwest::Method::GET, "/v1/approvals")
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// List policy bundles: GET /v1/policy-bundles
    /// Auth required.
    pub fn list_policy_bundles(&self) -> Result<serde_json::Value, GatewayError> {
        let request = self
            .build_request(reqwest::Method::GET, "/v1/policy-bundles")
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// List bridges: GET /v1/bridges
    /// Auth required.
    pub fn list_bridges(&self) -> Result<serde_json::Value, GatewayError> {
        let request = self
            .build_request(reqwest::Method::GET, "/v1/bridges")
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }

    /// List bridge tools: GET /v1/bridges/{bridge_id}/tools
    /// Auth required.
    pub fn list_bridge_tools(&self, bridge_id: &str) -> Result<serde_json::Value, GatewayError> {
        let path = format!("/v1/bridges/{}/tools", bridge_id);
        let request = self
            .build_request(reqwest::Method::GET, &path)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        self.execute(request)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert_eq!(config.base_url, "http://127.0.0.1:8080");
        assert!(config.bearer_token.is_none());
        assert_eq!(config.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_client_config_builder() {
        let config = ClientConfig::new()
            .base_url("http://localhost:9000")
            .bearer_token("secret-token")
            .timeout(Duration::from_secs(10));

        assert_eq!(config.base_url, "http://localhost:9000");
        assert_eq!(config.bearer_token, Some("secret-token".to_string()));
        assert_eq!(config.timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_gateway_error_codes() {
        let auth_err = GatewayError::auth("Token invalid");
        assert_eq!(auth_err.code(), -32002);

        let unreach_err = GatewayError::unreachable("Connection refused");
        assert_eq!(unreach_err.code(), -32003);

        let server_err = GatewayError::server_error(500, "Internal error");
        assert_eq!(server_err.code(), -32004);
    }

    #[test]
    fn test_gateway_error_messages() {
        let auth_err = GatewayError::auth("Token invalid");
        assert_eq!(auth_err.message(), "Token invalid");

        let unreach_err = GatewayError::unreachable("Connection refused");
        assert_eq!(unreach_err.message(), "Connection refused");

        let server_err = GatewayError::server_error(500, "Internal error");
        assert_eq!(server_err.message(), "Internal error");
    }

    #[test]
    fn test_gateway_error_display() {
        let auth_err = GatewayError::auth("Token invalid");
        assert!(auth_err.to_string().contains("Authentication failed"));

        let unreach_err = GatewayError::unreachable("Connection refused");
        assert!(unreach_err.to_string().contains("Gateway unreachable"));

        let server_err = GatewayError::server_error(500, "Internal error");
        let msg = server_err.to_string();
        assert!(msg.contains("Gateway error"));
        assert!(msg.contains("500"));
    }

    #[test]
    fn test_error_code_constants() {
        assert_eq!(crate::error_codes::AUTH_FAILED, -32002);
        assert_eq!(crate::error_codes::GATEWAY_UNREACHABLE, -32003);
        assert_eq!(crate::error_codes::GATEWAY_SERVER_ERROR, -32004);
    }
}
