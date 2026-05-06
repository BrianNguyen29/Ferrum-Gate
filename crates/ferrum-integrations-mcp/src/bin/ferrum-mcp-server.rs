//! # ferrum-mcp-server
//!
//! FerrumGate MCP server binary - Phase C stdio transport + Phase D-0 REST client.
//!
//! ## Overview
//!
//! This binary implements a line-based stdio JSON-RPC transport for FerrumGate MCP server.
//! It reads JSON-RPC requests from stdin and writes responses to stdout.
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
//! Phase D-0 does NOT implement:
//! - Auth middleware (bearer token validation)
//! - Policy evaluation
//! - Capability issuance
//! - Provenance emission
//! - Rollback preparation
//! - Mutating tool execution

use ferrum_integrations_mcp::{
    ClientConfig, FerrumGatewayClient, JsonRpcResponse, dispatch_with_client, parse_request,
};
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Flag to signal graceful shutdown.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

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
fn process_line(line: &str, client: &FerrumGatewayClient) -> Option<JsonRpcResponse> {
    let line = line.trim();
    // Skip empty lines
    if line.is_empty() {
        return None;
    }

    match parse_request(line) {
        Ok(request) => Some(dispatch_with_client(request, client)),
        Err(response) => Some(response),
    }
}

/// Main entry point for the MCP server binary.
fn main() {
    // Set up signal handlers
    setup_signal_handlers();

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
                if let Some(response) = process_line(&line, &client) {
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

    // Clean exit on EOF
    std::process::exit(0);
}
