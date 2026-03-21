use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

const CONTRACT_PATHS: &[&str] = &[
    "contracts/ferrumgate-agent-contract.v1.yaml",
    "contracts/ferrumgate-integrator-contract.v1.yaml",
];

const SCHEMA_PATHS: &[&str] = &[
    "schemas/jsonschema/action-proposal.json",
    "schemas/jsonschema/approval-request.json",
    "schemas/jsonschema/capability-lease.json",
    "schemas/jsonschema/common.json",
    "schemas/jsonschema/intent-envelope.json",
    "schemas/jsonschema/provenance-event.json",
    "schemas/jsonschema/rollback-contract.json",
];

/// Returns the repository root path by resolving from CARGO_MANIFEST_DIR.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// Returns the known contract paths used by inspect.
pub fn known_contract_paths() -> Vec<&'static str> {
    CONTRACT_PATHS.to_vec()
}

/// Returns the known schema paths used by inspect.
pub fn known_schema_paths() -> Vec<&'static str> {
    SCHEMA_PATHS.to_vec()
}

/// Schema inventory entry with existence status.
#[derive(Clone)]
pub struct SchemaEntry<'a> {
    pub path: &'a str,
    pub present: bool,
}

/// Builds the schema inventory by checking each path against the repo root.
pub fn build_schema_inventory(root: &Path) -> Vec<SchemaEntry<'static>> {
    SCHEMA_PATHS
        .iter()
        .map(|p| SchemaEntry {
            path: p,
            present: root.join(p).exists(),
        })
        .collect()
}

/// Schema inventory entry serialized for JSON output.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SchemaEntryJson<'a> {
    pub path: &'a str,
    pub present: bool,
}

/// Formats schema inventory as plain text, one line per schema.
pub fn format_schema_inventory(entries: &[SchemaEntry]) -> String {
    let mut entries_sorted = entries.to_vec();
    entries_sorted.sort_by(|a, b| a.path.cmp(b.path));
    let lines: Vec<String> = entries_sorted
        .iter()
        .map(|e| {
            let status = if e.present { "ok" } else { "missing" };
            format!("{}  {}", status, e.path)
        })
        .collect();
    lines.join("\n")
}

/// Formats schema inventory as a JSON array of objects.
pub fn format_schema_inventory_json(entries: &[SchemaEntry]) -> String {
    let mut entries_sorted = entries.to_vec();
    entries_sorted.sort_by(|a, b| a.path.cmp(b.path));
    let json_entries: Vec<SchemaEntryJson> = entries_sorted
        .iter()
        .map(|e| SchemaEntryJson {
            path: e.path,
            present: e.present,
        })
        .collect();
    serde_json::to_string(&json_entries).expect("schema inventory must serialize")
}

