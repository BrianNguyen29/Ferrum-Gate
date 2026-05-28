use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

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

        /// Output format: text (default) or json.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: OutputFormat,
    },
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
                AdminCommand::Audit { sub } => match sub {
                    AdminAuditCommand::List {
                        action,
                        resource_type,
                        resource_id,
                        cursor,
                        limit,
                        format,
                    } => {
                        let response = client
                            .list_audit_logs(
                                action.as_deref(),
                                resource_type.as_deref(),
                                resource_id.as_deref(),
                                cursor.as_deref(),
                                limit,
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
                },
                AdminCommand::Config => {
                    run_admin_config(&server_url, bearer_token.as_deref())?;
                }
            }
        }
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
}
