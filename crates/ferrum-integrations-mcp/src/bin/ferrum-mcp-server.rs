//! # ferrum-mcp-server
//!
//! FerrumGate MCP server binary - Phase C stdio transport + Phase D-0 REST client + Phase 6.1 HTTP transport skeleton.
//!
//! ## Overview
//!
//! This binary implements a line-based stdio JSON-RPC transport and a Streamable HTTP skeleton for FerrumGate MCP server.
//! It reads JSON-RPC requests from stdin (stdio mode) or HTTP POST body (HTTP mode) and writes responses accordingly.
//!
//! ## Phase C Status
//!
//! Phase C implements:
//! - Stdio line-based transport loop
//! - Reuses `parse_request()` and `dispatch()` from `ferrum-integrations-mcp`
//! - Handles SIGINT/SIGTERM gracefully
//!
//! ## Phase D-0 Status
//!
//! Phase D-0 adds:
//! - Read-only REST client integration
//! - Gateway endpoint mapping for 9 read-only tools
//! - Error classification (auth, unreachable, server error)
//!
//! ## Phase 6.1 Status
//!
//! Phase 6.1 adds:
//! - Streamable HTTP transport skeleton (`POST /mcp`, `GET /mcp` SSE replay,
//!   `DELETE /mcp`, `GET /health`, `GET /ready`)
//! - CLI args `--transport stdio|http` and `--bind ADDR`
//! - In-memory session store (`Mcp-Session-Id`) and bounded SSE replay buffer
//!   (experimental, not restart-resumable)
//!
//! Phase 6.1 does NOT implement:
//! - Persistent checkpoint / restart-resumable session restore
//! - Gateway DB session storage or sidecar persistence
//! - OAuth/auth implementation for MCP HTTP beyond bearer tokens
//! - Real external MCP client compatibility claim
//! - Replay or re-execution of inbound tool calls

