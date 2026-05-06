//! # ferrum-integrations-mcp
//!
//! FerrumGate MCP server integration crate (Phase A skeleton).
//!
//! ## Overview
//!
//! This crate provides:
//! - Read-only MCP tool schema definitions for FerrumGate
//! - Tool registry with metadata (name, description, input_schema, read_only marker)
//! - No mutating tools in Phase A
//!
//! ## Phase A Status
//!
//! Phase A is a skeleton only. It implements:
//! - Read-only tool schema draft (9 tools)
//! - Tool registry proving no mutating tools are present
//!
//! Phase A does NOT implement:
//! - MCP SDK or transport
//! - JSON-RPC handlers
//! - Authentication
//! - Governance pipeline integration

use serde::{Deserialize, Serialize};

/// A tool's JSON Schema definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSchema {
    /// JSON Schema for input parameters.
    pub input_schema: serde_json::Value,
    /// JSON Schema for output (optional).
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
}

/// Tool metadata for MCP tool registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Unique tool name.
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// JSON Schema for input parameters.
    pub input_schema: serde_json::Value,
    /// Whether this tool is read-only (no side effects).
    pub read_only: bool,
}

/// The tool registry containing all available MCP tools.
pub fn tool_registry() -> &'static [Tool] {
    TOOL_REGISTRY.get_or_init(|| {
        vec![
            // Health and readiness probes
            Tool {
                name: "ferrum_gate_health",
                description: "Health probe returning server status",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_readyz_deep",
                description: "Deep readiness check including dependencies",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            // Intent and execution queries
            Tool {
                name: "ferrum_gate_list_intents",
                description: "List intents with optional filters",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "intent_id": {
                            "type": "string",
                            "description": "Filter by intent ID"
                        },
                        "state": {
                            "type": "string",
                            "description": "Filter by intent state"
                        },
                        "cursor": {
                            "type": "string",
                            "description": "Pagination cursor"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results",
                            "default": 50
                        }
                    },
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_get_execution",
                description: "Get execution status by ID",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "The execution ID to query"
                        }
                    },
                    "required": ["execution_id"]
                }),
                read_only: true,
            },
            // Provenance and lineage
            Tool {
                name: "ferrum_gate_query_lineage",
                description: "Query provenance events for an execution",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "execution_id": {
                            "type": "string",
                            "description": "The execution ID to query lineage for"
                        },
                        "cursor": {
                            "type": "string",
                            "description": "Pagination cursor"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of events",
                            "default": 100
                        }
                    },
                    "required": []
                }),
                read_only: true,
            },
            // Approval and policy queries
            Tool {
                name: "ferrum_gate_list_approvals",
                description: "List pending approvals",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_list_policy_bundles",
                description: "List available policy bundles",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            // Bridge queries
            Tool {
                name: "ferrum_gate_list_bridges",
                description: "List registered runtime bridges",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
                read_only: true,
            },
            Tool {
                name: "ferrum_gate_list_bridge_tools",
                description: "List tools for a specific bridge",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bridge_id": {
                            "type": "string",
                            "description": "The bridge ID to query tools for"
                        }
                    },
                    "required": ["bridge_id"]
                }),
                read_only: true,
            },
        ]
    })
}

/// Lazy-initialized tool registry.
static TOOL_REGISTRY: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();

/// Set of tool names that are read-only (no side effects).
pub const READ_ONLY_TOOLS: &[&str] = &[
    "ferrum_gate_health",
    "ferrum_gate_readyz_deep",
    "ferrum_gate_list_intents",
    "ferrum_gate_get_execution",
    "ferrum_gate_query_lineage",
    "ferrum_gate_list_approvals",
    "ferrum_gate_list_policy_bundles",
    "ferrum_gate_list_bridges",
    "ferrum_gate_list_bridge_tools",
];

/// Set of tool names that are mutating (require governance pipeline).
/// Empty in Phase A - all tools are read-only.
pub const MUTATING_TOOLS: &[&str] = &[];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry_contains_nine_tools() {
        let registry = tool_registry();
        assert_eq!(
            registry.len(),
            9,
            "Tool registry should contain exactly 9 tools"
        );
    }

    #[test]
    fn test_tool_registry_contains_only_read_only_tools() {
        for tool in tool_registry() {
            assert!(
                tool.read_only,
                "Tool '{}' should be marked as read_only=true",
                tool.name
            );
        }
    }

    #[test]
    fn test_mutating_tools_set_is_empty() {
        assert!(
            MUTATING_TOOLS.is_empty(),
            "MUTATING_TOOLS should be empty in Phase A, but found: {:?}",
            MUTATING_TOOLS
        );
    }

    #[test]
    fn test_read_only_tools_set_has_all_tools() {
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        let read_only_set: std::collections::HashSet<_> = READ_ONLY_TOOLS.iter().copied().collect();
        assert_eq!(
            registry_names, read_only_set,
            "READ_ONLY_TOOLS should match tool registry names"
        );
    }

    #[test]
    fn test_all_tools_have_non_null_schemas() {
        for tool in tool_registry() {
            assert!(
                !tool.input_schema.is_null(),
                "Tool '{}' should have a non-null input_schema",
                tool.name
            );
            assert!(
                !tool.description.is_empty(),
                "Tool '{}' should have a non-empty description",
                tool.name
            );
        }
    }

    #[test]
    fn test_no_mutating_tool_names_in_registry() {
        // These are patterns that indicate mutating tools - none should be present
        let mutating_patterns = [
            "submit",
            "evaluate",
            "execute",
            "compensate",
            "rollback",
            "fs_write",
            "git_push",
            "sql_mutate",
            "http_post",
            "create",
            "update",
            "delete",
        ];
        for tool in tool_registry() {
            for pattern in mutating_patterns {
                assert!(
                    !tool.name.contains(pattern),
                    "Tool '{}' should not contain mutating pattern '{}'",
                    tool.name,
                    pattern
                );
            }
        }
    }

    #[test]
    fn test_expected_tools_are_present() {
        let expected_tools = [
            "ferrum_gate_health",
            "ferrum_gate_readyz_deep",
            "ferrum_gate_list_intents",
            "ferrum_gate_get_execution",
            "ferrum_gate_query_lineage",
            "ferrum_gate_list_approvals",
            "ferrum_gate_list_policy_bundles",
            "ferrum_gate_list_bridges",
            "ferrum_gate_list_bridge_tools",
        ];
        let registry_names: std::collections::HashSet<_> =
            tool_registry().iter().map(|t| t.name).collect();
        for expected in expected_tools {
            assert!(
                registry_names.contains(expected),
                "Expected tool '{}' should be in registry",
                expected
            );
        }
    }
}
