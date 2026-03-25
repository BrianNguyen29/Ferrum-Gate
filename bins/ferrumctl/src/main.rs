use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
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
struct ApprovalListEnvelope {
    items: Vec<ApprovalRequest>,
    #[serde(default)]
    next_cursor: Option<String>,
}

/// Edge from a parent event to this event.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceEdge {
    from_event_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceEvent {
    event_id: String,
    kind: String,
    occurred_at: String,
    intent_id: Option<String>,
    proposal_id: Option<String>,
    execution_id: Option<String>,
    #[serde(default)]
    parent_edges: Vec<ProvenanceEdge>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceEventResponse {
    event: ProvenanceEvent,
    #[serde(default)]
    ancestry: Option<Vec<ProvenanceEvent>>,
    #[serde(default)]
    descendants: Option<Vec<ProvenanceEvent>>,
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
    proposal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capability_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    until: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceQueryResponse {
    events: Vec<ProvenanceEvent>,
}

// =============================================================================
// External event ingest types
// =============================================================================

#[derive(Debug, Clone, Serialize)]
struct ExternalEventIngestRequest {
    execution_id: String,
    parent_event_id: String,
    source_system: String,
    source_event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    observed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExternalEventIngestResponse {
    event: ProvenanceEvent,
}

struct InspectProvenanceOptions {
    query: ProvenanceQueryRequest,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ListApprovalsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proposal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_id: Option<String>,
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

    async fn ready(&self) -> Result<HealthResponse> {
        let resp = self
            .request(reqwest::Method::GET, "/v1/readyz")
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn get_execution(&self, execution_id: &str) -> Result<ExecutionRecord> {
        let path = format!("/v1/executions/{}", execution_id);
        let resp = self.request(reqwest::Method::GET, &path).send().await?;
        self.decode_json(resp).await
    }

    async fn list_approvals(&self, query: &ListApprovalsQuery) -> Result<ApprovalListEnvelope> {
        let resp = self
            .request(reqwest::Method::GET, "/v1/approvals")
            .query(query)
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

    async fn get_event(
        &self,
        event_id: &str,
        ancestry: bool,
        descendants: bool,
    ) -> Result<ProvenanceEventResponse> {
        let path = format!("/v1/provenance/events/{}", event_id);
        let resp = self
            .request(reqwest::Method::GET, &path)
            .query(&[
                ("ancestry", ancestry.to_string()),
                ("descendants", descendants.to_string()),
            ])
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn post_external_event(
        &self,
        req: &ExternalEventIngestRequest,
    ) -> Result<ExternalEventIngestResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/v1/provenance/events/external")
            .json(req)
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LineageFormat {
    /// Human-readable text output (default).
    Text,
    /// JSON array of events.
    Json,
    /// Graphviz DOT format for visualization.
    Dot,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Server commands for remote inspection and control.
    Server {
        #[command(subcommand)]
        sub: Box<ServerCommand>,
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
    /// Check deep readiness.
    Ready {
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
        /// Maximum approvals to return.
        #[arg(long)]
        limit: Option<u32>,

        /// Cursor returned by a previous approval listing.
        #[arg(long)]
        cursor: Option<String>,

        /// Filter by proposal ID (UUID).
        #[arg(long)]
        proposal_id: Option<String>,

        /// Filter by execution ID (UUID).
        #[arg(long)]
        execution_id: Option<String>,

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

        /// Output format: text (default), json, or dot (Graphviz).
        #[arg(long, value_enum, default_value = "text")]
        format: LineageFormat,

        /// Output file path. When set, writes to file instead of stdout.
        /// Required for dot format when redirecting to a file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Query provenance events with filters.
    InspectProvenance {
        /// Filter by intent ID.
        #[arg(long)]
        intent_id: Option<String>,

        /// Filter by proposal ID.
        #[arg(long)]
        proposal_id: Option<String>,

        /// Filter by execution ID.
        #[arg(long)]
        execution_id: Option<String>,

        /// Filter by capability ID.
        #[arg(long)]
        capability_id: Option<String>,

        /// Filter by event kind.
        #[arg(long)]
        event_kind: Option<String>,

        /// Return only terminal provenance events.
        #[arg(long)]
        terminal_only: bool,

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
    /// Inspect a single provenance event by ID with optional ancestry/descendants.
    InspectEvent {
        /// Event ID (UUID).
        event_id: String,

        /// Include ancestor events in response.
        #[arg(long)]
        ancestry: bool,

        /// Include descendant events in response.
        #[arg(long)]
        descendants: bool,

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
    /// Ingest an externally-observed runtime event into the provenance lineage.
    IngestExternalEvent {
        /// Execution ID (UUID) to anchor the external event to.
        #[arg(long)]
        execution_id: String,

        /// Parent event ID (UUID) within the same execution that this external event observes.
        #[arg(long)]
        parent_event_id: String,

        /// Identifier for the external system or runtime that observed this event.
        #[arg(long)]
        source_system: String,

        /// Event identifier assigned by the external source system.
        #[arg(long)]
        source_event_id: String,

        /// Wall-clock time when the external system observed the event (ISO 8601).
        #[arg(long)]
        observed_at: Option<String>,

        /// Human-readable summary describing what was observed.
        #[arg(long)]
        summary: Option<String>,

        /// Digest of the external event payload for integrity verification.
        #[arg(long)]
        payload_digest: Option<String>,

        /// JSON object of metadata to attach to the external event.
        /// Must be a JSON object (e.g. --metadata-json '{"key":"value"}').
        #[arg(long, value_parser = parse_metadata_json)]
        metadata_json: Option<serde_json::Map<String, serde_json::Value>>,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output the returned event as JSON instead of human-readable.
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

async fn run_server_ready(url: Option<String>, token: Option<String>) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let ready = client.ready().await?;
    println!("{}", ready.status);
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
    limit: Option<u32>,
    cursor: Option<String>,
    proposal_id: Option<String>,
    execution_id: Option<String>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let approvals = client
        .list_approvals(&ListApprovalsQuery {
            limit,
            cursor,
            proposal_id,
            execution_id,
        })
        .await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&approvals)?);
    } else {
        if approvals.items.is_empty() {
            println!("No pending approvals.");
            return Ok(());
        }
        for approval in approvals.items {
            println!("Approval: {}", approval.approval_id);
            println!("  State:    {}", approval.state);
            println!("  Intent:   {}", approval.intent_id);
            println!("  Proposal: {}", approval.proposal_id);
            if let Some(execution_id) = approval.execution_id {
                println!("  Execution:{}", execution_id);
            }
            println!("  Reason:   {}", approval.reason);
            println!("  Action:   {}", approval.action_digest);
            println!("  Created:  {}", approval.created_at);
            println!("  Expires:  {}", approval.expires_at);
            println!();
        }
        if let Some(next_cursor) = approvals.next_cursor {
            println!("Next cursor: {}", next_cursor);
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
    format: LineageFormat,
    output: Option<PathBuf>,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let lineage = client.get_lineage(execution_id).await?;

    let rendered = match format {
        LineageFormat::Json => serde_json::to_string_pretty(&lineage)?,
        LineageFormat::Text => {
            let mut s = format!("Lineage for execution: {}\n", lineage.execution_id);
            s.push_str(&format!("{} events:\n", lineage.events.len()));
            for event in lineage.events {
                s.push_str(&format!(
                    "  [{}] {}  {}\n",
                    event.occurred_at, event.kind, event.event_id
                ));
                if let Some(iid) = &event.intent_id {
                    s.push_str(&format!("    intent: {}\n", iid));
                }
                if let Some(pid) = &event.proposal_id {
                    s.push_str(&format!("    proposal: {}\n", pid));
                }
                if let Some(eid) = &event.execution_id {
                    s.push_str(&format!("    execution: {}\n", eid));
                }
            }
            s
        }
        LineageFormat::Dot => render_lineage_dot(&lineage),
    };

    if let Some(path) = output {
        std::fs::write(&path, &rendered)
            .with_context(|| format!("failed to write to {}", path.display()))?;
    } else {
        println!("{}", rendered);
    }

    Ok(())
}

/// Renders a `LineageResponse` as a deterministic Graphviz DOT graph.
/// The graph is named after the execution ID and lists all events as nodes
/// with directed edges representing parent→child relationships via parent_edges.
fn render_lineage_dot(lineage: &LineageResponse) -> String {
    let exec_id = &lineage.execution_id;
    let mut lines = Vec::new();
    lines.push(format!("digraph {} {{", dot_escape(exec_id)));
    lines.push("  rankdir=TB;".to_string());
    lines.push("  node [shape=box fontname=\"Helvetica\"];".to_string());

    // Collect edges first for determinism: sort by (parent, child)
    // Edges come from parent_edges: from_event_id -> event.event_id
    let mut edges: Vec<(&str, &str)> = Vec::new();
    for event in &lineage.events {
        for parent_edge in &event.parent_edges {
            edges.push((parent_edge.from_event_id.as_str(), event.event_id.as_str()));
        }
    }
    edges.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));

    // Render event nodes (deduplicated by event_id)
    let mut event_ids: Vec<&str> = lineage.events.iter().map(|e| e.event_id.as_str()).collect();
    event_ids.sort();
    event_ids.dedup();

    for eid in &event_ids {
        lines.push(format!(
            "  \"{}\" [label=\"{}\"];",
            dot_escape(eid),
            dot_escape(eid)
        ));
    }

    // Render edges
    for (parent, child) in &edges {
        lines.push(format!(
            "  \"{}\" -> \"{}\";",
            dot_escape(parent),
            dot_escape(child)
        ));
    }

    lines.push("}".to_string());
    lines.join("\n")
}

/// Escapes a string for safe use inside a DOT node label or node ID.
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Parses a JSON string into a Map<String, JsonValue>.
/// Returns an error if the input is not a JSON object.
fn parse_metadata_json(s: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let value: serde_json::Value =
        serde_json::from_str(s).map_err(|e| format!("invalid JSON: {}", e))?;
    match value {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(String::from("metadata must be a JSON object")),
    }
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

async fn run_inspect_event(
    event_id: &str,
    ancestry: bool,
    descendants: bool,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let response = client.get_event(event_id, ancestry, descendants).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Event: {}", response.event.event_id);
        println!("  Kind:       {}", response.event.kind);
        println!("  Occurred:   {}", response.event.occurred_at);
        if let Some(iid) = response.event.intent_id {
            println!("  Intent:     {}", iid);
        }
        if let Some(pid) = response.event.proposal_id {
            println!("  Proposal:   {}", pid);
        }
        if let Some(eid) = response.event.execution_id {
            println!("  Execution:  {}", eid);
        }
        if let Some(anc) = response.ancestry {
            println!("\nAncestry ({} events):", anc.len());
            for e in anc {
                println!("  [{}] {}  {}", e.occurred_at, e.kind, e.event_id);
            }
        }
        if let Some(desc) = response.descendants {
            println!("\nDescendants ({} events):", desc.len());
            for e in desc {
                println!("  [{}] {}  {}", e.occurred_at, e.kind, e.event_id);
            }
        }
    }
    Ok(())
}

async fn run_ingest_external_event(
    execution_id: String,
    parent_event_id: String,
    source_system: String,
    source_event_id: String,
    observed_at: Option<String>,
    summary: Option<String>,
    payload_digest: Option<String>,
    metadata_json: Option<serde_json::Map<String, serde_json::Value>>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    let req = ExternalEventIngestRequest {
        execution_id,
        parent_event_id,
        source_system,
        source_event_id,
        observed_at,
        summary,
        payload_digest,
        metadata: metadata_json,
    };

    let response = client.post_external_event(&req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&response.event)?);
    } else {
        println!("External event ingested successfully.");
        println!("  Event ID:    {}", response.event.event_id);
        println!("  Kind:        {}", response.event.kind);
        println!("  Occurred at: {}", response.event.occurred_at);
        if let Some(iid) = &response.event.intent_id {
            println!("  Intent ID:   {}", iid);
        }
        if let Some(pid) = &response.event.proposal_id {
            println!("  Proposal ID: {}", pid);
        }
        if let Some(eid) = &response.event.execution_id {
            println!("  Execution:   {}", eid);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Server { sub } => match *sub {
            ServerCommand::Health {
                server_url,
                bearer_token,
            } => {
                run_server_health(server_url, bearer_token).await?;
            }
            ServerCommand::Ready {
                server_url,
                bearer_token,
            } => {
                run_server_ready(server_url, bearer_token).await?;
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
                limit,
                cursor,
                proposal_id,
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_approvals(
                    limit,
                    cursor,
                    proposal_id,
                    execution_id,
                    server_url,
                    bearer_token,
                    json,
                )
                .await?;
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
                format,
                output,
            } => {
                run_inspect_lineage(&execution_id, server_url, bearer_token, format, output)
                    .await?;
            }
            ServerCommand::InspectProvenance {
                intent_id,
                proposal_id,
                execution_id,
                capability_id,
                event_kind,
                terminal_only,
                since,
                until,
                server_url,
                bearer_token,
                json,
            } => {
                let query = ProvenanceQueryRequest {
                    intent_id,
                    proposal_id,
                    execution_id,
                    capability_id,
                    event_kind,
                    terminal_only: terminal_only.then_some(true),
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
            ServerCommand::InspectEvent {
                event_id,
                ancestry,
                descendants,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_event(
                    &event_id,
                    ancestry,
                    descendants,
                    server_url,
                    bearer_token,
                    json,
                )
                .await?;
            }
            ServerCommand::IngestExternalEvent {
                execution_id,
                parent_event_id,
                source_system,
                source_event_id,
                observed_at,
                summary,
                payload_digest,
                metadata_json,
                server_url,
                bearer_token,
                json,
            } => {
                run_ingest_external_event(
                    execution_id,
                    parent_event_id,
                    source_system,
                    source_event_id,
                    observed_at,
                    summary,
                    payload_digest,
                    metadata_json,
                    server_url,
                    bearer_token,
                    json,
                )
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

    // -------------------------------------------------------------------------
    // DOT rendering tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_render_lineage_dot_empty() {
        let lineage = LineageResponse {
            execution_id: "exec-0".to_string(),
            events: vec![],
        };
        let dot = render_lineage_dot(&lineage);
        assert!(dot.starts_with("digraph exec-0 {"));
        assert!(dot.contains("rankdir=TB;"));
        assert!(dot.ends_with("}"));
        // no nodes or edges for empty events
        assert!(!dot.contains(" -> "));
    }

    #[test]
    fn test_render_lineage_dot_determinism() {
        let lineage = LineageResponse {
            execution_id: "exec-deterministic".to_string(),
            events: vec![
                ProvenanceEvent {
                    event_id: "evt-2".to_string(),
                    kind: "executed".to_string(),
                    occurred_at: "2024-01-01T00:00:00Z".to_string(),
                    intent_id: Some("intent-a".to_string()),
                    proposal_id: Some("prop-1".to_string()),
                    execution_id: Some("evt-1".to_string()),
                    parent_edges: vec![ProvenanceEdge {
                        from_event_id: "evt-1".to_string(),
                    }],
                },
                ProvenanceEvent {
                    event_id: "evt-1".to_string(),
                    kind: "proposed".to_string(),
                    occurred_at: "2024-01-01T00:00:01Z".to_string(),
                    intent_id: Some("intent-a".to_string()),
                    proposal_id: None,
                    execution_id: None,
                    parent_edges: vec![],
                },
            ],
        };
        let dot1 = render_lineage_dot(&lineage);
        let dot2 = render_lineage_dot(&lineage);
        assert_eq!(dot1, dot2, "DOT output must be deterministic");
    }

    #[test]
    fn test_render_lineage_dot_edges() {
        let lineage = LineageResponse {
            execution_id: "exec-edges".to_string(),
            events: vec![
                ProvenanceEvent {
                    event_id: "evt-child".to_string(),
                    kind: "executed".to_string(),
                    occurred_at: "2024-01-01T00:00:02Z".to_string(),
                    intent_id: None,
                    proposal_id: Some("prop-parent".to_string()),
                    execution_id: Some("evt-parent".to_string()),
                    parent_edges: vec![ProvenanceEdge {
                        from_event_id: "evt-parent".to_string(),
                    }],
                },
                ProvenanceEvent {
                    event_id: "evt-parent".to_string(),
                    kind: "proposed".to_string(),
                    occurred_at: "2024-01-01T00:00:01Z".to_string(),
                    intent_id: None,
                    proposal_id: None,
                    execution_id: None,
                    parent_edges: vec![],
                },
            ],
        };
        let dot = render_lineage_dot(&lineage);
        // Should contain exactly one edge from parent to child
        assert!(dot.contains("\"evt-parent\" -> \"evt-child\""));
        // Should not contain duplicate edges
        let edge_count = dot.matches("\"evt-parent\" -> \"evt-child\"").count();
        assert_eq!(edge_count, 1, "edge should appear exactly once");
    }

    #[test]
    fn test_render_lineage_dot_escapes_special_chars() {
        let lineage = LineageResponse {
            execution_id: "exec\"special\\path".to_string(),
            events: vec![ProvenanceEvent {
                event_id: "evt\"new\nline".to_string(),
                kind: "kind".to_string(),
                occurred_at: "2024-01-01T00:00:00Z".to_string(),
                intent_id: None,
                proposal_id: None,
                execution_id: None,
                parent_edges: vec![],
            }],
        };
        let dot = render_lineage_dot(&lineage);
        // Escaped characters should not break DOT syntax
        assert!(!dot.contains("digraph exec\"special"));
        assert!(dot.contains("digraph exec\\\"special"));
    }

    #[test]
    fn test_render_lineage_dot_no_extraneous_edges() {
        // Events without parent_edges should not create edges
        let lineage = LineageResponse {
            execution_id: "exec-no-edge".to_string(),
            events: vec![
                ProvenanceEvent {
                    event_id: "evt-orphan".to_string(),
                    kind: "orphan".to_string(),
                    occurred_at: "2024-01-01T00:00:00Z".to_string(),
                    intent_id: None,
                    proposal_id: None,
                    execution_id: None,
                    parent_edges: vec![],
                },
                ProvenanceEvent {
                    event_id: "evt-half".to_string(),
                    kind: "half".to_string(),
                    occurred_at: "2024-01-01T00:00:01Z".to_string(),
                    intent_id: None,
                    proposal_id: Some("prop-only".to_string()),
                    execution_id: None,
                    parent_edges: vec![],
                },
            ],
        };
        let dot = render_lineage_dot(&lineage);
        // No edges should be present since no event has parent_edges
        assert!(!dot.contains(" -> "));
        // Both nodes should still be present
        assert!(dot.contains("\"evt-orphan\""));
        assert!(dot.contains("\"evt-half\""));
    }

    // -------------------------------------------------------------------------
    // External event metadata parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_metadata_json_valid_object() {
        let input = r#"{"key":"value","num":42}"#;
        let result = parse_metadata_json(input);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("key").unwrap(),
            &serde_json::Value::String("value".to_string())
        );
        assert_eq!(
            map.get("num").unwrap(),
            &serde_json::Value::Number(42.into())
        );
    }

    #[test]
    fn test_parse_metadata_json_empty_object() {
        let input = "{}";
        let result = parse_metadata_json(input);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_metadata_json_nested_object() {
        let input = r#"{"outer":{"inner":"value"}}"#;
        let result = parse_metadata_json(input);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert!(map.contains_key("outer"));
    }

    #[test]
    fn test_parse_metadata_json_invalid_json() {
        let input = "not json at all";
        let result = parse_metadata_json(input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("invalid JSON"));
    }

    #[test]
    fn test_parse_metadata_json_array_rejected() {
        let input = r#"[1,2,3]"#;
        let result = parse_metadata_json(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "metadata must be a JSON object");
    }

    #[test]
    fn test_parse_metadata_json_string_rejected() {
        let input = r#""just a string""#;
        let result = parse_metadata_json(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "metadata must be a JSON object");
    }

    #[test]
    fn test_parse_metadata_json_number_rejected() {
        let input = "12345";
        let result = parse_metadata_json(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "metadata must be a JSON object");
    }

    #[test]
    fn test_parse_metadata_json_bool_rejected() {
        let input = "true";
        let result = parse_metadata_json(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "metadata must be a JSON object");
    }

    #[test]
    fn test_parse_metadata_json_null_rejected() {
        let input = "null";
        let result = parse_metadata_json(input);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "metadata must be a JSON object");
    }
}
