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

    // -------------------------------------------------------------------------
    // D1.3.3 Compile-only tool endpoints
    // -------------------------------------------------------------------------

    /// Compile intent: POST /v1/intents/compile
    ///
    /// Per doc 79 P4: This is the compile-only gate (D1.3.3).
    /// It does NOT implement evaluate/mint/authorize/execute (D1.3.4+).
    ///
    /// Takes a `ferrum_proto::IntentCompileRequest` and returns the compiled
    /// `IntentEnvelope` with any warnings.
    ///
    /// # Arguments
    ///
    /// * `request` - The intent compile request with full governance fields
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::IntentCompileResponse, GatewayError>` containing:
    /// - `envelope`: The compiled intent with governance metadata
    /// - `warnings`: Any warnings from the compile process
    pub fn compile_intent(
        &self,
        request: &ferrum_proto::IntentCompileRequest,
    ) -> Result<ferrum_proto::IntentCompileResponse, GatewayError> {
        let request = self
            .build_request(reqwest::Method::POST, "/v1/intents/compile")
            .json(request)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { envelope: IntentEnvelope, warnings: Vec<String> }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse compile response: {}", e))
        })
    }

    // -------------------------------------------------------------------------
    // D1.3.4 Evaluate-only tool endpoint
    // -------------------------------------------------------------------------

    /// Evaluate proposal: POST /v1/proposals/{proposal_id}/evaluate
    ///
    /// Per doc 80: This is the evaluate-only gate (D1.3.4).
    /// It does NOT implement mint/authorize/prepare/execute (D1.4+).
    ///
    /// Takes a `proposal_id` in the URL path and an `ActionProposal` in the request body.
    /// The proposal is evaluated by the gateway's policy engine and returns an
    /// `EvaluateProposalResponse` with the decision.
    ///
    /// # Arguments
    ///
    /// * `proposal_id` - The proposal ID (must match `ActionProposal.proposal_id`)
    /// * `proposal` - The action proposal to evaluate
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::EvaluateProposalResponse, GatewayError>` containing:
    /// - `decision`: One of Allow, Deny, Quarantine, RequireApproval, AllowDraftOnly
    /// - `reason`: Human-readable reason for the decision
    /// - `matched_rule_ids`: Policy rules that matched during evaluation
    /// - `warnings`: Advisory warnings from the policy engine
    pub fn evaluate_proposal(
        &self,
        proposal_id: &ferrum_proto::ProposalId,
        proposal: &ferrum_proto::ActionProposal,
    ) -> Result<ferrum_proto::EvaluateProposalResponse, GatewayError> {
        let path = format!("/v1/proposals/{}/evaluate", proposal_id);
        let request = self
            .build_request(reqwest::Method::POST, &path)
            .json(proposal)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { decision, reason, matched_rule_ids, warnings }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse evaluate response: {}", e))
        })
    }

    // -------------------------------------------------------------------------
    // D1.4 Capability mint + authorize endpoints
    // -------------------------------------------------------------------------

    /// Mint a capability: POST /v1/capabilities/mint
    ///
    /// Per doc 81: This is the capability-mint gate (D1.4).
    /// It does NOT implement prepare/execute/verify/compensate (D1.5+).
    ///
    /// Takes a `CapabilityMintRequest` and returns a `CapabilityMintResponse`
    /// with the minted `lease` and any `warnings`.
    ///
    /// # Arguments
    ///
    /// * `request` - The capability mint request with tool/resource/constraint bindings
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::CapabilityMintResponse, GatewayError>` containing:
    /// - `lease`: The minted capability lease with capability_id, status, expiry
    /// - `warnings`: Advisory warnings from the minting process
    pub fn mint_capability(
        &self,
        request: &ferrum_proto::CapabilityMintRequest,
    ) -> Result<ferrum_proto::CapabilityMintResponse, GatewayError> {
        let request = self
            .build_request(reqwest::Method::POST, "/v1/capabilities/mint")
            .json(request)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { lease: CapabilityLease, warnings: Vec<String> }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse mint response: {}", e))
        })
    }

    /// Authorize execution: POST /v1/executions/authorize
    ///
    /// Per doc 81: This is the authorize-only gate (D1.4).
    /// It does NOT implement prepare/execute/verify/compensate (D1.5+).
    ///
    /// Takes an `AuthorizeExecutionRequest` and returns an `AuthorizeExecutionResponse`
    /// with the created `execution` record and any `warnings`.
    ///
    /// # Arguments
    ///
    /// * `request` - The authorize execution request with proposal_id, capability_id, dry_run
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::AuthorizeExecutionResponse, GatewayError>` containing:
    /// - `execution`: The execution record with execution_id, state, decision
    /// - `warnings`: Advisory warnings from the authorization process
    pub fn authorize_execution(
        &self,
        request: &ferrum_proto::AuthorizeExecutionRequest,
    ) -> Result<ferrum_proto::AuthorizeExecutionResponse, GatewayError> {
        let request = self
            .build_request(reqwest::Method::POST, "/v1/executions/authorize")
            .json(request)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { execution: ExecutionRecord, warnings: Vec<String> }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse authorize response: {}", e))
        })
    }

    // -------------------------------------------------------------------------
    // D1.5 Prepare-only tool endpoint
    // -------------------------------------------------------------------------

    /// Prepare execution: POST /v1/executions/{execution_id}/prepare
    ///
    /// Per doc 82: This is the prepare-only gate (D1.5).
    /// It does NOT implement execute/verify/compensate/rollback (D1.6+).
    ///
    /// Takes an `execution_id` in the URL path with NO request body.
    /// The gateway validates the execution is in Authorized or Prepared state,
    /// then creates a rollback contract for the execution.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID from authorize response
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::PrepareExecutionResponse, GatewayError>` containing:
    /// - `execution_id`: The execution ID
    /// - `prepared`: Whether preparation succeeded
    /// - `rollback_contract`: The created rollback contract (if successful)
    /// - `warnings`: Advisory warnings from the preparation process
    pub fn prepare_execution(
        &self,
        execution_id: &ferrum_proto::ExecutionId,
    ) -> Result<ferrum_proto::PrepareExecutionResponse, GatewayError> {
        let path = format!("/v1/executions/{}/prepare", execution_id);
        let request = self
            .build_request(reqwest::Method::POST, &path)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { execution_id, prepared, rollback_contract, warnings }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse prepare response: {}", e))
        })
    }

    // -------------------------------------------------------------------------
    // D1.6 Execute/Verify/Compensate endpoints
    // -------------------------------------------------------------------------

    /// Execute execution: POST /v1/executions/{execution_id}/execute
    ///
    /// Per doc 83: This is the execute-only gate (D1.6).
    /// It does NOT implement verify/compensate (D1.6+) but does not wire tool dispatch.
    ///
    /// Takes an `execution_id` in the URL path and an `ExecuteExecutionRequest` in the request body.
    /// The gateway validates the execution is in Authorized/Prepared/Proposed state,
    /// then executes the tool call via the adapter.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID from authorize response
    /// * `request` - The execute request with JSON payload for the adapter
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::ExecuteExecutionResponse, GatewayError>` containing:
    /// - `execution_id`: The execution ID
    /// - `executed`: Whether execution succeeded
    /// - `result_digest`: SHA-256 digest of the result (for verification)
    /// - `rollback_contract`: The updated rollback contract (state → ExecutedAwaitingVerify)
    /// - `warnings`: Advisory warnings from the execution
    pub fn execute_execution(
        &self,
        execution_id: &ferrum_proto::ExecutionId,
        request: &ferrum_proto::ExecuteExecutionRequest,
    ) -> Result<ferrum_proto::ExecuteExecutionResponse, GatewayError> {
        let path = format!("/v1/executions/{}/execute", execution_id);
        let request = self
            .build_request(reqwest::Method::POST, &path)
            .json(request)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { execution_id, executed, result_digest, rollback_contract, warnings }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse execute response: {}", e))
        })
    }

    /// Verify execution: POST /v1/executions/{execution_id}/verify
    ///
    /// Per doc 83: This is the verify-only gate (D1.6).
    /// It does NOT implement compensate (D1.6+) but does not wire tool dispatch.
    ///
    /// Takes an `execution_id` in the URL path with NO request body.
    /// The gateway validates the execution/contract are in ExecutedAwaitingVerify/Running state,
    /// then runs verification checks via the adapter.
    ///
    /// When verified=true and auto_commit=true: execution becomes Committed, SideEffectCommitted emitted.
    /// When verified=true and auto_commit=false: execution stays Running, SideEffectCommitted suppressed.
    /// When verified=false: execution becomes Failed, SideEffectCommitted suppressed.
    /// SideEffectVerified is ALWAYS emitted regardless of result.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID from execute response
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::VerifyExecutionResponse, GatewayError>` containing:
    /// - `execution_id`: The execution ID
    /// - `verified`: Whether verification succeeded
    /// - `rollback_contract`: The updated rollback contract (state → Verified or Failed)
    /// - `warnings`: Advisory warnings from verification
    pub fn verify_execution(
        &self,
        execution_id: &ferrum_proto::ExecutionId,
    ) -> Result<ferrum_proto::VerifyExecutionResponse, GatewayError> {
        let path = format!("/v1/executions/{}/verify", execution_id);
        let request = self
            .build_request(reqwest::Method::POST, &path)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { execution_id, verified, rollback_contract, warnings }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse verify response: {}", e))
        })
    }

    /// Compensate execution: POST /v1/executions/{execution_id}/compensate
    ///
    /// Per doc 83: This is the compensate-only gate (D1.6).
    /// It does NOT wire tool dispatch.
    ///
    /// Takes an `execution_id` in the URL path with NO request body.
    /// The gateway validates the execution/contract are in ExecutedAwaitingVerify/Running state,
    /// then runs compensation via the adapter to undo the executed side effect.
    ///
    /// # Arguments
    ///
    /// * `execution_id` - The execution ID from execute response
    ///
    /// # Returns
    ///
    /// `Result<ferrum_proto::CompensateExecutionResponse, GatewayError>` containing:
    /// - `execution_id`: The execution ID
    /// - `compensated`: Whether compensation succeeded
    /// - `rollback_contract`: The updated rollback contract (state → Compensated)
    /// - `warnings`: Advisory warnings from compensation
    pub fn compensate_execution(
        &self,
        execution_id: &ferrum_proto::ExecutionId,
    ) -> Result<ferrum_proto::CompensateExecutionResponse, GatewayError> {
        let path = format!("/v1/executions/{}/compensate", execution_id);
        let request = self
            .build_request(reqwest::Method::POST, &path)
            .build()
            .map_err(|_e| GatewayError::unreachable("Failed to build request"))?;

        let response: serde_json::Value = self.execute(request)?;

        // Parse the response into real ferrum-proto type
        // Real response is { execution_id, compensated, rollback_contract, warnings }
        serde_json::from_value(response).map_err(|e| {
            GatewayError::server_error(200, &format!("Failed to parse compensate response: {}", e))
        })
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

    // -------------------------------------------------------------------------
    // D1.3.3 Compile-only tests (mock-based, no live gateway)
    // -------------------------------------------------------------------------

    /// Captured HTTP request data from tiny_http server.
    #[derive(Debug, Clone)]
    struct CapturedRequest {
        method: String,
        path: String,
        body: String,
    }

    /// Starts a tiny_http server that captures the request method, path, and body.
    /// Returns base URL (without path) and a shared container for captured request.
    fn start_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedRequest>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        // Bind to a random available port
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        // Return base URL only (compile_intent will append /v1/intents/compile)
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            // Set a timeout so we don't block forever
            let response = tiny_http::Response::from_string(
                r#"{
                    "envelope": {
                        "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                        "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                        "session_id": null,
                        "channel_id": null,
                        "title": "test intent",
                        "goal": "test goal",
                        "normalized_goal": "test goal",
                        "allowed_outcomes": [
                            {
                                "id": "read",
                                "description": "read only",
                                "effect_type": "ReadOnlyAnalysis",
                                "required": true
                            }
                        ],
                        "forbidden_outcomes": [],
                        "resource_scope": [],
                        "risk_tier": "Low",
                        "approval_mode": "None",
                        "default_rollback_class": "R0NativeReversible",
                        "time_budget": { "max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1 },
                        "trust_context": {
                            "input_labels": [],
                            "sensitivity_labels": [],
                            "taint_score": 0,
                            "contains_external_metadata": false,
                            "contains_tool_output": false,
                            "contains_untrusted_text": false
                        },
                        "derived_from_event_ids": [],
                        "tags": [],
                        "metadata": {},
                        "status": "Active",
                        "created_at": "2025-01-01T00:00:00Z",
                        "expires_at": "2025-12-31T23:59:59Z"
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
            );

            // Use recv_timeout to avoid blocking forever
            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                // Capture method and path
                let method = request.method().to_string();
                let path = request.url().to_string();
                // Read the body
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedRequest { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_compile_intent_captures_request_body() {
        // Per doc 79 P4: compile_intent is compile-only (D1.3.3), NOT evaluate (D1.3.4+).
        // This test uses a tiny_http server to actually capture the HTTP request
        // and verify method, path, and body contain expected governance fields.

        let (captured_request, base_url, handle) = start_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        // Create a minimal IntentCompileRequest with a known principal_id
        let known_principal_id = ferrum_proto::PrincipalId(
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap(),
        );
        let request = ferrum_proto::IntentCompileRequest {
            principal_id: known_principal_id,
            session_id: None,
            channel_id: None,
            title: "test compile".to_string(),
            goal: "test goal".to_string(),
            agent_plan_summary: None,
            trusted_context: ferrum_proto::JsonMap::new(),
            raw_inputs: vec![],
            requested_resource_scope: vec![],
            requested_risk_tier: Some(ferrum_proto::RiskTier::Low),
            approval_mode: Some(ferrum_proto::ApprovalMode::None),
            metadata: ferrum_proto::JsonMap::new(),
        };

        let result = client.compile_intent(&request);
        assert!(
            result.is_ok(),
            "compile_intent should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.warnings.is_empty());
        assert_eq!(response.envelope.title, "test intent");

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is /v1/intents/compile
        assert_eq!(
            req.path, "/v1/intents/compile",
            "HTTP path should be /v1/intents/compile"
        );

        // Verify governance fields are present in the ACTUAL request body
        assert!(
            req.body
                .contains("\"principal_id\":\"550e8400-e29b-41d4-a716-446655440001\""),
            "Actual HTTP body should contain principal_id"
        );
        assert!(
            req.body.contains("\"title\":\"test compile\""),
            "Actual HTTP body should contain title"
        );
        assert!(
            req.body.contains("\"goal\":\"test goal\""),
            "Actual HTTP body should contain goal"
        );
        assert!(
            req.body.contains("\"requested_risk_tier\":\"Low\""),
            "Actual HTTP body should contain requested_risk_tier"
        );
        assert!(
            req.body.contains("\"approval_mode\":\"None\""),
            "Actual HTTP body should contain approval_mode"
        );

        // Also verify that the body is valid JSON
        let _: serde_json::Value =
            serde_json::from_str(&req.body).expect("Captured body should be valid JSON");

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_compile_intent_request_body_governance_fields() {
        // Verify that compile_intent sends governance fields (High risk, Required approval)
        // in the ACTUAL HTTP request (method, path, and body).

        let (captured_request, base_url, handle) = start_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        // Create request with known governance fields
        let known_principal_id = ferrum_proto::PrincipalId(
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap(),
        );
        let request = ferrum_proto::IntentCompileRequest {
            principal_id: known_principal_id,
            session_id: None,
            channel_id: None,
            title: "fs_write: /tmp/test.txt".to_string(),
            goal: "MCP tool call: fs_write on /tmp/test.txt".to_string(),
            agent_plan_summary: None,
            trusted_context: ferrum_proto::JsonMap::new(),
            raw_inputs: vec![],
            requested_resource_scope: vec![],
            requested_risk_tier: Some(ferrum_proto::RiskTier::High),
            approval_mode: Some(ferrum_proto::ApprovalMode::Required),
            metadata: ferrum_proto::JsonMap::new(),
        };

        let result = client.compile_intent(&request);
        assert!(
            result.is_ok(),
            "compile_intent should succeed: {:?}",
            result.err()
        );

        // Verify the ACTUAL HTTP request was captured
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is /v1/intents/compile
        assert_eq!(
            req.path, "/v1/intents/compile",
            "HTTP path should be /v1/intents/compile"
        );

        // Verify governance fields in the ACTUAL HTTP body sent by compile_intent
        assert!(
            req.body
                .contains("\"principal_id\":\"550e8400-e29b-41d4-a716-446655440002\""),
            "Actual HTTP body should contain principal_id"
        );
        assert!(
            req.body.contains("\"title\":\"fs_write: /tmp/test.txt\""),
            "Actual HTTP body should contain title"
        );
        assert!(
            req.body
                .contains("\"goal\":\"MCP tool call: fs_write on /tmp/test.txt\""),
            "Actual HTTP body should contain goal"
        );
        assert!(
            req.body.contains("\"requested_risk_tier\":\"High\""),
            "Actual HTTP body should contain requested_risk_tier: High"
        );
        assert!(
            req.body.contains("\"approval_mode\":\"Required\""),
            "Actual HTTP body should contain approval_mode: Required"
        );

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_compile_intent_response_with_warnings() {
        // Verify compile_intent correctly parses response with warnings.
        // Uses mockito since tiny_http returns a static response.

        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "envelope": {
                    "intent_id": "550e8400-e29b-41d4-a716-446655440000",
                    "principal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "session_id": null,
                    "channel_id": null,
                    "title": "test",
                    "goal": "test goal",
                    "normalized_goal": "test goal",
                    "allowed_outcomes": [
                        {
                            "id": "read",
                            "description": "read only",
                            "effect_type": "ReadOnlyAnalysis",
                            "required": true
                        }
                    ],
                    "forbidden_outcomes": [],
                    "resource_scope": [],
                    "risk_tier": "Low",
                    "approval_mode": "None",
                    "default_rollback_class": "R0NativeReversible",
                    "time_budget": { "max_duration_ms": 30000, "max_steps": 8, "max_retries_per_step": 1 },
                    "trust_context": {
                        "input_labels": [],
                        "sensitivity_labels": [],
                        "taint_score": 0,
                        "contains_external_metadata": false,
                        "contains_tool_output": false,
                        "contains_untrusted_text": false
                    },
                    "derived_from_event_ids": [],
                    "tags": [],
                    "metadata": {},
                    "status": "Active",
                    "created_at": "2025-01-01T00:00:00Z",
                    "expires_at": "2025-12-31T23:59:59Z"
                },
                "warnings": ["risk tier elevated to High", "approval required"]
            }"#)
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let known_principal_id = ferrum_proto::PrincipalId(
            uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap(),
        );
        let request = ferrum_proto::IntentCompileRequest {
            principal_id: known_principal_id,
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            agent_plan_summary: None,
            trusted_context: ferrum_proto::JsonMap::new(),
            raw_inputs: vec![],
            requested_resource_scope: vec![],
            requested_risk_tier: Some(ferrum_proto::RiskTier::Low),
            approval_mode: Some(ferrum_proto::ApprovalMode::None),
            metadata: ferrum_proto::JsonMap::new(),
        };

        let result = client.compile_intent(&request);
        assert!(
            result.is_ok(),
            "compile_intent should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert_eq!(response.warnings.len(), 2);
        assert!(
            response
                .warnings
                .contains(&"risk tier elevated to High".to_string())
        );
        assert!(response.warnings.contains(&"approval required".to_string()));

        mock.assert();
    }

    #[test]
    fn test_compile_intent_wrong_path_fails() {
        // Verify compile_intent fails when the path doesn't match.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = ferrum_proto::IntentCompileRequest {
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            agent_plan_summary: None,
            trusted_context: ferrum_proto::JsonMap::new(),
            raw_inputs: vec![],
            requested_resource_scope: vec![],
            requested_risk_tier: Some(ferrum_proto::RiskTier::Low),
            approval_mode: Some(ferrum_proto::ApprovalMode::None),
            metadata: ferrum_proto::JsonMap::new(),
        };

        // Request should fail because the mock on wrong path won't match
        let result = client.compile_intent(&request);
        assert!(
            result.is_err(),
            "compile_intent should fail when path doesn't match"
        );
    }

    #[test]
    fn test_compile_intent_wrong_method_fails() {
        // Verify compile_intent fails when the HTTP method is wrong.

        let mut server = mockito::Server::new();
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", "/v1/intents/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = ferrum_proto::IntentCompileRequest {
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            agent_plan_summary: None,
            trusted_context: ferrum_proto::JsonMap::new(),
            raw_inputs: vec![],
            requested_resource_scope: vec![],
            requested_risk_tier: Some(ferrum_proto::RiskTier::Low),
            approval_mode: Some(ferrum_proto::ApprovalMode::None),
            metadata: ferrum_proto::JsonMap::new(),
        };

        // Request should fail because the mock expects GET not POST
        let result = client.compile_intent(&request);
        assert!(
            result.is_err(),
            "compile_intent should fail when method doesn't match"
        );
    }

    // -------------------------------------------------------------------------
    // D1.3.4 Evaluate-only tests (mock-based, no live gateway)
    // -------------------------------------------------------------------------

    /// Creates a minimal ActionProposal for testing.
    fn make_test_action_proposal() -> ferrum_proto::ActionProposal {
        let proposal_id = ferrum_proto::ProposalId::new();
        let intent_id = ferrum_proto::IntentId::new();
        ferrum_proto::ActionProposal {
            proposal_id,
            intent_id,
            step_index: 1,
            title: "Test Proposal".to_string(),
            tool_name: "filesystem.read".to_string(),
            server_name: "fs-server".to_string(),
            raw_arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            expected_effect: "read file content".to_string(),
            estimated_risk: ferrum_proto::RiskTier::Low,
            requested_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            taint_inputs: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Captures HTTP request data for evaluate_proposal tests.
    #[derive(Debug, Clone)]
    struct CapturedEvaluateRequest {
        method: String,
        path: String,
        body: String,
    }

    /// Starts a tiny_http server that captures the evaluate request.
    fn start_evaluate_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedEvaluateRequest>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "decision": "Allow",
                    "reason": "policy_evaluation",
                    "matched_rule_ids": ["rule_001", "rule_002"],
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedEvaluateRequest { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_evaluate_proposal_captures_request_details() {
        // Per doc 80: evaluate_proposal is evaluate-only (D1.3.4).
        // This test verifies the HTTP method, path, and body are correct.

        let (captured_request, base_url, handle) = start_evaluate_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();
        let proposal_id = proposal.proposal_id;
        let expected_path = format!("/v1/proposals/{}/evaluate", proposal_id);

        let result = client.evaluate_proposal(&proposal_id, &proposal);
        assert!(
            result.is_ok(),
            "evaluate_proposal should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(matches!(response.decision, ferrum_proto::Decision::Allow));
        assert!(response.matched_rule_ids.len() == 2);

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/proposals/{proposal_id}/evaluate
        assert_eq!(
            req.path, expected_path,
            "HTTP path should be exactly /v1/proposals/{{id}}/evaluate"
        );

        // Verify ALL key ActionProposal governance fields are present in the body
        assert!(
            req.body.contains("\"proposal_id\""),
            "Actual HTTP body should contain proposal_id"
        );
        assert!(
            req.body.contains("\"intent_id\""),
            "Actual HTTP body should contain intent_id"
        );
        assert!(
            req.body.contains("\"tool_name\":\"filesystem.read\""),
            "Actual HTTP body should contain tool_name"
        );
        assert!(
            req.body.contains("\"server_name\":\"fs-server\""),
            "Actual HTTP body should contain server_name"
        );
        assert!(
            req.body.contains("\"title\":\"Test Proposal\""),
            "Actual HTTP body should contain title"
        );
        assert!(
            req.body.contains("\"estimated_risk\":\"Low\""),
            "Actual HTTP body should contain estimated_risk"
        );
        assert!(
            req.body
                .contains("\"requested_rollback_class\":\"R0NativeReversible\""),
            "Actual HTTP body should contain requested_rollback_class"
        );

        // Also verify that the body is valid JSON
        let _: serde_json::Value =
            serde_json::from_str(&req.body).expect("Captured body should be valid JSON");

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_evaluate_proposal_decision_allow() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "decision": "Allow",
                "reason": "policy_evaluation",
                "matched_rule_ids": ["rule_001"],
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let mut proposal = make_test_action_proposal();
        proposal.proposal_id = ferrum_proto::ProposalId::new();

        let result = client.evaluate_proposal(&proposal.proposal_id, &proposal);
        assert!(
            result.is_ok(),
            "evaluate_proposal should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(matches!(response.decision, ferrum_proto::Decision::Allow));
        assert_eq!(response.reason, "policy_evaluation");
        assert_eq!(response.matched_rule_ids, vec!["rule_001"]);

        _mock.assert();
    }

    #[test]
    fn test_evaluate_proposal_decision_deny() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "decision": "Deny",
                "reason": "risk_too_high",
                "matched_rule_ids": ["rule_deny_high_risk"],
                "warnings": ["elevated risk detected"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();

        let result = client.evaluate_proposal(&ferrum_proto::ProposalId::new(), &proposal);
        assert!(
            result.is_ok(),
            "evaluate_proposal should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(matches!(response.decision, ferrum_proto::Decision::Deny));
        assert_eq!(response.reason, "risk_too_high");
        assert!(
            response
                .warnings
                .contains(&"elevated risk detected".to_string())
        );

        _mock.assert();
    }

    #[test]
    fn test_evaluate_proposal_decision_quarantine() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "decision": "Quarantine",
                "reason": "suspicious_pattern_detected",
                "matched_rule_ids": ["rule_quarantine_001"],
                "warnings": ["manual review required"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();

        let result = client.evaluate_proposal(&ferrum_proto::ProposalId::new(), &proposal);
        assert!(
            result.is_ok(),
            "evaluate_proposal should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(matches!(
            response.decision,
            ferrum_proto::Decision::Quarantine
        ));
        assert_eq!(response.reason, "suspicious_pattern_detected");
        assert!(
            response
                .warnings
                .contains(&"manual review required".to_string())
        );

        _mock.assert();
    }

    #[test]
    fn test_evaluate_proposal_decision_require_approval() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "decision": "RequireApproval",
                "reason": "approval_required_for_high_risk",
                "matched_rule_ids": ["rule_approval_001", "rule_approval_002"],
                "warnings": ["high risk operation"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();

        let result = client.evaluate_proposal(&ferrum_proto::ProposalId::new(), &proposal);
        assert!(
            result.is_ok(),
            "evaluate_proposal should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(matches!(
            response.decision,
            ferrum_proto::Decision::RequireApproval
        ));
        assert_eq!(response.reason, "approval_required_for_high_risk");
        assert_eq!(response.matched_rule_ids.len(), 2);

        _mock.assert();
    }

    #[test]
    fn test_evaluate_proposal_decision_allow_draft_only() {
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "decision": "AllowDraftOnly",
                "reason": "draft_mode_only_permitted",
                "matched_rule_ids": ["rule_draft_001"],
                "warnings": ["only draft execution allowed"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();

        let result = client.evaluate_proposal(&ferrum_proto::ProposalId::new(), &proposal);
        assert!(
            result.is_ok(),
            "evaluate_proposal should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(matches!(
            response.decision,
            ferrum_proto::Decision::AllowDraftOnly
        ));
        assert_eq!(response.reason, "draft_mode_only_permitted");
        assert!(
            response
                .warnings
                .contains(&"only draft execution allowed".to_string())
        );

        _mock.assert();
    }

    #[test]
    fn test_evaluate_proposal_wrong_path_fails() {
        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();

        // Request should fail because the mock on wrong path won't match
        let result = client.evaluate_proposal(&ferrum_proto::ProposalId::new(), &proposal);
        assert!(
            result.is_err(),
            "evaluate_proposal should fail when path doesn't match"
        );
    }

    #[test]
    fn test_evaluate_proposal_wrong_method_fails() {
        let mut server = mockito::Server::new();
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", "/v1/proposals/test-proposal-id/evaluate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let proposal = make_test_action_proposal();

        // Request should fail because the mock expects GET not POST
        let result = client.evaluate_proposal(&ferrum_proto::ProposalId::new(), &proposal);
        assert!(
            result.is_err(),
            "evaluate_proposal should fail when method doesn't match"
        );
    }

    // -------------------------------------------------------------------------
    // D1.4 Mint capability tests (mock-based, no live gateway)
    // -------------------------------------------------------------------------

    /// Creates a minimal CapabilityMintRequest for testing.
    fn make_test_mint_request() -> ferrum_proto::CapabilityMintRequest {
        let intent_id = ferrum_proto::IntentId::new();
        let proposal_id = ferrum_proto::ProposalId::new();
        ferrum_proto::CapabilityMintRequest {
            intent_id,
            proposal_id,
            tool_binding: ferrum_proto::ToolBinding {
                server_name: "fs-server".to_string(),
                tool_name: "filesystem.write".to_string(),
                tool_version: Some("1.0.0".to_string()),
            },
            resource_bindings: vec![ferrum_proto::ResourceBinding::File {
                path: "/tmp/output.txt".to_string(),
                mode: ferrum_proto::ResourceMode::Write,
                required_hash: None,
            }],
            argument_constraints: vec![ferrum_proto::ArgumentConstraint::JsonPointerMustExist {
                pointer: "/content".to_string(),
            }],
            taint_budget: ferrum_proto::TaintBudget {
                max_taint_score: 30,
                allow_external_tool_output: false,
                allow_external_metadata: false,
                allow_untrusted_text: false,
            },
            approval_binding: None,
            requested_ttl_secs: 120,
            metadata: ferrum_proto::JsonMap::new(),
        }
    }

    /// Captures HTTP request data for mint_capability tests.
    #[derive(Debug, Clone)]
    struct CapturedMintRequest {
        method: String,
        path: String,
        body: String,
    }

    /// Starts a tiny_http server that captures the mint request.
    fn start_mint_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedMintRequest>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "lease": {
                        "capability_id": "550e8400-e29b-41d4-a716-446655440099",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                        "tool_binding": {
                            "server_name": "fs-server",
                            "tool_name": "filesystem.write",
                            "tool_version": "1.0.0"
                        },
                        "resource_bindings": [
                            {
                                "kind": "File",
                                "path": "/tmp/output.txt",
                                "mode": "Write",
                                "required_hash": null
                            }
                        ],
                        "argument_constraints": [
                            {"type": "JsonPointerMustExist", "pointer": "/content"}
                        ],
                        "taint_budget": {
                            "max_taint_score": 30,
                            "allow_external_tool_output": false,
                            "allow_external_metadata": false,
                            "allow_untrusted_text": false
                        },
                        "approval_binding": null,
                        "issued_by": "ferrum-cap",
                        "policy_bundle_id": "550e8400-e29b-41d4-a716-446655440003",
                        "tool_manifest_id": null,
                        "manifest_hash": null,
                        "status": "Active",
                        "issued_at": "2026-05-07T00:00:00Z",
                        "expires_at": "2026-05-07T00:02:00Z",
                        "revoked_at": null,
                        "metadata": {}
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedMintRequest { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_mint_capability_captures_request_details() {
        // Per doc 81: mint_capability is capability-mint only (D1.4).
        // This test verifies the HTTP method, path, and body are correct.

        let (captured_request, base_url, handle) = start_mint_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_mint_request();
        let result = client.mint_capability(&request);
        assert!(
            result.is_ok(),
            "mint_capability should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.warnings.is_empty());
        assert!(matches!(
            response.lease.status,
            ferrum_proto::CapabilityStatus::Active
        ));

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/capabilities/mint
        assert_eq!(
            req.path, "/v1/capabilities/mint",
            "HTTP path should be exactly /v1/capabilities/mint"
        );

        // Verify key request fields are present in the body
        assert!(
            req.body.contains("\"tool_name\":\"filesystem.write\""),
            "Actual HTTP body should contain tool_name"
        );
        assert!(
            req.body.contains("\"server_name\":\"fs-server\""),
            "Actual HTTP body should contain server_name"
        );
        assert!(
            req.body.contains("\"requested_ttl_secs\":120"),
            "Actual HTTP body should contain requested_ttl_secs"
        );
        assert!(
            req.body.contains("\"/tmp/output.txt\""),
            "Actual HTTP body should contain resource path"
        );

        // Also verify that the body is valid JSON
        let _: serde_json::Value =
            serde_json::from_str(&req.body).expect("Captured body should be valid JSON");

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_mint_capability_response_with_warnings() {
        // Verify mint_capability correctly parses response with warnings.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/capabilities/mint")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "lease": {
                    "capability_id": "550e8400-e29b-41d4-a716-446655440099",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "tool_binding": {
                        "server_name": "fs-server",
                        "tool_name": "filesystem.write",
                        "tool_version": "1.0.0"
                    },
                    "resource_bindings": [],
                    "argument_constraints": [],
                    "taint_budget": {
                        "max_taint_score": 30,
                        "allow_external_tool_output": false,
                        "allow_external_metadata": false,
                        "allow_untrusted_text": false
                    },
                    "approval_binding": null,
                    "issued_by": "ferrum-cap",
                    "policy_bundle_id": "550e8400-e29b-41d4-a716-446655440003",
                    "tool_manifest_id": null,
                    "manifest_hash": null,
                    "status": "Active",
                    "issued_at": "2026-05-07T00:00:00Z",
                    "expires_at": "2026-05-07T00:02:00Z",
                    "revoked_at": null,
                    "metadata": {}
                },
                "warnings": ["TTL capped at 300 seconds", "resource scope validated"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_mint_request();
        let result = client.mint_capability(&request);
        assert!(
            result.is_ok(),
            "mint_capability should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert_eq!(response.warnings.len(), 2);
        assert!(
            response
                .warnings
                .contains(&"TTL capped at 300 seconds".to_string())
        );
        assert!(
            response
                .warnings
                .contains(&"resource scope validated".to_string())
        );
        assert!(matches!(
            response.lease.status,
            ferrum_proto::CapabilityStatus::Active
        ));

        _mock.assert();
    }

    #[test]
    fn test_mint_capability_wrong_path_fails() {
        // Verify mint_capability fails when the path doesn't match.
        // This ensures we are NOT calling prepare/execute/verify/compensate.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_mint_request();

        // Request should fail because the mock on wrong path won't match
        let result = client.mint_capability(&request);
        assert!(
            result.is_err(),
            "mint_capability should fail when path doesn't match"
        );
    }

    #[test]
    fn test_mint_capability_wrong_method_fails() {
        // Verify mint_capability fails when the HTTP method is wrong.
        // This ensures we are NOT calling prepare/execute/verify/compensate.

        let mut server = mockito::Server::new();
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", "/v1/capabilities/mint")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_mint_request();

        // Request should fail because the mock expects GET not POST
        let result = client.mint_capability(&request);
        assert!(
            result.is_err(),
            "mint_capability should fail when method doesn't match"
        );
    }

    // -------------------------------------------------------------------------
    // D1.4 Authorize execution tests (mock-based, no live gateway)
    // -------------------------------------------------------------------------

    /// Creates a minimal AuthorizeExecutionRequest for testing.
    fn make_test_authorize_request(dry_run: bool) -> ferrum_proto::AuthorizeExecutionRequest {
        ferrum_proto::AuthorizeExecutionRequest {
            proposal_id: ferrum_proto::ProposalId::new(),
            capability_id: ferrum_proto::CapabilityId::new(),
            dry_run,
        }
    }

    /// Captures HTTP request data for authorize_execution tests.
    #[derive(Debug, Clone)]
    struct CapturedAuthorizeRequest {
        method: String,
        path: String,
        body: String,
    }

    /// Starts a tiny_http server that captures the authorize request.
    fn start_authorize_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedAuthorizeRequest>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "execution": {
                        "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                        "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                        "rollback_contract_id": null,
                        "decision": "Allow",
                        "state": "Prepared",
                        "started_at": "2026-05-07T00:00:00Z",
                        "finished_at": null,
                        "result_digest": null,
                        "metadata": {}
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedAuthorizeRequest { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_authorize_execution_captures_request_details() {
        // Per doc 81: authorize_execution is authorize-only (D1.4).
        // This test verifies the HTTP method, path, and body are correct.

        let (captured_request, base_url, handle) = start_authorize_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(false);
        let result = client.authorize_execution(&request);
        assert!(
            result.is_ok(),
            "authorize_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.warnings.is_empty());
        assert!(matches!(
            response.execution.state,
            ferrum_proto::ExecutionState::Prepared
        ));

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/executions/authorize
        assert_eq!(
            req.path, "/v1/executions/authorize",
            "HTTP path should be exactly /v1/executions/authorize"
        );

        // Verify key request fields are present in the body
        assert!(
            req.body.contains("\"proposal_id\""),
            "Actual HTTP body should contain proposal_id"
        );
        assert!(
            req.body.contains("\"capability_id\""),
            "Actual HTTP body should contain capability_id"
        );
        assert!(
            req.body.contains("\"dry_run\":false"),
            "Actual HTTP body should contain dry_run: false"
        );

        // Also verify that the body is valid JSON
        let _: serde_json::Value =
            serde_json::from_str(&req.body).expect("Captured body should be valid JSON");

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_authorize_execution_dry_run_true() {
        // Verify authorize_execution with dry_run=true returns state=Authorized.
        // Both dry_run=true and dry_run=false consume capability and emit provenance.
        // The only difference is the ExecutionState (Authorized vs Prepared).

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/executions/authorize")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution": {
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                    "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                    "rollback_contract_id": null,
                    "decision": "Allow",
                    "state": "Authorized",
                    "started_at": "2026-05-07T00:00:00Z",
                    "finished_at": null,
                    "result_digest": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(true);
        let result = client.authorize_execution(&request);
        assert!(
            result.is_ok(),
            "authorize_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.warnings.is_empty());
        // dry_run=true should return state=Authorized
        assert!(matches!(
            response.execution.state,
            ferrum_proto::ExecutionState::Authorized
        ));

        _mock.assert();
    }

    #[test]
    fn test_authorize_execution_dry_run_false() {
        // Verify authorize_execution with dry_run=false returns state=Prepared.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/executions/authorize")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution": {
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                    "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                    "rollback_contract_id": null,
                    "decision": "Allow",
                    "state": "Prepared",
                    "started_at": "2026-05-07T00:00:00Z",
                    "finished_at": null,
                    "result_digest": null,
                    "metadata": {}
                },
                "warnings": ["execution prepared for running"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(false);
        let result = client.authorize_execution(&request);
        assert!(
            result.is_ok(),
            "authorize_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert_eq!(response.warnings.len(), 1);
        assert!(
            response
                .warnings
                .contains(&"execution prepared for running".to_string())
        );
        // dry_run=false should return state=Prepared
        assert!(matches!(
            response.execution.state,
            ferrum_proto::ExecutionState::Prepared
        ));

        _mock.assert();
    }

    #[test]
    fn test_authorize_execution_response_with_warnings() {
        // Verify authorize_execution correctly parses response with warnings.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", "/v1/executions/authorize")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution": {
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                    "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                    "rollback_contract_id": null,
                    "decision": "Allow",
                    "state": "Prepared",
                    "started_at": "2026-05-07T00:00:00Z",
                    "finished_at": null,
                    "result_digest": null,
                    "metadata": {}
                },
                "warnings": ["elevated risk detected", "approval required for this operation"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(false);
        let result = client.authorize_execution(&request);
        assert!(
            result.is_ok(),
            "authorize_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert_eq!(response.warnings.len(), 2);
        assert!(
            response
                .warnings
                .contains(&"elevated risk detected".to_string())
        );
        assert!(
            response
                .warnings
                .contains(&"approval required for this operation".to_string())
        );

        _mock.assert();
    }

    #[test]
    fn test_authorize_execution_wrong_path_fails() {
        // Verify authorize_execution fails when the path doesn't match.
        // This ensures we are NOT calling prepare/execute/verify/compensate.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(false);

        // Request should fail because the mock on wrong path won't match
        let result = client.authorize_execution(&request);
        assert!(
            result.is_err(),
            "authorize_execution should fail when path doesn't match"
        );
    }

    #[test]
    fn test_authorize_execution_wrong_method_fails() {
        // Verify authorize_execution fails when the HTTP method is wrong.
        // This ensures we are NOT calling prepare/execute/verify/compensate.

        let mut server = mockito::Server::new();
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", "/v1/executions/authorize")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(false);

        // Request should fail because the mock expects GET not POST
        let result = client.authorize_execution(&request);
        assert!(
            result.is_err(),
            "authorize_execution should fail when method doesn't match"
        );
    }

    // -------------------------------------------------------------------------
    // D1.4 negative tests: verify no prepare/execute/verify/compensate paths
    // -------------------------------------------------------------------------

    #[test]
    fn test_mint_is_not_prepare_execute_verify_compensate() {
        // Verify mint_capability does NOT call any D1.5+ paths.
        // Mock the D1.5+ paths and ensure they are NOT called.

        let mut server = mockito::Server::new();

        // Mock prepare path - should NOT be called
        let _mock_prepare = server
            .mock("POST", "/v1/executions/prepare")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock execute path - should NOT be called
        let _mock_execute = server
            .mock("POST", "/v1/executions/execute")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock verify path - should NOT be called
        let _mock_verify = server
            .mock("POST", "/v1/executions/verify")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock compensate path - should NOT be called
        let _mock_compensate = server
            .mock("POST", "/v1/executions/compensate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Correct mock for mint
        let _mock_mint = server
            .mock("POST", "/v1/capabilities/mint")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "lease": {
                    "capability_id": "550e8400-e29b-41d4-a716-446655440099",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "tool_binding": {
                        "server_name": "fs-server",
                        "tool_name": "filesystem.write",
                        "tool_version": "1.0.0"
                    },
                    "resource_bindings": [],
                    "argument_constraints": [],
                    "taint_budget": {
                        "max_taint_score": 30,
                        "allow_external_tool_output": false,
                        "allow_external_metadata": false,
                        "allow_untrusted_text": false
                    },
                    "approval_binding": null,
                    "issued_by": "ferrum-cap",
                    "policy_bundle_id": "550e8400-e29b-41d4-a716-446655440003",
                    "tool_manifest_id": null,
                    "manifest_hash": null,
                    "status": "Active",
                    "issued_at": "2026-05-07T00:00:00Z",
                    "expires_at": "2026-05-07T00:02:00Z",
                    "revoked_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_mint_request();
        let result = client.mint_capability(&request);
        assert!(
            result.is_ok(),
            "mint_capability should succeed: {:?}",
            result.err()
        );

        // All the wrong-path mocks should have 0 invocations (verified by mockito automatically)
        _mock_mint.assert();
    }

    #[test]
    fn test_authorize_is_not_prepare_execute_verify_compensate() {
        // Verify authorize_execution does NOT call any D1.5+ paths.
        // Mock the D1.5+ paths and ensure they are NOT called.

        let mut server = mockito::Server::new();

        // Mock prepare path - should NOT be called
        let _mock_prepare = server
            .mock("POST", "/v1/executions/prepare")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock execute path - should NOT be called
        let _mock_execute = server
            .mock("POST", "/v1/executions/execute")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock verify path - should NOT be called
        let _mock_verify = server
            .mock("POST", "/v1/executions/verify")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock compensate path - should NOT be called
        let _mock_compensate = server
            .mock("POST", "/v1/executions/compensate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Correct mock for authorize
        let _mock_authorize = server
            .mock("POST", "/v1/executions/authorize")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution": {
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                    "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                    "rollback_contract_id": null,
                    "decision": "Allow",
                    "state": "Prepared",
                    "started_at": "2026-05-07T00:00:00Z",
                    "finished_at": null,
                    "result_digest": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = make_test_authorize_request(false);
        let result = client.authorize_execution(&request);
        assert!(
            result.is_ok(),
            "authorize_execution should succeed: {:?}",
            result.err()
        );

        // All the wrong-path mocks should have 0 invocations (verified by mockito automatically)
        _mock_authorize.assert();
    }

    // -------------------------------------------------------------------------
    // D1.5 Prepare execution tests (mock-based, no live gateway)
    // -------------------------------------------------------------------------

    /// Captures HTTP request data for prepare_execution tests.
    #[derive(Debug, Clone)]
    struct CapturedPrepareRequest {
        method: String,
        path: String,
        body: String,
    }

    /// Starts a tiny_http server that captures the prepare request.
    fn start_prepare_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedPrepareRequest>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "prepared": true,
                    "rollback_contract": {
                        "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                        "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                        "action_type": "FileWrite",
                        "rollback_class": "R0NativeReversible",
                        "adapter_key": "fs-adapter",
                        "target": {
                            "kind": "FilePath",
                            "path": "/tmp/output.txt",
                            "before_hash": null,
                            "after_hash": null
                        },
                        "prepare_checks": [],
                        "verify_checks": [],
                        "compensation_plan": [],
                        "auto_commit": false,
                        "state": "Prepared",
                        "created_at": "2026-05-07T00:00:00Z",
                        "expires_at": null,
                        "metadata": {}
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedPrepareRequest { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_prepare_execution_captures_request_details() {
        // Per doc 82: prepare_execution is prepare-only (D1.5).
        // This test verifies the HTTP method, path, and NO body are correct.

        let (captured_request, base_url, handle) = start_prepare_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let result = client.prepare_execution(&execution_id);
        assert!(
            result.is_ok(),
            "prepare_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.prepared);
        assert!(response.warnings.is_empty());
        assert!(response.rollback_contract.is_some());

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/executions/{execution_id}/prepare
        let expected_path = format!("/v1/executions/{}/prepare", execution_id);
        assert_eq!(
            req.path, expected_path,
            "HTTP path should be exactly /v1/executions/{{id}}/prepare"
        );

        // Verify NO body is sent (prepare takes no request body)
        assert!(
            req.body.is_empty() || req.body == "{}",
            "Prepare should have no body, got: {}",
            req.body
        );

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_prepare_execution_response_with_rollback_contract() {
        // Verify prepare_execution correctly parses response with rollback_contract and warnings.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "prepared": true,
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": null
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "Prepared",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": ["file already exists, backup created"]
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let result = client.prepare_execution(&execution_id);
        assert!(
            result.is_ok(),
            "prepare_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.prepared);
        assert_eq!(response.warnings.len(), 1);
        assert!(
            response
                .warnings
                .contains(&"file already exists, backup created".to_string())
        );
        assert!(response.rollback_contract.is_some());

        let contract = response.rollback_contract.unwrap();
        assert!(matches!(
            contract.state,
            ferrum_proto::RollbackState::Prepared
        ));

        _mock.assert();
    }

    #[test]
    fn test_prepare_execution_wrong_path_fails() {
        // Verify prepare_execution fails when the path doesn't match.
        // This ensures we are NOT calling execute/verify/compensate/rollback.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();

        // Request should fail because the mock on wrong path won't match
        let result = client.prepare_execution(&execution_id);
        assert!(
            result.is_err(),
            "prepare_execution should fail when path doesn't match"
        );
    }

    #[test]
    fn test_prepare_execution_wrong_method_fails() {
        // Verify prepare_execution fails when the HTTP method is wrong.
        // This ensures we are NOT calling execute/verify/compensate/rollback.

        let mut server = mockito::Server::new();
        // Use actual execution ID in path for correct matching
        let execution_id = ferrum_proto::ExecutionId::new();
        let path = format!("/v1/executions/{}/prepare", execution_id);
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        // Request should fail because the mock expects GET not POST
        let result = client.prepare_execution(&execution_id);
        assert!(
            result.is_err(),
            "prepare_execution should fail when method doesn't match"
        );
    }

    #[test]
    fn test_prepare_is_not_execute_verify_compensate_rollback() {
        // Verify prepare_execution does NOT call any D1.6+ paths.
        // Mock the D1.6+ paths and ensure they are NOT called.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let prepare_path = format!("/v1/executions/{}/prepare", execution_id);

        // Mock execute path - should NOT be called
        let _mock_execute = server
            .mock(
                "POST",
                format!("/v1/executions/{}/execute", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock verify path - should NOT be called
        let _mock_verify = server
            .mock(
                "POST",
                format!("/v1/executions/{}/verify", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock compensate path - should NOT be called
        let _mock_compensate = server
            .mock(
                "POST",
                format!("/v1/executions/{}/compensate", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Correct mock for prepare
        let _mock_prepare = server
            .mock("POST", prepare_path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "prepared": true,
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": null
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "Prepared",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = client.prepare_execution(&execution_id);
        assert!(
            result.is_ok(),
            "prepare_execution should succeed: {:?}",
            result.err()
        );

        // All the wrong-path mocks should have 0 invocations (verified by mockito automatically)
        _mock_prepare.assert();
    }

    // -------------------------------------------------------------------------
    // D1.6 Execute/Verify/Compensate tests (mock-based, no live gateway)
    // -------------------------------------------------------------------------

    /// Captures HTTP request data for D1.6 execute/verify/compensate tests.
    #[derive(Debug, Clone)]
    struct CapturedD1_6Request {
        method: String,
        path: String,
        body: String,
    }

    // -------------------------------------------------------------------------
    // execute_execution tests
    // -------------------------------------------------------------------------

    /// Starts a tiny_http server that captures the execute request.
    fn start_execute_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedD1_6Request>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "executed": true,
                    "result_digest": "sha256:abc123",
                    "rollback_contract": {
                        "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                        "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                        "action_type": "FileWrite",
                        "rollback_class": "R0NativeReversible",
                        "adapter_key": "fs-adapter",
                        "target": {
                            "kind": "FilePath",
                            "path": "/tmp/output.txt",
                            "before_hash": null,
                            "after_hash": "sha256:abc123"
                        },
                        "prepare_checks": [],
                        "verify_checks": [],
                        "compensation_plan": [],
                        "auto_commit": false,
                        "state": "ExecutedAwaitingVerify",
                        "created_at": "2026-05-07T00:00:00Z",
                        "expires_at": null,
                        "metadata": {}
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedD1_6Request { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_execute_execution_captures_request_details() {
        // Per doc 83: execute_execution is execute-only (D1.6), NOT verify/compensate.
        // This test verifies the HTTP method, path, and JSON body are correct.

        let (captured_request, base_url, handle) = start_execute_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let request = ferrum_proto::ExecuteExecutionRequest {
            payload: serde_json::json!({"content": "hello world"}),
        };

        let result = client.execute_execution(&execution_id, &request);
        assert!(
            result.is_ok(),
            "execute_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.executed);
        assert_eq!(response.result_digest, Some("sha256:abc123".to_string()));

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/executions/{execution_id}/execute
        let expected_path = format!("/v1/executions/{}/execute", execution_id);
        assert_eq!(
            req.path, expected_path,
            "HTTP path should be exactly /v1/executions/{{id}}/execute"
        );

        // Verify request body contains the payload
        assert!(
            req.body.contains("\"content\":\"hello world\""),
            "Actual HTTP body should contain payload content"
        );

        // Also verify that the body is valid JSON
        let _: serde_json::Value =
            serde_json::from_str(&req.body).expect("Captured body should be valid JSON");

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_execute_execution_response_parsing() {
        // Verify execute_execution correctly parses response with result_digest and rollback_contract.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "executed": true,
                "result_digest": "sha256:def456",
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": "sha256:def456"
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": true,
                    "state": "ExecutedAwaitingVerify",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let request = ferrum_proto::ExecuteExecutionRequest {
            payload: serde_json::json!({"content": "test"}),
        };

        let result = client.execute_execution(&execution_id, &request);
        assert!(
            result.is_ok(),
            "execute_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.executed);
        assert_eq!(response.result_digest, Some("sha256:def456".to_string()));
        assert!(response.rollback_contract.is_some());

        let contract = response.rollback_contract.unwrap();
        assert!(matches!(
            contract.state,
            ferrum_proto::RollbackState::ExecutedAwaitingVerify
        ));
        assert!(contract.auto_commit); // auto_commit=true in this mock

        _mock.assert();
    }

    #[test]
    fn test_execute_execution_wrong_path_fails() {
        // Verify execute_execution fails when the path doesn't match.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let request = ferrum_proto::ExecuteExecutionRequest {
            payload: serde_json::json!({"content": "test"}),
        };

        let result = client.execute_execution(&execution_id, &request);
        assert!(
            result.is_err(),
            "execute_execution should fail when path doesn't match"
        );
    }

    #[test]
    fn test_execute_execution_wrong_method_fails() {
        // Verify execute_execution fails when the HTTP method is wrong.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let path = format!("/v1/executions/{}/execute", execution_id);
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = ferrum_proto::ExecuteExecutionRequest {
            payload: serde_json::json!({"content": "test"}),
        };

        let result = client.execute_execution(&execution_id, &request);
        assert!(
            result.is_err(),
            "execute_execution should fail when method doesn't match"
        );
    }

    #[test]
    fn test_execute_is_not_verify_or_compensate() {
        // Verify execute_execution does NOT call verify or compensate paths.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let execute_path = format!("/v1/executions/{}/execute", execution_id);

        // Mock verify path - should NOT be called
        let _mock_verify = server
            .mock(
                "POST",
                format!("/v1/executions/{}/verify", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock compensate path - should NOT be called
        let _mock_compensate = server
            .mock(
                "POST",
                format!("/v1/executions/{}/compensate", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Correct mock for execute
        let _mock_execute = server
            .mock("POST", execute_path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "executed": true,
                "result_digest": "sha256:abc123",
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": "sha256:abc123"
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "ExecutedAwaitingVerify",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let request = ferrum_proto::ExecuteExecutionRequest {
            payload: serde_json::json!({"content": "test"}),
        };

        let result = client.execute_execution(&execution_id, &request);
        assert!(
            result.is_ok(),
            "execute_execution should succeed: {:?}",
            result.err()
        );

        _mock_execute.assert();
    }

    // -------------------------------------------------------------------------
    // verify_execution tests
    // -------------------------------------------------------------------------

    /// Starts a tiny_http server that captures the verify request.
    fn start_verify_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedD1_6Request>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "verified": true,
                    "rollback_contract": {
                        "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                        "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                        "action_type": "FileWrite",
                        "rollback_class": "R0NativeReversible",
                        "adapter_key": "fs-adapter",
                        "target": {
                            "kind": "FilePath",
                            "path": "/tmp/output.txt",
                            "before_hash": null,
                            "after_hash": "sha256:abc123"
                        },
                        "prepare_checks": [],
                        "verify_checks": [],
                        "compensation_plan": [],
                        "auto_commit": true,
                        "state": "Verified",
                        "created_at": "2026-05-07T00:00:00Z",
                        "expires_at": null,
                        "metadata": {}
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedD1_6Request { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_verify_execution_captures_request_details() {
        // Per doc 83: verify_execution is verify-only (D1.6).
        // This test verifies the HTTP method, path, and NO body are correct.

        let (captured_request, base_url, handle) = start_verify_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let result = client.verify_execution(&execution_id);
        assert!(
            result.is_ok(),
            "verify_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.verified);

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/executions/{execution_id}/verify
        let expected_path = format!("/v1/executions/{}/verify", execution_id);
        assert_eq!(
            req.path, expected_path,
            "HTTP path should be exactly /v1/executions/{{id}}/verify"
        );

        // Verify NO body is sent (verify takes no request body)
        assert!(
            req.body.is_empty() || req.body == "{}",
            "Verify should have no body, got: {}",
            req.body
        );

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_verify_execution_response_parsing() {
        // Verify verify_execution correctly parses response with verified flag and rollback_contract.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "verified": true,
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": "sha256:abc123"
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "Verified",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let result = client.verify_execution(&execution_id);
        assert!(
            result.is_ok(),
            "verify_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.verified);
        assert!(response.rollback_contract.is_some());

        let contract = response.rollback_contract.unwrap();
        assert!(matches!(
            contract.state,
            ferrum_proto::RollbackState::Verified
        ));

        _mock.assert();
    }

    #[test]
    fn test_verify_execution_wrong_path_fails() {
        // Verify verify_execution fails when the path doesn't match.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();

        let result = client.verify_execution(&execution_id);
        assert!(
            result.is_err(),
            "verify_execution should fail when path doesn't match"
        );
    }

    #[test]
    fn test_verify_execution_wrong_method_fails() {
        // Verify verify_execution fails when the HTTP method is wrong.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let path = format!("/v1/executions/{}/verify", execution_id);
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = client.verify_execution(&execution_id);
        assert!(
            result.is_err(),
            "verify_execution should fail when method doesn't match"
        );
    }

    #[test]
    fn test_verify_is_not_execute_or_compensate() {
        // Verify verify_execution does NOT call execute or compensate paths.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let verify_path = format!("/v1/executions/{}/verify", execution_id);

        // Mock execute path - should NOT be called
        let _mock_execute = server
            .mock(
                "POST",
                format!("/v1/executions/{}/execute", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock compensate path - should NOT be called
        let _mock_compensate = server
            .mock(
                "POST",
                format!("/v1/executions/{}/compensate", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Correct mock for verify
        let _mock_verify = server
            .mock("POST", verify_path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "verified": true,
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": "sha256:abc123"
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "Verified",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = client.verify_execution(&execution_id);
        assert!(
            result.is_ok(),
            "verify_execution should succeed: {:?}",
            result.err()
        );

        _mock_verify.assert();
    }

    // -------------------------------------------------------------------------
    // compensate_execution tests
    // -------------------------------------------------------------------------

    /// Starts a tiny_http server that captures the compensate request.
    fn start_compensate_capture_server() -> (
        std::sync::Arc<std::sync::Mutex<Option<CapturedD1_6Request>>>,
        String,
        std::thread::JoinHandle<()>,
    ) {
        let captured_request = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request_clone = captured_request.clone();

        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let server_url = server.server_addr().to_string();
        let base_url = format!("http://{}", server_url);

        let handle = std::thread::spawn(move || {
            let response = tiny_http::Response::from_string(
                r#"{
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "compensated": true,
                    "rollback_contract": {
                        "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                        "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                        "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                        "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                        "action_type": "FileWrite",
                        "rollback_class": "R0NativeReversible",
                        "adapter_key": "fs-adapter",
                        "target": {
                            "kind": "FilePath",
                            "path": "/tmp/output.txt",
                            "before_hash": null,
                            "after_hash": "sha256:abc123"
                        },
                        "prepare_checks": [],
                        "verify_checks": [],
                        "compensation_plan": [],
                        "auto_commit": false,
                        "state": "Compensated",
                        "created_at": "2026-05-07T00:00:00Z",
                        "expires_at": null,
                        "metadata": {}
                    },
                    "warnings": []
                }"#,
            )
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );

            if let Ok(Some(mut request)) = server.recv_timeout(std::time::Duration::from_secs(5)) {
                let method = request.method().to_string();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                *captured_request_clone.lock().unwrap() =
                    Some(CapturedD1_6Request { method, path, body });
                let _ = request.respond(response);
            }
        });

        (captured_request, base_url, handle)
    }

    #[test]
    fn test_compensate_execution_captures_request_details() {
        // Per doc 83: compensate_execution is compensate-only (D1.6).
        // This test verifies the HTTP method, path, and NO body are correct.

        let (captured_request, base_url, handle) = start_compensate_capture_server();

        let config = ClientConfig::new().base_url(&base_url);
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let result = client.compensate_execution(&execution_id);
        assert!(
            result.is_ok(),
            "compensate_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.compensated);

        // Verify the ACTUAL HTTP request captured by tiny_http
        let captured = captured_request.lock().unwrap();
        let req = captured
            .as_ref()
            .expect("Request should have been captured");

        // Verify HTTP method is POST
        assert_eq!(req.method, "POST", "HTTP method should be POST");
        // Verify HTTP path is EXACTLY /v1/executions/{execution_id}/compensate
        let expected_path = format!("/v1/executions/{}/compensate", execution_id);
        assert_eq!(
            req.path, expected_path,
            "HTTP path should be exactly /v1/executions/{{id}}/compensate"
        );

        // Verify NO body is sent (compensate takes no request body)
        assert!(
            req.body.is_empty() || req.body == "{}",
            "Compensate should have no body, got: {}",
            req.body
        );

        handle.join().expect("Server thread should join");
    }

    #[test]
    fn test_compensate_execution_response_parsing() {
        // Verify compensate_execution correctly parses response with compensated flag.

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "compensated": true,
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": "sha256:abc123"
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "Compensated",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();
        let result = client.compensate_execution(&execution_id);
        assert!(
            result.is_ok(),
            "compensate_execution should succeed: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.compensated);
        assert!(response.rollback_contract.is_some());

        let contract = response.rollback_contract.unwrap();
        assert!(matches!(
            contract.state,
            ferrum_proto::RollbackState::Compensated
        ));

        _mock.assert();
    }

    #[test]
    fn test_compensate_execution_wrong_path_fails() {
        // Verify compensate_execution fails when the path doesn't match.

        let mut server = mockito::Server::new();
        // Mock on WRONG path - should NOT be called
        let _mock_wrong_path = server
            .mock("POST", "/v1/wrong/path")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let execution_id = ferrum_proto::ExecutionId::new();

        let result = client.compensate_execution(&execution_id);
        assert!(
            result.is_err(),
            "compensate_execution should fail when path doesn't match"
        );
    }

    #[test]
    fn test_compensate_execution_wrong_method_fails() {
        // Verify compensate_execution fails when the HTTP method is wrong.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let path = format!("/v1/executions/{}/compensate", execution_id);
        // Mock on GET instead of POST - should NOT be called
        let _mock_wrong_method = server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = client.compensate_execution(&execution_id);
        assert!(
            result.is_err(),
            "compensate_execution should fail when method doesn't match"
        );
    }

    #[test]
    fn test_compensate_is_not_execute_or_verify() {
        // Verify compensate_execution does NOT call execute or verify paths.

        let mut server = mockito::Server::new();
        let execution_id = ferrum_proto::ExecutionId::new();
        let compensate_path = format!("/v1/executions/{}/compensate", execution_id);

        // Mock execute path - should NOT be called
        let _mock_execute = server
            .mock(
                "POST",
                format!("/v1/executions/{}/execute", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Mock verify path - should NOT be called
        let _mock_verify = server
            .mock(
                "POST",
                format!("/v1/executions/{}/verify", execution_id).as_str(),
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{}"#)
            .expect(0) // Expect 0 invocations
            .create();

        // Correct mock for compensate
        let _mock_compensate = server
            .mock("POST", compensate_path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                "compensated": true,
                "rollback_contract": {
                    "contract_id": "550e8400-e29b-41d4-a716-446655440100",
                    "intent_id": "550e8400-e29b-41d4-a716-446655440001",
                    "proposal_id": "550e8400-e29b-41d4-a716-446655440002",
                    "execution_id": "550e8400-e29b-41d4-a716-446655440099",
                    "action_type": "FileWrite",
                    "rollback_class": "R0NativeReversible",
                    "adapter_key": "fs-adapter",
                    "target": {
                        "kind": "FilePath",
                        "path": "/tmp/output.txt",
                        "before_hash": null,
                        "after_hash": "sha256:abc123"
                    },
                    "prepare_checks": [],
                    "verify_checks": [],
                    "compensation_plan": [],
                    "auto_commit": false,
                    "state": "Compensated",
                    "created_at": "2026-05-07T00:00:00Z",
                    "expires_at": null,
                    "metadata": {}
                },
                "warnings": []
            }"#,
            )
            .expect_at_least(1)
            .create();

        let config = ClientConfig::new().base_url(&server.url());
        let client = FerrumGatewayClient::new(&config).expect("client should create");

        let result = client.compensate_execution(&execution_id);
        assert!(
            result.is_ok(),
            "compensate_execution should succeed: {:?}",
            result.err()
        );

        _mock_compensate.assert();
    }
}
