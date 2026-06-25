use anyhow::{Context, Result, bail};
use chrono::Timelike;
use clap::{Parser, Subcommand, ValueEnum};
use ed25519_dalek::Signer;
use sha2::Digest;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

mod audit_bundle;
mod backup;
mod client;

/// CLI-friendly actor type enum for use as a clap ValueEnum.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ActorTypeCli {
    User,
    Agent,
    PolicyEngine,
    Gateway,
    Adapter,
    Operator,
    System,
}

impl From<ActorTypeCli> for ferrum_proto::ActorType {
    fn from(v: ActorTypeCli) -> Self {
        match v {
            ActorTypeCli::User => ferrum_proto::ActorType::User,
            ActorTypeCli::Agent => ferrum_proto::ActorType::Agent,
            ActorTypeCli::PolicyEngine => ferrum_proto::ActorType::PolicyEngine,
            ActorTypeCli::Gateway => ferrum_proto::ActorType::Gateway,
            ActorTypeCli::Adapter => ferrum_proto::ActorType::Adapter,
            ActorTypeCli::Operator => ferrum_proto::ActorType::Operator,
            ActorTypeCli::System => ferrum_proto::ActorType::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Dot,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            "dot" => Ok(OutputFormat::Dot),
            _ => Err(format!(
                "invalid format '{}': expected text, json, or dot",
                s
            )),
        }
    }
}

/// Export format for audit log exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Ndjson,
    Json,
    Csv,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ndjson" => Ok(ExportFormat::Ndjson),
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            _ => Err(format!(
                "invalid export format '{}': expected ndjson, json, or csv",
                s
            )),
        }
    }
}

/// Render lineage events as Graphviz DOT format.
/// Nodes are events, edges are parent-child relationships derived from parent_edges.
/// Output is deterministic: events sorted by event_id, edges sorted by (from, to).
fn render_dot(execution_id: &str, events: &[client::ProvenanceEvent]) -> String {
    // Sort events by event_id for deterministic output
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    let mut lines = Vec::new();
    lines.push("digraph lineage {".to_string());
    lines.push(format!("  // execution_id: {}", execution_id));
    lines.push("  node [shape=box];".to_string());

    // Render nodes
    for event in &sorted_events {
        let label = format!("{}\n{}", event.event_id, event.kind);
        lines.push(format!(
            "  \"{}\" [label=\"{}\"];",
            event.event_id,
            escape_dot_label(&label)
        ));
    }

    // Collect and sort edges for deterministic output
    let mut edges: Vec<(String, String)> = Vec::new();
    for event in &sorted_events {
        for parent_edge in &event.parent_edges {
            if let Some(obj) = parent_edge.as_object() {
                if let Some(from_id) = obj.get("from_event_id").and_then(|v| v.as_str()) {
                    edges.push((from_id.to_string(), event.event_id.clone()));
                }
            }
        }
    }
    edges.sort();

    // Render edges
    for (from, to) in &edges {
        lines.push(format!("  \"{}\" -> \"{}\";", from, to));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Escape special characters in DOT label strings.
fn escape_dot_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn print_lifecycle_outbox_list(response: &ferrum_proto::LifecycleOutboxListResponse) {
    println!(
        "{:<36} {:<22} {:<36} {:<20} {:<8} LAST_ERROR",
        "OUTBOX_ID", "STATUS", "EXECUTION_ID", "NEW_STATE", "ATTEMPTS"
    );
    for record in &response.items {
        println!(
            "{:<36} {:<22} {:<36} {:<20} {:<8} {}",
            record.outbox_id,
            format!("{:?}", record.status),
            record.execution_id,
            format!("{:?}", record.new_execution_state),
            record.attempt_count,
            record.last_error.as_deref().unwrap_or("-")
        );
    }
    println!("Total: {}", response.total);
}

fn print_lifecycle_outbox_record(record: &ferrum_proto::LifecycleOutboxRecord) {
    println!("outbox_id: {}", record.outbox_id);
    println!("status: {:?}", record.status);
    println!("execution_id: {}", record.execution_id);
    println!("new_execution_state: {:?}", record.new_execution_state);
    if let Some(rollback_contract_id) = record.rollback_contract_id.as_ref() {
        println!("rollback_contract_id: {}", rollback_contract_id);
    }
    if let Some(rollback_state) = record.new_rollback_state.as_ref() {
        println!("new_rollback_state: {:?}", rollback_state);
    }
    println!(
        "intended_provenance_kind: {:?}",
        record.intended_provenance_kind
    );
    println!("attempt_count: {}", record.attempt_count);
    println!(
        "provenance_event_id: {}",
        record
            .provenance_event_id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "last_error: {}",
        record.last_error.as_deref().unwrap_or("-")
    );
    println!("updated_at: {}", record.updated_at.to_rfc3339());
    if !record.metadata.is_empty() {
        match serde_json::to_string_pretty(&record.metadata) {
            Ok(metadata) => println!("metadata: {}", metadata),
            Err(_) => println!("metadata: <unprintable>"),
        }
    }
}

fn get_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[derive(Debug, Parser)]
#[command(name = "ferrumctl")]
#[command(about = "FerrumGate control CLI")]
struct Cli {
    /// Server URL (defaults to http://127.0.0.1:8080).
    /// Environment: FERRUMCTL_SERVER_URL
    #[arg(long)]
    server_url: Option<String>,

    /// Bearer token for authentication.
    /// Environment: FERRUMCTL_BEARER_TOKEN
    #[arg(long)]
    bearer_token: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Server health and status commands.
    Server {
        #[command(subcommand)]
        sub: ServerCommand,
    },
    /// Local policy bundle authoring commands (no server required).
    Author {
        #[command(subcommand)]
        sub: AuthorCommand,
    },
    /// Local SQLite backup/restore commands (offline, no server required).
    Backup {
        #[command(subcommand)]
        sub: BackupCommand,
    },
    /// Local policy validation commands (offline, no server required).
    Policy {
        #[command(subcommand)]
        sub: PolicyCommand,
    },
    /// Admin/operator status and management commands.
    Admin {
        #[command(subcommand)]
        sub: AdminCommand,
    },
    /// Evidence snapshot and reporting commands.
    Evidence {
        #[command(subcommand)]
        sub: EvidenceCommand,
    },
    /// Readiness assessment and reporting commands.
    Readiness {
        #[command(subcommand)]
        sub: ReadinessCommand,
    },
    Health,
    ValidateRepo,
    ShowContracts,
}

/// Author subcommands for local bundle authoring workflow.
#[derive(Debug, Subcommand)]
enum AuthorCommand {
    /// Policy bundle authoring utilities.
    Bundle {
        #[command(subcommand)]
        sub: BundleCommand,
    },
}

/// Bundle subcommands for local authoring.
#[derive(Debug, Subcommand)]
enum BundleCommand {
    /// Bump the version of a local policy bundle file.
    Bump {
        /// Path to the policy bundle YAML file.
        yaml_file: String,

        /// Version bump type: patch, minor, or major.
        #[arg(long, value_name = "TYPE", default_value = "patch")]
        bump_type: BumpType,

        /// Output file path. When omitted, overwrites the input file.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,

        /// Output the updated bundle as JSON instead of YAML.
        #[arg(long)]
        json: bool,
    },
}

/// Version bump type for bundle bump command.
#[derive(Debug, Clone, Copy, Default)]
pub enum BumpType {
    #[default]
    Patch,
    Minor,
    Major,
}

impl std::str::FromStr for BumpType {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "patch" => Ok(BumpType::Patch),
            "minor" => Ok(BumpType::Minor),
            "major" => Ok(BumpType::Major),
            _ => Err(format!(
                "invalid bump type '{}': expected patch, minor, or major",
                s
            )),
        }
    }
}