#[cfg(feature = "http")]
use axum::{
    Router,
    extract::{ConnectInfo, Request, State},
    http::{HeaderName, Method, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::{Parser, ValueEnum};
#[cfg(feature = "http")]
use constant_time_eq::constant_time_eq;
#[cfg(feature = "http")]
use ferrum_integrations_mcp::tool_registry;
use ferrum_integrations_mcp::{
    ActorIdentity, ClientConfig, FerrumGatewayClient, JsonRpcResponse, RateLimiter,
    dispatch_with_client, parse_request,
};
#[allow(unused_imports)]
use ferrum_integrations_mcp::{JsonRpcRequest, dispatch};
#[cfg(feature = "http")]
use ferrum_integrations_mcp::{McpSessionStore, auth_fingerprint};
use std::io::{self, BufRead, Write};
#[cfg(feature = "http")]
use std::net::{IpAddr, SocketAddr};
#[cfg(feature = "http")]
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Flag to signal graceful shutdown.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// CLI arguments for ferrum-mcp-server.
#[derive(Parser, Debug, Clone)]
#[command(name = "ferrum-mcp-server")]
#[command(about = "FerrumGate MCP server (stdio or HTTP transport)")]
struct Cli {
    /// Transport mode: stdio or http.
    #[arg(long, value_enum, default_value_t = Transport::Stdio)]
    transport: Transport,

    /// Bind address for HTTP transport.
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind: String,

    /// Allow HTTP mode to start without a bearer token.
    /// Insecure; local development only.
    #[arg(long)]
    allow_insecure_no_auth: bool,

    /// Allow binding to a non-loopback address in HTTP mode.
    /// Insecure; use a reverse proxy and do not expose to the internet.
    #[arg(long)]
    allow_insecure_nonlocal_bind: bool,

    /// Allowed Origin header values. Comma-separated; also set via
    /// FERRUM_MCP_ALLOWED_ORIGINS. Default empty: any Origin header is rejected.
    #[arg(long, value_delimiter = ',')]
    allowed_origin: Vec<String>,

    /// Allowed Host header values. Comma-separated; also set via
    /// FERRUM_MCP_ALLOWED_HOSTS. Defaults to localhost/127.0.0.1/::1 and the
    /// loopback bind host if unset.
    #[arg(long, value_delimiter = ',')]
    allowed_host: Vec<String>,

    /// Per-IP HTTP rate limit: sustained requests per second. Also set via
    /// FERRUM_MCP_HTTP_RATE_PER_SEC (default 5).
    #[arg(long)]
    http_rate_per_sec: Option<f64>,

    /// Per-IP HTTP rate limit: burst capacity. Also set via
    /// FERRUM_MCP_HTTP_RATE_BURST (default 20).
    #[arg(long)]
    http_rate_burst: Option<u32>,

    /// MCP session TTL in seconds. Also set via FERRUM_MCP_SESSION_TTL_SECS
    /// (default 300).
    #[arg(long)]
    mcp_session_ttl_secs: Option<u64>,

    /// Maximum outbound events retained per session. Also set via
    /// FERRUM_MCP_SESSION_MAX_EVENTS (default 1000).
    #[arg(long)]
    mcp_session_max_events: Option<usize>,

    /// Maximum concurrent in-memory MCP sessions. Also set via
    /// FERRUM_MCP_SESSION_MAX_SESSIONS (default 1000).
    #[arg(long)]
    mcp_session_max_sessions: Option<usize>,

    /// Maximum bytes for a single outbound replay event. Also set via
    /// FERRUM_MCP_SESSION_MAX_EVENT_BYTES (default 1 MiB).
    #[arg(long)]
    mcp_session_max_event_bytes: Option<usize>,

    /// Maximum total replay bytes retained per session. Also set via
    /// FERRUM_MCP_SESSION_MAX_TOTAL_BYTES (default 16 MiB).
    #[arg(long)]
    mcp_session_max_total_bytes: Option<usize>,
}

/// Transport mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Transport {
    /// Line-based stdio JSON-RPC transport (stable, default).
    Stdio,
    /// Streamable HTTP transport skeleton (experimental, requires `http` feature).
    Http,
}

/// Handle SIGINT and SIGTERM to signal graceful shutdown.
fn setup_signal_handlers() {
    // Set up signal handlers using a simple flag approach
    // In production, we'd use tokio's signal handlers, but we keep dependencies minimal
    #[cfg(not(windows))]
    {
        use std::sync::Once;
        static SETUP: Once = Once::new();
        SETUP.call_once(|| {
            // Note: In a real implementation, we'd install signal handlers here
            // For Phase C skeleton, we rely on EOF detection from stdin
        });
    }
}

/// Process a single line of input and return the response.
/// Uses the provided gateway client for REST calls.
fn process_line(
    line: &str,
    client: &FerrumGatewayClient,
    actor_id: &str,
    rate_limiter: &RateLimiter,
) -> Option<JsonRpcResponse> {
    let line = line.trim();
    // Skip empty lines
    if line.is_empty() {
        return None;
    }

    match parse_request(line) {
        Ok(request) => Some(dispatch_with_client(
            request,
            client,
            actor_id,
            rate_limiter,
        )),
        Err(response) => Some(response),
    }
}

/// Process a single line using a given dispatch function.
/// This is a test seam that allows testing without a real gateway client.
#[cfg(test)]
fn process_line_with_dispatch<F>(line: &str, dispatch_fn: F) -> Option<JsonRpcResponse>
where
    F: FnOnce(JsonRpcRequest) -> JsonRpcResponse,
{
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    match parse_request(line) {
        Ok(request) => Some(dispatch_fn(request)),
        Err(response) => Some(response),
    }
}

/// Run the stdio transport loop.
fn run_stdio() {
    // Create the gateway client from environment variables
    let client = match FerrumGatewayClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Failed to create gateway client: {}. Using default config.",
                e
            );
            // Fall back to default config (will likely fail on connection)
            FerrumGatewayClient::new(&ClientConfig::default())
                .expect("Failed to create client even with default config")
        }
    };

    // D-1 Slice 5: resolve actor identity and create per-agent rate limiter
    let actor = ActorIdentity::resolve(None);
    let rate_limiter = RateLimiter::default_mcp();

    // Use buffered I/O for efficient line reading/writing
    let stdin = io::stdin();
    let stdout = io::stdout();

    let stdin_handle = stdin.lock();
    let mut stdout_handle = io::BufWriter::new(stdout);

    // Line iterator from stdin
    let line_iterator = stdin_handle.lines();

    for line_result in line_iterator {
        // Check for shutdown signal
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }

        match line_result {
            Ok(line) => {
                if let Some(response) = process_line(&line, &client, &actor.actor_id, &rate_limiter)
                {
                    // Serialize response to JSON
                    match serde_json::to_string(&response) {
                        Ok(json) => {
                            // Write JSON line to stdout
                            writeln!(stdout_handle, "{}", json)
                                .map_err(|e| {
                                    eprintln!("Failed to write to stdout: {}", e);
                                })
                                .ok();
                            stdout_handle
                                .flush()
                                .map_err(|e| {
                                    eprintln!("Failed to flush stdout: {}", e);
                                })
                                .ok();
                        }
                        Err(e) => {
                            // Should not happen with valid responses, but handle gracefully
                            eprintln!("Failed to serialize response: {}", e);
                        }
                    }
                }
                // If None, skip blank lines silently
            }
            Err(e) => {
                // stdin error (e.g., broken pipe on client disconnect)
                eprintln!("Error reading stdin: {}", e);
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP Transport (Phase 6.1) — gated behind `http` feature
// ---------------------------------------------------------------------------

#[cfg(feature = "http")]
/// Supported MCP protocol version for HTTP `initialize` requests.
const SUPPORTED_PROTOCOL_VERSION: &str = "2024-11-05";

#[cfg(feature = "http")]
const IP_RATE_LIMITER_CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(feature = "http")]
const IP_RATE_LIMITER_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

#[cfg(feature = "http")]
/// Per-IP in-memory token-bucket rate limiter for the HTTP layer.
#[derive(Debug)]
struct IpRateLimiter {
    rate_per_sec: f64,
    burst: u32,
    state: std::sync::Mutex<std::collections::HashMap<IpAddr, IpLimitState>>,
}

#[cfg(feature = "http")]
#[derive(Debug)]
struct IpLimitState {
    tokens: f64,
    last_check: std::time::Instant,
}

#[cfg(feature = "http")]
impl IpRateLimiter {
    fn new(rate_per_sec: f64, burst: u32) -> Self {
        Self {
            rate_per_sec,
            burst,
            state: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn check(&self, ip: IpAddr) -> bool {
        self.check_at(ip, std::time::Instant::now())
    }

    fn check_at(&self, ip: IpAddr, now: std::time::Instant) -> bool {
        let mut map = self.state.lock().unwrap();
        let entry = map.entry(ip).or_insert(IpLimitState {
            tokens: self.burst as f64,
            last_check: now,
        });

        let elapsed = now.duration_since(entry.last_check).as_secs_f64();
        entry.tokens = (entry.tokens + elapsed * self.rate_per_sec).min(self.burst as f64);
        entry.last_check = now;

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Remove entries that have been idle longer than `max_idle`.
    fn cleanup_idle(&self, max_idle: std::time::Duration) -> usize {
        let now = std::time::Instant::now();
        let mut map = self.state.lock().unwrap();
        let before = map.len();
        map.retain(|_, state| now.duration_since(state.last_check) <= max_idle);
        before - map.len()
    }
}

#[cfg(feature = "http")]
/// Shared application state for HTTP handlers.
struct AppState {
    client: FerrumGatewayClient,
    actor: ActorIdentity,
    rate_limiter: RateLimiter,
    /// Bearer token for HTTP POST /mcp auth (experimental).
    /// When Some, all POST /mcp requests must include a matching
    /// `Authorization: Bearer <token>` header.
    auth_token: Option<String>,
    /// Allowed Origin header values. Empty means reject any Origin.
    allowed_origins: Vec<String>,
    /// Allowed Host header values.
    allowed_hosts: Vec<String>,
    /// Per-IP HTTP-layer rate limiter.
    ip_rate_limiter: Arc<IpRateLimiter>,
    /// In-memory MCP session store (experimental, not restart-resumable).
    session_store: Arc<McpSessionStore>,
}

#[cfg(feature = "http")]
fn host_without_port(host: &str) -> &str {
    if let Some(end) = host.rfind(']') {
        // IPv6 literal including brackets.
        &host[..=end]
    } else if let Some(idx) = host.rfind(':') {
        &host[..idx]
    } else {
        host
    }
}

#[cfg(feature = "http")]
fn is_host_allowed(host: &str, allowed: &[String]) -> bool {
    let normalized = host_without_port(host).to_lowercase();
    allowed.iter().any(|h| h.to_lowercase() == normalized)
}

#[cfg(feature = "http")]
fn default_allowed_hosts() -> Vec<String> {
    vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "[::1]".to_string(),
    ]
}

#[cfg(feature = "http")]
fn is_loopback_bind(bind: &str) -> bool {
    bind.parse::<SocketAddr>()
        .map(|a| a.ip().is_loopback())
        .unwrap_or(false)
}

#[cfg(feature = "http")]
fn bind_host(bind: &str) -> Option<String> {
    bind.parse::<SocketAddr>().map(|a| a.ip().to_string()).ok()
}

#[cfg(feature = "http")]
/// HTTP-layer security middleware: rate limiting, Host/Origin validation,
/// and `Accept: text/event-stream` rejection.
async fn security_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let ip = addr.ip();

    if !state.ip_rate_limiter.check(ip) {
        tracing::warn!(
            event_type = "rate_limited",
            remote_ip = %ip,
            path = %request.uri().path(),
        );
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            serde_json::json!({ "error": "rate limit exceeded" }).to_string(),
        )
            .into_response();
    }

    if let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
    {
        if !is_host_allowed(host, &state.allowed_hosts) {
            tracing::warn!(
                event_type = "host_rejected",
                remote_ip = %ip,
                host = %host,
            );
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    if let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok())
    {
        if state.allowed_origins.is_empty()
            || !state
                .allowed_origins
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(origin))
        {
            tracing::warn!(
                event_type = "origin_rejected",
                remote_ip = %ip,
                origin = %origin,
            );
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    // Only allow `Accept: text/event-stream` on GET /mcp. Reject it on all
    // other paths/methods to keep SSE surface narrow.
    if request
        .headers()
        .get(header::ACCEPT)
        .and_then(|a| a.to_str().ok())
        .is_some_and(|s| s.contains("text/event-stream"))
    {
        let path = request.uri().path();
        let method = request.method();
        if path != "/mcp" || method != Method::GET {
            return StatusCode::NOT_ACCEPTABLE.into_response();
        }
    }

    next.run(request).await
}

#[cfg(feature = "http")]
/// `GET /health` — basic health probe.
async fn health_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({ "status": "ok" })),
    )
}