/// Formats contract paths as either plain text (one per line) or JSON array.
pub fn format_contract_paths(paths: &[&str], as_json: bool) -> String {
    if as_json {
        serde_json::to_string(paths).expect("contract paths must serialize")
    } else {
        paths.join("\n")
    }
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

// =============================================================================
// Server/Remote inspection types
// =============================================================================

#[derive(Debug, Clone, Deserialize)]
struct ApiError {
    code: String,
    message: String,
    #[serde(default, rename = "details")]
    _details: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ExecutionRecord {
    execution_id: String,
    proposal_id: String,
    intent_id: String,
    capability_id: String,
    #[serde(default)]
    rollback_contract_id: Option<String>,
    decision: String,
    state: String,
    started_at: String,
    #[serde(default)]
    finished_at: Option<String>,
    #[serde(default)]
    result_digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ApprovalRequest {
    approval_id: String,
    intent_id: String,
    proposal_id: String,
    #[serde(default)]
    execution_id: Option<String>,
    reason: String,
    action_digest: String,
    expires_at: String,
    state: String,
    created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceEvent {
    event_id: String,
    kind: String,
    occurred_at: String,
    intent_id: Option<String>,
    proposal_id: Option<String>,
    execution_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LineageResponse {
    execution_id: String,
    events: Vec<ProvenanceEvent>,
}

#[derive(Debug, Clone, Serialize)]
struct ProvenanceQueryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    intent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    until: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceQueryResponse {
    events: Vec<ProvenanceEvent>,
}

struct InspectProvenanceOptions {
    query: ProvenanceQueryRequest,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
}

// =============================================================================
// Server client
// =============================================================================

#[derive(Clone)]
struct ServerClient {
    base_url: String,
    bearer_token: Option<String>,
    client: Client,
}

impl ServerClient {
    fn new(base_url: &str, bearer_token: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            bearer_token,
            client: Client::new(),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.client.request(method, &url);
        if let Some(token) = &self.bearer_token {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }
        builder
    }

    async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .request(reqwest::Method::GET, "/v1/healthz")
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn get_execution(&self, execution_id: &str) -> Result<ExecutionRecord> {
        let path = format!("/v1/executions/{}", execution_id);
        let resp = self.request(reqwest::Method::GET, &path).send().await?;
        self.decode_json(resp).await
    }

    async fn list_approvals(&self) -> Result<Vec<ApprovalRequest>> {
        let resp = self
            .request(reqwest::Method::GET, "/v1/approvals")
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn get_approval(&self, approval_id: &str) -> Result<ApprovalRequest> {
        let path = format!("/v1/approvals/{}", approval_id);
        let resp = self.request(reqwest::Method::GET, &path).send().await?;
        self.decode_json(resp).await
    }

    async fn get_lineage(&self, execution_id: &str) -> Result<LineageResponse> {
        let path = format!("/v1/provenance/lineage/{}", execution_id);
        let resp = self.request(reqwest::Method::GET, &path).send().await?;
        self.decode_json(resp).await
    }

    async fn query_provenance(
        &self,
        query: &ProvenanceQueryRequest,
    ) -> Result<ProvenanceQueryResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/v1/provenance/query")
            .json(query)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn decode_json<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        if !resp.status().is_success() {
            return self.render_error(resp).await;
        }

        Ok(resp.json().await?)
    }

    async fn render_error<T>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if body.is_empty() {
            bail!("HTTP {}: (empty body)", status);
        }
        // Try to parse as ApiError
        if let Ok(err) = serde_json::from_str::<ApiError>(&body) {
            bail!("HTTP {} [{}]: {}", status, err.code, err.message);
        }
        bail!("HTTP {}: {}", status, body);
    }
}

// =============================================================================
// CLI definition
// =============================================================================

#[derive(Debug, Parser)]
#[command(name = "ferrumctl")]
#[command(about = "FerrumGate control CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Server commands for remote inspection and control.
    Server {
        #[command(subcommand)]
        sub: ServerCommand,
    },
    /// Debug commands for repository introspection.
    Debug {
        #[command(subcommand)]
        sub: DebugCommand,
    },
    /// Inspect commands for querying known data.
    Inspect {
        #[command(subcommand)]
        sub: InspectCommand,
    },
    /// Validate commands for checking repository state.
    Validate {
        #[command(subcommand)]
        sub: ValidateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ServerCommand {
    /// Check server health.
    Health {
        /// Server base URL (e.g. http://127.0.0.1:8080).
        /// Can also be set via FERRUMCTL_SERVER_URL env var.
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        /// Can also be set via FERRUMCTL_BEARER_TOKEN env var.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,
    },
    /// Inspect an execution by ID.
    InspectExecution {
        /// Execution ID (UUID).
        execution_id: String,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },
    /// List pending approvals.
    InspectApprovals {
        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },
    /// Inspect a specific approval by ID.
    InspectApproval {
        /// Approval ID (UUID).
        approval_id: String,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },
    /// Inspect execution lineage (event chain).
    InspectLineage {
        /// Execution ID (UUID).
        execution_id: String,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },
    /// Query provenance events with filters.
    InspectProvenance {
        /// Filter by intent ID.
        #[arg(long)]
        intent_id: Option<String>,

        /// Filter by execution ID.
        #[arg(long)]
        execution_id: Option<String>,

        /// Filter by capability ID.
        #[arg(long)]
        capability_id: Option<String>,

        /// Filter by event kind.
        #[arg(long)]
        event_kind: Option<String>,

        /// Filter events since timestamp (ISO 8601).
        #[arg(long)]
        since: Option<String>,

        /// Filter events until timestamp (ISO 8601).
        #[arg(long)]
        until: Option<String>,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum DebugCommand {
    /// Print the resolved repository root path.
    RepoRoot,
}

#[derive(Debug, Subcommand)]
enum InspectCommand {
    /// Print the known contract paths, one per line or as JSON.
    Contracts {
        /// Output paths as a JSON array instead of one per line.
        #[clap(long)]
        json: bool,
    },
    /// Print the schema inventory with presence status.
    Schemas {
        /// Output inventory as a JSON array instead of plain text.
        #[clap(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ValidateCommand {
    /// Run repository validation using check_contract_consistency.py.
    Repo,
}

// =============================================================================
// CLI handlers
// =============================================================================

fn resolve_server_url(url: Option<String>) -> Result<String> {
    url.or_else(|| std::env::var("FERRUMCTL_SERVER_URL").ok())
        .or_else(|| Some("http://127.0.0.1:8080".to_string()))
        .context("failed to resolve server URL")
}

async fn run_server_health(url: Option<String>, token: Option<String>) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let health = client.health().await?;
    println!("{}", health.status);
    Ok(())
}

async fn run_inspect_execution(
    execution_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let record = client.get_execution(execution_id).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&record)?);
    } else {
        println!("Execution: {}", record.execution_id);
        println!("  State:     {}", record.state);
        println!("  Decision:  {}", record.decision);
        println!("  Intent:    {}", record.intent_id);
        println!("  Proposal:  {}", record.proposal_id);
        println!("  Capability:{}", record.capability_id);
        if let Some(cid) = record.rollback_contract_id {
            println!("  Rollback:  {}", cid);
        }
        if let Some(digest) = record.result_digest {
            println!("  Digest:    {}", digest);
        }
        println!("  Started:   {}", record.started_at);
        if let Some(finished) = record.finished_at {
            println!("  Finished:  {}", finished);
        }
    }
    Ok(())
}

async fn run_inspect_approvals(
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let approvals = client.list_approvals().await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&approvals)?);
    } else {
        if approvals.is_empty() {
            println!("No pending approvals.");
            return Ok(());
        }
        for approval in approvals {
            println!("Approval: {}", approval.approval_id);
            println!("  State:    {}", approval.state);
            println!("  Intent:   {}", approval.intent_id);
            println!("  Reason:   {}", approval.reason);
            println!("  Action:   {}", approval.action_digest);
            println!("  Created:  {}", approval.created_at);
            println!("  Expires:  {}", approval.expires_at);
            println!();
        }
    }
    Ok(())
}

async fn run_inspect_approval(
    approval_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let approval = client.get_approval(approval_id).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&approval)?);
    } else {
        println!("Approval: {}", approval.approval_id);
        println!("  State:    {}", approval.state);
        println!("  Intent:   {}", approval.intent_id);
        println!("  Proposal: {}", approval.proposal_id);
        if let Some(eid) = approval.execution_id {
            println!("  Execution:{}", eid);
        }
        println!("  Reason:   {}", approval.reason);
        println!("  Action:   {}", approval.action_digest);
        println!("  Created:  {}", approval.created_at);
        println!("  Expires:  {}", approval.expires_at);
    }
    Ok(())
}

