//! Vendor-specific mapping helpers.
//!
//! This module is a placeholder for vendor-specific event normalization.
//! In this slice, no vendor-specific logic is implemented.
//!
//! Future slices can add sub-modules here, e.g.:
//! - `vendors/claude.rs` — Claude Code / Claude Desktop events
//! - `vendors/openai.rs` — OpenAI tool events
//! - `vendors/cursor.rs` — Cursor IDE events
//!
//! Each vendor helper would expose a `normalize` function that converts
//! a vendor-specific event payload into a bridge-local `McpRuntimeEvent`.

pub mod mod_ {
    // Re-export the module itself to keep the module tree valid.
    // Real vendor modules will be added in future slices.
}