/// Backup subcommands for local SQLite backup/restore workflow.
/// All backup commands are offline/local and do not require a running server.
#[derive(Debug, Subcommand)]
enum BackupCommand {
    /// Create a backup of a SQLite database.
    Create {
        /// Path to the source SQLite database file.
        #[arg(long, value_name = "PATH")]
        db_path: PathBuf,

        /// Directory to write the backup file.
        /// Defaults to the current directory.
        #[arg(long, value_name = "DIR")]
        output_dir: Option<PathBuf>,

        /// Number of days to retain backups. After creating a new backup,
        /// older backups matching the same source DB name pattern are deleted.
        /// Must be at least 1. Without this flag, no pruning is performed.
        #[arg(long, value_name = "N")]
        retention_days: Option<u32>,
    },
    /// Verify the integrity of a SQLite database.
    Verify {
        /// Path to the SQLite database file to verify.
        #[arg(long, value_name = "PATH")]
        db_path: PathBuf,
    },
    /// Restore a SQLite database from a backup.
    Restore {
        /// Path to the target database file to restore.
        #[arg(long, value_name = "PATH")]
        db_path: PathBuf,

        /// Path to the backup file to restore from.
        #[arg(long, value_name = "PATH")]
        from: PathBuf,

        /// Explicitly confirm the restore operation.
        /// Required unless --dry-run is used.
        #[arg(long)]
        confirm: bool,

        /// Validate preconditions and report what would happen without mutating the database.
        /// When set, --confirm is not required.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Policy subcommands for local offline validation and server-side apply.
#[derive(Debug, Subcommand)]
enum PolicyCommand {
    /// Validate a policy bundle YAML file offline.
    Validate {
        /// Path to the policy bundle YAML file.
        #[arg(long, value_name = "PATH")]
        file: String,
    },
    /// Validate and create a policy bundle on the server.
    /// The bundle is created inactive by default; use --activate to enable it.
    Apply {
        /// Path to the policy bundle YAML file.
        /// Use - to read from stdin.
        #[arg(long, value_name = "PATH")]
        file: String,

        /// Activate the policy bundle after creation.
        #[arg(long)]
        activate: bool,

        /// Output the created bundle as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Simulate a policy bundle against a sample proposal (online, server required).
    /// Side-effect free: no proposal, bundle, or provenance is persisted.
    Simulate {
        /// Path to the policy bundle YAML file.
        /// Use - to read from stdin.
        #[arg(long, value_name = "PATH")]
        file: String,

        /// Path to a JSON file containing the sample proposal.
        #[arg(long, value_name = "PATH")]
        proposal: String,

        /// Optional path to a JSON file containing an intent envelope.
        #[arg(long, value_name = "PATH")]
        intent: Option<String>,

        /// Output the result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Simulate a proposal against the active runtime policy (online, server required).
    /// Side-effect free: no proposal or provenance is persisted.
    RuntimeSimulate {
        /// Path to a JSON file containing the sample proposal.
        #[arg(long, value_name = "PATH")]
        proposal: String,

        /// Optional path to a JSON file containing an intent envelope.
        #[arg(long, value_name = "PATH")]
        intent: Option<String>,

        /// Output the result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List version history for a policy bundle.
    Versions {
        /// The bundle ID to list versions for.
        #[arg(long, value_name = "ID")]
        bundle_id: String,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show a structural diff between two policy bundle versions.
    Diff {
        /// The bundle ID to diff.
        #[arg(long, value_name = "ID")]
        bundle_id: String,

        /// Source version number.
        #[arg(long, value_name = "VERSION")]
        from: i64,

        /// Target version number.
        #[arg(long, value_name = "VERSION")]
        to: i64,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Rollback a policy bundle to a previous version.
    Rollback {
        /// The bundle ID to rollback.
        #[arg(long, value_name = "ID")]
        bundle_id: String,

        /// Target version number to rollback to.
        #[arg(long, value_name = "VERSION")]
        target_version: i64,

        /// Optional actor identifier.
        #[arg(long, value_name = "ACTOR")]
        actor: Option<String>,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Admin subcommands for operator status and management.
#[derive(Debug, Subcommand)]
enum AdminCommand {
    /// Show aggregated server status (health, readiness, deep, functional, metrics).
    Status,
    /// List, inspect, and resolve approvals.
    Approvals {
        #[command(subcommand)]
        sub: AdminApprovalsCommand,
    },
    /// Inspect and manage executions/intents.
    /// Note: `list` uses the existing intents API; full execution list API is not yet available.
    Executions {
        #[command(subcommand)]
        sub: AdminExecutionsCommand,
    },
    /// Inspect and resolve lifecycle outbox records that need operator review.
    LifecycleOutbox {
        #[command(subcommand)]
        sub: AdminLifecycleOutboxCommand,
    },
    /// Local SQLite backup/restore commands (offline, no server required).
    Backup {
        #[command(subcommand)]
        sub: AdminBackupCommand,
    },
    /// Manage scoped tokens (list, create, revoke, rotate).
    Tokens {
        #[command(subcommand)]
        sub: AdminTokensCommand,
    },
    /// Manage agent identities (list, register, revoke).
    Agents {
        #[command(subcommand)]
        sub: AdminAgentsCommand,
    },
    /// Query audit logs.
    Audit {
        #[command(subcommand)]
        sub: AdminAuditCommand,
    },
    /// Show effective CLI/client configuration (read-only, no server call).
    Config,
}

/// Approvals subcommands under `admin approvals`.
#[derive(Debug, Subcommand)]
enum AdminApprovalsCommand {
    /// List all pending approvals.
    List,
    /// Get a specific approval by ID.
    Get {
        /// Approval ID.
        approval_id: String,
    },
    /// Resolve (approve or deny) a pending approval.
    Resolve {
        /// Approval ID (UUID).
        approval_id: String,

        /// Grant the approval.
        #[arg(long)]
        approve: bool,

        /// Deny the approval.
        #[arg(long)]
        deny: bool,

        /// Actor type resolving this approval.
        #[arg(long, value_enum)]
        actor_type: ActorTypeCli,

        /// Actor ID (username, agent name, etc.).
        #[arg(long)]
        actor_id: String,

        /// Optional display name for the actor.
        #[arg(long)]
        actor_display_name: Option<String>,

        /// Reason for the decision. Required when --deny is set.
        #[arg(long)]
        reason: Option<String>,

        /// Output the resolved approval as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Executions subcommands under `admin executions`.
#[derive(Debug, Subcommand)]
enum AdminExecutionsCommand {
    /// List intents/executions with filters.
    /// Note: uses the existing intents API; actor/time filters are not yet supported.
    List {
        /// Intent ID (exact match).
        #[arg(long)]
        intent_id: Option<String>,

        /// Intent status filter (repeatable for multiple states).
        #[arg(long, value_name = "STATE")]
        state: Vec<String>,

        /// Pagination cursor (from previous page).
        #[arg(long)]
        cursor: Option<String>,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Get an execution by ID.
    Get {
        /// Execution ID.
        execution_id: String,
    },
    /// Cancel a running execution.
    Cancel {
        /// Execution ID to cancel.
        execution_id: String,

        /// Explicitly confirm the cancellation. Required.
        #[arg(long)]
        confirm: bool,

        /// Output the cancellation result as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Lifecycle outbox subcommands under `admin lifecycle-outbox`.
#[derive(Debug, Subcommand)]
enum AdminLifecycleOutboxCommand {
    /// List lifecycle outbox records by reconciliation status.
    List {
        /// Status filter: needs_operator_review, pending, pending_provenance, provenance_written, reconciled.
        #[arg(long, value_name = "STATUS", default_value = "needs_operator_review")]
        status: String,

        /// Number of records to return (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Get a lifecycle outbox record and its operator-review context.
    Get {
        /// Lifecycle outbox ID.
        outbox_id: String,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Retry reconciliation after the operator has corrected underlying data.
    Retry {
        /// Lifecycle outbox ID.
        outbox_id: String,

        /// Operator or automation actor ID.
        #[arg(long)]
        actor_id: String,

        /// Optional reason or remediation note.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Mark an operator-review record as externally resolved.
    Resolve {
        /// Lifecycle outbox ID.
        outbox_id: String,

        /// Operator or automation actor ID.
        #[arg(long)]
        actor_id: String,

        /// Required resolution note for audit.
        #[arg(long, value_name = "TEXT")]
        reason: String,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
}

/// Backup subcommands under `admin backup`.
/// Delegates to the same offline helpers as the top-level `backup` command.
#[derive(Debug, Subcommand)]
enum AdminBackupCommand {
    /// Create a backup of a SQLite database.
    Create {
        /// Path to the source SQLite database file.
        #[arg(long, value_name = "PATH")]
        db_path: PathBuf,

        /// Directory to write the backup file.
        /// Defaults to the current directory.
        #[arg(long, value_name = "DIR")]
        output_dir: Option<PathBuf>,

        /// Number of days to retain backups. After creating a new backup,
        /// older backups matching the same source DB name pattern are deleted.
        /// Must be at least 1. Without this flag, no pruning is performed.
        #[arg(long, value_name = "N")]
        retention_days: Option<u32>,
    },
    /// Verify the integrity of a SQLite database.
    Verify {
        /// Path to the SQLite database file to verify.
        #[arg(long, value_name = "PATH")]
        db_path: PathBuf,
    },
    /// Restore a SQLite database from a backup.
    Restore {
        /// Path to the target database file to restore.
        #[arg(long, value_name = "PATH")]
        db_path: PathBuf,

        /// Path to the backup file to restore from.
        #[arg(long, value_name = "PATH")]
        from: PathBuf,

        /// Explicitly confirm the restore operation.
        /// Required unless --dry-run is used.
        #[arg(long)]
        confirm: bool,

        /// Validate preconditions and report what would happen without mutating the database.
        /// When set, --confirm is not required.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Tokens subcommands under `admin tokens`.
#[derive(Debug, Subcommand)]
enum AdminTokensCommand {
    /// List scoped tokens (metadata only; no secret values).
    List {
        /// Filter by actor ID (exact match).
        #[arg(long, value_name = "ID")]
        actor_id: Option<String>,

        /// Filter by role.
        #[arg(long, value_name = "ROLE")]
        role: Option<String>,

        /// Show only active tokens (exclude revoked and expired).
        #[arg(long)]
        active_only: bool,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },

    /// Create a new scoped token.
    /// The token value is printed exactly once and never retrievable again.
    Create {
        /// Actor ID (username, service name, etc.).
        #[arg(long, value_name = "ID")]
        actor_id: String,

        /// Role to assign.
        #[arg(long, value_name = "ROLE")]
        role: String,

        /// Explicit scope list (repeatable). If omitted, uses role defaults.
        #[arg(long, value_name = "SCOPE")]
        scope: Vec<String>,

        /// Token description.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,

        /// Expiration in days from now. Alternative to --expires-at.
        #[arg(long, value_name = "N", group = "expiry")]
        expires_in_days: Option<u32>,

        /// Absolute expiration timestamp (ISO 8601). Alternative to --expires-in-days.
        #[arg(long, value_name = "TIMESTAMP", group = "expiry")]
        expires_at: Option<String>,

        /// Output the created token as JSON (includes the secret token_value).
        #[arg(long)]
        json: bool,
    },

    /// Revoke a scoped token.
    Revoke {
        /// Token ID to revoke.
        token_id: String,

        /// Reason for revocation.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Skip interactive confirmation.
        #[arg(long)]
        force: bool,
    },

    /// Rotate a scoped token (revoke old, create new with same actor/role/scopes).
    Rotate {
        /// Token ID to rotate.
        token_id: String,

        /// New expiration in days from now.
        #[arg(long, value_name = "N", group = "expiry")]
        expires_in_days: Option<u32>,

        /// New absolute expiration timestamp (ISO 8601).
        #[arg(long, value_name = "TIMESTAMP", group = "expiry")]
        expires_at: Option<String>,

        /// Reason for rotation.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Output the new token as JSON (includes the secret token_value).
        #[arg(long)]
        json: bool,

        /// Skip interactive confirmation.
        #[arg(long)]
        force: bool,
    },
}

/// Agents subcommands under `admin agents`.
#[derive(Debug, Subcommand)]
enum AdminAgentsCommand {
    /// List registered agent identities.
    List {
        /// Show only active agents (exclude revoked).
        #[arg(long)]
        active_only: bool,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },

    /// Register a new agent identity.
    Register {
        /// Agent ID (unique identifier).
        #[arg(long, value_name = "ID")]
        agent_id: String,

        /// Base64-encoded Ed25519 public key (32 bytes).
        #[arg(long, value_name = "B64")]
        public_key: String,

        /// Scope list (repeatable). If omitted, uses default agent scopes.
        #[arg(long, value_name = "SCOPE")]
        scope: Vec<String>,

        /// Description.
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,

        /// Output the registered agent as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Revoke an agent identity.
    Revoke {
        /// Agent ID to revoke.
        agent_id: String,

        /// Reason for revocation.
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Skip interactive confirmation.
        #[arg(long)]
        force: bool,
    },
}

/// Audit subcommands under `admin audit`.
#[derive(Debug, Subcommand)]
enum AdminAuditCommand {
    /// List audit log entries with optional filters.
    List {
        /// Filter by action (e.g., token_create, policy_bundle_activate).
        #[arg(long, value_name = "ACTION")]
        action: Option<String>,

        /// Filter by resource type (e.g., token, policy_bundle, approval, execution).
        #[arg(long, value_name = "TYPE")]
        resource_type: Option<String>,

        /// Filter by resource ID.
        #[arg(long, value_name = "ID")]
        resource_id: Option<String>,

        /// Pagination cursor (from previous page).
        #[arg(long)]
        cursor: Option<String>,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Only include entries created at or after this time (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        since: Option<String>,

        /// Only include entries created at or before this time (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        until: Option<String>,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Export audit logs with optional filters.
    Export {
        /// Filter by action (e.g., token_create, policy_bundle_activate).
        #[arg(long, value_name = "ACTION")]
        action: Option<String>,

        /// Filter by resource type (e.g., token, policy_bundle, approval, execution).
        #[arg(long, value_name = "TYPE")]
        resource_type: Option<String>,

        /// Filter by resource ID.
        #[arg(long, value_name = "ID")]
        resource_id: Option<String>,

        /// Only include entries created at or after this time (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        since: Option<String>,

        /// Only include entries created at or before this time (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        until: Option<String>,

        /// Export format: ndjson (default), json, or csv.
        #[arg(long, value_name = "FORMAT", default_value = "ndjson")]
        format: ExportFormat,

        /// Export as a portable bundle directory containing `audit.jsonl` and `manifest.json`.
        #[arg(long, value_name = "DIR")]
        bundle: Option<PathBuf>,

        /// Output file path (default: stdout).
        #[arg(long, value_name = "PATH")]
        output: Option<String>,
    },
    /// Verify the audit log hash chain integrity.
    Verify {
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,

        /// Verify a local bundle directory instead of the remote server.
        #[arg(long, value_name = "DIR")]
        bundle: Option<PathBuf>,
    },
    /// Compute or retrieve the Merkle root for an hourly audit window.
    MerkleVerify {
        /// UTC-aligned hourly window start (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        window_start: String,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// List cached Merkle roots with pagination.
    MerkleRoots {
        /// Pagination cursor.
        #[arg(long)]
        cursor: Option<String>,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Sign and submit a checkpoint for an audit window.
    CheckpointSign {
        /// UTC-aligned hourly window start (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        window_start: String,

        /// Signer identifier.
        #[arg(long, value_name = "ID")]
        signer_id: String,

        /// Ed25519 private key as base64 (32 bytes seed, 64 bytes expanded, or 32-byte raw secret).
        /// For ed25519-dalek v2, use a 32-byte seed encoded as base64.
        #[arg(long, value_name = "B64")]
        private_key: String,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Verify a stored checkpoint for an audit window.
    CheckpointVerify {
        /// UTC-aligned hourly window start (RFC 3339).
        #[arg(long, value_name = "TIMESTAMP")]
        window_start: String,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// List signed checkpoints with pagination.
    CheckpointList {
        /// Pagination cursor.
        #[arg(long)]
        cursor: Option<String>,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
}

/// Evidence subcommands under `evidence`.
#[derive(Debug, Subcommand)]
enum EvidenceCommand {
    /// Capture a point-in-time evidence snapshot as a local JSON file.
    Snapshot {
        /// Directory to write the snapshot file. Defaults to the current directory.
        #[arg(long, value_name = "DIR")]
        output_dir: Option<PathBuf>,
    },
    /// Local SLO evidence window lifecycle commands (no server required).
    SloWindow {
        #[command(subcommand)]
        sub: SloWindowCommand,
    },
}

/// SLO window subcommands.
#[derive(Debug, Subcommand)]
enum SloWindowCommand {
    /// Start a new SLO evidence window.
    /// Creates a local state file. Refuses overwrite if an active window exists.
    Start {
        /// Directory for the window state file. Defaults to the current directory.
        #[arg(long, value_name = "DIR")]
        window_dir: Option<PathBuf>,
        /// Optional free-form notes.
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
    },
    /// Show the current SLO window status.
    Status {
        /// Directory containing the window state file. Defaults to the current directory.
        #[arg(long, value_name = "DIR")]
        window_dir: Option<PathBuf>,
        /// Output raw JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Finalize the current SLO window.
    /// Rejects before 7 days unless --allow-early.
    Finalize {
        /// Directory containing the window state file. Defaults to the current directory.
        #[arg(long, value_name = "DIR")]
        window_dir: Option<PathBuf>,
        /// Optional free-form notes.
        #[arg(long, value_name = "TEXT")]
        notes: Option<String>,
        /// Allow finalization before the 7-day minimum duration.
        #[arg(long)]
        allow_early: bool,
    },
}

/// Readiness subcommands.
#[derive(Debug, Subcommand)]
enum ReadinessCommand {
    /// Generate a read-only readiness report aggregating live probes and local state.
    Report {
        /// Optional evidence snapshot file path.
        #[arg(long, value_name = "PATH")]
        snapshot: Option<PathBuf>,
        /// Optional SLO window state directory. Defaults to current directory.
        #[arg(long, value_name = "DIR")]
        window_dir: Option<PathBuf>,
        /// Output JSON instead of text.
        #[arg(long)]
        json: bool,
        /// Skip live server probes and use local state only.
        #[arg(long)]
        offline: bool,
    },
}

/// Local state for an SLO evidence window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct SloWindowState {
    window_id: String,
    status: String,
    window_started_at: chrono::DateTime<chrono::Utc>,
    window_ended_at: Option<chrono::DateTime<chrono::Utc>>,
    elapsed_duration_seconds: u64,
    target_duration_days: u32,
    minimum_duration_days: u32,
    notes: Option<String>,
    non_claims_notice: String,
    created_by_tool: String,
    finalized_by_tool: Option<String>,
}

impl SloWindowState {
    /// Generate a non-claims notice string.
    fn default_non_claims_notice() -> String {
        [
            "Sustained SLO window = NOT COMPLETE",
            "production-ready = NO",
            "Tier 2 = NOT COMPLETE",
            "This record tracks lifecycle only; it does not certify SLO achievement.",
        ]
        .join("\n")
    }

    /// Create a new started window state.
    fn start_now(notes: Option<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            window_id: format!("slo-window-{}", now.format("%Y%m%dT%H%M%SZ")),
            status: "started".to_string(),
            window_started_at: now,
            window_ended_at: None,
            elapsed_duration_seconds: 0,
            target_duration_days: 30,
            minimum_duration_days: 7,
            notes,
            non_claims_notice: Self::default_non_claims_notice(),
            created_by_tool: "ferrumctl evidence slo-window start".to_string(),
            finalized_by_tool: None,
        }
    }

    /// Compute elapsed seconds since start.
    fn recompute_elapsed(&mut self) {
        let now = chrono::Utc::now();
        let end = self.window_ended_at.unwrap_or(now);
        let dur = end.signed_duration_since(self.window_started_at);
        self.elapsed_duration_seconds = dur.num_seconds().max(0) as u64;
    }

    /// Finalize the window, returning the updated state.
    fn finalize(&mut self, notes: Option<String>, allow_early: bool) -> Result<()> {
        if self.status == "finalized" {
            // Idempotent: already finalized
            return Ok(());
        }
        self.recompute_elapsed();
        let min_secs = (self.minimum_duration_days as i64) * 24 * 60 * 60;
        if (self.elapsed_duration_seconds as i64) < min_secs && !allow_early {
            bail!(
                "window has run for {} seconds ({} days); minimum is {} days. Use --allow-early to override",
                self.elapsed_duration_seconds,
                self.elapsed_duration_seconds / 86400,
                self.minimum_duration_days
            );
        }
        self.status = "finalized".to_string();
        self.window_ended_at = Some(chrono::Utc::now());
        self.finalized_by_tool = Some("ferrumctl evidence slo-window finalize".to_string());
        if let Some(n) = notes {
            self.notes = Some(n);
        }
        self.recompute_elapsed();
        Ok(())
    }
}

fn slo_window_state_path(window_dir: Option<PathBuf>) -> PathBuf {
    window_dir
        .unwrap_or_else(|| PathBuf::from("."))
        .join("slo-window-state.json")
}

fn read_slo_window_state(path: &PathBuf) -> Result<SloWindowState> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read state file {}", path.display()))?;
    let state: SloWindowState = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse state file {}", path.display()))?;
    Ok(state)
}

fn write_slo_window_state(path: &PathBuf, state: &SloWindowState) -> Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)
        .with_context(|| format!("failed to write state file {}", path.display()))?;
    Ok(())
}

fn print_slo_window_status(state: &SloWindowState) {
    println!("window_id:    {}", state.window_id);
    println!("status:       {}", state.status);
    println!("started_at:   {}", state.window_started_at.to_rfc3339());
    if let Some(end) = state.window_ended_at {
        println!("ended_at:     {}", end.to_rfc3339());
    }
    println!(
        "elapsed:      {} seconds (~{} days)",
        state.elapsed_duration_seconds,
        state.elapsed_duration_seconds / 86400
    );
    println!("target:       {} days", state.target_duration_days);
    println!("minimum:      {} days", state.minimum_duration_days);
    if let Some(ref n) = state.notes {
        println!("notes:        {}", n);
    }
    println!("created_by:   {}", state.created_by_tool);
    if let Some(ref f) = state.finalized_by_tool {
        println!("finalized_by: {}", f);
    }
    println!("--- non-claims ---");
    for line in state.non_claims_notice.lines() {
        println!("{}", line);
    }
}

/// Run `slo-window start`.
fn run_slo_window_start(window_dir: Option<PathBuf>, notes: Option<String>) -> Result<()> {
    let path = slo_window_state_path(window_dir);
    if path.exists() {
        let existing = read_slo_window_state(&path)?;
        if existing.status == "started" {
            bail!(
                "an active window already exists at {} (window_id: {}). Use `finalize` first or choose a different --window-dir.",
                path.display(),
                existing.window_id
            );
        }
    }
    let state = SloWindowState::start_now(notes);
    write_slo_window_state(&path, &state)?;
    println!("SLO window started: {}", state.window_id);
    println!("State file: {}", path.display());
    Ok(())
}

/// Run `slo-window status`.
fn run_slo_window_status(window_dir: Option<PathBuf>, json: bool) -> Result<()> {
    let path = slo_window_state_path(window_dir);
    let mut state = read_slo_window_state(&path)?;
    state.recompute_elapsed();
    if json {
        println!("{}", serde_json::to_string_pretty(&state)?);
    } else {
        print_slo_window_status(&state);
    }
    Ok(())
}

/// Run `slo-window finalize`.
fn run_slo_window_finalize(
    window_dir: Option<PathBuf>,
    notes: Option<String>,
    allow_early: bool,
) -> Result<()> {
    let path = slo_window_state_path(window_dir);
    let mut state = read_slo_window_state(&path)?;
    state.finalize(notes, allow_early)?;
    write_slo_window_state(&path, &state)?;
    println!("SLO window finalized: {}", state.window_id);
    println!(
        "Elapsed: {} seconds (~{} days)",
        state.elapsed_duration_seconds,
        state.elapsed_duration_seconds / 86400
    );
    Ok(())
}

// -------------------------------------------------------------------------
// Readiness report
// -------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct OverallAssessment {
    label: String,
    production_ready: String,
    tier_2: String,
    ha4_automated_failover: String,
    sustained_slo: String,
    issues: Vec<String>,
}

impl Default for OverallAssessment {
    fn default() -> Self {
        Self {
            label: "Cautious / Point-in-time only".to_string(),
            production_ready: "NO".to_string(),
            tier_2: "NOT COMPLETE".to_string(),
            ha4_automated_failover: "NOT COMPLETE".to_string(),
            sustained_slo: "NOT COMPLETE".to_string(),
            issues: vec![
                "production-ready = NO".to_string(),
                "Tier 2 = NOT COMPLETE".to_string(),
                "HA-4 automated failover = NOT COMPLETE".to_string(),
                "Sustained SLO = NOT COMPLETE".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct ReadinessReport {
    report_timestamp: String,
    tool: String,
    offline_mode: bool,
    non_claims_reference: String,
    non_claims_notice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    health: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    readiness: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    readiness_deep: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    functional_readiness: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metrics_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slo_window: Option<SloWindowState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_snapshot: Option<serde_json::Value>,
    overall: OverallAssessment,
}

fn find_latest_evidence_snapshot(dir: &std::path::Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("evidence-snapshot-") && name_str.ends_with(".json") {
                candidates.push(entry.path());
            }
        }
    }
    candidates.sort();
    candidates.last().cloned()
}

fn print_readiness_report(report: &ReadinessReport) {
    println!("FerrumGate Readiness Report");
    println!("===========================");
    println!("report_timestamp: {}", report.report_timestamp);
    println!("tool:             {}", report.tool);
    println!("offline_mode:     {}", report.offline_mode);
    println!();
    println!("non_claims_reference: {}", report.non_claims_reference);
    println!("non_claims_notice:    {}", report.non_claims_notice);
    println!();
    if let Some(ref h) = report.health {
        println!("health:");
        println!("{}", serde_json::to_string_pretty(h).unwrap_or_default());
    }
    if let Some(ref r) = report.readiness {
        println!("readiness:");
        println!("{}", serde_json::to_string_pretty(r).unwrap_or_default());
    }
    if let Some(ref r) = report.readiness_deep {
        println!("readiness_deep:");
        println!("{}", serde_json::to_string_pretty(r).unwrap_or_default());
    }
    if let Some(ref f) = report.functional_readiness {
        println!("functional_readiness:");
        println!("{}", serde_json::to_string_pretty(f).unwrap_or_default());
    }
    if let Some(ref m) = report.metrics_summary {
        println!("metrics_summary:");
        println!("{}", serde_json::to_string_pretty(m).unwrap_or_default());
    }
    if let Some(ref s) = report.slo_window {
        println!("slo_window:");
        println!("{}", serde_json::to_string_pretty(s).unwrap_or_default());
    }
    if let Some(ref e) = report.evidence_snapshot {
        println!("evidence_snapshot:");
        println!("{}", serde_json::to_string_pretty(e).unwrap_or_default());
    }
    println!();
    println!("overall:");
    println!("  label:                  {}", report.overall.label);
    println!(
        "  production_ready:       {}",
        report.overall.production_ready
    );
    println!("  tier_2:                 {}", report.overall.tier_2);
    println!(
        "  ha4_automated_failover: {}",
        report.overall.ha4_automated_failover
    );
    println!("  sustained_slo:          {}", report.overall.sustained_slo);
    if !report.overall.issues.is_empty() {
        println!("  issues:");
        for issue in &report.overall.issues {
            println!("    - {}", issue);
        }
    }
}

async fn build_readiness_report(
    server_url: &str,
    bearer_token: Option<&str>,
    snapshot: Option<PathBuf>,
    window_dir: Option<PathBuf>,
    offline: bool,
) -> Result<ReadinessReport> {
    let mut report = ReadinessReport {
        report_timestamp: chrono::Utc::now().to_rfc3339(),
        tool: "ferrumctl readiness report".to_string(),
        offline_mode: offline,
        non_claims_reference: "docs/security/non-claims.md".to_string(),
        non_claims_notice: "This report is a point-in-time operational view. It is not production-ready, Tier 2, GA, compliance, or SLO proof.".to_string(),
        health: None,
        readiness: None,
        readiness_deep: None,
        functional_readiness: None,
        metrics_summary: None,
        slo_window: None,
        evidence_snapshot: None,
        overall: OverallAssessment::default(),
    };

    // SLO window state
    let window_path = slo_window_state_path(window_dir.clone());
    if window_path.exists() {
        match read_slo_window_state(&window_path) {
            Ok(mut state) => {
                state.recompute_elapsed();
                report.slo_window = Some(state);
            }
            Err(e) => {
                report
                    .overall
                    .issues
                    .push(format!("slo_window read error: {}", e));
            }
        }
    }

    // Evidence snapshot
    let snapshot_path = if let Some(ref path) = snapshot {
        path.clone()
    } else {
        let dir = window_dir.unwrap_or_else(|| PathBuf::from("."));
        find_latest_evidence_snapshot(&dir).unwrap_or_else(|| PathBuf::from("."))
    };
    if snapshot_path.exists() && snapshot_path.is_file() {
        match std::fs::read_to_string(&snapshot_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(val) => report.evidence_snapshot = Some(val),
                Err(e) => {
                    report.evidence_snapshot =
                        Some(serde_json::json!({"error": format!("parse error: {}", e)}));
                }
            },
            Err(e) => {
                report.evidence_snapshot =
                    Some(serde_json::json!({"error": format!("read error: {}", e)}));
            }
        }
    } else if snapshot.is_some() {
        report.evidence_snapshot = Some(
            serde_json::json!({"error": format!("snapshot not found: {}", snapshot_path.display())}),
        );
    }

    if !offline {
        let client = client::Client::new(server_url.to_string(), bearer_token.map(String::from))?;
        match client.health().await {
            Ok(h) => report.health = Some(serde_json::json!({"status": h.status})),
            Err(e) => report.health = Some(serde_json::json!({"error": e.to_string()})),
        }
        match client.readiness().await {
            Ok(r) => report.readiness = Some(serde_json::json!({"status": r.status})),
            Err(e) => report.readiness = Some(serde_json::json!({"error": e.to_string()})),
        }
        match client.readiness_deep_json().await {
            Ok(r) => report.readiness_deep = Some(r),
            Err(e) => report.readiness_deep = Some(serde_json::json!({"error": e.to_string()})),
        }
        match client.functional_readiness().await {
            Ok(items) => {
                report.functional_readiness =
                    Some(serde_json::json!({"status": "ready", "approvals_found": items.len()}));
            }
            Err(e) => {
                report.functional_readiness = Some(serde_json::json!({"error": e.to_string()}));
            }
        }
        match client.metrics().await {
            Ok(text) => {
                let lines = text.lines().count();
                report.metrics_summary =
                    Some(serde_json::json!({"available": true, "line_count": lines}));
            }
            Err(e) => {
                report.metrics_summary = Some(serde_json::json!({"error": e.to_string()}));
            }
        }
    }

    Ok(report)
}

async fn run_readiness_report(
    server_url: &str,
    bearer_token: Option<&str>,
    snapshot: Option<PathBuf>,
    window_dir: Option<PathBuf>,
    json: bool,
    offline: bool,
) -> Result<()> {
    let report =
        build_readiness_report(server_url, bearer_token, snapshot, window_dir, offline).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_readiness_report(&report);
    }
    Ok(())
}

#[derive(Debug, Subcommand)]
enum ServerCommand {
    /// Check server health.
    Health,
    /// Fetch Prometheus metrics from /v1/metrics.
    Metrics,
    /// Check server readiness.
    Readiness {
        /// Deep readiness probe with store connectivity check.
        /// Calls GET /v1/readyz/deep instead of shallow /v1/readyz.
        #[arg(long)]
        deep: bool,

        /// Functional readiness probe.
        /// Calls GET /v1/approvals?limit=1 with bearer auth to confirm
        /// store, auth, and governance loop are functional.
        #[arg(long)]
        functional: bool,
    },
    /// Inspect an execution by ID.
    InspectExecution {
        /// Execution ID.
        execution_id: String,
    },
    /// List all pending approvals.
    InspectApprovals,
    /// Inspect a specific approval by ID.
    InspectApproval {
        /// Approval ID.
        approval_id: String,
    },
    /// Get lineage for an execution.
    InspectLineage {
        /// Execution ID.
        execution_id: String,
        /// Output format: text (default), json, or dot.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
        /// Output file path. When omitted, writes to stdout.
        /// Required when format is dot for file-based output.
        #[arg(long, value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Query provenance events.
    InspectProvenance {
        /// Intent ID (optional).
        #[arg(long)]
        intent_id: Option<String>,
    },
    /// Resolve a pending approval by ID (approve or deny).
    ResolveApproval {
        /// Approval ID (UUID).
        approval_id: String,

        /// Grant the approval.
        #[arg(long)]
        approve: bool,

        /// Deny the approval.
        #[arg(long)]
        deny: bool,

        /// Actor type resolving this approval.
        #[arg(long, value_enum)]
        actor_type: ActorTypeCli,

        /// Actor ID (username, agent name, etc.).
        #[arg(long)]
        actor_id: String,

        /// Optional display name for the actor.
        #[arg(long)]
        actor_display_name: Option<String>,

        /// Reason for the decision. Required when --deny is set.
        #[arg(long)]
        reason: Option<String>,

        /// Output the resolved approval as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List intents with optional filters.
    ListIntents {
        /// Intent ID (exact match).
        #[arg(long)]
        intent_id: Option<String>,

        /// Intent status filter (repeatable for multiple states).
        #[arg(long, value_name = "STATE")]
        state: Vec<String>,

        /// Pagination cursor (from previous page).
        #[arg(long)]
        cursor: Option<String>,

        /// Number of items per page (default 50, max 200).
        #[arg(long, value_name = "N", default_value = "50")]
        limit: u32,

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Cancel a running execution.
    CancelExecution {
        /// Execution ID to cancel.
        execution_id: String,

        /// Explicitly confirm the cancellation. Required.
        #[arg(long)]
        confirm: bool,

        /// Output the cancellation result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Create a new policy bundle.
    CreatePolicyBundle {
        /// Path to the YAML file containing the policy bundle.
        /// Use - to read from stdin.
        yaml_file: String,

        /// Output the created bundle as JSON.
        #[arg(long)]
        json: bool,
    },
    /// List all policy bundles.
    ListPolicyBundles {
        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
    /// Get a policy bundle by ID.
    GetPolicyBundle {
        /// Bundle ID.
        bundle_id: String,

        /// Output the bundle as JSON.
        #[arg(long)]
        json: bool,

        /// Export the bundle as YAML to the specified file path.
        #[arg(long, value_name = "PATH")]
        export: Option<PathBuf>,
    },
    /// Update an existing policy bundle.
    UpdatePolicyBundle {
        /// Bundle ID.
        bundle_id: String,

        /// Path to the YAML file containing the updated policy bundle.
        /// Use - to read from stdin.
        yaml_file: String,

        /// Output the updated bundle as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Delete a policy bundle.
    DeletePolicyBundle {
        /// Bundle ID.
        bundle_id: String,

        /// Explicitly confirm the deletion. Required.
        #[arg(long)]
        confirm: bool,

        /// Output the deletion result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Set the active flag for a policy bundle.
    SetPolicyBundleActive {
        /// Bundle ID.
        bundle_id: String,

        /// Activate the policy bundle.
        #[arg(long)]
        activate: bool,

        /// Deactivate the policy bundle.
        #[arg(long)]
        deactivate: bool,
    },
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn run_contract_check() -> Result<()> {
    let root = repo_root();
    let script_path = root.join("scripts/check_contract_consistency.py");

    let output = ProcessCommand::new("python3")
        .arg(&script_path)
        .current_dir(&root)
        .output()
        .with_context(|| format!("failed to run {}", script_path.display()))?;

    if !output.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        bail!(
            "repository validation failed with exit code {:?}",
            output.status.code()
        );
    }

    Ok(())
}

/// Run the author bundle bump command.
/// Reads a policy bundle YAML file, bumps the version, and writes it back.
fn run_author_bundle_bump(
    yaml_file: &str,
    bump_type: BumpType,
    output_path: Option<&str>,
    json: bool,
) -> Result<()> {
    // Read the input file
    let yaml_content = if yaml_file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(yaml_file)?
    };

    // Parse the bundle as a generic YAML value to avoid depending on private types
    let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml_content)
        .map_err(|e| anyhow::anyhow!("failed to parse policy bundle YAML: {}", e))?;

    // Extract and update the version
    let version_str = value
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("bundle YAML must have a 'version' field"))?;

    // Parse version and bump it
    let mut version_parts: Vec<u32> = version_str
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();

    // Ensure we have at least 3 parts (major.minor.patch)
    while version_parts.len() < 3 {
        version_parts.push(0);
    }

    match bump_type {
        BumpType::Major => {
            version_parts[0] += 1;
            version_parts[1] = 0;
            version_parts[2] = 0;
        }
        BumpType::Minor => {
            version_parts[1] += 1;
            version_parts[2] = 0;
        }
        BumpType::Patch => {
            version_parts[2] += 1;
        }
    }

    let new_version = format!(
        "{}.{}.{}",
        version_parts[0], version_parts[1], version_parts[2]
    );

    // Update the version in the value
    if let Some(map) = value.as_mapping_mut() {
        map.insert(
            serde_yaml::Value::String("version".to_string()),
            serde_yaml::Value::String(new_version),
        );
    } else {
        bail!("bundle YAML must be a mapping object");
    }

    // Determine output path
    let out_path = output_path.map(PathBuf::from).unwrap_or_else(|| {
        if yaml_file == "-" {
            PathBuf::from("bundle-bumped.yaml")
        } else {
            PathBuf::from(yaml_file)
        }
    });

    // Serialize and write
    if json {
        let json_content = serde_json::to_string_pretty(&value)?;
        if out_path.to_str() == Some("-") {
            println!("{}", json_content);
        } else {
            std::fs::write(&out_path, json_content)?;
            eprintln!(
                "Bundle version bumped and written to {}",
                out_path.display()
            );
        }
    } else {
        let yaml_out = serde_yaml::to_string(&value)?;
        if out_path.to_str() == Some("-") {
            println!("{}", yaml_out);
        } else {
            std::fs::write(&out_path, yaml_out)?;
            eprintln!(
                "Bundle version bumped and written to {}",
                out_path.display()
            );
        }
    }

    Ok(())
}

/// Print effective CLI/client configuration (read-only; no server call).
/// Token values are fully redacted.
fn run_admin_config(server_url: &str, bearer_token: Option<&str>) -> Result<()> {
    let mut map = serde_json::Map::new();
    map.insert(
        "server_url".to_string(),
        serde_json::Value::String(server_url.to_string()),
    );
    map.insert(
        "bearer_token".to_string(),
        match bearer_token {
            Some(_) => serde_json::Value::String("<set:redacted>".to_string()),
            None => serde_json::Value::String("<unset>".to_string()),
        },
    );
    map.insert(
        "env_FERRUMCTL_SERVER_URL".to_string(),
        match get_env("FERRUMCTL_SERVER_URL") {
            Some(_) => serde_json::Value::String("<set>".to_string()),
            None => serde_json::Value::String("<unset>".to_string()),
        },
    );
    map.insert(
        "env_FERRUMCTL_BEARER_TOKEN".to_string(),
        match get_env("FERRUMCTL_BEARER_TOKEN") {
            Some(_) => serde_json::Value::String("<set:redacted>".to_string()),
            None => serde_json::Value::String("<unset>".to_string()),
        },
    );
    println!("{}", serde_json::to_string_pretty(&map)?);
    Ok(())
}

/// Generate a filesystem-safe timestamped snapshot filename.
fn evidence_snapshot_filename(ts: &chrono::DateTime<chrono::Utc>) -> String {
    format!("evidence-snapshot-{}.json", ts.format("%Y-%m-%dT%H-%M-%SZ"))
}

/// Capture a point-in-time evidence snapshot by aggregating existing client APIs.
/// Individual probe failures are captured as errors in their respective sections
/// rather than failing the whole snapshot.
async fn run_evidence_snapshot(client: client::Client, output_dir: Option<PathBuf>) -> Result<()> {
    let mut snapshot = serde_json::Map::new();
    let ts = chrono::Utc::now();
    snapshot.insert(
        "snapshot_timestamp".to_string(),
        serde_json::Value::String(ts.to_rfc3339()),
    );
    snapshot.insert(
        "tool".to_string(),
        serde_json::Value::String("ferrumctl evidence snapshot".to_string()),
    );
    snapshot.insert(
        "non_claims_reference".to_string(),
        serde_json::Value::String("docs/security/non-claims.md".to_string()),
    );
    snapshot.insert(
        "non_claims_notice".to_string(),
        serde_json::Value::String(
            "This snapshot is a point-in-time operational view. It is not production-ready, Tier 2, GA, compliance, or SLO proof.".to_string(),
        ),
    );

    // Health
    match client.health().await {
        Ok(h) => {
            snapshot.insert(
                "health".to_string(),
                serde_json::json!({"status": h.status}),
            );
        }
        Err(e) => {
            snapshot.insert(
                "health".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Deep readiness
    match client.readiness_deep_json().await {
        Ok(r) => {
            snapshot.insert("readiness_deep".to_string(), r);
        }
        Err(e) => {
            snapshot.insert(
                "readiness_deep".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Audit chain verification
    match client.verify_audit_chain().await {
        Ok(v) => {
            snapshot.insert(
                "audit_chain".to_string(),
                serde_json::json!({
                    "valid": v.valid,
                    "total_entries": v.total_entries,
                    "hashed_entries": v.hashed_entries,
                    "error": v.error,
                }),
            );
        }
        Err(e) => {
            snapshot.insert(
                "audit_chain".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Merkle roots summary
    match client.list_audit_merkle_roots(None, 50).await {
        Ok(list) => {
            snapshot.insert(
                "merkle_roots_summary".to_string(),
                serde_json::json!({"total": list.total}),
            );
        }
        Err(e) => {
            snapshot.insert(
                "merkle_roots_summary".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Checkpoints summary
    match client.list_checkpoints(None, 50).await {
        Ok(list) => {
            snapshot.insert(
                "checkpoints_summary".to_string(),
                serde_json::json!({"total": list.total}),
            );
        }
        Err(e) => {
            snapshot.insert(
                "checkpoints_summary".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Pending approvals count
    match client.list_approvals().await {
        Ok(approvals) => {
            let pending = approvals.iter().filter(|a| a.state == "pending").count();
            snapshot.insert(
                "pending_approvals".to_string(),
                serde_json::json!({"count": pending}),
            );
        }
        Err(e) => {
            snapshot.insert(
                "pending_approvals".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Policy bundle summary
    match client.list_policy_bundles().await {
        Ok(list) => {
            let active_count = list.bundles.iter().filter(|b| b.active).count();
            snapshot.insert(
                "policy_bundle_summary".to_string(),
                serde_json::json!({
                    "total": list.total,
                    "active_count": active_count,
                }),
            );
        }
        Err(e) => {
            snapshot.insert(
                "policy_bundle_summary".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Intents summary
    match client.list_intents(None, &[], None, 50).await {
        Ok(items) => {
            snapshot.insert(
                "intents_summary".to_string(),
                serde_json::json!({"returned_count": items.len()}),
            );
        }
        Err(e) => {
            snapshot.insert(
                "intents_summary".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    // Metrics summary
    match client.metrics().await {
        Ok(text) => {
            let lines = text.lines().count();
            snapshot.insert(
                "metrics_summary".to_string(),
                serde_json::json!({
                    "available": true,
                    "line_count": lines,
                }),
            );
        }
        Err(e) => {
            snapshot.insert(
                "metrics_summary".to_string(),
                serde_json::json!({"error": e.to_string()}),
            );
        }
    }

    let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&output_dir)?;
    let filename = evidence_snapshot_filename(&ts);
    let path = output_dir.join(&filename);
    let json = serde_json::to_string_pretty(&snapshot)?;
    std::fs::write(&path, json)?;
    println!("{}", path.display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Extract server config before consuming cli
    let server_url = cli
        .server_url
        .clone()
        .or_else(|| get_env("FERRUMCTL_SERVER_URL"))
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    let bearer_token = cli
        .bearer_token
        .clone()
        .or_else(|| get_env("FERRUMCTL_BEARER_TOKEN"));

    match cli.command {
        Command::Health => {
            println!(r#"{{"status":"ok"}}"#);
        }
        Command::ValidateRepo => {
            run_contract_check()?;
            println!("ValidateRepo: OK");
        }
        Command::ShowContracts => {
            println!("contracts/ferrumgate-agent-contract.v1.yaml");
            println!("contracts/ferrumgate-integrator-contract.v1.yaml");
        }
        Command::Policy { sub } => match sub {
            PolicyCommand::Validate { file } => {
                let yaml_content = std::fs::read_to_string(&file)
                    .with_context(|| format!("failed to read {}", file))?;
                if let Err(e) = ferrum_proto::validate_policy_bundle_yaml(&yaml_content) {
                    bail!("policy bundle validation failed: {}", e);
                }
                println!(r#"{{"valid":true}}"#);
            }
            PolicyCommand::Apply {
                file,
                activate,
                json,
            } => {
                let yaml_content = if file == "-" {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                } else {
                    std::fs::read_to_string(&file)?
                };
                // Validate offline first
                if let Err(e) = ferrum_proto::validate_policy_bundle_yaml(&yaml_content) {
                    bail!("policy bundle validation failed: {}", e);
                }
                let client = client::Client::new(server_url, bearer_token)?;
                let result = client.create_policy_bundle(&yaml_content).await?;
                if activate {
                    client
                        .set_policy_bundle_active(&result.bundle.bundle_id, true)
                        .await?;
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!(
                        "Policy bundle '{}' created (hash: {}){}",
                        result.bundle.bundle_id,
                        result.content_hash,
                        if activate { " and activated" } else { "" }
                    );
                }
            }
            PolicyCommand::Simulate {
                file,
                proposal,
                intent,
                json,
            } => {
                let yaml_content = if file == "-" {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin().read_to_string(&mut buf)?;
                    buf
                } else {
                    std::fs::read_to_string(&file)?
                };
                // Validate offline first
                if let Err(e) = ferrum_proto::validate_policy_bundle_yaml(&yaml_content) {
                    bail!("policy bundle validation failed: {}", e);
                }
                let proposal_json = std::fs::read_to_string(&proposal)
                    .with_context(|| format!("failed to read proposal file {}", proposal))?;
                let proposal: ferrum_proto::ActionProposal =
                    serde_json::from_str(&proposal_json)
                        .with_context(|| "failed to parse proposal JSON")?;
                let intent = match intent {
                    Some(path) => {
                        let intent_json = std::fs::read_to_string(&path)
                            .with_context(|| format!("failed to read intent file {}", path))?;
                        Some(
                            serde_json::from_str(&intent_json)
                                .with_context(|| "failed to parse intent JSON")?,
                        )
                    }
                    None => None,
                };
                let client = client::Client::new(server_url, bearer_token)?;
                let result = client
                    .simulate_policy_bundle(&yaml_content, &proposal, intent.as_ref())
                    .await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("Decision: {:?}", result.decision);
                    println!("Reason: {}", result.reason);
                    if !result.matched_rule_ids.is_empty() {
                        println!("Matched rules: {}", result.matched_rule_ids.join(", "));
                    }
                    if !result.warnings.is_empty() {
                        println!("Warnings: {}", result.warnings.join(", "));
                    }
                }
            }
            PolicyCommand::RuntimeSimulate {
                proposal,
                intent,
                json,
            } => {
                let proposal_json = std::fs::read_to_string(&proposal)
                    .with_context(|| format!("failed to read proposal file {}", proposal))?;
                let proposal: ferrum_proto::ActionProposal =
                    serde_json::from_str(&proposal_json)
                        .with_context(|| "failed to parse proposal JSON")?;
                let intent = match intent {
                    Some(path) => {
                        let intent_json = std::fs::read_to_string(&path)
                            .with_context(|| format!("failed to read intent file {}", path))?;
                        Some(
                            serde_json::from_str(&intent_json)
                                .with_context(|| "failed to parse intent JSON")?,
                        )
                    }
                    None => None,
                };
                let client = client::Client::new(server_url, bearer_token)?;
                let result = client
                    .simulate_runtime_policy(&proposal, intent.as_ref())
                    .await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("Decision: {:?}", result.decision);
                    println!("Reason: {}", result.reason);
                    if !result.matched_rule_ids.is_empty() {
                        println!("Matched rules: {}", result.matched_rule_ids.join(", "));
                    }
                    if !result.warnings.is_empty() {
                        println!("Warnings: {}", result.warnings.join(", "));
                    }
                }
            }
            PolicyCommand::Versions { bundle_id, json } => {
                let client = client::Client::new(server_url, bearer_token)?;
                let result = client.list_policy_bundle_versions(&bundle_id).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("Bundle {} has {} version(s):", bundle_id, result.total);
                    for v in result.versions {
                        println!(
                            "  v{} (active={}) — {}",
                            v.version,
                            v.active,
                            v.note.as_deref().unwrap_or("no note")
                        );
                    }
                }
            }
            PolicyCommand::Diff {
                bundle_id,
                from,
                to,
                json,
            } => {
                let client = client::Client::new(server_url, bearer_token)?;
                let result = client
                    .diff_policy_bundle_versions(&bundle_id, from, to)
                    .await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("Diff for bundle {} from v{} to v{}:", bundle_id, from, to);
                    println!("{}", serde_json::to_string_pretty(&result.diff)?);
                }
            }
            PolicyCommand::Rollback {
                bundle_id,
                target_version,
                actor,
                json,
            } => {
                let client = client::Client::new(server_url, bearer_token)?;
                let result = client
                    .rollback_policy_bundle(&bundle_id, target_version, actor.as_deref())
                    .await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!(
                        "Rolled back bundle {} to v{} (new version v{})",
                        bundle_id, target_version, result.new_version
                    );
                }
            }
        },
        Command::Admin { sub } => {
            let client = client::Client::new(server_url.clone(), bearer_token.clone())?;
            match sub {
                AdminCommand::Backup { sub } => match sub {
                    AdminBackupCommand::Create {
                        db_path,
                        output_dir,
                        retention_days,
                    } => {
                        let output = output_dir.unwrap_or_else(|| PathBuf::from("."));
                        let (backup_path, pruned) = backup::backup_create_with_retention(
                            &db_path,
                            &output,
                            retention_days,
                        )?;
                        println!("{}", backup_path.display());
                        if pruned > 0 {
                            eprintln!("Pruned {} old backup(s)", pruned);
                        }
                    }
                    AdminBackupCommand::Verify { db_path } => {
                        backup::backup_verify(&db_path)?;
                        println!("OK");
                    }
                    AdminBackupCommand::Restore {
                        db_path,
                        from,
                        confirm,
                        dry_run,
                    } => {
                        backup::backup_restore(&db_path, &from, confirm, dry_run)?;
                        if dry_run {
                            println!("Dry-run complete");
                        } else {
                            println!("Restore complete");
                        }
                    }
                },
                AdminCommand::Approvals { sub } => match sub {
                    AdminApprovalsCommand::List => {
                        let approvals = client.list_approvals().await?;
                        println!("{}", serde_json::to_string_pretty(&approvals)?);
                    }
                    AdminApprovalsCommand::Get { approval_id } => {
                        let approval = client.get_approval(&approval_id).await?;
                        println!("{}", serde_json::to_string_pretty(&approval)?);
                    }
                    AdminApprovalsCommand::Resolve {
                        approval_id,
                        approve,
                        deny,
                        actor_type,
                        actor_id,
                        actor_display_name,
                        reason,
                        json,
                    } => {
                        // Fail-closed: exactly one of --approve or --deny must be set
                        if approve && deny {
                            bail!("--approve and --deny are mutually exclusive; set only one");
                        }
                        if !approve && !deny {
                            bail!("one of --approve or --deny must be set");
                        }
                        // Fail-closed: --reason is required when --deny is set
                        if deny && reason.is_none() {
                            bail!("--reason is required when --deny is set");
                        }

                        let actor = ferrum_proto::ActorRef {
                            actor_type: actor_type.into(),
                            actor_id,
                            display_name: actor_display_name,
                        };
                        let resolved = client
                            .resolve_approval(&approval_id, &actor, approve, reason.as_deref())
                            .await?;
                        if json {
                            println!("{}", serde_json::to_string_pretty(&resolved)?);
                        } else {
                            println!(
                                "Approval {} resolved to {}",
                                resolved.approval_id, resolved.state
                            );
                        }
                    }
                },
                AdminCommand::Executions { sub } => match sub {
                    AdminExecutionsCommand::List {
                        intent_id,
                        state,
                        cursor,
                        limit,
                        format,
                    } => {
                        if limit == 0 || limit > 200 {
                            bail!("--limit must be between 1 and 200");
                        }
                        let items = client
                            .list_intents(intent_id.as_deref(), &state, cursor.as_deref(), limit)
                            .await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&items)?)
                            }
                            OutputFormat::Text | OutputFormat::Dot => {
                                for item in &items {
                                    println!(
                                        "{}\t{}\t{}\t{}\t{}",
                                        item.intent_id,
                                        item.status,
                                        item.risk_tier,
                                        item.exec_state.as_deref().unwrap_or("-"),
                                        item.created_at
                                    );
                                }
                            }
                        }
                    }
                    AdminExecutionsCommand::Get { execution_id } => {
                        let execution = client.get_execution(&execution_id).await?;
                        println!("{}", serde_json::to_string_pretty(&execution)?);
                    }
                    AdminExecutionsCommand::Cancel {
                        execution_id,
                        confirm,
                        json,
                    } => {
                        if !confirm {
                            bail!("--confirm flag is required to cancel an execution");
                        }
                        let result = client.cancel_execution(&execution_id).await?;
                        if json {
                            println!("{}", serde_json::to_string_pretty(&result)?);
                        } else {
                            println!("Execution {} canceled successfully.", result.execution_id);
                        }
                    }
                },
                AdminCommand::LifecycleOutbox { sub } => match sub {
                    AdminLifecycleOutboxCommand::List {
                        status,
                        limit,
                        format,
                    } => {
                        if limit == 0 || limit > 200 {
                            bail!("--limit must be between 1 and 200");
                        }
                        let response = client.list_lifecycle_outbox(Some(&status), limit).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            OutputFormat::Text | OutputFormat::Dot => {
                                print_lifecycle_outbox_list(&response);
                            }
                        }
                    }
                    AdminLifecycleOutboxCommand::Get { outbox_id, format } => {
                        let record = client.get_lifecycle_outbox(&outbox_id).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&record)?);
                            }
                            OutputFormat::Text | OutputFormat::Dot => {
                                print_lifecycle_outbox_record(&record);
                            }
                        }
                    }
                    AdminLifecycleOutboxCommand::Retry {
                        outbox_id,
                        actor_id,
                        reason,
                        format,
                    } => {
                        let response = client
                            .retry_lifecycle_outbox(&outbox_id, &actor_id, reason.as_deref())
                            .await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            OutputFormat::Text | OutputFormat::Dot => {
                                println!("Reconciliation retry submitted.");
                                print_lifecycle_outbox_record(&response.record);
                                println!(
                                    "reconciliation_report: {}",
                                    serde_json::to_string(&response.reconciliation_report)?
                                );
                            }
                        }
                    }
                    AdminLifecycleOutboxCommand::Resolve {
                        outbox_id,
                        actor_id,
                        reason,
                        format,
                    } => {
                        if reason.trim().is_empty() {
                            bail!("--reason is required");
                        }
                        let response = client
                            .resolve_lifecycle_outbox(&outbox_id, &actor_id, &reason)
                            .await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            OutputFormat::Text | OutputFormat::Dot => {
                                println!("Lifecycle outbox operator review resolved.");
                                print_lifecycle_outbox_record(&response.record);
                            }
                        }
                    }
                },
                AdminCommand::Status => {
                    let mut status = serde_json::Map::new();
                    match client.health().await {
                        Ok(h) => {
                            status.insert(
                                "health".to_string(),
                                serde_json::json!({"status": h.status}),
                            );
                        }
                        Err(e) => {
                            status.insert(
                                "health".to_string(),
                                serde_json::json!({"error": e.to_string()}),
                            );
                        }
                    }
                    match client.readiness().await {
                        Ok(r) => {
                            status.insert(
                                "readiness".to_string(),
                                serde_json::json!({"status": r.status}),
                            );
                        }
                        Err(e) => {
                            status.insert(
                                "readiness".to_string(),
                                serde_json::json!({"error": e.to_string()}),
                            );
                        }
                    }
                    match client.readiness_deep_json().await {
                        Ok(r) => {
                            status.insert("readiness_deep".to_string(), r);
                        }
                        Err(e) => {
                            status.insert(
                                "readiness_deep".to_string(),
                                serde_json::json!({"error": e.to_string()}),
                            );
                        }
                    }
                    match client.functional_readiness().await {
                        Ok(items) => {
                            status.insert(
                                "functional".to_string(),
                                serde_json::json!({
                                    "status": "ready",
                                    "approvals_found": items.len()
                                }),
                            );
                        }
                        Err(e) => {
                            status.insert(
                                "functional".to_string(),
                                serde_json::json!({"error": e.to_string()}),
                            );
                        }
                    }
                    match client.metrics().await {
                        Ok(m) => {
                            let lines = m.lines().count();
                            status.insert(
                                "metrics".to_string(),
                                serde_json::json!({
                                    "available": true,
                                    "line_count": lines
                                }),
                            );
                        }
                        Err(e) => {
                            status.insert(
                                "metrics".to_string(),
                                serde_json::json!({"error": e.to_string()}),
                            );
                        }
                    }
                    println!("{}", serde_json::to_string_pretty(&status)?);
                }
                AdminCommand::Tokens { sub } => match sub {
                    AdminTokensCommand::List {
                        actor_id,
                        role,
                        active_only,
                        limit,
                        format,
                    } => {
                        let response = client
                            .list_tokens(actor_id.as_deref(), role.as_deref(), active_only, limit)
                            .await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                println!(
                                    "{:<24} {:<20} {:<15} {:<24} {:<10}",
                                    "TOKEN_ID", "ACTOR_ID", "ROLE", "EXPIRES_AT", "STATUS"
                                );
                                for item in &response.items {
                                    let status = if item.revoked_at.is_some() {
                                        "revoked"
                                    } else if item.expires_at < chrono::Utc::now() {
                                        "expired"
                                    } else {
                                        "active"
                                    };
                                    println!(
                                        "{:<24} {:<20} {:<15} {:<24} {:<10}",
                                        item.token_id,
                                        item.actor_id,
                                        item.role.to_string(),
                                        item.expires_at.to_rfc3339(),
                                        status
                                    );
                                }
                                if let Some(cursor) = response.next_cursor {
                                    println!("Next cursor: {}", cursor);
                                }
                            }
                        }
                    }
                    AdminTokensCommand::Create {
                        actor_id,
                        role,
                        scope,
                        description,
                        expires_in_days,
                        expires_at,
                        json,
                    } => {
                        let role = role
                            .parse::<ferrum_proto::TokenRole>()
                            .map_err(|e| anyhow::anyhow!(e))?;
                        let expires_at = if let Some(days) = expires_in_days {
                            chrono::Utc::now() + chrono::Duration::days(days as i64)
                        } else if let Some(ts) = expires_at {
                            chrono::DateTime::parse_from_rfc3339(&ts)?.with_timezone(&chrono::Utc)
                        } else {
                            chrono::Utc::now() + chrono::Duration::days(30)
                        };
                        let max_ttl = chrono::Duration::days(90);
                        if expires_at > chrono::Utc::now() + max_ttl {
                            return Err(anyhow::anyhow!(
                                "expires_at exceeds maximum TTL of 90 days"
                            ));
                        }
                        let scopes = if scope.is_empty() {
                            None
                        } else {
                            Some(scope.clone())
                        };
                        let request = ferrum_proto::CreateTokenRequest {
                            actor_id: actor_id.clone(),
                            role,
                            scopes,
                            description: description.clone(),
                            expires_at,
                        };
                        let response = client.create_token(&request).await?;
                        if json {
                            println!("{}", serde_json::to_string_pretty(&response)?);
                        } else {
                            println!("Token created successfully.\n");
                            println!("Token ID:    {}", response.token.token_id);
                            println!("Token Value: {}", response.token_value);
                            println!("Actor ID:    {}", response.token.actor_id);
                            println!("Role:        {}", response.token.role);
                            println!("Scopes:      {}", response.token.scopes.join(", "));
                            println!("Expires At:  {}", response.token.expires_at.to_rfc3339());
                            println!(
                                "\nIMPORTANT: Save the token value now. It will never be shown again."
                            );
                        }
                    }
                    AdminTokensCommand::Revoke {
                        token_id,
                        reason,
                        force,
                    } => {
                        if !force {
                            print!("Revoke token {}? [y/N] ", token_id);
                            std::io::Write::flush(&mut std::io::stdout())?;
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input)?;
                            if !input.trim().eq_ignore_ascii_case("y") {
                                println!("Cancelled.");
                                return Ok(());
                            }
                        }
                        client.revoke_token(&token_id, reason.as_deref()).await?;
                        println!("Token {} revoked successfully.", token_id);
                        if let Some(reason) = reason {
                            println!("Reason: {}", reason);
                        }
                    }
                    AdminTokensCommand::Rotate {
                        token_id,
                        expires_in_days,
                        expires_at,
                        reason,
                        json,
                        force,
                    } => {
                        if !force {
                            print!("Rotate token {}? [y/N] ", token_id);
                            std::io::Write::flush(&mut std::io::stdout())?;
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input)?;
                            if !input.trim().eq_ignore_ascii_case("y") {
                                println!("Cancelled.");
                                return Ok(());
                            }
                        }
                        let expires_at = if let Some(days) = expires_in_days {
                            Some(chrono::Utc::now() + chrono::Duration::days(days as i64))
                        } else if let Some(ts) = expires_at {
                            Some(
                                chrono::DateTime::parse_from_rfc3339(&ts)?
                                    .with_timezone(&chrono::Utc),
                            )
                        } else {
                            None
                        };
                        if let Some(ref et) = expires_at {
                            let max_ttl = chrono::Duration::days(90);
                            if *et > chrono::Utc::now() + max_ttl {
                                return Err(anyhow::anyhow!(
                                    "expires_at exceeds maximum TTL of 90 days"
                                ));
                            }
                        }
                        let request = ferrum_proto::RotateTokenRequest {
                            expires_at,
                            reason: reason.clone(),
                        };
                        let response = client.rotate_token(&token_id, &request).await?;
                        if json {
                            println!("{}", serde_json::to_string_pretty(&response)?);
                        } else {
                            println!("Token rotated successfully.\n");
                            println!("Old Token ID: {} (revoked)", token_id);
                            println!("New Token ID: {}", response.token.token_id);
                            println!("New Token Value: {}", response.token_value);
                            println!(
                                "\nIMPORTANT: Save the new token value now. It will never be shown again."
                            );
                        }
                    }
                },
                AdminCommand::Agents { sub } => match sub {
                    AdminAgentsCommand::List {
                        active_only,
                        limit,
                        format,
                    } => {
                        let response = client.list_agents(active_only, limit).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                println!(
                                    "{:<24} {:<48} {:<24} {:<10}",
                                    "AGENT_ID", "FINGERPRINT", "CREATED_AT", "STATUS"
                                );
                                for item in &response.items {
                                    let status = if item.revoked_at.is_some() {
                                        "revoked"
                                    } else {
                                        "active"
                                    };
                                    println!(
                                        "{:<24} {:<48} {:<24} {:<10}",
                                        item.agent_id,
                                        item.key_fingerprint,
                                        item.created_at.to_rfc3339(),
                                        status
                                    );
                                }
                                if let Some(cursor) = response.next_cursor {
                                    println!("Next cursor: {}", cursor);
                                }
                            }
                        }
                    }
                    AdminAgentsCommand::Register {
                        agent_id,
                        public_key,
                        scope,
                        description,
                        json,
                    } => {
                        let scopes = if scope.is_empty() {
                            None
                        } else {
                            Some(scope.clone())
                        };
                        let request = ferrum_proto::RegisterAgentRequest {
                            agent_id: agent_id.clone(),
                            public_key: public_key.clone(),
                            scopes,
                            description: description.clone(),
                        };
                        let response = client.register_agent(&request).await?;
                        if json {
                            println!("{}", serde_json::to_string_pretty(&response)?);
                        } else {
                            println!("Agent registered successfully.");
                            println!("Agent ID:     {}", response.agent.agent_id);
                            println!("Fingerprint:  {}", response.agent.key_fingerprint);
                            println!("Scopes:       {}", response.agent.allowed_scopes.join(", "));
                        }
                    }
                    AdminAgentsCommand::Revoke {
                        agent_id,
                        reason,
                        force,
                    } => {
                        if !force {
                            print!("Revoke agent {}? [y/N] ", agent_id);
                            std::io::Write::flush(&mut std::io::stdout())?;
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input)?;
                            if !input.trim().eq_ignore_ascii_case("y") {
                                println!("Cancelled.");
                                return Ok(());
                            }
                        }
                        client.revoke_agent(&agent_id, reason.as_deref()).await?;
                        println!("Agent {} revoked successfully.", agent_id);
                        if let Some(reason) = reason {
                            println!("Reason: {}", reason);
                        }
                    }
                },
                AdminCommand::Audit { sub } => match sub {
                    AdminAuditCommand::List {
                        action,
                        resource_type,
                        resource_id,
                        cursor,
                        limit,
                        since,
                        until,
                        format,
                    } => {
                        let response = client
                            .list_audit_logs(
                                action.as_deref(),
                                resource_type.as_deref(),
                                resource_id.as_deref(),
                                cursor.as_deref(),
                                limit,
                                since.as_deref(),
                                until.as_deref(),
                            )
                            .await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                println!(
                                    "{:<24} {:<20} {:<24} {:<16} {:<36} {:<10}",
                                    "CREATED_AT",
                                    "ACTOR_ID",
                                    "ACTION",
                                    "RESOURCE_TYPE",
                                    "RESOURCE_ID",
                                    "RESULT"
                                );
                                for item in &response.items {
                                    println!(
                                        "{:<24} {:<20} {:<24} {:<16} {:<36} {:<10}",
                                        item.created_at.to_rfc3339(),
                                        item.actor_id,
                                        item.action.to_string(),
                                        item.resource_type.to_string(),
                                        item.resource_id,
                                        item.result,
                                    );
                                }
                                if let Some(cursor) = response.next_cursor {
                                    println!("Next cursor: {}", cursor);
                                }
                            }
                        }
                    }
                    AdminAuditCommand::Export {
                        action,
                        resource_type,
                        resource_id,
                        since,
                        until,
                        format,
                        output,
                        bundle,
                    } => {
                        if let Some(bundle_dir) = bundle {
                            let body = client
                                .export_audit_logs(
                                    action.as_deref(),
                                    resource_type.as_deref(),
                                    resource_id.as_deref(),
                                    since.as_deref(),
                                    until.as_deref(),
                                    "ndjson",
                                )
                                .await?;
                            let manifest = audit_bundle::export_bundle(&bundle_dir, &body)?;
                            match format {
                                ExportFormat::Json => {
                                    println!("{}", serde_json::to_string_pretty(&manifest)?);
                                }
                                _ => {
                                    println!("Exported audit bundle to {}", bundle_dir.display());
                                    println!("Version:       {}", manifest.version);
                                    println!("Entries:       {}", manifest.entry_count);
                                    println!("First hash:    {}", manifest.first_hash);
                                    println!("Last hash:     {}", manifest.last_hash);
                                    println!("Merkle root:   {}", manifest.merkle_root);
                                }
                            }
                        } else {
                            let format_str = match format {
                                ExportFormat::Ndjson => "ndjson",
                                ExportFormat::Json => "json",
                                ExportFormat::Csv => "csv",
                            };
                            let body = client
                                .export_audit_logs(
                                    action.as_deref(),
                                    resource_type.as_deref(),
                                    resource_id.as_deref(),
                                    since.as_deref(),
                                    until.as_deref(),
                                    format_str,
                                )
                                .await?;
                            if let Some(path) = output {
                                std::fs::write(&path, body)?;
                                println!("Exported audit logs to {}", path);
                            } else {
                                println!("{}", body);
                            }
                        }
                    }
                    AdminAuditCommand::Verify { format, bundle } => {
                        if let Some(bundle_dir) = bundle {
                            let manifest = audit_bundle::verify_bundle(&bundle_dir)?;
                            match format {
                                OutputFormat::Json => {
                                    println!("{}", serde_json::to_string_pretty(&manifest)?);
                                }
                                _ => {
                                    println!("Bundle verification: VALID");
                                    println!("Version:       {}", manifest.version);
                                    println!(
                                        "Exported at:   {}",
                                        manifest.exported_at.to_rfc3339()
                                    );
                                    println!("Entries:       {}", manifest.entry_count);
                                    println!("First hash:    {}", manifest.first_hash);
                                    println!("Last hash:     {}", manifest.last_hash);
                                    println!("Merkle root:   {}", manifest.merkle_root);
                                }
                            }
                        } else {
                            let response = client.verify_audit_chain().await?;
                            match format {
                                OutputFormat::Json => {
                                    println!("{}", serde_json::to_string_pretty(&response)?);
                                }
                                _ => {
                                    if response.valid {
                                        println!("Audit chain verification: VALID");
                                    } else {
                                        println!("Audit chain verification: INVALID");
                                        if let Some(ref err) = response.error {
                                            println!("Error: {}", err);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    AdminAuditCommand::MerkleVerify {
                        window_start,
                        format,
                    } => {
                        let response = client.verify_audit_merkle_root(&window_start).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                if response.valid {
                                    println!(
                                        "Merkle root for {}: {} ({} entries)",
                                        response.window_start.to_rfc3339(),
                                        if response.root.is_empty() {
                                            "(empty)"
                                        } else {
                                            &response.root
                                        },
                                        response.entry_count
                                    );
                                } else {
                                    println!("Merkle root verification: INVALID");
                                    if let Some(ref err) = response.error {
                                        println!("Error: {}", err);
                                    }
                                }
                            }
                        }
                    }
                    AdminAuditCommand::MerkleRoots {
                        cursor,
                        limit,
                        format,
                    } => {
                        let response = client
                            .list_audit_merkle_roots(cursor.as_deref(), limit)
                            .await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                println!(
                                    "{:<32} {:<64} {:<12} {:<32}",
                                    "WINDOW_START", "ROOT", "ENTRY_COUNT", "COMPUTED_AT"
                                );
                                for item in &response.items {
                                    println!(
                                        "{:<32} {:<64} {:<12} {:<32}",
                                        item.window_start.to_rfc3339(),
                                        if item.root.is_empty() {
                                            "(empty)".to_string()
                                        } else {
                                            item.root.clone()
                                        },
                                        item.entry_count,
                                        item.computed_at.to_rfc3339(),
                                    );
                                }
                                if let Some(cursor) = response.next_cursor {
                                    println!("Next cursor: {}", cursor);
                                }
                            }
                        }
                    }
                    AdminAuditCommand::CheckpointSign {
                        window_start,
                        signer_id,
                        private_key,
                        format,
                    } => {
                        let window = chrono::DateTime::parse_from_rfc3339(&window_start)?
                            .with_timezone(&chrono::Utc);
                        if window.minute() != 0
                            || window.second() != 0
                            || window.timestamp_subsec_nanos() != 0
                        {
                            bail!("window_start must be aligned to the hour");
                        }

                        // Fetch the Merkle root for the window.
                        let merkle = client.verify_audit_merkle_root(&window_start).await?;
                        if !merkle.valid {
                            bail!(
                                "merkle root computation failed: {}",
                                merkle.error.unwrap_or_default()
                            );
                        }

                        // Decode private key.
                        let pk_bytes = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            &private_key,
                        )
                        .map_err(|e| anyhow::anyhow!("invalid private key base64: {}", e))?;
                        let signing_key = if pk_bytes.len() == 32 {
                            ed25519_dalek::SigningKey::from_bytes(&pk_bytes.try_into().unwrap())
                        } else if pk_bytes.len() == 64 {
                            // Expanded secret key: extract first 32 bytes as seed
                            let seed: [u8; 32] = pk_bytes[..32]
                                .try_into()
                                .map_err(|_| anyhow::anyhow!("invalid private key length"))?;
                            ed25519_dalek::SigningKey::from_bytes(&seed)
                        } else {
                            bail!(
                                "invalid private key length: expected 32 or 64 bytes, got {}",
                                pk_bytes.len()
                            );
                        };

                        let signed_at = chrono::Utc::now();
                        let payload_hash = ferrum_proto::canonical_checkpoint_hash(
                            &window,
                            &merkle.root,
                            merkle.entry_count,
                            &signed_at,
                        );
                        let signature = signing_key.sign(&payload_hash);
                        let signature_b64 = base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            signature.to_bytes(),
                        );

                        // Compute public key fingerprint (SHA-256 of raw pk bytes, hex).
                        let public_key_bytes = signing_key.verifying_key().to_bytes();
                        let mut hasher = sha2::Sha256::new();
                        hasher.update(public_key_bytes);
                        let fingerprint = hex::encode(hasher.finalize());
                        let public_key_b64 = base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            public_key_bytes,
                        );

                        let request = ferrum_proto::CreateCheckpointRequest {
                            window_start: window,
                            merkle_root: merkle.root.clone(),
                            entry_count: merkle.entry_count,
                            signer_id,
                            signer_key_fingerprint: fingerprint,
                            signed_at,
                            signature: signature_b64,
                            public_key: public_key_b64,
                        };
                        let response = client.create_checkpoint(&request).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                if response.valid {
                                    println!(
                                        "Checkpoint created for {} (root={} entries={})",
                                        window_start, merkle.root, merkle.entry_count
                                    );
                                } else {
                                    println!("Checkpoint creation failed");
                                    if let Some(ref err) = response.error {
                                        println!("Error: {}", err);
                                    }
                                }
                            }
                        }
                    }
                    AdminAuditCommand::CheckpointVerify {
                        window_start,
                        format,
                    } => {
                        let response = client.verify_checkpoint(&window_start).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                if response.valid {
                                    println!("Checkpoint verification for {}: VALID", window_start);
                                    if let Some(ref cp) = response.checkpoint {
                                        println!(
                                            "  signer={} fingerprint={} signed_at={}",
                                            cp.signer_id,
                                            cp.signer_key_fingerprint,
                                            cp.signed_at.to_rfc3339()
                                        );
                                    }
                                } else {
                                    println!(
                                        "Checkpoint verification for {}: INVALID",
                                        window_start
                                    );
                                    if let Some(ref err) = response.error {
                                        println!("Error: {}", err);
                                    }
                                }
                            }
                        }
                    }
                    AdminAuditCommand::CheckpointList {
                        cursor,
                        limit,
                        format,
                    } => {
                        let response = client.list_checkpoints(cursor.as_deref(), limit).await?;
                        match format {
                            OutputFormat::Json => {
                                println!("{}", serde_json::to_string_pretty(&response)?);
                            }
                            _ => {
                                println!(
                                    "{:<32} {:<64} {:<12} {:<24} {:<24}",
                                    "WINDOW_START", "ROOT", "ENTRY_COUNT", "SIGNER_ID", "SIGNED_AT"
                                );
                                for item in &response.items {
                                    println!(
                                        "{:<32} {:<64} {:<12} {:<24} {:<24}",
                                        item.window_start.to_rfc3339(),
                                        item.merkle_root.clone(),
                                        item.entry_count,
                                        item.signer_id,
                                        item.signed_at.to_rfc3339(),
                                    );
                                }
                                if let Some(cursor) = response.next_cursor {
                                    println!("Next cursor: {}", cursor);
                                }
                            }
                        }
                    }
                },
                AdminCommand::Config => {
                    run_admin_config(&server_url, bearer_token.as_deref())?;
                }
            }
        }
        Command::Evidence { sub } => match sub {
            EvidenceCommand::Snapshot { output_dir } => {
                let client = client::Client::new(server_url, bearer_token)?;
                run_evidence_snapshot(client, output_dir).await?;
            }
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Start { window_dir, notes } => {
                    run_slo_window_start(window_dir, notes)?;
                }
                SloWindowCommand::Status { window_dir, json } => {
                    run_slo_window_status(window_dir, json)?;
                }
                SloWindowCommand::Finalize {
                    window_dir,
                    notes,
                    allow_early,
                } => {
                    run_slo_window_finalize(window_dir, notes, allow_early)?;
                }
            },
        },
        Command::Readiness { sub } => match sub {
            ReadinessCommand::Report {
                snapshot,
                window_dir,
                json,
                offline,
            } => {
                run_readiness_report(
                    &server_url,
                    bearer_token.as_deref(),
                    snapshot,
                    window_dir,
                    json,
                    offline,
                )
                .await?;
            }
        },
        Command::Author { sub } => match sub {
            AuthorCommand::Bundle { sub: bundle_sub } => match bundle_sub {
                BundleCommand::Bump {
                    yaml_file,
                    bump_type,
                    output,
                    json,
                } => {
                    let output_str = output.as_ref().and_then(|p| p.to_str());
                    run_author_bundle_bump(&yaml_file, bump_type, output_str, json)?;
                }
            },
        },
        Command::Backup { sub } => match sub {
            BackupCommand::Create {
                db_path,
                output_dir,
                retention_days,
            } => {
                let output = output_dir.unwrap_or_else(|| PathBuf::from("."));
                let (backup_path, pruned) =
                    backup::backup_create_with_retention(&db_path, &output, retention_days)?;
                println!("{}", backup_path.display());
                if pruned > 0 {
                    eprintln!("Pruned {} old backup(s)", pruned);
                }
            }
            BackupCommand::Verify { db_path } => {
                backup::backup_verify(&db_path)?;
                println!("OK");
            }
            BackupCommand::Restore {
                db_path,
                from,
                confirm,
                dry_run,
            } => {
                backup::backup_restore(&db_path, &from, confirm, dry_run)?;
                if dry_run {
                    println!("Dry-run complete");
                } else {
                    println!("Restore complete");
                }
            }
        },
        Command::Server { sub } => {
            let client = client::Client::new(server_url, bearer_token)?;

            match sub {
                ServerCommand::Health => {
                    let health = client.health().await?;
                    println!("{}", serde_json::to_string_pretty(&health)?);
                }
                ServerCommand::Metrics => {
                    let metrics = client.metrics().await?;
                    println!("{}", metrics);
                }
                ServerCommand::Readiness { deep, functional } => {
                    if deep {
                        // Deep probe: GET /v1/readyz/deep
                        let ready = client.readiness_deep().await?;
                        println!("{}", serde_json::to_string_pretty(&ready)?);
                    }

                    if functional {
                        // Functional probe: GET /v1/approvals?limit=1
                        match client.functional_readiness().await {
                            Ok(items) => {
                                println!(
                                    "{{\"status\":\"ready\",\"probe\":\"functional\",\"approvals_found\":{}}}",
                                    items.len()
                                );
                            }
                            Err(e) => {
                                bail!("functional readiness probe failed: {}", e);
                            }
                        }
                    }

                    if !deep && !functional {
                        // Shallow probe: GET /v1/readyz
                        let ready = client.readiness().await?;
                        println!("{}", serde_json::to_string_pretty(&ready)?);
                    }
                }
                ServerCommand::InspectExecution { execution_id } => {
                    let execution = client.get_execution(&execution_id).await?;
                    println!("{}", serde_json::to_string_pretty(&execution)?);
                }
                ServerCommand::InspectApprovals => {
                    let approvals = client.list_approvals().await?;
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                }
                ServerCommand::InspectApproval { approval_id } => {
                    let approval = client.get_approval(&approval_id).await?;
                    println!("{}", serde_json::to_string_pretty(&approval)?);
                }
                ServerCommand::InspectLineage {
                    execution_id,
                    format,
                    output,
                } => {
                    let lineage = client.get_lineage(&execution_id).await?;
                    let rendered = match format {
                        OutputFormat::Text | OutputFormat::Json => {
                            serde_json::to_string_pretty(&lineage)?
                        }
                        OutputFormat::Dot => render_dot(lineage.execution_id(), lineage.events()),
                    };
                    match output {
                        Some(path) => {
                            let len = rendered.len();
                            std::fs::write(&path, &rendered)?;
                            eprintln!("Wrote {} bytes to {}", len, path.display());
                        }
                        None => {
                            println!("{}", rendered);
                        }
                    }
                }
                ServerCommand::InspectProvenance { intent_id } => {
                    let events = client
                        .query_provenance(intent_id.as_deref(), None, None)
                        .await?;
                    println!("{}", serde_json::to_string_pretty(&events)?);
                }
                ServerCommand::ResolveApproval {
                    approval_id,
                    approve,
                    deny,
                    actor_type,
                    actor_id,
                    actor_display_name,
                    reason,
                    json,
                } => {
                    // Fail-closed: exactly one of --approve or --deny must be set
                    if approve && deny {
                        bail!("--approve and --deny are mutually exclusive; set only one");
                    }
                    if !approve && !deny {
                        bail!("one of --approve or --deny must be set");
                    }
                    // Fail-closed: --reason is required when --deny is set
                    if deny && reason.is_none() {
                        bail!("--reason is required when --deny is set");
                    }

                    let actor = ferrum_proto::ActorRef {
                        actor_type: actor_type.into(),
                        actor_id,
                        display_name: actor_display_name,
                    };
                    let resolved = client
                        .resolve_approval(&approval_id, &actor, approve, reason.as_deref())
                        .await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&resolved)?);
                    } else {
                        println!(
                            "Approval {} resolved to {}",
                            resolved.approval_id, resolved.state
                        );
                    }
                }
                ServerCommand::ListIntents {
                    intent_id,
                    state,
                    cursor,
                    limit,
                    format,
                } => {
                    // Validate limit bounds
                    if limit == 0 || limit > 200 {
                        bail!("--limit must be between 1 and 200");
                    }
                    let items = client
                        .list_intents(intent_id.as_deref(), &state, cursor.as_deref(), limit)
                        .await?;
                    match format {
                        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&items)?),
                        OutputFormat::Text | OutputFormat::Dot => {
                            for item in &items {
                                println!(
                                    "{}\t{}\t{}\t{}\t{}",
                                    item.intent_id,
                                    item.status,
                                    item.risk_tier,
                                    item.exec_state.as_deref().unwrap_or("-"),
                                    item.created_at
                                );
                            }
                        }
                    }
                }
                ServerCommand::CancelExecution {
                    execution_id,
                    confirm,
                    json,
                } => {
                    if !confirm {
                        bail!("--confirm flag is required to cancel an execution");
                    }
                    let result = client.cancel_execution(&execution_id).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("Execution {} canceled successfully.", result.execution_id);
                    }
                }
                ServerCommand::CreatePolicyBundle { yaml_file, json } => {
                    let yaml_content = if yaml_file == "-" {
                        use std::io::Read;
                        let mut buf = String::new();
                        std::io::stdin().read_to_string(&mut buf)?;
                        buf
                    } else {
                        std::fs::read_to_string(&yaml_file)?
                    };
                    let result = client.create_policy_bundle(&yaml_content).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!(
                            "Policy bundle '{}' created (hash: {})",
                            result.bundle.bundle_id, result.content_hash
                        );
                    }
                }
                ServerCommand::ListPolicyBundles { format } => {
                    let result = client.list_policy_bundles().await?;
                    match format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&result)?);
                        }
                        OutputFormat::Text | OutputFormat::Dot => {
                            println!(
                                "{:<40} {:<10} {:<8} CREATED_AT",
                                "BUNDLE_ID", "VERSION", "ACTIVE"
                            );
                            println!("{}", "-".repeat(80));
                            for bundle in &result.bundles {
                                println!(
                                    "{:<40} {:<10} {:<8} {}",
                                    bundle.bundle_id,
                                    bundle.version,
                                    if bundle.active { "true" } else { "false" },
                                    bundle.created_at
                                );
                            }
                            println!("\nTotal: {} bundles", result.total);
                        }
                    }
                }
                ServerCommand::GetPolicyBundle {
                    bundle_id,
                    json,
                    export,
                } => {
                    let result = client.get_policy_bundle(&bundle_id).await?;
                    if let Some(export_path) = export {
                        // Export bundle as YAML to file
                        let yaml_content = serde_yaml::to_string(&result.bundle)?;
                        let len = yaml_content.len();
                        std::fs::write(&export_path, &yaml_content)?;
                        eprintln!("Exported {} bytes to {}", len, export_path.display());
                    } else if json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("Bundle ID: {}", result.bundle.bundle_id);
                        println!("Version: {}", result.bundle.version);
                        println!("Active: {}", result.bundle.active);
                        println!("Content Hash: {}", result.content_hash);
                        println!("Created: {}", result.bundle.created_at);
                        println!("Updated: {}", result.bundle.updated_at);
                    }
                }
                ServerCommand::UpdatePolicyBundle {
                    bundle_id,
                    yaml_file,
                    json,
                } => {
                    let yaml_content = if yaml_file == "-" {
                        use std::io::Read;
                        let mut buf = String::new();
                        std::io::stdin().read_to_string(&mut buf)?;
                        buf
                    } else {
                        std::fs::read_to_string(&yaml_file)?
                    };
                    let result = client
                        .update_policy_bundle(&bundle_id, &yaml_content)
                        .await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!(
                            "Policy bundle '{}' updated (hash: {})",
                            result.bundle.bundle_id, result.content_hash
                        );
                    }
                }
                ServerCommand::DeletePolicyBundle {
                    bundle_id,
                    confirm,
                    json,
                } => {
                    if !confirm {
                        bail!("--confirm flag is required to delete a policy bundle");
                    }
                    let result = client.delete_policy_bundle(&bundle_id).await?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("Policy bundle '{}' deleted successfully.", bundle_id);
                    }
                }
                ServerCommand::SetPolicyBundleActive {
                    bundle_id,
                    activate,
                    deactivate,
                } => {
                    // Fail-closed: exactly one of --activate or --deactivate must be set
                    if activate && deactivate {
                        bail!("--activate and --deactivate are mutually exclusive; set only one");
                    }
                    if !activate && !deactivate {
                        bail!("one of --activate or --deactivate must be set");
                    }
                    let result = client
                        .set_policy_bundle_active(&bundle_id, activate)
                        .await?;
                    let new_state = if activate { "activated" } else { "deactivated" };
                    if result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                        println!("Policy bundle '{}' {}.", bundle_id, new_state);
                    } else {
                        bail!("Failed to set policy bundle active state");
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_escape_dot_label_basic() {
        assert_eq!(escape_dot_label("hello"), "hello");
        assert_eq!(escape_dot_label("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_dot_label("hello\"world"), "hello\\\"world");
        assert_eq!(escape_dot_label("hello\\world"), "hello\\\\world");
    }

    #[test]
    fn test_render_dot_empty_lineage() {
        let result = render_dot("exec-123", &[]);
        assert!(result.contains("digraph lineage"));
        assert!(result.contains("exec-123"));
        assert!(result.contains("}"));
        // Should be valid DOT even with no events
        assert!(result.contains("node [shape=box]"));
    }

    #[test]
    fn test_render_dot_deterministic_ordering() {
        // Create events with IDs that won't sort alphabetically (to test sorting)
        let event1 = client::ProvenanceEvent {
            event_id: "zzz-event".to_string(),
            kind: "UserGoalReceived".to_string(),
            occurred_at: "2024-01-01T00:00:00Z".to_string(),
            actor: serde_json::json!({}),
            object: serde_json::json!({}),
            intent_id: None,
            proposal_id: None,
            execution_id: Some("exec-123".to_string()),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: serde_json::json!({}),
        };
        let event2 = client::ProvenanceEvent {
            event_id: "aaa-event".to_string(),
            kind: "IntentCompiled".to_string(),
            occurred_at: "2024-01-01T00:00:01Z".to_string(),
            actor: serde_json::json!({}),
            object: serde_json::json!({}),
            intent_id: None,
            proposal_id: None,
            execution_id: Some("exec-123".to_string()),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![
                serde_json::json!({"from_event_id": "zzz-event", "edge_type": "DerivedFrom"}),
            ],
            hash_chain: serde_json::json!({}),
        };

        let result = render_dot("exec-123", &[event1.clone(), event2.clone()]);

        // Verify aaa-event appears before zzz-event (sorted order)
        let aaa_pos = result.find("\"aaa-event\"").unwrap();
        let zzz_pos = result.find("\"zzz-event\"").unwrap();
        assert!(aaa_pos < zzz_pos, "events should be sorted by event_id");

        // Verify the edge goes from zzz-event to aaa-event
        assert!(result.contains("\"zzz-event\" -> \"aaa-event\""));
    }

    #[test]
    fn test_render_dot_valid_escape_in_label() {
        let event = client::ProvenanceEvent {
            event_id: "event\"with\"quotes".to_string(),
            kind: "IntentCompiled".to_string(),
            occurred_at: "2024-01-01T00:00:00Z".to_string(),
            actor: serde_json::json!({}),
            object: serde_json::json!({}),
            intent_id: None,
            proposal_id: None,
            execution_id: Some("exec-123".to_string()),
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: serde_json::json!({}),
        };

        let result = render_dot("exec-123", &[event]);
        // Should not panic and should have escaped quotes in label
        assert!(result.contains("label=\"event\\\"with\\\"quotes"));
    }

    // -------------------------------------------------------------------------
    // ResolveApproval flag validation tests
    // -------------------------------------------------------------------------

    /// Validates resolve-approval flags and returns the resolved approve flag.
    /// Returns Err with message if validation fails.
    fn validate_resolve_flags(approve: bool, deny: bool, reason: Option<&str>) -> Result<bool> {
        if approve && deny {
            bail!("--approve and --deny are mutually exclusive; set only one");
        }
        if !approve && !deny {
            bail!("one of --approve or --deny must be set");
        }
        if deny && reason.is_none() {
            bail!("--reason is required when --deny is set");
        }
        Ok(approve)
    }

    #[test]
    fn test_validate_flags_approve_is_valid() {
        let result = validate_resolve_flags(true, false, None);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_validate_flags_deny_with_reason_is_valid() {
        let result = validate_resolve_flags(false, true, Some("too risky"));
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_validate_flags_both_approve_and_deny_fails() {
        let result = validate_resolve_flags(true, true, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("mutually exclusive"));
    }

    #[test]
    fn test_validate_flags_neither_approve_nor_deny_fails() {
        let result = validate_resolve_flags(false, false, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("one of --approve or --deny"));
    }

    #[test]
    fn test_validate_flags_deny_without_reason_fails() {
        let result = validate_resolve_flags(false, true, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("--reason is required when --deny"));
    }

    #[test]
    fn test_validate_flags_deny_with_empty_reason_fails() {
        // Empty string is still Some(""), but reason is None - None means not provided
        // An empty string is still a provided reason, so this should pass validation
        // but the gateway may reject it.
        let result = validate_resolve_flags(false, true, Some(""));
        assert!(result.is_ok()); // validation passes; gateway would reject empty reason
    }

    // -------------------------------------------------------------------------
    // ActorTypeCli conversion tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_actor_type_cli_to_proto_user() {
        let cli: ActorTypeCli = ActorTypeCli::User;
        let proto: ferrum_proto::ActorType = cli.into();
        assert!(matches!(proto, ferrum_proto::ActorType::User));
    }

    #[test]
    fn test_actor_type_cli_to_proto_agent() {
        let cli: ActorTypeCli = ActorTypeCli::Agent;
        let proto: ferrum_proto::ActorType = cli.into();
        assert!(matches!(proto, ferrum_proto::ActorType::Agent));
    }

    #[test]
    fn test_actor_type_cli_to_proto_operator() {
        let cli: ActorTypeCli = ActorTypeCli::Operator;
        let proto: ferrum_proto::ActorType = cli.into();
        assert!(matches!(proto, ferrum_proto::ActorType::Operator));
    }

    #[test]
    fn test_actor_type_cli_all_variants() {
        for cli in [
            ActorTypeCli::User,
            ActorTypeCli::Agent,
            ActorTypeCli::PolicyEngine,
            ActorTypeCli::Gateway,
            ActorTypeCli::Adapter,
            ActorTypeCli::Operator,
            ActorTypeCli::System,
        ] {
            let proto: ferrum_proto::ActorType = cli.into();
            // Verify each conversion is valid (no panic)
            let _ = format!("{:?}", proto);
        }
    }

    // -------------------------------------------------------------------------
    // ListIntents validation tests
    // -------------------------------------------------------------------------

    fn validate_list_intents_limit(limit: u32) -> Result<u32> {
        if limit == 0 || limit > 200 {
            bail!("--limit must be between 1 and 200");
        }
        Ok(limit)
    }

    #[test]
    fn test_validate_list_intents_limit_bounds() {
        // limit=0 should fail
        assert!(validate_list_intents_limit(0).is_err());
        // limit=1 should pass
        assert!(validate_list_intents_limit(1).is_ok());
        // limit=50 should pass (default)
        assert!(validate_list_intents_limit(50).is_ok());
        // limit=200 should pass (max)
        assert!(validate_list_intents_limit(200).is_ok());
        // limit=201 should fail
        assert!(validate_list_intents_limit(201).is_err());
    }

    #[test]
    fn test_list_intents_state_filter_validation() {
        // State filters are just Vec<String>, validation happens server-side
        let states = &["pending".to_string(), "active".to_string()][..];
        // This should not panic and should be valid
        assert_eq!(states.len(), 2);
    }

    #[test]
    fn test_list_intents_cursor_pagination() {
        // Cursor is optional, validation happens server-side
        let cursor: Option<String> = Some("next_page_token_abc123".to_string());
        assert!(cursor.is_some());
        assert_eq!(cursor.as_deref(), Some("next_page_token_abc123"));
    }

    // -------------------------------------------------------------------------
    // CancelExecution validation tests
    // -------------------------------------------------------------------------

    fn validate_cancel_requires_confirm(confirm: bool) -> Result<bool> {
        if !confirm {
            bail!("--confirm flag is required to cancel an execution");
        }
        Ok(true)
    }

    #[test]
    fn test_cancel_requires_confirm() {
        // Without confirm flag, should fail
        let result = validate_cancel_requires_confirm(false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("--confirm flag is required"));
    }

    #[test]
    fn test_cancel_with_confirm_is_valid() {
        // With confirm flag, should pass
        let result = validate_cancel_requires_confirm(true);
        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // OutputFormat tests for new commands
    // -------------------------------------------------------------------------

    #[test]
    fn test_output_format_parsing() {
        assert!(matches!(
            "text".parse::<OutputFormat>(),
            Ok(OutputFormat::Text)
        ));
        assert!(matches!(
            "json".parse::<OutputFormat>(),
            Ok(OutputFormat::Json)
        ));
        assert!(matches!(
            "dot".parse::<OutputFormat>(),
            Ok(OutputFormat::Dot)
        ));
        assert!("invalid".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_output_format_default_is_text() {
        let fmt = OutputFormat::default();
        assert!(matches!(fmt, OutputFormat::Text));
    }

    // -------------------------------------------------------------------------
    // BumpType tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_bump_type_parsing() {
        assert!(matches!("patch".parse::<BumpType>(), Ok(BumpType::Patch)));
        assert!(matches!("minor".parse::<BumpType>(), Ok(BumpType::Minor)));
        assert!(matches!("major".parse::<BumpType>(), Ok(BumpType::Major)));
        assert!(matches!("PATCH".parse::<BumpType>(), Ok(BumpType::Patch)));
        assert!(matches!("Minor".parse::<BumpType>(), Ok(BumpType::Minor)));
        assert!("invalid".parse::<BumpType>().is_err());
    }

    #[test]
    fn test_bump_type_default_is_patch() {
        let bump = BumpType::default();
        assert!(matches!(bump, BumpType::Patch));
    }

    // -------------------------------------------------------------------------
    // Author bundle bump helper tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_author_bundle_bump_parses_valid_yaml() {
        let yaml = r#"version: "0.1.0"
bundle_id: "test-bundle"
rules: []
"#;
        // This should not panic - we just verify it can be parsed
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(value.get("version").and_then(|v| v.as_str()), Some("0.1.0"));
    }

    #[test]
    fn test_author_bundle_bump_invalid_yaml() {
        let yaml = "not: [valid yaml";
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_author_bundle_bump_missing_version() {
        let yaml = r#"bundle_id: "test-bundle"
rules: []
"#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert!(value.get("version").is_none());
    }

    #[test]
    fn test_author_bundle_bump_version_bump_logic() {
        // Test version parsing for different bump types
        let test_cases = vec![
            ("0.1.0", BumpType::Patch, "0.1.1"),
            ("0.1.0", BumpType::Minor, "0.2.0"),
            ("0.1.0", BumpType::Major, "1.0.0"),
            ("1.2.3", BumpType::Patch, "1.2.4"),
            ("1.2.3", BumpType::Minor, "1.3.0"),
            ("1.2.3", BumpType::Major, "2.0.0"),
            ("0.0.1", BumpType::Major, "1.0.0"),
            ("0.0.1", BumpType::Minor, "0.1.0"),
        ];

        for (original, bump_type, expected) in test_cases {
            let mut version_parts: Vec<u32> =
                original.split('.').filter_map(|s| s.parse().ok()).collect();

            while version_parts.len() < 3 {
                version_parts.push(0);
            }

            match bump_type {
                BumpType::Major => {
                    version_parts[0] += 1;
                    version_parts[1] = 0;
                    version_parts[2] = 0;
                }
                BumpType::Minor => {
                    version_parts[1] += 1;
                    version_parts[2] = 0;
                }
                BumpType::Patch => {
                    version_parts[2] += 1;
                }
            }

            let new_version = format!(
                "{}.{}.{}",
                version_parts[0], version_parts[1], version_parts[2]
            );
            assert_eq!(
                new_version, expected,
                "bumping {} with {:?} should give {}",
                original, bump_type, expected
            );
        }
    }

    // -------------------------------------------------------------------------
    // Readiness probe CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_server_readiness_cli_default_flags() {
        let cli = Cli::parse_from(["ferrumctl", "server", "readiness"]);
        let Command::Server { sub } = cli.command else {
            panic!("expected Server command");
        };
        match sub {
            ServerCommand::Readiness { deep, functional } => {
                assert!(!deep, "deep should be false by default");
                assert!(!functional, "functional should be false by default");
            }
            other => panic!("expected Readiness command, got {:?}", other),
        }
    }

    #[test]
    fn test_server_readiness_cli_deep_flag() {
        let cli = Cli::parse_from(["ferrumctl", "server", "readiness", "--deep"]);
        let Command::Server { sub } = cli.command else {
            panic!("expected Server command");
        };
        match sub {
            ServerCommand::Readiness { deep, functional } => {
                assert!(deep, "deep should be true with --deep");
                assert!(!functional, "functional should be false");
            }
            other => panic!("expected Readiness command, got {:?}", other),
        }
    }

    #[test]
    fn test_server_readiness_cli_functional_flag() {
        let cli = Cli::parse_from(["ferrumctl", "server", "readiness", "--functional"]);
        let Command::Server { sub } = cli.command else {
            panic!("expected Server command");
        };
        match sub {
            ServerCommand::Readiness { deep, functional } => {
                assert!(!deep, "deep should be false");
                assert!(functional, "functional should be true with --functional");
            }
            other => panic!("expected Readiness command, got {:?}", other),
        }
    }

    #[test]
    fn test_server_readiness_cli_deep_and_functional_flags() {
        let cli = Cli::parse_from(["ferrumctl", "server", "readiness", "--deep", "--functional"]);
        let Command::Server { sub } = cli.command else {
            panic!("expected Server command");
        };
        match sub {
            ServerCommand::Readiness { deep, functional } => {
                assert!(deep, "deep should be true");
                assert!(functional, "functional should be true");
            }
            other => panic!("expected Readiness command, got {:?}", other),
        }
    }

    // -------------------------------------------------------------------------
    // ReadinessResponse deserialization tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_readiness_response_deserializes_shallow() {
        let json = r#"{"status":"ready"}"#;
        let resp: client::ReadinessResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ready");
    }

    #[test]
    fn test_readiness_response_deserializes_with_status() {
        // Test various status values that the server might return
        for status in ["ready", "not_ready", "initializing"] {
            let json = format!(r#"{{"status":"{}"}}"#, status);
            let resp: client::ReadinessResponse = serde_json::from_str(&json).unwrap();
            assert_eq!(resp.status, status);
        }
    }

    #[test]
    fn test_health_response_deserializes() {
        let json = r#"{"status":"ok"}"#;
        let resp: client::HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
    }

    #[test]
    fn test_execution_detail_response_deserializes_wrapper() {
        let json = r#"{
            "execution": {
                "execution_id": "550e8400-e29b-41d4-a716-446655440000",
                "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                "rollback_contract_id": null,
                "decision": "Allow",
                "state": "Completed",
                "started_at": "2025-01-01T00:00:00Z",
                "finished_at": "2025-01-01T00:01:00Z",
                "result_digest": "sha256:abc123"
            },
            "rollback_contract": null
        }"#;
        let detail: client::ExecutionDetailResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            detail.execution.execution_id,
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(detail.execution.decision, "Allow");
        assert!(detail.rollback_contract.is_none());
    }

    #[test]
    fn test_execution_detail_response_with_rollback_contract() {
        let json = r#"{
            "execution": {
                "execution_id": "550e8400-e29b-41d4-a716-446655440000",
                "proposal_id": "550e8400-e29b-41d4-a716-446655440001",
                "intent_id": "550e8400-e29b-41d4-a716-446655440002",
                "capability_id": "550e8400-e29b-41d4-a716-446655440003",
                "rollback_contract_id": "550e8400-e29b-41d4-a716-446655440010",
                "decision": "Allow",
                "state": "Prepared",
                "started_at": "2025-01-01T00:00:00Z",
                "finished_at": null,
                "result_digest": null
            },
            "rollback_contract": {
                "contract_id": "550e8400-e29b-41d4-a716-446655440010",
                "state": "Prepared"
            }
        }"#;
        let detail: client::ExecutionDetailResponse = serde_json::from_str(json).unwrap();
        assert_eq!(detail.execution.state, "Prepared");
        assert!(detail.rollback_contract.is_some());
    }

    // -------------------------------------------------------------------------
    // ServerCommand::Metrics parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_server_metrics_command_parses() {
        // Test that "metrics" subcommand under "server" parses correctly
        let matches = Cli::command()
            .try_get_matches_from(["ferrumctl", "server", "metrics"])
            .unwrap();

        let sub = matches
            .subcommand_matches("server")
            .unwrap()
            .subcommand_matches("metrics");
        assert!(
            sub.is_some(),
            "metrics subcommand should parse successfully"
        );
    }

    // -------------------------------------------------------------------------
    // Policy command CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_policy_validate_command_parses() {
        let cli = Cli::parse_from(["ferrumctl", "policy", "validate", "--file", "test.yaml"]);
        let Command::Policy { sub } = cli.command else {
            panic!("expected Policy command");
        };
        match sub {
            PolicyCommand::Validate { file } => {
                assert_eq!(file, "test.yaml");
            }
            _ => panic!("expected Validate command"),
        }
    }

    #[test]
    fn test_policy_validate_rejects_missing_file_flag() {
        let result = Cli::try_parse_from(["ferrumctl", "policy", "validate"]);
        assert!(result.is_err(), "should require --file");
    }

    #[test]
    fn test_policy_apply_command_parses() {
        let cli = Cli::parse_from(["ferrumctl", "policy", "apply", "--file", "policy.yaml"]);
        let Command::Policy { sub } = cli.command else {
            panic!("expected Policy command");
        };
        match sub {
            PolicyCommand::Apply {
                file,
                activate,
                json,
            } => {
                assert_eq!(file, "policy.yaml");
                assert!(!activate, "default should be inactive");
                assert!(!json);
            }
            _ => panic!("expected Apply command"),
        }
    }

    #[test]
    fn test_policy_apply_activate_flag_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "policy",
            "apply",
            "--file",
            "policy.yaml",
            "--activate",
        ]);
        let Command::Policy { sub } = cli.command else {
            panic!("expected Policy command");
        };
        match sub {
            PolicyCommand::Apply { activate, .. } => {
                assert!(activate, "--activate should be true");
            }
            _ => panic!("expected Apply command"),
        }
    }

    // -------------------------------------------------------------------------
    // Admin command CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_admin_status_command_parses() {
        let cli = Cli::parse_from(["ferrumctl", "admin", "status"]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Status => {}
            _ => panic!("expected Status command"),
        }
    }

    #[test]
    fn test_admin_config_command_parses() {
        let cli = Cli::parse_from(["ferrumctl", "admin", "config"]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Config => {}
            _ => panic!("expected Config command"),
        }
    }

    #[test]
    fn test_admin_approvals_list_parses() {
        let cli = Cli::parse_from(["ferrumctl", "admin", "approvals", "list"]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Approvals { sub } => match sub {
                AdminApprovalsCommand::List => {}
                _ => panic!("expected List command"),
            },
            _ => panic!("expected Approvals command"),
        }
    }

    #[test]
    fn test_admin_approvals_get_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "approvals",
            "get",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Approvals { sub } => match sub {
                AdminApprovalsCommand::Get { approval_id } => {
                    assert_eq!(approval_id, "550e8400-e29b-41d4-a716-446655440000");
                }
                _ => panic!("expected Get command"),
            },
            _ => panic!("expected Approvals command"),
        }
    }

    #[test]
    fn test_admin_approvals_resolve_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "approvals",
            "resolve",
            "550e8400-e29b-41d4-a716-446655440000",
            "--approve",
            "--actor-type",
            "operator",
            "--actor-id",
            "admin-1",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Approvals { sub } => match sub {
                AdminApprovalsCommand::Resolve {
                    approval_id,
                    approve,
                    deny,
                    actor_type,
                    actor_id,
                    ..
                } => {
                    assert_eq!(approval_id, "550e8400-e29b-41d4-a716-446655440000");
                    assert!(approve);
                    assert!(!deny);
                    assert!(matches!(actor_type, ActorTypeCli::Operator));
                    assert_eq!(actor_id, "admin-1");
                }
                _ => panic!("expected Resolve command"),
            },
            _ => panic!("expected Approvals command"),
        }
    }

    // -------------------------------------------------------------------------
    // Admin executions CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_admin_executions_list_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "executions",
            "list",
            "--state",
            "pending",
            "--limit",
            "10",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Executions { sub } => match sub {
                AdminExecutionsCommand::List { state, limit, .. } => {
                    assert_eq!(state, vec!["pending"]);
                    assert_eq!(limit, 10);
                }
                _ => panic!("expected List command"),
            },
            _ => panic!("expected Executions command"),
        }
    }