async fn run_inspect_lineage(
    execution_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let lineage = client.get_lineage(execution_id).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&lineage)?);
    } else {
        println!("Lineage for execution: {}", lineage.execution_id);
        println!("{} events:", lineage.events.len());
        for event in lineage.events {
            println!(
                "  [{}] {}  {}",
                event.occurred_at, event.kind, event.event_id
            );
            if let Some(iid) = event.intent_id {
                println!("    intent: {}", iid);
            }
            if let Some(pid) = event.proposal_id {
                println!("    proposal: {}", pid);
            }
            if let Some(eid) = event.execution_id {
                println!("    execution: {}", eid);
            }
        }
    }
    Ok(())
}

async fn run_inspect_provenance(options: InspectProvenanceOptions) -> Result<()> {
    let InspectProvenanceOptions {
        query,
        url,
        token,
        as_json,
    } = options;

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    let response = client.query_provenance(&query).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        if response.events.is_empty() {
            println!("No events found.");
            return Ok(());
        }
        println!("{} events:", response.events.len());
        for event in response.events {
            println!(
                "  [{}] {}  {}",
                event.occurred_at, event.kind, event.event_id
            );
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Server { sub } => match sub {
            ServerCommand::Health {
                server_url,
                bearer_token,
            } => {
                run_server_health(server_url, bearer_token).await?;
            }
            ServerCommand::InspectExecution {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_execution(&execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::InspectApprovals {
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_approvals(server_url, bearer_token, json).await?;
            }
            ServerCommand::InspectApproval {
                approval_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_approval(&approval_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::InspectLineage {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_lineage(&execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::InspectProvenance {
                intent_id,
                execution_id,
                capability_id,
                event_kind,
                since,
                until,
                server_url,
                bearer_token,
                json,
            } => {
                let query = ProvenanceQueryRequest {
                    intent_id,
                    execution_id,
                    capability_id,
                    event_kind,
                    since,
                    until,
                };
                run_inspect_provenance(InspectProvenanceOptions {
                    query,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                })
                .await?;
            }
        },
        Command::Debug { sub } => match sub {
            DebugCommand::RepoRoot => {
                println!("{}", repo_root().display());
            }
        },
        Command::Inspect { sub } => match sub {
            InspectCommand::Contracts { json } => {
                let paths = known_contract_paths();
                println!("{}", format_contract_paths(&paths, json));
            }
            InspectCommand::Schemas { json } => {
                let root = repo_root();
                let inventory = build_schema_inventory(&root);
                if json {
                    println!("{}", format_schema_inventory_json(&inventory));
                } else {
                    println!("{}", format_schema_inventory(&inventory));
                }
            }
        },
        Command::Validate { sub } => match sub {
            ValidateCommand::Repo => {
                run_contract_check()?;
                println!("ValidateRepo: OK");
            }
        },
    }
    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_root_is_absolute() {
        let root = repo_root();
        assert!(
            root.is_absolute(),
            "repo_root should return an absolute path"
        );
    }

    #[test]
    fn test_repo_root_contains_contracts_dir() {
        let root = repo_root();
        assert!(
            root.join("contracts").exists(),
            "repo_root should point to a directory containing contracts/"
        );
    }

    #[test]
    fn test_known_contract_paths_not_empty() {
        let paths = known_contract_paths();
        assert!(
            !paths.is_empty(),
            "known_contract_paths should not be empty"
        );
    }

    #[test]
    fn test_known_contract_paths_contains_expected() {
        let paths = known_contract_paths();
        assert!(
            paths.contains(&"contracts/ferrumgate-agent-contract.v1.yaml"),
            "should contain agent contract"
        );
        assert!(
            paths.contains(&"contracts/ferrumgate-integrator-contract.v1.yaml"),
            "should contain integrator contract"
        );
    }

    #[test]
    fn test_contract_paths_are_relative() {
        for path in known_contract_paths() {
            assert!(
                !path.starts_with('/'),
                "contract path '{path}' should be relative"
            );
        }
    }

    #[test]
    fn test_format_contract_paths_plain_text() {
        let paths = ["a.txt", "b.txt"];
        let result = format_contract_paths(&paths, false);
        assert_eq!(result, "a.txt\nb.txt");
    }

    #[test]
    fn test_format_contract_paths_json() {
        let paths = ["a.txt", "b.txt"];
        let result = format_contract_paths(&paths, true);
        assert_eq!(result, r#"["a.txt","b.txt"]"#);
    }

    #[test]
    fn test_format_contract_paths_single_item_plain() {
        let paths = ["only.txt"];
        let result = format_contract_paths(&paths, false);
        assert_eq!(result, "only.txt");
    }

    #[test]
    fn test_format_contract_paths_single_item_json() {
        let paths = ["only.txt"];
        let result = format_contract_paths(&paths, true);
        assert_eq!(result, r#"["only.txt"]"#);
    }

    #[test]
    fn test_format_contract_paths_empty_plain() {
        let paths: [&str; 0] = [];
        let result = format_contract_paths(&paths, false);
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_contract_paths_empty_json() {
        let paths: [&str; 0] = [];
        let result = format_contract_paths(&paths, true);
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_known_schema_paths_not_empty() {
        let paths = known_schema_paths();
        assert!(!paths.is_empty(), "known_schema_paths should not be empty");
        assert!(
            paths.contains(&"schemas/jsonschema/intent-envelope.json"),
            "should contain intent-envelope.json"
        );
    }

    #[test]
    fn test_schema_paths_are_relative() {
        for path in known_schema_paths() {
            assert!(
                !path.starts_with('/'),
                "schema path '{path}' should be relative"
            );
        }
    }

    #[test]
    fn test_schema_inventory_count() {
        let root = repo_root();
        let inventory = build_schema_inventory(&root);
        assert_eq!(
            inventory.len(),
            SCHEMA_PATHS.len(),
            "inventory should have entry per schema path"
        );
    }

    #[test]
    fn test_format_schema_inventory_sorted() {
        // Verify alphabetical sorting regardless of status prefix
        let entries = &[
            SchemaEntry {
                path: "z-schema.json",
                present: false,
            },
            SchemaEntry {
                path: "a-schema.json",
                present: true,
            },
        ];
        let result = format_schema_inventory(entries);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        // After sorting, "a-schema.json" line comes before "z-schema.json" line
        assert!(
            lines[0].contains("a-schema"),
            "first line should contain a-schema (alphabetically first)"
        );
        assert!(
            lines[1].contains("z-schema"),
            "second line should contain z-schema (alphabetically second)"
        );
    }

    #[test]
    fn test_format_schema_inventory_missing_line() {
        let entries = &[SchemaEntry {
            path: "schemas/jsonschema/missing.json",
            present: false,
        }];
        let result = format_schema_inventory(entries);
        assert!(
            result.starts_with("missing  "),
            "should start with 'missing'"
        );
        assert!(result.contains("schemas/jsonschema/missing.json"));
    }

    #[test]
    fn test_format_schema_inventory_multiple() {
        let entries = &[
            SchemaEntry {
                path: "b.json",
                present: true,
            },
            SchemaEntry {
                path: "a.json",
                present: false,
            },
        ];
        let result = format_schema_inventory(entries);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(result.contains("ok  b.json"));
        assert!(result.contains("missing  a.json"));
    }

    #[test]
    fn test_format_schema_inventory_json_array_structure() {
        let entries = &[SchemaEntry {
            path: "schemas/jsonschema/test.json",
            present: true,
        }];
        let result = format_schema_inventory_json(entries);
        let parsed: Vec<SchemaEntryJson> =
            serde_json::from_str(&result).expect("must be valid JSON");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].path, "schemas/jsonschema/test.json");
        assert!(parsed[0].present);
    }

    #[test]
    fn test_format_schema_inventory_json_sorted() {
        let entries = &[
            SchemaEntry {
                path: "z-schema.json",
                present: false,
            },
            SchemaEntry {
                path: "a-schema.json",
                present: true,
            },
        ];
        let result = format_schema_inventory_json(entries);
        let parsed: Vec<SchemaEntryJson> =
            serde_json::from_str(&result).expect("must be valid JSON");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, "a-schema.json");
        assert_eq!(parsed[1].path, "z-schema.json");
    }

    #[test]
    fn test_format_schema_inventory_json_multiple() {
        let entries = &[
            SchemaEntry {
                path: "b.json",
                present: true,
            },
            SchemaEntry {
                path: "a.json",
                present: false,
            },
        ];
        let result = format_schema_inventory_json(entries);
        let parsed: Vec<SchemaEntryJson> =
            serde_json::from_str(&result).expect("must be valid JSON");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, "a.json");
        assert!(!parsed[0].present);
        assert_eq!(parsed[1].path, "b.json");
        assert!(parsed[1].present);
    }
}