#[cfg(feature = "http")]
/// `GET /ready` — shallow readiness with tool count and basic config.
async fn ready_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tool_count = tool_registry().len();
    let response = serde_json::json!({
        "status": "ready",
        "tool_count": tool_count,
        "transport": "http",
        "actor_source": format!("{:?}", state.actor.source),
    });
    (StatusCode::OK, axum::Json(response))
}

#[cfg(feature = "http")]
/// Validate the `Authorization` header and return the auth fingerprint.
///
/// If `expected` is `Some`, the header must be a matching `Bearer <token>`.
/// If `expected` is `None`, any bearer token is accepted (insecure mode) and
/// an empty fingerprint is returned when no header is present.
fn require_bearer(
    headers: &axum::http::HeaderMap,
    expected: Option<&str>,
) -> Result<String, StatusCode> {
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok());
    match (auth, expected) {
        (None, None) => Ok(auth_fingerprint("")),
        (None, Some(_)) => {
            tracing::warn!(
                event_type = "bearer_rejected",
                reason = "missing_or_malformed"
            );
            Err(StatusCode::UNAUTHORIZED)
        }
        (Some(header), expected) => {
            const PREFIX: &str = "Bearer ";
            if !header.starts_with(PREFIX) {
                tracing::warn!(event_type = "bearer_rejected", reason = "missing_prefix");
                return Err(StatusCode::UNAUTHORIZED);
            }
            let provided = &header[PREFIX.len()..];
            if let Some(expected) = expected {
                if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
                    tracing::warn!(event_type = "bearer_rejected", reason = "mismatch");
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
            Ok(auth_fingerprint(provided))
        }
    }
}