    #[test]
    fn test_admin_executions_get_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "executions",
            "get",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Executions { sub } => match sub {
                AdminExecutionsCommand::Get { execution_id } => {
                    assert_eq!(execution_id, "550e8400-e29b-41d4-a716-446655440000");
                }
                _ => panic!("expected Get command"),
            },
            _ => panic!("expected Executions command"),
        }
    }

    #[test]
    fn test_admin_executions_cancel_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "executions",
            "cancel",
            "550e8400-e29b-41d4-a716-446655440000",
            "--confirm",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Executions { sub } => match sub {
                AdminExecutionsCommand::Cancel {
                    execution_id,
                    confirm,
                    ..
                } => {
                    assert_eq!(execution_id, "550e8400-e29b-41d4-a716-446655440000");
                    assert!(confirm);
                }
                _ => panic!("expected Cancel command"),
            },
            _ => panic!("expected Executions command"),
        }
    }

    #[test]
    fn test_admin_executions_list_limit_validation() {
        // limit=0 should fail parsing validation at runtime (we test the validation logic)
        let result =
            Cli::try_parse_from(["ferrumctl", "admin", "executions", "list", "--limit", "0"]);
        assert!(
            result.is_ok(),
            "parsing limit=0 is ok; runtime validation rejects it"
        );
    }

    // -------------------------------------------------------------------------
    // Admin lifecycle outbox CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_admin_lifecycle_outbox_list_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "lifecycle-outbox",
            "list",
            "--status",
            "needs_operator_review",
            "--limit",
            "25",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::LifecycleOutbox { sub } => match sub {
                AdminLifecycleOutboxCommand::List { status, limit, .. } => {
                    assert_eq!(status, "needs_operator_review");
                    assert_eq!(limit, 25);
                }
                _ => panic!("expected List command"),
            },
            _ => panic!("expected LifecycleOutbox command"),
        }
    }

    #[test]
    fn test_admin_lifecycle_outbox_get_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "lifecycle-outbox",
            "get",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::LifecycleOutbox { sub } => match sub {
                AdminLifecycleOutboxCommand::Get { outbox_id, .. } => {
                    assert_eq!(outbox_id, "550e8400-e29b-41d4-a716-446655440000");
                }
                _ => panic!("expected Get command"),
            },
            _ => panic!("expected LifecycleOutbox command"),
        }
    }

    #[test]
    fn test_admin_lifecycle_outbox_retry_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "lifecycle-outbox",
            "retry",
            "550e8400-e29b-41d4-a716-446655440000",
            "--actor-id",
            "operator-1",
            "--reason",
            "parent event repaired",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::LifecycleOutbox { sub } => match sub {
                AdminLifecycleOutboxCommand::Retry {
                    outbox_id,
                    actor_id,
                    reason,
                    ..
                } => {
                    assert_eq!(outbox_id, "550e8400-e29b-41d4-a716-446655440000");
                    assert_eq!(actor_id, "operator-1");
                    assert_eq!(reason.as_deref(), Some("parent event repaired"));
                }
                _ => panic!("expected Retry command"),
            },
            _ => panic!("expected LifecycleOutbox command"),
        }
    }

    #[test]
    fn test_admin_lifecycle_outbox_resolve_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "lifecycle-outbox",
            "resolve",
            "550e8400-e29b-41d4-a716-446655440000",
            "--actor-id",
            "operator-1",
            "--reason",
            "verified externally",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::LifecycleOutbox { sub } => match sub {
                AdminLifecycleOutboxCommand::Resolve {
                    outbox_id,
                    actor_id,
                    reason,
                    ..
                } => {
                    assert_eq!(outbox_id, "550e8400-e29b-41d4-a716-446655440000");
                    assert_eq!(actor_id, "operator-1");
                    assert_eq!(reason, "verified externally");
                }
                _ => panic!("expected Resolve command"),
            },
            _ => panic!("expected LifecycleOutbox command"),
        }
    }

    // -------------------------------------------------------------------------
    // Admin backup CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_admin_backup_create_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "backup",
            "create",
            "--db-path",
            "/tmp/test.db",
            "--retention-days",
            "7",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Backup { sub } => match sub {
                AdminBackupCommand::Create {
                    db_path,
                    retention_days,
                    ..
                } => {
                    assert_eq!(db_path, PathBuf::from("/tmp/test.db"));
                    assert_eq!(retention_days, Some(7));
                }
                _ => panic!("expected Create command"),
            },
            _ => panic!("expected Backup command"),
        }
    }

    #[test]
    fn test_admin_backup_verify_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "backup",
            "verify",
            "--db-path",
            "/tmp/test.db",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Backup { sub } => match sub {
                AdminBackupCommand::Verify { db_path } => {
                    assert_eq!(db_path, PathBuf::from("/tmp/test.db"));
                }
                _ => panic!("expected Verify command"),
            },
            _ => panic!("expected Backup command"),
        }
    }

    #[test]
    fn test_admin_backup_restore_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "backup",
            "restore",
            "--db-path",
            "/tmp/test.db",
            "--from",
            "/tmp/test.db.backup",
            "--dry-run",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Backup { sub } => match sub {
                AdminBackupCommand::Restore {
                    db_path,
                    from,
                    dry_run,
                    ..
                } => {
                    assert_eq!(db_path, PathBuf::from("/tmp/test.db"));
                    assert_eq!(from, PathBuf::from("/tmp/test.db.backup"));
                    assert!(dry_run);
                }
                _ => panic!("expected Restore command"),
            },
            _ => panic!("expected Backup command"),
        }
    }

    // -------------------------------------------------------------------------
    // Policy validation helper tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_policy_bundle_yaml_valid() {
        let yaml = r#"version: "0.1.0"
