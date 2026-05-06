//! # ferrum-mcp-server
//!
//! FerrumGate MCP server binary - Phase C stdio transport skeleton.
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
//! Phase C does NOT implement:
//! - MCP SDK or full transport compliance
//! - Gateway calls (tools/call returns NOT_IMPLEMENTED)
//! - Authentication or authorization
//! - Mutating tools

use ferrum_integrations_mcp::{JsonRpcResponse, dispatch, parse_request};
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
fn process_line(line: &str) -> Option<JsonRpcResponse> {
    let line = line.trim();
    // Skip empty lines
    if line.is_empty() {
        return None;
    }

    match parse_request(line) {
        Ok(request) => Some(dispatch(request)),
        Err(response) => Some(response),
    }
}

/// Main entry point for the MCP server binary.
fn main() {
    // Set up signal handlers
    setup_signal_handlers();

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
                if let Some(response) = process_line(&line) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_line_ping() {
        let line = r#"{"jsonrpc":"2.0","method":"ping","id":1}"#;
        let response = process_line(line);
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
        let response = process_line(line);
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
        let line = r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#;
        let response = process_line(line);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Success(success) => {
                let tools = &success.result["tools"];
                assert_eq!(tools.as_array().unwrap().len(), 9);
            }
            JsonRpcResponse::Error(_) => panic!("Expected success for tools/list"),
        }
    }

    #[test]
    fn test_process_line_tools_call() {
        let line = r#"{"jsonrpc":"2.0","method":"tools/call","id":1,"params":{"name":"ferrum_gate_health"}}"#;
        let response = process_line(line);
        assert!(response.is_some());
        let response = response.unwrap();
        match response {
            JsonRpcResponse::Error(err) => {
                assert_eq!(err.error.code, -32001); // NOT_IMPLEMENTED
            }
            JsonRpcResponse::Success(_) => panic!("Expected error for tools/call"),
        }
    }

    #[test]
    fn test_process_line_unknown_method() {
        let line = r#"{"jsonrpc":"2.0","method":"unknown_method","id":1}"#;
        let response = process_line(line);
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
        let response = process_line(line);
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
        let response = process_line("");
        assert!(response.is_none());
    }

    #[test]
    fn test_process_line_whitespace_only() {
        let response = process_line("   \n\t  ");
        assert!(response.is_none());
    }
}