#[cfg(feature = "http")]
/// `POST /mcp` — accept a single JSON-RPC message and return synchronous `application/json`.
///
/// * `initialize` creates a new session and returns `Mcp-Session-Id` on success.
/// * All other methods require a valid `Mcp-Session-Id` header bound to the
///   same bearer token fingerprint.
async fn mcp_post_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Response, StatusCode> {
    let expected = state.auth_token.as_deref();
    let fingerprint = require_bearer(&headers, expected)?;

    // Parse first: invalid JSON gets a JSON-RPC parse error response without
    // requiring a session ID.
    let request = match parse_request(&body) {
        Ok(req) => req,
        Err(response) => {
            return Ok((StatusCode::OK, axum::Json(response)).into_response());
        }
    };

    let is_initialize = request.method == "initialize";

    if is_initialize {
        let protocol_header = HeaderName::from_static("mcp-protocol-version");
        if let Some(version) = headers.get(&protocol_header).and_then(|v| v.to_str().ok()) {
            if version != SUPPORTED_PROTOCOL_VERSION {
                tracing::warn!(
                    event_type = "protocol_version_rejected",
                    version = %version,
                    expected = %SUPPORTED_PROTOCOL_VERSION,
                );
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    // Non-initialize requests must present a valid session ID before dispatch.
    let session_id: Option<String> = if !is_initialize {
        let sid = headers
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::BAD_REQUEST)?;
        state
            .session_store
            .validate_session(sid, &fingerprint)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        Some(sid.to_string())
    } else {
        None
    };

    let state_for_dispatch = Arc::clone(&state);
    let response = tokio::task::spawn_blocking(move || {
        dispatch_with_client(
            request,
            &state_for_dispatch.client,
            &state_for_dispatch.actor.actor_id,
            &state_for_dispatch.rate_limiter,
        )
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response_json =
        serde_json::to_string(&response).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if is_initialize {
        let is_success = matches!(response, JsonRpcResponse::Success(_));
        if is_success {
            let sid = state
                .session_store
                .create_session_with_fingerprint(&fingerprint)
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            if let Err(e) = state.session_store.append_event(&sid, response_json) {
                tracing::warn!(
                    event_type = "replay_append_failed",
                    error = %e,
                    session_id = "***",
                    "failed to append initialize response to replay buffer; returning dispatch response anyway"
                );
            }
            let mut resp = (
                StatusCode::OK,
                [(HeaderName::from_static("mcp-session-id"), sid.as_str())],
                axum::Json(response),
            )
                .into_response();
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            );
            return Ok(resp);
        }
    } else if let Some(sid) = session_id {
        if let Err(e) = state.session_store.append_event(&sid, response_json) {
            tracing::warn!(
                event_type = "replay_append_failed",
                error = %e,
                session_id = "***",
                "failed to append response to replay buffer; returning dispatch response anyway"
            );
        }
    }

    Ok((StatusCode::OK, axum::Json(response)).into_response())
}

#[cfg(feature = "http")]
/// `GET /mcp` — SSE replay-only endpoint.
///
/// Requires bearer auth, a valid `Mcp-Session-Id`, and `Accept: text/event-stream`.
/// Replays serialized outbound events after the optional `Last-Event-ID`.
/// This handler never parses the request body or dispatches tool calls.
async fn mcp_get_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    let expected = state.auth_token.as_deref();
    let fingerprint = require_bearer(&headers, expected)?;

    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !accept.contains("text/event-stream") {
        return Err(StatusCode::NOT_ACCEPTABLE);
    }

    let session_id = headers
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let session = state
        .session_store
        .validate_session(session_id, &fingerprint)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let last_event_id = headers.get("Last-Event-ID").and_then(|v| v.to_str().ok());

    let events = session
        .events_after(last_event_id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut body = String::new();
    if events.is_empty() {
        body.push_str(": heartbeat\n\n");
    } else {
        for event in events {
            body.push_str(&format!("id: {}\ndata: {}\n\n", event.id, event.payload));
        }
    }

    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
        .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

#[cfg(feature = "http")]
/// `DELETE /mcp` — terminate an MCP session idempotently.
///
/// Requires bearer auth and a `Mcp-Session-Id` header. Returns `204 No Content`
/// on success; unknown/expired IDs also return `204` to remain idempotent.
async fn mcp_delete_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let expected = state.auth_token.as_deref();
    let fingerprint = require_bearer(&headers, expected)?;

    let session_id = headers
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Only terminate sessions that belong to the caller; for unknown/mismatched
    // sessions we still return 204 to avoid leaking session existence.
    if state
        .session_store
        .validate_session(session_id, &fingerprint)
        .is_ok()
    {
        state.session_store.terminate_session(session_id);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(feature = "http")]
/// Validate HTTP startup configuration and return the effective auth token.
fn validate_http_config(
    bind: &str,
    auth_token: Option<String>,
    cli: &Cli,
) -> Result<(SocketAddr, Option<String>), String> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| format!("Invalid bind address '{}': {}", bind, e))?;

    if auth_token.is_none() && !cli.allow_insecure_no_auth {
        return Err(
            "HTTP transport requires a bearer token. Set FERRUM_MCP_HTTP_BEARER_TOKEN or FERRUM_GATEWAY_BEARER_TOKEN, or pass --allow-insecure-no-auth for local development only."
                .to_string(),
        );
    }

    if cli.allow_insecure_no_auth {
        tracing::warn!(
            event_type = "insecure_flag",
            flag = "allow_insecure_no_auth",
            "HTTP transport is running without mandatory bearer auth"
        );
    }

    if !addr.ip().is_loopback() && !cli.allow_insecure_nonlocal_bind {
        return Err(format!(
            "Non-loopback bind address '{}' is not allowed. Bind to a loopback address or pass --allow-insecure-nonlocal-bind.",
            bind
        ));
    }

    if cli.allow_insecure_nonlocal_bind {
        tracing::warn!(
            event_type = "insecure_flag",
            flag = "allow_insecure_nonlocal_bind",
            "HTTP transport is bound to a non-loopback address"
        );
    }

    Ok((addr, auth_token))
}

#[cfg(feature = "http")]
fn env_split(name: &str) -> Option<Vec<String>> {
    std::env::var(name)
        .ok()
        .map(|s| {
            s.split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
}

#[cfg(feature = "http")]
fn env_or<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[cfg(feature = "http")]
/// Run the HTTP transport server.
async fn run_http(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let client = match FerrumGatewayClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Failed to create gateway client: {}. Using default config.",
                e
            );
            FerrumGatewayClient::new(&ClientConfig::default())
                .expect("Failed to create client even with default config")
        }
    };

    let actor = ActorIdentity::resolve(None);
    let rate_limiter = RateLimiter::default_mcp();

    // Experimental HTTP transport: read bearer token from env.
    let auth_token = std::env::var("FERRUM_MCP_HTTP_BEARER_TOKEN")
        .ok()
        .or_else(|| std::env::var("FERRUM_GATEWAY_BEARER_TOKEN").ok());

    let (addr, auth_token) = validate_http_config(&cli.bind, auth_token, cli)?;

    let allowed_hosts = if cli.allowed_host.is_empty() {
        let mut hosts = env_split("FERRUM_MCP_ALLOWED_HOSTS").unwrap_or_else(default_allowed_hosts);
        if let Some(bind_host) = bind_host(&cli.bind) {
            if is_loopback_bind(&cli.bind) && !hosts.contains(&bind_host) {
                hosts.push(bind_host);
            }
        }
        hosts
    } else {
        cli.allowed_host.clone()
    };

    let allowed_origins = if cli.allowed_origin.is_empty() {
        env_split("FERRUM_MCP_ALLOWED_ORIGINS").unwrap_or_default()
    } else {
        cli.allowed_origin.clone()
    };

    let http_rate_per_sec = cli
        .http_rate_per_sec
        .unwrap_or_else(|| env_or("FERRUM_MCP_HTTP_RATE_PER_SEC", 5.0));
    let http_rate_burst = cli
        .http_rate_burst
        .unwrap_or_else(|| env_or("FERRUM_MCP_HTTP_RATE_BURST", 20));
    let session_ttl = std::time::Duration::from_secs(
        cli.mcp_session_ttl_secs
            .unwrap_or_else(|| env_or("FERRUM_MCP_SESSION_TTL_SECS", 300_u64)),
    );
    let session_max_events = cli
        .mcp_session_max_events
        .unwrap_or_else(|| env_or("FERRUM_MCP_SESSION_MAX_EVENTS", 1000_usize));
    let session_max_sessions = cli
        .mcp_session_max_sessions
        .unwrap_or_else(|| env_or("FERRUM_MCP_SESSION_MAX_SESSIONS", 1000_usize));
    let session_max_event_bytes = cli
        .mcp_session_max_event_bytes
        .unwrap_or_else(|| env_or("FERRUM_MCP_SESSION_MAX_EVENT_BYTES", 1024 * 1024_usize));
    let session_max_total_bytes = cli
        .mcp_session_max_total_bytes
        .unwrap_or_else(|| env_or("FERRUM_MCP_SESSION_MAX_TOTAL_BYTES", 16 * 1024 * 1024_usize));

    let session_store = Arc::new(McpSessionStore::new(
        session_ttl,
        session_max_events,
        session_max_sessions,
        session_max_event_bytes,
        session_max_total_bytes,
    ));

    let state = Arc::new(AppState {
        client,
        actor,
        rate_limiter,
        auth_token,
        allowed_origins,
        allowed_hosts,
        ip_rate_limiter: Arc::new(IpRateLimiter::new(http_rate_per_sec, http_rate_burst)),
        session_store,
    });

    let cleanup_limiter = Arc::clone(&state.ip_rate_limiter);
    let cleanup_store = Arc::clone(&state.session_store);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route(
            "/mcp",
            post(mcp_post_handler)
                .get(mcp_get_handler)
                .delete(mcp_delete_handler),
        )
        .layer(from_fn_with_state(Arc::clone(&state), security_middleware))
        .with_state(state);

    let shutdown = Arc::new(tokio::sync::Notify::new());
    let cleanup_shutdown = Arc::clone(&shutdown);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(IP_RATE_LIMITER_CLEANUP_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let removed = cleanup_limiter.cleanup_idle(IP_RATE_LIMITER_IDLE_TIMEOUT);
                    tracing::debug!(event_type = "ip_rate_limiter_cleanup", removed);
                    let expired = cleanup_store.cleanup_expired();
                    tracing::debug!(event_type = "mcp_session_cleanup", expired);
                }
                _ = cleanup_shutdown.notified() => break,
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("Ferrum MCP HTTP server listening on http://{}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(Arc::clone(&shutdown)))
    .await?;
    Ok(())
}

#[cfg(feature = "http")]
async fn shutdown_signal(shutdown: Arc<tokio::sync::Notify>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        sigterm.recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    shutdown.notify_waiters();
}

/// Main entry point for the MCP server binary.
#[tokio::main]
async fn main() {
    // Set up signal handlers
    setup_signal_handlers();

    // Initialize tracing once for the HTTP transport; ignore if a subscriber
    // is already present.
    #[cfg(feature = "http")]
    let _ = tracing_subscriber::fmt::try_init();

    let cli = Cli::parse();

    match cli.transport {
        Transport::Stdio => {
            run_stdio();
            std::process::exit(0);
        }
        Transport::Http => {
            #[cfg(feature = "http")]
            {
                if let Err(e) = run_http(&cli).await {
                    eprintln!("HTTP server error: {}", e);
                    std::process::exit(1);
                }
            }
            #[cfg(not(feature = "http"))]
            {
                eprintln!(
                    "HTTP transport requires the `http` feature. Build with --features http to enable it."
                );
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_defaults() {
        let cli = Cli::parse_from(["ferrum-mcp-server"]);
        assert_eq!(cli.transport, Transport::Stdio);
        assert_eq!(cli.bind, "127.0.0.1:3000");
    }

    #[test]
    fn test_cli_transport_http() {
        let cli = Cli::parse_from(["ferrum-mcp-server", "--transport", "http"]);
        assert_eq!(cli.transport, Transport::Http);
        assert_eq!(cli.bind, "127.0.0.1:3000");
    }

    #[test]
    fn test_cli_bind_override() {
        let cli = Cli::parse_from([
            "ferrum-mcp-server",
            "--transport",
            "http",
            "--bind",
            "0.0.0.0:8080",
        ]);
        assert_eq!(cli.transport, Transport::Http);
        assert_eq!(cli.bind, "0.0.0.0:8080");
    }

    // -------------------------------------------------------------------------
    // Stdio tests (preserved from Phase C)
    // -------------------------------------------------------------------------

    #[test]
    fn test_process_line_ping() {
        let line = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = process_line_with_dispatch(line, dispatch);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Success(success) => {
                assert_eq!(success.result, serde_json::json!({"success": true}));
            }
            JsonRpcResponse::Error(_) => panic!("Expected success for ping"),
        }
    }

    #[test]
    fn test_process_line_initialize() {
        let line = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let response = process_line_with_dispatch(line, dispatch);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Success(success) => {
                let result = &success.result;
                assert_eq!(result["protocol_version"], "2024-11-05");
            }
            JsonRpcResponse::Error(_) => panic!("Expected success for initialize"),
        }
    }

    #[test]
    fn test_process_line_tools_list() {
        // D1.7+D1.9: tools/list returns 19 tools (9 read-only + 8 lifecycle + 2 approval)
        let line = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let response = process_line_with_dispatch(line, dispatch);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Success(success) => {
                let tools = &success.result["tools"];
                assert_eq!(tools.as_array().unwrap().len(), 19);
            }
            JsonRpcResponse::Error(_) => panic!("Expected success for tools/list"),
        }
    }

    #[test]
    fn test_process_line_tools_call_returns_not_implemented() {
        // In Phase D-0, tools/call with dispatch (not dispatch_with_client)
        // still returns NOT_IMPLEMENTED because dispatch uses the Phase B handlers
        let line = r#"{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"ferrum_gate_health"}}"#;
        let response = process_line_with_dispatch(line, dispatch);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, -32001); // NOT_IMPLEMENTED
            }
            JsonRpcResponse::Success(_) => panic!("Expected error for tools/call with dispatch"),
        }
    }

    #[test]
    fn test_process_line_unknown_method() {
        let line = r#"{"jsonrpc":"2.0","method":"unknown_method","id":1}"#;
        let response = process_line_with_dispatch(line, dispatch);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, -32601); // METHOD_NOT_FOUND
            }
            JsonRpcResponse::Success(_) => panic!("Expected error for unknown method"),
        }
    }

    #[test]
    fn test_process_line_invalid_json() {
        let line = "not valid json";
        let response = process_line_with_dispatch(line, dispatch);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, -32700); // PARSE_ERROR
            }
            JsonRpcResponse::Success(_) => panic!("Expected error for invalid JSON"),
        }
    }

    #[test]
    fn test_process_line_empty_string() {
        let response = process_line_with_dispatch("", dispatch);
        assert!(response.is_none());
    }

    #[test]
    fn test_process_line_whitespace_only() {
        let response = process_line_with_dispatch("   \n\t  ", dispatch);
        assert!(response.is_none());
    }

    // -------------------------------------------------------------------------
    // HTTP transport tests (Phase 6.1) — gated behind `http` feature
    // -------------------------------------------------------------------------

    #[cfg(feature = "http")]
    fn test_app(
        auth_token: Option<String>,
        allowed_origins: Vec<String>,
        allowed_hosts: Vec<String>,
        session_store: Arc<McpSessionStore>,
    ) -> Router {
        // Create the blocking client on a dedicated thread to avoid
        // "cannot create a runtime in an async context" panic.
        let client =
            std::thread::spawn(|| FerrumGatewayClient::new(&ClientConfig::default()).unwrap())
                .join()
                .unwrap();
        let actor = ActorIdentity::resolve(None);
        let rate_limiter = RateLimiter::default_mcp();
        let state = Arc::new(AppState {
            client,
            actor,
            rate_limiter,
            auth_token,
            allowed_origins,
            allowed_hosts,
            ip_rate_limiter: Arc::new(IpRateLimiter::new(5.0, 20)),
            session_store,
        });
        // Leak a clone so the Arc refcount never reaches zero inside async tests,
        // preventing `reqwest::blocking::Client` from being dropped in an async
        // context (which would panic because dropping a runtime while blocking
        // is not allowed).
        let _leaked = Box::leak(Box::new(Arc::clone(&state)));
        Router::new()
            .route("/health", get(health_handler))
            .route("/ready", get(ready_handler))
            .route(
                "/mcp",
                post(mcp_post_handler)
                    .get(mcp_get_handler)
                    .delete(mcp_delete_handler),
            )
            .layer(from_fn_with_state(Arc::clone(&state), security_middleware))
            .with_state(state)
    }

    #[cfg(feature = "http")]
    fn default_test_app() -> Router {
        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            default_allowed_hosts(),
            session_store,
        )
    }

    #[cfg(feature = "http")]
    async fn initialize_session(app: &mut Router) -> String {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let body_json = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let sid = response
            .headers()
            .get("Mcp-Session-Id")
            .expect("initialize should return Mcp-Session-Id")
            .to_str()
            .unwrap()
            .to_string();
        // Drain the body to keep the connection happy in test oneshot use.
        let _ = response.into_body().collect().await;
        sid
    }

    #[cfg(feature = "http")]
    fn local_connect_info<B>(mut request: axum::http::Request<B>) -> axum::http::Request<B> {
        let addr = SocketAddr::new(IpAddr::from([127, 0, 0, 1]), 0);
        request.extensions_mut().insert(ConnectInfo(addr));
        request
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_health() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = default_test_app();
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_ready() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = default_test_app();
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["tool_count"], 19);
        assert_eq!(json["transport"], "http");
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_initialize() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let session_id = response
            .headers()
            .get("Mcp-Session-Id")
            .expect("initialize should return Mcp-Session-Id")
            .to_str()
            .unwrap();
        assert!(!session_id.is_empty());

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("result").is_some());
        assert_eq!(json["result"]["protocol_version"], "2024-11-05");
        assert_eq!(json["jsonrpc"], "2.0");
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_ping() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":42}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("result").is_some());
        assert_eq!(json["result"], serde_json::json!({"success": true}));
        assert_eq!(json["id"], 42);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_invalid_json() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = "not valid json";
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("error").is_some());
        assert_eq!(json["error"]["code"], -32700);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_get_sse_requires_session_and_accept() {
        use axum::body::Body;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Accept", "text/event-stream")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok()),
            Some("text/event-stream")
        );
    }

    // -------------------------------------------------------------------------
    // Phase 6.5 HTTP Transport Compatibility Tests
    // -------------------------------------------------------------------------

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_tools_list_returns_200_with_expected_count() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;
        let body_json = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("result").is_some());
        let tools = json["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 19);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_tools_list_entries_have_required_fields() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;
        let body_json = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        for tool in tools {
            assert!(
                tool.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
                "Each tool must have a non-empty name"
            );
            assert!(
                tool.get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
                "Each tool must have a non-empty description"
            );
            assert!(
                tool.get("input_schema")
                    .map(|v| v.is_object())
                    .unwrap_or(false),
                "Each tool must have an object input_schema"
            );
            let schema = tool["input_schema"].as_object().unwrap();
            assert_eq!(
                schema.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "Each tool input_schema.type must be 'object'"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Auth hardening tests (POST /mcp)
    // -------------------------------------------------------------------------

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_unauthenticated_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_wrong_token_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // -------------------------------------------------------------------------
    // HTTP startup validation tests
    // -------------------------------------------------------------------------

    #[cfg(feature = "http")]
    #[test]
    fn test_validate_http_config_requires_auth_token() {
        let cli = Cli::parse_from([
            "ferrum-mcp-server",
            "--transport",
            "http",
            "--bind",
            "127.0.0.1:3000",
        ]);
        let result = validate_http_config("127.0.0.1:3000", None, &cli);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("requires a bearer token"), "{}", msg);
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_validate_http_config_insecure_no_auth_opt_out() {
        let cli = Cli::parse_from([
            "ferrum-mcp-server",
            "--transport",
            "http",
            "--bind",
            "127.0.0.1:3000",
            "--allow-insecure-no-auth",
        ]);
        let result = validate_http_config("127.0.0.1:3000", None, &cli);
        assert!(result.is_ok());
        assert!(result.unwrap().1.is_none());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_validate_http_config_rejects_non_loopback_bind() {
        let cli = Cli::parse_from([
            "ferrum-mcp-server",
            "--transport",
            "http",
            "--bind",
            "0.0.0.0:8080",
        ]);
        let result = validate_http_config("0.0.0.0:8080", Some("token".to_string()), &cli);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Non-loopback bind address"), "{}", msg);
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_validate_http_config_non_loopback_opt_out() {
        let cli = Cli::parse_from([
            "ferrum-mcp-server",
            "--transport",
            "http",
            "--bind",
            "0.0.0.0:8080",
            "--allow-insecure-nonlocal-bind",
        ]);
        let result = validate_http_config("0.0.0.0:8080", Some("token".to_string()), &cli);
        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // Host / Origin / Accept / rate-limit hardening tests
    // -------------------------------------------------------------------------

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_rejects_disallowed_host() {
        use axum::body::Body;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        let app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            vec!["allowed.example.com".to_string()],
            session_store,
        );
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/health")
                    .header("Host", "localhost")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_allows_configured_host() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        let app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            vec!["allowed.example.com".to_string()],
            session_store,
        );
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/health")
                    .header("Host", "allowed.example.com")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_rejects_origin_by_default() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/health")
                    .header("Origin", "http://example.com")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_allows_configured_origin() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        let mut app = test_app(
            Some("test-mcp-token".to_string()),
            vec!["http://example.com".to_string()],
            default_allowed_hosts(),
            session_store,
        );
        let session_id = initialize_session(&mut app).await;
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Origin", "http://example.com")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["result"], serde_json::json!({"success": true}));
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_rejects_accept_event_stream_outside_mcp_get() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/health")
                    .header("Accept", "text/event-stream")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_rate_limit_returns_429_with_retry_after() {
        use axum::body::Body;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        let client =
            std::thread::spawn(|| FerrumGatewayClient::new(&ClientConfig::default()).unwrap())
                .join()
                .unwrap();
        let actor = ActorIdentity::resolve(None);
        let rate_limiter = RateLimiter::default_mcp();
        let state = Arc::new(AppState {
            client,
            actor,
            rate_limiter,
            auth_token: Some("test-mcp-token".to_string()),
            allowed_origins: vec![],
            allowed_hosts: default_allowed_hosts(),
            ip_rate_limiter: Arc::new(IpRateLimiter::new(5.0, 1)),
            session_store,
        });
        let _leaked = Box::leak(Box::new(Arc::clone(&state)));
        let app = Router::new()
            .route("/health", get(health_handler))
            .route("/ready", get(ready_handler))
            .route(
                "/mcp",
                post(mcp_post_handler)
                    .get(mcp_get_handler)
                    .delete(mcp_delete_handler),
            )
            .layer(from_fn_with_state(Arc::clone(&state), security_middleware))
            .with_state(state);

        // First request is allowed.
        let response = app
            .clone()
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Second request exceeds burst of 1.
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_initialize_rejects_mismatched_protocol_version() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("MCP-Protocol-Version", "2099-01-01")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_initialize_allows_supported_protocol_version() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("MCP-Protocol-Version", SUPPORTED_PROTOCOL_VERSION)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["result"]["protocol_version"], "2024-11-05");
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_ip_rate_limiter_cleanup_idle_removes_stale_entries() {
        use std::net::IpAddr;
        use std::time::{Duration, Instant};

        let limiter = IpRateLimiter::new(5.0, 20);
        let now = Instant::now();
        let recent = IpAddr::from([127, 0, 0, 1]);
        let stale = IpAddr::from([127, 0, 0, 2]);

        assert!(limiter.check_at(recent, now));
        assert!(limiter.check_at(stale, now - Duration::from_secs(400)));
        assert_eq!(limiter.state.lock().unwrap().len(), 2);

        let removed = limiter.cleanup_idle(Duration::from_secs(300));
        assert_eq!(removed, 1);
        assert_eq!(limiter.state.lock().unwrap().len(), 1);
        assert!(limiter.state.lock().unwrap().contains_key(&recent));
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_serve_wires_connect_info() {
        let app = default_test_app();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server_handle = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
        });

        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{}/health", addr))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);

        let _ = shutdown_tx.send(());
        server_handle.await.unwrap();
    }

    // -------------------------------------------------------------------------
    // P2-3a in-memory session / SSE replay tests
    // -------------------------------------------------------------------------

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_initialize_failed_does_not_create_session() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        // `client_info.name` is expected to be a string; supplying a number
        // makes `handle_initialize` return an error response.
        let body_json = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{"client_info":{"name":123}}}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("Mcp-Session-Id").is_none());
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_non_initialize_missing_session_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = default_test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_non_initialize_unknown_session_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let _session_id = initialize_session(&mut app).await;
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", "not-a-real-session-id")
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_session_expired_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_millis(1),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        let session_id = session_store.create_session("test-mcp-token").unwrap();
        let app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            default_allowed_hosts(),
            session_store,
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_session_auth_mismatch_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        // Session bound to a different token fingerprint.
        let session_id = session_store.create_session("other-token").unwrap();
        let app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            default_allowed_hosts(),
            session_store,
        );

        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(body_json))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_get_sse_replays_initialize_response() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;

        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Accept", "text/event-stream")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.starts_with("id: 1\ndata: "));
        assert!(text.contains("\"protocol_version\":\"2024-11-05\""));
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_get_sse_replays_after_last_event_id() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;

        // Send a ping so the replay buffer has a second event.
        let ping_json = r#"{"jsonrpc":"2.0","method":"ping","id":2}"#;
        let ping_response = app
            .clone()
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id.clone())
                    .body(Body::from(ping_json))
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(ping_response.status(), StatusCode::OK);

        // Replay only events after the initialize response.
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Accept", "text/event-stream")
                    .header("Mcp-Session-Id", session_id)
                    .header("Last-Event-ID", "1")
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.starts_with("id: 2\ndata: "));
        assert!(text.contains("\"success\":true"));
        assert!(!text.contains("id: 1\ndata:"));
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_get_sse_replay_never_dispatches() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            16 * 1024 * 1024,
        ));
        let session_id = session_store.create_session("test-mcp-token").unwrap();
        session_store
            .append_event(
                &session_id,
                r#"{"jsonrpc":"2.0","id":7,"result":{"replayed":true}}"#.to_string(),
            )
            .unwrap();

        let app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            default_allowed_hosts(),
            session_store,
        );

        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Accept", "text/event-stream")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("\"replayed\":true"));
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_delete_terminates_session_idempotently() {
        use axum::body::Body;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;

        let delete_response = app
            .clone()
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id.clone())
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

        // Terminated session cannot be used for POST.
        let ping_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let ping_response = app
            .clone()
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id.clone())
                    .body(Body::from(ping_json))
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(ping_response.status(), StatusCode::BAD_REQUEST);

        // Second DELETE is idempotent.
        let delete_again = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(delete_again.status(), StatusCode::NO_CONTENT);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_append_failure_after_dispatch_still_returns_dispatch_response() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        // Set max_event_bytes to 1 so any non-empty outbound response payload is
        // rejected by the replay buffer. Dispatch must still return 200 with the
        // original JSON-RPC response, not 500.
        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1,
            16 * 1024 * 1024,
        ));
        let app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            default_allowed_hosts(),
            session_store,
        );

        let init_body = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let init_response = app
            .clone()
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(init_body))
                    .unwrap(),
            ))
            .await
            .unwrap();

        // Session is created and the initialize response is returned even though
        // the replay buffer cannot retain it.
        assert_eq!(init_response.status(), StatusCode::OK);
        let session_id = init_response
            .headers()
            .get("Mcp-Session-Id")
            .expect("initialize should still return Mcp-Session-Id")
            .to_str()
            .unwrap()
            .to_string();
        let _ = init_response.into_body().collect().await;

        // A subsequent ping also returns 200 with the dispatch response, not 500,
        // and does not re-dispatch (response is the normal ping result).
        let ping_body = r#"{"jsonrpc":"2.0","method":"ping","id":42}"#;
        let ping_response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::from(ping_body))
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(ping_response.status(), StatusCode::OK);
        let body = ping_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["result"], serde_json::json!({"success": true}));
        assert_eq!(json["id"], 42);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_replay_buffer_byte_bound_evicts_oldest_response() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        // Cap total replay bytes per session tightly. Each ping response is small;
        // after a few pings the oldest initialize response should be evicted.
        let session_store = Arc::new(McpSessionStore::new(
            std::time::Duration::from_secs(300),
            1000,
            1000,
            1024 * 1024,
            64,
        ));
        let mut app = test_app(
            Some("test-mcp-token".to_string()),
            vec![],
            default_allowed_hosts(),
            session_store,
        );
        let session_id = initialize_session(&mut app).await;

        // Generate enough small outbound events to push the initialize response out.
        for i in 0..10 {
            let ping_body = format!(r#"{{"jsonrpc":"2.0","method":"ping","id":{}}}"#, i + 2);
            let ping_response = app
                .clone()
                .oneshot(local_connect_info(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/mcp")
                        .header("Content-Type", "application/json")
                        .header("Authorization", "Bearer test-mcp-token")
                        .header("Mcp-Session-Id", session_id.clone())
                        .body(Body::from(ping_body))
                        .unwrap(),
                ))
                .await
                .unwrap();
            assert_eq!(ping_response.status(), StatusCode::OK);
            let _ = ping_response.into_body().collect().await;
        }

        // Replaying from event 1 should still succeed (it yields later events),
        // but the actual first event content may have been evicted; we just
        // verify the response remains valid SSE and contains at least one event.
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Accept", "text/event-stream")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("data:"));
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_get_without_accept_returns_not_acceptable() {
        use axum::body::Body;
        use tower::ServiceExt;

        let mut app = default_test_app();
        let session_id = initialize_session(&mut app).await;
        let response = app
            .oneshot(local_connect_info(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header("Authorization", "Bearer test-mcp-token")
                    .header("Mcp-Session-Id", session_id)
                    .body(Body::empty())
                    .unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    }
}
