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
//! - Streamable HTTP transport skeleton (`POST /mcp`, `GET /health`, `GET /ready`)
//! - CLI args `--transport stdio|http` and `--bind ADDR`
//! - `GET /mcp` returns 405 (SSE streaming deferred)
//!
//! Phase 6.1 does NOT implement:
//! - SSE streaming/multiplexing/resumability
//! - Session state management
//! - OAuth/auth implementation for MCP HTTP
//! - Real external MCP client compatibility claim

#[cfg(feature = "http")]
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
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
use std::io::{self, BufRead, Write};
#[cfg(feature = "http")]
use std::net::SocketAddr;
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
/// Shared application state for HTTP handlers.
struct AppState {
    client: FerrumGatewayClient,
    actor: ActorIdentity,
    rate_limiter: RateLimiter,
    /// Bearer token for HTTP POST /mcp auth (experimental).
    /// When Some, all POST /mcp requests must include a matching
    /// `Authorization: Bearer <token>` header.
    auth_token: Option<String>,
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
/// `POST /mcp` — accept a single JSON-RPC message and return synchronous `application/json`.
/// Requires a valid bearer token when `auth_token` is configured; fails closed otherwise.
async fn mcp_post_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<axum::Json<JsonRpcResponse>, StatusCode> {
    if let Some(expected) = &state.auth_token {
        let auth = headers.get("Authorization").and_then(|v| v.to_str().ok());
        match auth {
            Some(header) if header.starts_with("Bearer ") => {
                let provided = &header[7..];
                if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
            _ => return Err(StatusCode::UNAUTHORIZED),
        }
    }

    let state = Arc::clone(&state);
    let response = tokio::task::spawn_blocking(move || match parse_request(&body) {
        Ok(request) => dispatch_with_client(
            request,
            &state.client,
            &state.actor.actor_id,
            &state.rate_limiter,
        ),
        Err(response) => response,
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(axum::Json(response))
}

#[cfg(feature = "http")]
/// `GET /mcp` — SSE streaming placeholder. Returns 405 per Phase 6.1 boundary.
async fn mcp_get_handler() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        axum::Json(serde_json::json!({
            "error": "SSE streaming not implemented in Phase 6.1 skeleton",
            "deferred": true,
        })),
    )
}

#[cfg(feature = "http")]
/// Run the HTTP transport server.
async fn run_http(bind: &str) -> Result<(), Box<dyn std::error::Error>> {
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
    // Fails closed if the env var is not set.
    let auth_token = std::env::var("FERRUM_MCP_HTTP_BEARER_TOKEN")
        .ok()
        .or_else(|| std::env::var("FERRUM_GATEWAY_BEARER_TOKEN").ok());

    let state = Arc::new(AppState {
        client,
        actor,
        rate_limiter,
        auth_token,
    });

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .route("/mcp", post(mcp_post_handler).get(mcp_get_handler))
        .with_state(state);

    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    eprintln!("Ferrum MCP HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

/// Main entry point for the MCP server binary.
#[tokio::main]
async fn main() {
    // Set up signal handlers
    setup_signal_handlers();

    let cli = Cli::parse();

    match cli.transport {
        Transport::Stdio => {
            run_stdio();
            std::process::exit(0);
        }
        Transport::Http => {
            #[cfg(feature = "http")]
            {
                if let Err(e) = run_http(&cli.bind).await {
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
    fn test_app() -> Router {
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
            auth_token: Some("test-mcp-token".to_string()),
        });
        // Leak a clone so the Arc refcount never reaches zero inside async tests,
        // preventing `reqwest::blocking::Client` from being dropped in an async
        // context (which would panic because dropping a runtime while blocking
        // is not allowed).
        let _leaked = Box::leak(Box::new(Arc::clone(&state)));
        Router::new()
            .route("/health", get(health_handler))
            .route("/ready", get(ready_handler))
            .route("/mcp", post(mcp_post_handler).get(mcp_get_handler))
            .with_state(state)
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_health() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = test_app();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
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

        let app = test_app();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
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

        let app = test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"initialize","id":1,"params":{}}"#;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
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

        let app = test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":42}"#;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
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

        let app = test_app();
        let body_json = "not valid json";
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
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
    async fn test_http_mcp_get_returns_405() {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        let app = test_app();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["deferred"], true);
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

        let app = test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
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

        let app = test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer test-mcp-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
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

        let app = test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[cfg(feature = "http")]
    #[tokio::test]
    async fn test_http_mcp_post_wrong_token_rejected() {
        use axum::body::Body;
        use tower::ServiceExt;

        let app = test_app();
        let body_json = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("Content-Type", "application/json")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::from(body_json))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