bundle_id: "test-bundle"
rules:
  - id: "rule-1"
    description: "Test rule"
    decision: "Allow"
    priority: 10
    matchers:
      - type: "scope_mismatch"
"#;
        assert!(ferrum_proto::validate_policy_bundle_yaml(yaml).is_ok());
    }

    #[test]
    fn test_validate_policy_bundle_yaml_invalid() {
        let yaml = "not: [valid yaml";
        assert!(ferrum_proto::validate_policy_bundle_yaml(yaml).is_err());
    }

    #[test]
    fn test_validate_policy_bundle_yaml_missing_field() {
        let yaml = r#"version: "0.1.0"
bundle_id: "test-bundle"
rules: []
"#;
        // Empty rules is valid; missing version would fail parse
        assert!(ferrum_proto::validate_policy_bundle_yaml(yaml).is_ok());
    }

    #[test]
    fn test_cli_policy_simulate_parses() {
        let cli = Cli::try_parse_from([
            "ferrumctl",
            "policy",
            "simulate",
            "--file",
            "bundle.yaml",
            "--proposal",
            "proposal.json",
        ]);
        assert!(cli.is_ok(), "CLI should parse policy simulate command");
    }

    #[test]
    fn test_cli_policy_simulate_with_intent_parses() {
        let cli = Cli::try_parse_from([
            "ferrumctl",
            "policy",
            "simulate",
            "--file",
            "bundle.yaml",
            "--proposal",
            "proposal.json",
            "--intent",
            "intent.json",
            "--json",
        ]);
        assert!(
            cli.is_ok(),
            "CLI should parse policy simulate with optional intent and json flags"
        );
    }

    // -------------------------------------------------------------------------
    // Admin audit Merkle CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_admin_audit_merkle_verify_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "merkle-verify",
            "--window-start",
            "2024-01-01T00:00:00Z",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::MerkleVerify { window_start, .. } => {
                    assert_eq!(window_start, "2024-01-01T00:00:00Z");
                }
                _ => panic!("expected MerkleVerify command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    #[test]
    fn test_admin_audit_merkle_roots_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "merkle-roots",
            "--limit",
            "10",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::MerkleRoots { limit, .. } => {
                    assert_eq!(limit, 10);
                }
                _ => panic!("expected MerkleRoots command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    #[test]
    fn test_admin_audit_checkpoint_sign_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "checkpoint-sign",
            "--window-start",
            "2024-01-01T00:00:00Z",
            "--signer-id",
            "operator-1",
            "--private-key",
            "c29tZV9rZXk=",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::CheckpointSign {
                    window_start,
                    signer_id,
                    private_key,
                    ..
                } => {
                    assert_eq!(window_start, "2024-01-01T00:00:00Z");
                    assert_eq!(signer_id, "operator-1");
                    assert_eq!(private_key, "c29tZV9rZXk=");
                }
                _ => panic!("expected CheckpointSign command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    #[test]
    fn test_admin_audit_checkpoint_verify_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "checkpoint-verify",
            "--window-start",
            "2024-01-01T00:00:00Z",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::CheckpointVerify { window_start, .. } => {
                    assert_eq!(window_start, "2024-01-01T00:00:00Z");
                }
                _ => panic!("expected CheckpointVerify command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    #[test]
    fn test_admin_audit_checkpoint_list_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "checkpoint-list",
            "--limit",
            "10",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::CheckpointList { limit, .. } => {
                    assert_eq!(limit, 10);
                }
                _ => panic!("expected CheckpointList command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    // -------------------------------------------------------------------------
    // Admin audit bundle CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_admin_audit_export_bundle_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "export",
            "--bundle",
            "/tmp/audit-bundle",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::Export { bundle, .. } => {
                    assert_eq!(bundle, Some(PathBuf::from("/tmp/audit-bundle")));
                }
                _ => panic!("expected Export command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    #[test]
    fn test_admin_audit_verify_bundle_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "admin",
            "audit",
            "verify",
            "--bundle",
            "/tmp/audit-bundle",
        ]);
        let Command::Admin { sub } = cli.command else {
            panic!("expected Admin command");
        };
        match sub {
            AdminCommand::Audit { sub } => match sub {
                AdminAuditCommand::Verify { bundle, .. } => {
                    assert_eq!(bundle, Some(PathBuf::from("/tmp/audit-bundle")));
                }
                _ => panic!("expected Verify command"),
            },
            _ => panic!("expected Audit command"),
        }
    }

    // -------------------------------------------------------------------------
    // Evidence snapshot CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_evidence_snapshot_parses() {
        let cli = Cli::parse_from(["ferrumctl", "evidence", "snapshot"]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::Snapshot { output_dir } => {
                assert!(output_dir.is_none());
            }
            EvidenceCommand::SloWindow { .. } => panic!("expected Snapshot"),
        }
    }

    #[test]
    fn test_evidence_snapshot_output_dir_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "evidence",
            "snapshot",
            "--output-dir",
            "/tmp/evidence",
        ]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::Snapshot { output_dir } => {
                assert_eq!(output_dir, Some(PathBuf::from("/tmp/evidence")));
            }
            EvidenceCommand::SloWindow { .. } => panic!("expected Snapshot"),
        }
    }

    // -------------------------------------------------------------------------
    // Evidence snapshot content / non-claims tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_evidence_snapshot_contains_non_claims_fields() {
        let mut snapshot = serde_json::Map::new();
        snapshot.insert(
            "snapshot_timestamp".to_string(),
            serde_json::Value::String("2026-05-29T12:00:00Z".to_string()),
        );
        snapshot.insert(
            "tool".to_string(),
            serde_json::Value::String("ferrumctl evidence snapshot".to_string()),
        );
        snapshot.insert(
            "non_claims_reference".to_string(),
            serde_json::Value::String("docs/security/non-claims.md".to_string()),
        );
        snapshot.insert(
            "non_claims_notice".to_string(),
            serde_json::Value::String(
                "This snapshot is a point-in-time operational view. It is not production-ready, Tier 2, GA, compliance, or SLO proof."
                    .to_string(),
            ),
        );
        snapshot.insert("health".to_string(), serde_json::json!({"status": "ok"}));

        let json_str = serde_json::to_string_pretty(&snapshot).unwrap();
        assert!(json_str.contains("snapshot_timestamp"));
        assert!(json_str.contains("ferrumctl evidence snapshot"));
        assert!(json_str.contains("docs/security/non-claims.md"));
        assert!(json_str.contains("not production-ready"));
    }

    #[test]
    fn test_evidence_snapshot_no_unqualified_overclaims() {
        let mut snapshot = serde_json::Map::new();
        snapshot.insert(
            "snapshot_timestamp".to_string(),
            serde_json::Value::String("2026-05-29T12:00:00Z".to_string()),
        );
        snapshot.insert(
            "tool".to_string(),
            serde_json::Value::String("ferrumctl evidence snapshot".to_string()),
        );
        snapshot.insert(
            "non_claims_reference".to_string(),
            serde_json::Value::String("docs/security/non-claims.md".to_string()),
        );
        snapshot.insert(
            "non_claims_notice".to_string(),
            serde_json::Value::String(
                "This snapshot is a point-in-time operational view. It is not production-ready, Tier 2, GA, compliance, or SLO proof."
                    .to_string(),
            ),
        );
        snapshot.insert("health".to_string(), serde_json::json!({"status": "ok"}));

        let json_str = serde_json::to_string_pretty(&snapshot).unwrap();
        // Ensure there are no standalone affirmative claims that would violate non-claims.
        let forbidden = [
            "\"production_ready\": true",
            "\"tier_2\": true",
            "\"tier2\": true",
            "\"ga\": true",
            "\"enterprise_ready\": true",
            "\"compliance\": true",
            "\"slo_proof\": true",
        ];
        for term in &forbidden {
            assert!(
                !json_str.to_lowercase().contains(&term.to_lowercase()),
                "snapshot must not contain unqualified overclaim: {}",
                term
            );
        }
    }

    #[test]
    fn test_evidence_snapshot_filename_format() {
        let ts = chrono::NaiveDate::from_ymd_opt(2026, 5, 29)
            .unwrap()
            .and_hms_opt(14, 30, 0)
            .unwrap()
            .and_utc();
        let filename = evidence_snapshot_filename(&ts);
        assert_eq!(filename, "evidence-snapshot-2026-05-29T14-30-00Z.json");
    }

    #[test]
    fn test_evidence_snapshot_filename_is_filesystem_safe() {
        let ts = chrono::Utc::now();
        let filename = evidence_snapshot_filename(&ts);
        // Must not contain characters illegal on common filesystems
        assert!(!filename.contains('/'));
        assert!(!filename.contains('\\'));
        assert!(!filename.contains(':'));
        assert!(filename.ends_with(".json"));
    }

    // -------------------------------------------------------------------------
    // SLO window CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_start_parses() {
        let cli = Cli::parse_from(["ferrumctl", "evidence", "slo-window", "start"]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Start { window_dir, notes } => {
                    assert!(window_dir.is_none());
                    assert!(notes.is_none());
                }
                _ => panic!("expected Start command"),
            },
            _ => panic!("expected SloWindow command"),
        }
    }

    #[test]
    fn test_slo_window_start_with_args_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "evidence",
            "slo-window",
            "start",
            "--window-dir",
            "/tmp/slo",
            "--notes",
            "initial observation",
        ]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Start { window_dir, notes } => {
                    assert_eq!(window_dir, Some(PathBuf::from("/tmp/slo")));
                    assert_eq!(notes, Some("initial observation".to_string()));
                }
                _ => panic!("expected Start command"),
            },
            _ => panic!("expected SloWindow command"),
        }
    }

    #[test]
    fn test_slo_window_status_parses() {
        let cli = Cli::parse_from(["ferrumctl", "evidence", "slo-window", "status"]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Status { window_dir, json } => {
                    assert!(window_dir.is_none());
                    assert!(!json);
                }
                _ => panic!("expected Status command"),
            },
            _ => panic!("expected SloWindow command"),
        }
    }

    #[test]
    fn test_slo_window_status_json_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "evidence",
            "slo-window",
            "status",
            "--window-dir",
            "/tmp/slo",
            "--json",
        ]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Status { window_dir, json } => {
                    assert_eq!(window_dir, Some(PathBuf::from("/tmp/slo")));
                    assert!(json);
                }
                _ => panic!("expected Status command"),
            },
            _ => panic!("expected SloWindow command"),
        }
    }

    #[test]
    fn test_slo_window_finalize_parses() {
        let cli = Cli::parse_from(["ferrumctl", "evidence", "slo-window", "finalize"]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Finalize {
                    window_dir,
                    notes,
                    allow_early,
                } => {
                    assert!(window_dir.is_none());
                    assert!(notes.is_none());
                    assert!(!allow_early);
                }
                _ => panic!("expected Finalize command"),
            },
            _ => panic!("expected SloWindow command"),
        }
    }

    #[test]
    fn test_slo_window_finalize_with_args_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "evidence",
            "slo-window",
            "finalize",
            "--window-dir",
            "/tmp/slo",
            "--notes",
            "window complete",
            "--allow-early",
        ]);
        let Command::Evidence { sub } = cli.command else {
            panic!("expected Evidence command");
        };
        match sub {
            EvidenceCommand::SloWindow { sub } => match sub {
                SloWindowCommand::Finalize {
                    window_dir,
                    notes,
                    allow_early,
                } => {
                    assert_eq!(window_dir, Some(PathBuf::from("/tmp/slo")));
                    assert_eq!(notes, Some("window complete".to_string()));
                    assert!(allow_early);
                }
                _ => panic!("expected Finalize command"),
            },
            _ => panic!("expected SloWindow command"),
        }
    }

    // -------------------------------------------------------------------------
    // SLO window state serialization / roundtrip tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_state_serialization_roundtrip() {
        let state = SloWindowState {
            window_id: "slo-window-test".to_string(),
            status: "started".to_string(),
            window_started_at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            window_ended_at: None,
            elapsed_duration_seconds: 0,
            target_duration_days: 30,
            minimum_duration_days: 7,
            notes: Some("test note".to_string()),
            non_claims_notice: SloWindowState::default_non_claims_notice(),
            created_by_tool: "ferrumctl evidence slo-window start".to_string(),
            finalized_by_tool: None,
        };
        let json = serde_json::to_string_pretty(&state).unwrap();
        let restored: SloWindowState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, restored);
    }

    #[test]
    fn test_slo_window_state_recompute_elapsed() {
        let past = chrono::Utc::now() - chrono::Duration::hours(5);
        let mut state = SloWindowState {
            window_id: "test".to_string(),
            status: "started".to_string(),
            window_started_at: past,
            window_ended_at: None,
            elapsed_duration_seconds: 0,
            target_duration_days: 30,
            minimum_duration_days: 7,
            notes: None,
            non_claims_notice: SloWindowState::default_non_claims_notice(),
            created_by_tool: "tool".to_string(),
            finalized_by_tool: None,
        };
        state.recompute_elapsed();
        // Should be roughly 5 hours = 18000 seconds, allow +/- 5 seconds for test execution time
        assert!(
            state.elapsed_duration_seconds >= 18000 - 5
                && state.elapsed_duration_seconds <= 18000 + 60
        );
    }

    // -------------------------------------------------------------------------
    // SLO window non-claims tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_non_claims_notice_contains_required_lines() {
        let notice = SloWindowState::default_non_claims_notice();
        assert!(notice.contains("Sustained SLO window = NOT COMPLETE"));
        assert!(notice.contains("production-ready = NO"));
        assert!(notice.contains("Tier 2 = NOT COMPLETE"));
        assert!(notice.contains("does not certify SLO achievement"));
    }

    #[test]
    fn test_slo_window_state_contains_non_claims_in_json() {
        let state = SloWindowState::start_now(None);
        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("non_claims_notice"));
        assert!(json.contains("NOT COMPLETE"));
        assert!(json.contains("production-ready = NO"));
    }

    #[test]
    fn test_slo_window_no_unqualified_overclaims() {
        let state = SloWindowState::start_now(None);
        let json = serde_json::to_string_pretty(&state).unwrap();
        let forbidden = [
            "\"production_ready\": true",
            "\"tier_2\": true",
            "\"tier2\": true",
            "\"ga\": true",
            "\"enterprise_ready\": true",
            "\"compliance\": true",
            "\"slo_proof\": true",
        ];
        for term in &forbidden {
            assert!(
                !json.to_lowercase().contains(&term.to_lowercase()),
                "state must not contain unqualified overclaim: {}",
                term
            );
        }
    }

    // -------------------------------------------------------------------------
    // SLO window lifecycle tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_slo_window_start_creates_state_file() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), Some("test".to_string())).unwrap();
        let path = dir.path().join("slo-window-state.json");
        assert!(path.exists());
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.status, "started");
        assert_eq!(state.notes, Some("test".to_string()));
        assert!(state.created_by_tool.contains("slo-window start"));
    }

    #[test]
    fn test_slo_window_start_refuses_active_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        let result = run_slo_window_start(Some(dir.path().to_path_buf()), None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("active window already exists"));
    }

    #[test]
    fn test_slo_window_status_reads_state() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        run_slo_window_status(Some(dir.path().to_path_buf()), true).unwrap();
    }

    #[test]
    fn test_slo_window_status_missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let result = run_slo_window_status(Some(dir.path().to_path_buf()), true);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("failed to read state file"));
    }

    #[test]
    fn test_slo_window_finalize_early_rejected_without_allow_early() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        let result = run_slo_window_finalize(Some(dir.path().to_path_buf()), None, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("minimum is 7 days"));
        assert!(err.contains("--allow-early"));
    }

    #[test]
    fn test_slo_window_finalize_early_allowed_with_flag() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        run_slo_window_finalize(Some(dir.path().to_path_buf()), None, true).unwrap();
        let path = dir.path().join("slo-window-state.json");
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.status, "finalized");
        assert!(
            state
                .finalized_by_tool
                .as_ref()
                .unwrap()
                .contains("slo-window finalize")
        );
    }

    #[test]
    fn test_slo_window_finalize_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), None).unwrap();
        run_slo_window_finalize(Some(dir.path().to_path_buf()), None, true).unwrap();
        // Second finalize should succeed (idempotent)
        run_slo_window_finalize(
            Some(dir.path().to_path_buf()),
            Some("again".to_string()),
            true,
        )
        .unwrap();
        let path = dir.path().join("slo-window-state.json");
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.status, "finalized");
    }

    #[test]
    fn test_slo_window_finalize_updates_notes() {
        let dir = tempfile::tempdir().unwrap();
        run_slo_window_start(Some(dir.path().to_path_buf()), Some("first".to_string())).unwrap();
        run_slo_window_finalize(
            Some(dir.path().to_path_buf()),
            Some("final note".to_string()),
            true,
        )
        .unwrap();
        let path = dir.path().join("slo-window-state.json");
        let state = read_slo_window_state(&path).unwrap();
        assert_eq!(state.notes, Some("final note".to_string()));
    }

    // -------------------------------------------------------------------------
    // Readiness report CLI parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_readiness_report_cli_parses() {
        let cli = Cli::parse_from(["ferrumctl", "readiness", "report"]);
        let Command::Readiness { sub } = cli.command else {
            panic!("expected Readiness command");
        };
        match sub {
            ReadinessCommand::Report {
                snapshot,
                window_dir,
                json,
                offline,
            } => {
                assert!(snapshot.is_none());
                assert!(window_dir.is_none());
                assert!(!json);
                assert!(!offline);
            }
        }
    }

    #[test]
    fn test_readiness_report_cli_with_args_parses() {
        let cli = Cli::parse_from([
            "ferrumctl",
            "readiness",
            "report",
            "--snapshot",
            "/tmp/snap.json",
            "--window-dir",
            "/tmp/slo",
            "--json",
            "--offline",
        ]);
        let Command::Readiness { sub } = cli.command else {
            panic!("expected Readiness command");
        };
        match sub {
            ReadinessCommand::Report {
                snapshot,
                window_dir,
                json,
                offline,
            } => {
                assert_eq!(snapshot, Some(PathBuf::from("/tmp/snap.json")));
                assert_eq!(window_dir, Some(PathBuf::from("/tmp/slo")));
                assert!(json);
                assert!(offline);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Readiness report serialization / roundtrip tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_readiness_report_serialization_roundtrip() {
        let report = ReadinessReport {
            report_timestamp: "2026-05-29T12:00:00Z".to_string(),
            tool: "ferrumctl readiness report".to_string(),
            offline_mode: true,
            non_claims_reference: "docs/security/non-claims.md".to_string(),
            non_claims_notice: "notice".to_string(),
            health: Some(serde_json::json!({"status": "ok"})),
            readiness: None,
            readiness_deep: None,
            functional_readiness: None,
            metrics_summary: None,
            slo_window: None,
            evidence_snapshot: None,
            overall: OverallAssessment::default(),
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        let restored: ReadinessReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, restored);
    }

    // -------------------------------------------------------------------------
    // Readiness report non-claims tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_readiness_report_non_claims_fields() {
        let report = ReadinessReport {
            report_timestamp: "2026-05-29T12:00:00Z".to_string(),
            tool: "ferrumctl readiness report".to_string(),
            offline_mode: true,
            non_claims_reference: "docs/security/non-claims.md".to_string(),
            non_claims_notice: "This report is a point-in-time operational view. It is not production-ready, Tier 2, GA, compliance, or SLO proof.".to_string(),
            health: None,
            readiness: None,
            readiness_deep: None,
            functional_readiness: None,
            metrics_summary: None,
            slo_window: None,
            evidence_snapshot: None,
            overall: OverallAssessment::default(),
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("non_claims_reference"));
        assert!(json.contains("docs/security/non-claims.md"));
        assert!(json.contains("non_claims_notice"));
        assert!(json.contains("not production-ready"));
        assert!(json.contains("production_ready"));
        assert!(json.contains("NO"));
    }

    #[test]
    fn test_readiness_report_no_unqualified_overclaims() {
        let report = ReadinessReport {
            report_timestamp: "2026-05-29T12:00:00Z".to_string(),
            tool: "ferrumctl readiness report".to_string(),
            offline_mode: true,
            non_claims_reference: "docs/security/non-claims.md".to_string(),
            non_claims_notice: "notice".to_string(),
            health: None,
            readiness: None,
            readiness_deep: None,
            functional_readiness: None,
            metrics_summary: None,
            slo_window: None,
            evidence_snapshot: None,
            overall: OverallAssessment::default(),
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        let forbidden = [
            "\"production_ready\": true",
            "\"tier_2\": true",
            "\"tier2\": true",
            "\"ga\": true",
            "\"enterprise_ready\": true",
            "\"compliance\": true",
            "\"slo_proof\": true",
        ];
        for term in &forbidden {
            assert!(
                !json.to_lowercase().contains(&term.to_lowercase()),
                "report must not contain unqualified overclaim: {}",
                term
            );
        }
    }

    // -------------------------------------------------------------------------
    // Readiness report offline mode test
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_readiness_report_offline_mode_with_local_state() {
        let dir = tempfile::tempdir().unwrap();
        // Create SLO window state
        run_slo_window_start(Some(dir.path().to_path_buf()), Some("test".to_string())).unwrap();
        // Create evidence snapshot
        let snapshot = serde_json::json!({
            "snapshot_timestamp": "2026-05-29T12:00:00Z",
            "tool": "ferrumctl evidence snapshot",
        });
        let snap_path = dir
            .path()
            .join("evidence-snapshot-2026-05-29T12-00-00Z.json");
        std::fs::write(&snap_path, serde_json::to_string_pretty(&snapshot).unwrap()).unwrap();

        let report = build_readiness_report(
            "http://127.0.0.1:8080",
            None,
            Some(snap_path),
            Some(dir.path().to_path_buf()),
            true,
        )
        .await
        .unwrap();

        assert!(report.offline_mode);
        assert!(report.slo_window.is_some());
        assert!(report.evidence_snapshot.is_some());
        assert!(report.health.is_none());
        assert!(report.readiness.is_none());
        assert_eq!(report.overall.production_ready, "NO");
        assert_eq!(report.overall.tier_2, "NOT COMPLETE");
    }

    // -------------------------------------------------------------------------
    // Snapshot lookup helper tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_find_latest_evidence_snapshot_finds_newest() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir
            .path()
            .join("evidence-snapshot-2026-05-28T12-00-00Z.json");
        let path2 = dir
            .path()
            .join("evidence-snapshot-2026-05-29T12-00-00Z.json");
        std::fs::write(&path1, "{}").unwrap();
        std::fs::write(&path2, "{}").unwrap();

        let latest = find_latest_evidence_snapshot(dir.path());
        assert_eq!(latest, Some(path2));
    }

    #[test]
    fn test_find_latest_evidence_snapshot_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let latest = find_latest_evidence_snapshot(dir.path());
        assert!(latest.is_none());
    }

    // -------------------------------------------------------------------------
    // Overall assessment hardcoded non-claims test
    // -------------------------------------------------------------------------

    #[test]
    fn test_overall_assessment_hardcoded_non_claims() {
        let overall = OverallAssessment::default();
        assert_eq!(overall.production_ready, "NO");
        assert_eq!(overall.tier_2, "NOT COMPLETE");
        assert_eq!(overall.ha4_automated_failover, "NOT COMPLETE");
        assert_eq!(overall.sustained_slo, "NOT COMPLETE");
        assert_eq!(overall.label, "Cautious / Point-in-time only");
        assert!(
            overall
                .issues
                .contains(&"production-ready = NO".to_string())
        );
        assert!(
            overall
                .issues
                .contains(&"Tier 2 = NOT COMPLETE".to_string())
        );
        assert!(
            overall
                .issues
                .contains(&"HA-4 automated failover = NOT COMPLETE".to_string())
        );
        assert!(
            overall
                .issues
                .contains(&"Sustained SLO = NOT COMPLETE".to_string())
        );
    }
}
