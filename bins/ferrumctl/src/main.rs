use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use ferrum_proto::api::{
    CancelExecutionRequest, CancelExecutionResponse, CompensateRequest, CompensateResponse,
    LedgerVerificationResponse, PauseExecutionRequest, PauseExecutionResponse, RollbackRequest,
    RollbackResponse,
};
use ferrum_proto::approval::ApprovalResolveRequest;
use ferrum_proto::common::{ActorRef, ActorType};
use ferrum_proto::provenance::{
    LineageQueryRequest, LineageQueryResponse, ProvenanceEdgeType, ProvenanceEventKind,
    ProvenanceExportRequest, ProvenanceExportResponse, ProvenanceReplayRequest,
    ProvenanceReplayResponse, ProvenanceStatsRequest, ProvenanceStatsResponse,
};
use ferrum_proto::{CapabilityId, ExecutionId, IntentId, ProposalId};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use uuid::Uuid;

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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    execution_ids: Vec<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProvenanceQueryResponse {
    events: Vec<ProvenanceEvent>,
    #[serde(default)]
    next_cursor: Option<String>,
}

// =============================================================================
// Provenance stats aggregation types
// =============================================================================

/// Terminal event kinds that represent completed execution outcomes.
const TERMINAL_KINDS: &[&str] = &[
    "SideEffectCommitted",
    "SideEffectCompensated",
    "SideEffectRolledBack",
    "ApprovalDenied",
    "Quarantined",
    "ErrorRaised",
];

/// Event kinds that indicate a problem condition worth flagging.
const ISSUE_KINDS: &[&str] = &[
    "ErrorRaised",
    "Quarantined",
    "ApprovalDenied",
    "SideEffectRolledBack",
];

/// Aggregated provenance statistics over a set of events.
#[derive(Debug, Clone, Default)]
struct ProvenanceStats {
    total_events: usize,
    kinds: std::collections::HashMap<String, usize>,
    terminal_count: usize,
    issue_count: usize,
    events_without_execution_id: usize,
    events_by_intent: std::collections::HashMap<String, usize>,
    events_by_proposal: std::collections::HashMap<String, usize>,
    events_by_execution: std::collections::HashMap<String, usize>,
    /// Events flagged by checks (event_id -> reason)
    flagged_events: Vec<FlaggedEvent>,
}

/// A single event flagged by a consistency check.
#[derive(Debug, Clone)]
struct FlaggedEvent {
    event_id: String,
    kind: String,
    reason: String,
}

/// JSON-serializable view of ProvenanceStats for --json output.
#[derive(Debug, Clone, Serialize)]
struct ProvenanceStatsJson {
    total_events: usize,
    kinds: std::collections::HashMap<String, usize>,
    terminal_count: usize,
    issue_count: usize,
    events_without_execution_id: usize,
    events_by_intent_count: usize,
    events_by_proposal_count: usize,
    events_by_execution_count: usize,
    flagged_events: Vec<FlaggedEventJson>,
}

#[derive(Debug, Clone, Serialize)]
struct FlaggedEventJson {
    event_id: String,
    kind: String,
    reason: String,
}

impl From<ProvenanceStats> for ProvenanceStatsJson {
    fn from(s: ProvenanceStats) -> Self {
        Self {
            total_events: s.total_events,
            kinds: s.kinds,
            terminal_count: s.terminal_count,
            issue_count: s.issue_count,
            events_without_execution_id: s.events_without_execution_id,
            events_by_intent_count: s.events_by_intent.len(),
            events_by_proposal_count: s.events_by_proposal.len(),
            events_by_execution_count: s.events_by_execution.len(),
            flagged_events: s
                .flagged_events
                .into_iter()
                .map(|f| FlaggedEventJson {
                    event_id: f.event_id,
                    kind: f.kind,
                    reason: f.reason,
                })
                .collect(),
        }
    }
}

/// Collects aggregate statistics from a list of provenance events.
fn aggregate_provenance_stats(events: &[ProvenanceEvent]) -> ProvenanceStats {
    let mut stats = ProvenanceStats {
        total_events: events.len(),
        ..Default::default()
    };

    // Build lookup sets for checks
    let mut event_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut execution_events: std::collections::HashMap<String, Vec<&ProvenanceEvent>> =
        std::collections::HashMap::new();

    for event in events {
        // Count by kind
        *stats.kinds.entry(event.kind.clone()).or_insert(0) += 1;

        // Check if terminal
        if TERMINAL_KINDS.contains(&event.kind.as_str()) {
            stats.terminal_count += 1;
        }

        // Check if issue
        if ISSUE_KINDS.contains(&event.kind.as_str()) {
            stats.issue_count += 1;
        }

        // Track events without execution_id
        if event.execution_id.is_none() {
            stats.events_without_execution_id += 1;
        }

        // Track by intent/proposal/execution
        if let Some(ref intent_id) = event.intent_id {
            *stats.events_by_intent.entry(intent_id.clone()).or_insert(0) += 1;
        }
        if let Some(ref proposal_id) = event.proposal_id {
            *stats
                .events_by_proposal
                .entry(proposal_id.clone())
                .or_insert(0) += 1;
        }
        if let Some(ref execution_id) = event.execution_id {
            *stats
                .events_by_execution
                .entry(execution_id.clone())
                .or_insert(0) += 1;
            execution_events
                .entry(execution_id.clone())
                .or_default()
                .push(event);
        }

        event_ids.insert(event.event_id.clone());
    }

    // Check: terminal events without execution_id (data inconsistency)
    for event in events {
        if TERMINAL_KINDS.contains(&event.kind.as_str()) && event.execution_id.is_none() {
            stats.flagged_events.push(FlaggedEvent {
                event_id: event.event_id.clone(),
                kind: event.kind.clone(),
                reason: "terminal event missing execution_id".to_string(),
            });
        }
    }

    // Check: orphan terminal events - terminal events whose execution has no non-terminal ancestors
    // An execution is considered "complete" if it has any terminal events
    for (exec_id, exec_events) in &execution_events {
        let has_terminal = exec_events
            .iter()
            .any(|e| TERMINAL_KINDS.contains(&e.kind.as_str()));
        if !has_terminal && !exec_events.is_empty() {
            // Non-terminal-only execution - flag if no parent_edges (potential gap)
            for event in exec_events {
                if event.parent_edges.is_empty() && exec_events.len() > 1 {
                    stats.flagged_events.push(FlaggedEvent {
                        event_id: event.event_id.clone(),
                        kind: event.kind.clone(),
                        reason: format!(
                            "execution {} has {} events but root has no parent_edges",
                            exec_id,
                            exec_events.len()
                        ),
                    });
                    break; // Only flag one per execution
                }
            }
        }
    }

    // Limit flagged events to avoid overwhelming output
    if stats.flagged_events.len() > 100 {
        stats.flagged_events.truncate(100);
    }

    stats
}

/// Formats provenance stats as human-readable text.
fn format_provenance_stats_text(stats: &ProvenanceStats) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Total events: {}", stats.total_events));
    lines.push(format!("Terminal events: {}", stats.terminal_count));
    lines.push(format!(
        "Issue events (error/denied/quarantined/rolledback): {}",
        stats.issue_count
    ));
    lines.push(format!(
        "Events missing execution_id: {}",
        stats.events_without_execution_id
    ));
    lines.push(format!(
        "Unique intents: {}, proposals: {}, executions: {}",
        stats.events_by_intent.len(),
        stats.events_by_proposal.len(),
        stats.events_by_execution.len()
    ));

    // Sort kinds by count descending for readability
    let mut kinds: Vec<(String, usize)> =
        stats.kinds.iter().map(|(k, v)| (k.clone(), *v)).collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1));
    lines.push("\nEvents by kind:".to_string());
    for (kind, count) in kinds {
        lines.push(format!("  {}: {}", kind, count));
    }

    if !stats.flagged_events.is_empty() {
        lines.push(format!(
            "\nFlagged events ({}):",
            stats.flagged_events.len()
        ));
        for flagged in &stats.flagged_events {
            lines.push(format!(
                "  [{}] {}  {}",
                flagged.kind, flagged.event_id, flagged.reason
            ));
        }
    } else {
        lines.push("\nNo flagged events.".to_string());
    }

    lines.join("\n")
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
    all_pages: bool,
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

    async fn resolve_approval(
        &self,
        approval_id: &str,
        req: &ApprovalResolveRequest,
    ) -> Result<ApprovalRequest> {
        let path = format!("/v1/approvals/{}/resolve", approval_id);
        let resp = self
            .request(reqwest::Method::POST, &path)
            .json(req)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn compensate_execution(
        &self,
        execution_id: &str,
        req: &CompensateRequest,
    ) -> Result<CompensateResponse> {
        let path = format!("/v1/executions/{}/compensate", execution_id);
        let resp = self
            .request(reqwest::Method::POST, &path)
            .json(req)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn rollback_execution(
        &self,
        execution_id: &str,
        req: &RollbackRequest,
    ) -> Result<RollbackResponse> {
        let path = format!("/v1/executions/{}/rollback", execution_id);
        let resp = self
            .request(reqwest::Method::POST, &path)
            .json(req)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn cancel_execution(
        &self,
        execution_id: &str,
        req: &CancelExecutionRequest,
    ) -> Result<CancelExecutionResponse> {
        let path = format!("/v1/executions/{}/cancel", execution_id);
        let resp = self
            .request(reqwest::Method::POST, &path)
            .json(req)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn pause_execution(
        &self,
        execution_id: &str,
        req: &PauseExecutionRequest,
    ) -> Result<PauseExecutionResponse> {
        let path = format!("/v1/executions/{}/pause", execution_id);
        let resp = self
            .request(reqwest::Method::POST, &path)
            .json(req)
            .send()
            .await?;
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
        edge_types: Option<Vec<ProvenanceEdgeType>>,
    ) -> Result<ProvenanceEventResponse> {
        let path = format!("/v1/provenance/events/{}", event_id);
        let mut req = self.request(reqwest::Method::GET, &path);
        req = req.query(&[
            ("ancestry", ancestry.to_string()),
            ("descendants", descendants.to_string()),
        ]);
        if let Some(ref types) = edge_types {
            // Send multiple edge types as one comma-separated query value
            req = req.query(&[("edge_types", edge_types_to_query_string(types))]);
        }
        let resp = req.send().await?;
        self.decode_json(resp).await
    }

    async fn lineage_query(&self, req: &LineageQueryRequest) -> Result<LineageQueryResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/v1/provenance/lineage")
            .json(req)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn replay_provenance(
        &self,
        req: &ProvenanceReplayRequest,
    ) -> Result<ProvenanceReplayResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/v1/provenance/replay")
            .json(req)
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn export_provenance(
        &self,
        req: &ProvenanceExportRequest,
    ) -> Result<ProvenanceExportResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/v1/provenance/export")
            .json(req)
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

    async fn verify_ledger(&self) -> Result<LedgerVerificationResponse> {
        let resp = self
            .request(reqwest::Method::GET, "/v1/ledger/verify")
            .send()
            .await?;
        self.decode_json(resp).await
    }

    async fn get_provenance_stats(
        &self,
        req: &ProvenanceStatsRequest,
    ) -> Result<ProvenanceStatsResponse> {
        let resp = self
            .request(reqwest::Method::POST, "/v1/provenance/stats")
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

/// CLI actor type enum — mirrors `ferrum_proto::common::ActorType` as a clap ValueEnum.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum ActorTypeCli {
    /// Human actor (user).
    User,
    /// Autonomous agent.
    Agent,
    /// Automated policy engine.
    PolicyEngine,
    /// Gateway system actor.
    Gateway,
    /// External adapter actor.
    Adapter,
    /// Human operator.
    Operator,
    /// System-level actor.
    System,
}

impl From<ActorTypeCli> for ActorType {
    fn from(value: ActorTypeCli) -> Self {
        match value {
            ActorTypeCli::User => ActorType::User,
            ActorTypeCli::Agent => ActorType::Agent,
            ActorTypeCli::PolicyEngine => ActorType::PolicyEngine,
            ActorTypeCli::Gateway => ActorType::Gateway,
            ActorTypeCli::Adapter => ActorType::Adapter,
            ActorTypeCli::Operator => ActorType::Operator,
            ActorTypeCli::System => ActorType::System,
        }
    }
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
    /// Multi-hop lineage traversal from a seed event via ancestry and/or descendant edges.
    InspectLineageQuery {
        /// Execution ID (UUID) — all traversed events must belong to this execution.
        #[arg(long)]
        execution_id: String,

        /// Seed event ID (UUID) to start traversal from.
        #[arg(long)]
        event_id: String,

        /// Walk ancestry backwards via parent edges.
        #[arg(long)]
        ancestry: bool,

        /// Walk descendants forwards via child edges.
        #[arg(long)]
        descendants: bool,

        /// Maximum BFS hops (1–32). Defaults to 8. Hard-capped at 32 by the server.
        #[arg(long)]
        max_hops: Option<u32>,

        /// Filter traversal to only include these edge types.
        /// Can be specified multiple times. Valid values: DerivedFrom, AuthorizedBy,
        /// ApprovedBy, TaintedBy, UsesManifest, EvaluatedByPolicy, Caused,
        /// Compensates, Verifies, References, ObservedBy.
        #[arg(long, value_name = "EDGE_TYPE")]
        edge_type: Option<Vec<String>>,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output raw JSON instead of human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Replay a read-only provenance reconstruction for a single execution.
    Replay {
        /// Execution ID (UUID) to replay.
        #[arg(long)]
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
    /// Aggregate provenance statistics and run consistency checks over queried events.
    InspectProvenanceStats {
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

        /// Filter events since timestamp (ISO 8601).
        #[arg(long)]
        since: Option<String>,

        /// Filter events until timestamp (ISO 8601).
        #[arg(long)]
        until: Option<String>,

        /// Maximum total events to collect across all pages (1-100000).
        /// Default is 10000. Use a lower value for faster, bounded output.
        #[arg(long)]
        max_events: Option<u32>,

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

        /// Filter by proposal ID.
        #[arg(long)]
        proposal_id: Option<String>,

        /// Filter by execution ID.
        #[arg(long)]
        execution_id: Option<String>,

        /// Filter by multiple execution IDs (additive, can be specified multiple times).
        #[arg(long, value_delimiter = ',')]
        execution_ids: Vec<String>,

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

        /// Maximum number of events to return per page (1-10000).
        #[arg(long)]
        limit: Option<u32>,

        /// Cursor from a previous query's next_cursor to fetch the next page.
        #[arg(long)]
        cursor: Option<String>,

        /// Export all pages by following cursors until exhaustion.
        /// Automatically sets --json output.
        #[arg(long)]
        all_pages: bool,

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
    /// Export provenance events as a deterministic audit payload.
    ExportProvenance {
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

        /// Maximum number of events to export (1-10000). Defaults to 1000.
        #[arg(long)]
        limit: Option<u32>,

        /// Cursor from a previous export's next_cursor to fetch the next page.
        #[arg(long)]
        cursor: Option<String>,

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

        /// Filter ancestry/descendants to only include edges of this type.
        /// Can be specified multiple times to include multiple edge types.
        /// Valid values: DerivedFrom, AuthorizedBy, ApprovedBy, TaintedBy,
        /// UsesManifest, EvaluatedByPolicy, Caused, Compensates, Verifies,
        /// References, ObservedBy.
        #[arg(long, value_parser = parse_edge_type)]
        edge_type: Vec<ProvenanceEdgeType>,

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
    /// Resolve a pending approval (approve or deny).
    ResolveApproval {
        /// Approval ID (UUID) to resolve.
        approval_id: String,

        /// Approve the pending approval.
        #[arg(long, conflicts_with = "deny")]
        approve: bool,

        /// Deny the pending approval.
        #[arg(long, conflicts_with = "approve")]
        deny: bool,

        /// Actor type resolving this approval.
        #[arg(long, value_enum, default_value = "operator")]
        actor_type: ActorTypeCli,

        /// Actor ID (username, agent name, etc.).
        #[arg(long, default_value = "ferrumctl")]
        actor_id: String,

        /// Optional display name for the actor.
        #[arg(long)]
        actor_display_name: Option<String>,

        /// Reason for the decision. Required when --deny is set.
        #[arg(long, requires = "deny")]
        reason: Option<String>,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output the returned approval as JSON instead of human-readable.
        #[arg(long)]
        json: bool,
    },
    /// Resolve multiple pending approvals in bulk (single-page, confirm-gated).
    ResolveApprovalBulk {
        /// Activate bulk mode and resolve all pending approvals matching the filter.
        /// Bulk mode requires: --proposal-id or --execution-id, --limit, --yes, --expect-count.
        #[arg(long)]
        all_pending: bool,

        /// Filter by proposal ID (UUID). At least one of --proposal-id or --execution-id
        /// is required in bulk mode.
        #[arg(long)]
        proposal_id: Option<String>,

        /// Filter by execution ID (UUID). At least one of --proposal-id or --execution-id
        /// is required in bulk mode.
        #[arg(long)]
        execution_id: Option<String>,

        /// Maximum number of approvals to resolve in this bulk operation.
        /// Required in bulk mode to bound the mutation.
        #[arg(long)]
        limit: Option<u32>,

        /// Confirm the bulk operation. Required in bulk mode to prevent accidental mutations.
        #[arg(long)]
        yes: bool,

        /// Expected count of pending approvals. The bulk operation will fail if the
        /// actual count of pending approvals does not match this number, preventing
        /// accidental resolution of an unexpected set.
        #[arg(long)]
        expect_count: Option<u32>,

        /// Approve all the pending approvals.
        #[arg(long, conflicts_with = "deny")]
        approve: bool,

        /// Deny all the pending approvals.
        #[arg(long, conflicts_with = "approve")]
        deny: bool,

        /// Actor type resolving these approvals.
        #[arg(long, value_enum, default_value = "operator")]
        actor_type: ActorTypeCli,

        /// Actor ID (username, agent name, etc.).
        #[arg(long, default_value = "ferrumctl")]
        actor_id: String,

        /// Optional display name for the actor.
        #[arg(long)]
        actor_display_name: Option<String>,

        /// Reason for the decision. Required when --deny is set.
        #[arg(long, requires = "deny")]
        reason: Option<String>,

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
    /// Watch pending approvals by polling the list API at a fixed interval.
    WatchApprovals {
        /// Filter by proposal ID (UUID).
        #[arg(long)]
        proposal_id: Option<String>,

        /// Filter by execution ID (UUID).
        #[arg(long)]
        execution_id: Option<String>,

        /// Maximum approvals to return per iteration.
        #[arg(long)]
        limit: Option<u32>,

        /// Cursor from a previous listing to resume from.
        #[arg(long)]
        cursor: Option<String>,

        /// Polling interval in milliseconds. Default is 5000ms.
        /// Must be between 100ms and 300000ms (5 minutes).
        #[arg(long)]
        poll_interval_ms: Option<u64>,

        /// Maximum number of polling iterations. Default is 1 (single shot).
        /// Use this to bound watch loops in tests and scripting.
        #[arg(long)]
        iterations: Option<u32>,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output raw JSON envelope per iteration instead of human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Watch an execution by polling until a terminal state is reached.
    WatchExecution {
        /// Execution ID (UUID) to watch.
        execution_id: String,

        /// Polling interval in milliseconds. Default is 2000ms.
        /// Must be between 100ms and 300000ms (5 minutes).
        #[arg(long)]
        poll_interval_ms: Option<u64>,

        /// Maximum number of polling iterations. Default is 60 (~2 minutes at 2000ms interval).
        /// Use this to bound watch loops in tests and scripting.
        #[arg(long)]
        iterations: Option<u32>,

        /// Server base URL (e.g. http://127.0.0.1:8080).
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        #[arg(long, env = "FERRUMCTL_BEARER_TOKEN")]
        bearer_token: Option<String>,

        /// Output raw JSON per iteration instead of human-readable summary.
        #[arg(long)]
        json: bool,

        /// Exit non-zero if the iteration cap is reached without a terminal state.
        /// Without this flag, the command exits 0 after max iterations regardless of state.
        #[arg(long)]
        require_terminal: bool,
    },
    /// Request compensation for an execution.
    CompensateExecution {
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
    /// Request rollback for an execution.
    RollbackExecution {
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
    /// Cancel an execution that is in a pre-execute state (Proposed, Authorized, Prepared).
    CancelExecution {
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
    /// Pause an execution that is in a running state (Running, AwaitingVerification).
    PauseExecution {
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
    /// Verify ledger hash-chain integrity via the server.
    VerifyLedger {
        /// Server base URL (e.g. http://127.0.0.1:8080).
        /// Can also be set via FERRUMCTL_SERVER_URL env var.
        #[arg(long, env = "FERRUMCTL_SERVER_URL")]
        server_url: Option<String>,

        /// Bearer token for authentication.
        /// Can also be set via FERRUMCTL_BEARER_TOKEN env var.
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

/// Validates poll_interval_ms locally, returning an error if outside the 100..=300_000 range.
fn validate_poll_interval_ms(interval_ms: Option<u64>) -> Result<u64> {
    const MIN_MS: u64 = 100;
    const MAX_MS: u64 = 300_000;
    const DEFAULT_MS: u64 = 5_000;

    match interval_ms {
        None => Ok(DEFAULT_MS),
        Some(v) if (MIN_MS..=MAX_MS).contains(&v) => Ok(v),
        Some(v) => bail!(
            "--poll-interval-ms must be between {} and {}, got {}",
            MIN_MS,
            MAX_MS,
            v
        ),
    }
}

/// Formats an approval list envelope as a deterministic human-readable summary
/// for a single watch iteration.
fn format_watch_iteration_text(envelope: &ApprovalListEnvelope, iteration: u32) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "--- iteration {} ({} approval(s), next_cursor={}) ---",
        iteration,
        envelope.items.len(),
        envelope.next_cursor.as_deref().unwrap_or("none")
    ));

    // Sort approvals deterministically: first by state (Pending first), then by created_at desc
    let mut sorted: Vec<&ApprovalRequest> = envelope.items.iter().collect();
    sorted.sort_by(|a, b| {
        // Pending state sorts before others
        let a_pending = if a.state == "Pending" { 0 } else { 1 };
        let b_pending = if b.state == "Pending" { 0 } else { 1 };
        a_pending
            .cmp(&b_pending)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.approval_id.cmp(&b.approval_id))
    });

    for approval in sorted {
        lines.push(format!("Approval: {}", approval.approval_id));
        lines.push(format!("  State:    {}", approval.state));
        lines.push(format!("  Intent:   {}", approval.intent_id));
        lines.push(format!("  Proposal: {}", approval.proposal_id));
        if let Some(ref eid) = approval.execution_id {
            lines.push(format!("  Execution:{}", eid));
        }
        lines.push(format!("  Reason:   {}", approval.reason));
        lines.push(format!("  Created:  {}", approval.created_at));
        lines.push(format!("  Expires:  {}", approval.expires_at));
    }

    lines.join("\n")
}

/// Formats an execution record as a deterministic human-readable summary
/// for a single watch iteration.
fn format_execution_record_text(record: &ExecutionRecord, iteration: u32) -> String {
    let terminal = is_execution_terminal_state(&record.state);
    let terminal_marker = if terminal { " [TERMINAL]" } else { "" };
    let mut lines = Vec::new();
    lines.push(format!(
        "--- iteration {} (execution_id={}, state={}{}) ---",
        iteration, record.execution_id, record.state, terminal_marker
    ));
    lines.push(format!("  Decision:  {}", record.decision));
    lines.push(format!("  Intent:    {}", record.intent_id));
    lines.push(format!("  Proposal:  {}", record.proposal_id));
    lines.push(format!("  Capability:{}", record.capability_id));
    if let Some(ref cid) = record.rollback_contract_id {
        lines.push(format!("  Rollback:  {}", cid));
    }
    if let Some(ref digest) = record.result_digest {
        lines.push(format!("  Digest:    {}", digest));
    }
    lines.push(format!("  Started:   {}", record.started_at));
    if let Some(ref finished) = record.finished_at {
        lines.push(format!("  Finished:  {}", finished));
    }
    lines.join("\n")
}

/// Configuration for watch approvals operations.
struct WatchApprovalsConfig {
    proposal_id: Option<String>,
    execution_id: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
    poll_interval_ms: Option<u64>,
    iterations: Option<u32>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
}

async fn run_watch_approvals(config: WatchApprovalsConfig) -> Result<()> {
    let WatchApprovalsConfig {
        proposal_id,
        execution_id,
        limit,
        cursor,
        poll_interval_ms,
        iterations,
        url,
        token,
        as_json,
    } = config;
    // Validate poll interval before any network call
    let poll_interval_ms = validate_poll_interval_ms(poll_interval_ms)?;

    // Default to 1 iteration if not specified (single-shot watch)
    let max_iterations = iterations.unwrap_or(1);
    if max_iterations == 0 {
        bail!("--iterations must be at least 1, got 0");
    }

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    let mut current_cursor = cursor;
    let mut iteration = 0u32;

    loop {
        iteration += 1;

        let envelope = client
            .list_approvals(&ListApprovalsQuery {
                limit,
                cursor: current_cursor.clone(),
                proposal_id: proposal_id.clone(),
                execution_id: execution_id.clone(),
            })
            .await?;

        if as_json {
            // Raw JSON output per iteration
            println!("{}", serde_json::to_string(&envelope)?);
        } else {
            // Human-readable summary per iteration
            println!("{}", format_watch_iteration_text(&envelope, iteration));
        }

        // Check if we've reached max iterations
        if iteration >= max_iterations {
            break;
        }

        // If there's no next cursor, we've exhausted the listing
        if envelope.next_cursor.is_none() {
            break;
        }

        // Wait before next poll
        tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)).await;
        current_cursor = envelope.next_cursor;
    }

    Ok(())
}

/// Default polling interval for watch-execution: 2000ms.
const WATCH_EXECUTION_DEFAULT_INTERVAL_MS: u64 = 2_000;

/// Default maximum iterations for watch-execution: 60 (~2 minutes at 2000ms interval).
const WATCH_EXECUTION_DEFAULT_ITERATIONS: u32 = 60;

async fn run_watch_execution(
    execution_id: &str,
    poll_interval_ms: Option<u64>,
    iterations: Option<u32>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
    require_terminal: bool,
) -> Result<()> {
    // Validate poll interval before any network call (use 2000ms default for watch-execution)
    let poll_interval_ms = match poll_interval_ms {
        None => WATCH_EXECUTION_DEFAULT_INTERVAL_MS,
        Some(v) if (100..=300_000).contains(&v) => v,
        Some(v) => {
            bail!(
                "--poll-interval-ms must be between 100 and 300000, got {}",
                v
            );
        }
    };

    // Default to 60 iterations (~2 minutes at default interval)
    let max_iterations = iterations.unwrap_or(WATCH_EXECUTION_DEFAULT_ITERATIONS);
    if max_iterations == 0 {
        bail!("--iterations must be at least 1, got 0");
    }

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    let mut iteration = 0u32;

    loop {
        iteration += 1;

        let record = client.get_execution(execution_id).await?;

        if as_json {
            println!("{}", serde_json::to_string(&record)?);
        } else {
            println!("{}", format_execution_record_text(&record, iteration));
        }

        // Check if we've reached a terminal state and stop early
        if is_execution_terminal_state(&record.state) {
            // Reached terminal state before hitting iteration cap
            return Ok(());
        }

        // Check if we've reached max iterations
        if iteration >= max_iterations {
            if require_terminal {
                bail!(
                    "iteration cap ({}) reached without a terminal state (current state: {}); \
                     use --require-terminal to make this an error",
                    max_iterations,
                    record.state
                );
            }
            // Not require_terminal: exit 0 after max iterations
            return Ok(());
        }

        // Wait before next poll
        tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)).await;
    }
}

async fn run_compensate_execution(
    execution_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let exec_id = ExecutionId(execution_id.parse().context("invalid execution_id UUID")?);
    let req = CompensateRequest {
        execution_id: exec_id,
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.compensate_execution(execution_id, &req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "Compensation requested for execution: {}",
            resp.execution_id
        );
        println!("  Compensated: {}", resp.compensated);
        if let Some(ts) = resp.compensated_at {
            println!("  Compensated at: {}", ts);
        }
    }
    Ok(())
}

async fn run_rollback_execution(
    execution_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let exec_id = ExecutionId(execution_id.parse().context("invalid execution_id UUID")?);
    let req = RollbackRequest {
        execution_id: exec_id,
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.rollback_execution(execution_id, &req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Rollback requested for execution: {}", resp.execution_id);
        println!("  Rolled back: {}", resp.rolled_back);
        if let Some(ts) = resp.rolled_back_at {
            println!("  Rolled back at: {}", ts);
        }
    }
    Ok(())
}

async fn run_cancel_execution(
    execution_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let exec_id = ExecutionId(execution_id.parse().context("invalid execution_id UUID")?);
    let req = CancelExecutionRequest {
        execution_id: exec_id,
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.cancel_execution(execution_id, &req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Cancel requested for execution: {}", resp.execution_id);
        println!("  Cancelled: {}", resp.cancelled);
        if let Some(ts) = resp.cancelled_at {
            println!("  Cancelled at: {}", ts);
        }
    }
    Ok(())
}

async fn run_pause_execution(
    execution_id: &str,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let exec_id = ExecutionId(execution_id.parse().context("invalid execution_id UUID")?);
    let req = PauseExecutionRequest {
        execution_id: exec_id,
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.pause_execution(execution_id, &req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Pause requested for execution: {}", resp.execution_id);
        println!("  Paused: {}", resp.paused);
        if let Some(ts) = resp.paused_at {
            println!("  Paused at: {}", ts);
        }
    }
    Ok(())
}

async fn run_verify_ledger(
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.verify_ledger().await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        if resp.valid {
            println!("Ledger verification: PASSED");
        } else {
            println!("Ledger verification: FAILED");
        }
        println!("  Entry count: {}", resp.entry_count);
        println!("  Verified at: {}", resp.verified_at);
        if let Some(ref err) = resp.error {
            println!("  Error: {:?}", err);
        }
    }
    Ok(())
}

/// Configuration for single approval resolution.
struct ResolveApprovalConfig<'a> {
    approval_id: &'a str,
    approve: bool,
    deny: bool,
    actor_type: ActorTypeCli,
    actor_id: &'a str,
    actor_display_name: Option<String>,
    reason: Option<String>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
}

async fn run_resolve_approval(config: ResolveApprovalConfig<'_>) -> Result<()> {
    let ResolveApprovalConfig {
        approval_id,
        approve,
        deny,
        actor_type,
        actor_id,
        actor_display_name,
        reason,
        url,
        token,
        as_json,
    } = config;
    // Fail-closed: require explicit approve xor deny
    if !approve && !deny {
        bail!("must specify either --approve or --deny");
    }
    if approve && deny {
        bail!("cannot specify both --approve and --deny");
    }

    // Reason is required when denying
    if deny && reason.is_none() {
        bail!("--reason is required when --deny is set");
    }

    let actor = ActorRef {
        actor_type: actor_type.into(),
        actor_id: actor_id.to_string(),
        display_name: actor_display_name,
    };

    let req = ApprovalResolveRequest {
        actor,
        approve,
        reason,
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let approval = client.resolve_approval(approval_id, &req).await?;

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

/// Result of attempting to resolve a single approval in a bulk operation.
/// Classification is determined by the final observed state after reconciliation.
#[derive(Debug, Clone, serde::Serialize)]
enum BulkResolutionOutcome {
    /// Mutation was accepted (2xx from resolve endpoint) and the approval reached
    /// a terminal decided state (Approved or Denied).
    Resolved {
        approval_id: String,
        decision: String,
        state: String,
    },
    /// Mutation request returned a non-2xx HTTP status. Follow-up read showed the
    /// approval is still pending — the mutation was not applied.
    MutationRejected {
        approval_id: String,
        http_status: u16,
        state: String,
    },
    /// Mutation request returned a non-2xx HTTP status. Follow-up read showed the
    /// approval reached a terminal decided state despite the error — the mutation
    /// may have been applied despite the error response.
    MutationConflicted {
        approval_id: String,
        http_status: u16,
        decision: String,
        state: String,
    },
    /// Mutation request returned a non-2xx HTTP status. Follow-up read failed —
    /// the final state is unreadable. This is a hard failure.
    Unreadable {
        approval_id: String,
        http_status: u16,
        read_error: String,
    },
}

/// Classifies the outcome of a bulk-resolve attempt for a single approval.
/// On non-2xx, fetches the final approval state to determine whether the mutation
/// was applied despite the error, or rejected, or unreadable.
async fn classify_resolve_outcome(
    client: &ServerClient,
    approval_id: &str,
    http_status: u16,
) -> BulkResolutionOutcome {
    match client.get_approval(approval_id).await {
        Ok(approval) => {
            let state = approval.state.clone();
            // Check if it reached a terminal decided state
            if state == "Approved" || state == "Denied" {
                BulkResolutionOutcome::MutationConflicted {
                    approval_id: approval_id.to_string(),
                    http_status,
                    decision: approval.state.clone(),
                    state,
                }
            } else {
                BulkResolutionOutcome::MutationRejected {
                    approval_id: approval_id.to_string(),
                    http_status,
                    state,
                }
            }
        }
        Err(read_err) => BulkResolutionOutcome::Unreadable {
            approval_id: approval_id.to_string(),
            http_status,
            read_error: read_err.to_string(),
        },
    }
}

/// Returns true if the given approval state is considered "pending" and therefore
/// eligible for resolution.
fn is_pending_state(state: &str) -> bool {
    state == "Pending"
}

/// Returns true if the given execution state is considered terminal (complete).
/// Terminal states are those where the execution has reached a final outcome and
/// will not transition to any other state.
fn is_execution_terminal_state(state: &str) -> bool {
    matches!(
        state,
        "Completed"
            | "Committed"
            | "Approved"
            | "Denied"
            | "RolledBack"
            | "Error"
            | "Quarantined"
            | "Cancelled"
            | "TimedOut"
    )
}

/// Formats a single bulk resolution outcome for human-readable output.
fn format_bulk_outcome(outcome: &BulkResolutionOutcome) -> String {
    match outcome {
        BulkResolutionOutcome::Resolved {
            approval_id,
            decision,
            state,
        } => {
            format!(
                "RESOLVED  {}  decision={} state={}",
                approval_id, decision, state
            )
        }
        BulkResolutionOutcome::MutationRejected {
            approval_id,
            http_status,
            state,
        } => {
            format!(
                "REJECTED  {}  HTTP {}  state={}",
                approval_id, http_status, state
            )
        }
        BulkResolutionOutcome::MutationConflicted {
            approval_id,
            http_status,
            decision,
            state,
        } => {
            format!(
                "CONFLICT  {}  HTTP {}  decision={} state={}",
                approval_id, http_status, decision, state
            )
        }
        BulkResolutionOutcome::Unreadable {
            approval_id,
            http_status,
            read_error,
        } => {
            format!(
                "UNREADABLE  {}  HTTP {}  read failed: {}",
                approval_id, http_status, read_error
            )
        }
    }
}

/// Configuration for bulk approval resolution.
struct ResolveApprovalBulkConfig<'a> {
    all_pending: bool,
    proposal_id: Option<String>,
    execution_id: Option<String>,
    limit: Option<u32>,
    yes: bool,
    expect_count: Option<u32>,
    approve: bool,
    deny: bool,
    actor_type: ActorTypeCli,
    actor_id: &'a str,
    actor_display_name: Option<String>,
    reason: Option<String>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
}

async fn run_resolve_approval_bulk(config: ResolveApprovalBulkConfig<'_>) -> Result<()> {
    let ResolveApprovalBulkConfig {
        all_pending,
        proposal_id,
        execution_id,
        limit,
        yes,
        expect_count,
        approve,
        deny,
        actor_type,
        actor_id,
        actor_display_name,
        reason,
        url,
        token,
        as_json,
    } = config;
    // Determine if bulk mode is active based on any bulk-mode flag being set.
    // Bulk mode is triggered by --all-pending, scope filters, or --limit.
    let bulk_mode = all_pending
        || proposal_id.is_some()
        || execution_id.is_some()
        || limit.is_some()
        || yes
        || expect_count.is_some();

    if bulk_mode {
        // --- Bulk mode guardrails ---
        // Fail-closed: require explicit approve xor deny
        if !approve && !deny {
            bail!("bulk mode: must specify either --approve or --deny");
        }
        if approve && deny {
            bail!("bulk mode: cannot specify both --approve and --deny");
        }

        // Reason is required when denying
        if deny && reason.is_none() {
            bail!("bulk mode: --reason is required when --deny is set");
        }

        // Require at least one scope filter
        if proposal_id.is_none() && execution_id.is_none() {
            bail!(
                "bulk mode: at least one scope filter is required \
                 (--proposal-id or --execution-id)"
            );
        }

        // Require explicit limit
        let limit = limit.context("bulk mode: --limit is required")?;

        // Require explicit confirmation
        if !yes {
            bail!("bulk mode: --yes is required to confirm the bulk mutation");
        }
        let expect_count = expect_count.context("bulk mode: --expect-count is required")?;

        // Build actor and request
        let actor = ActorRef {
            actor_type: actor_type.into(),
            actor_id: actor_id.to_string(),
            display_name: actor_display_name,
        };

        let req = ApprovalResolveRequest {
            actor,
            approve,
            reason,
        };

        let url = resolve_server_url(url)?;
        let client = ServerClient::new(&url, token);

        // Fetch one page of pending approvals matching the scope filter
        let pending = client
            .list_approvals(&ListApprovalsQuery {
                limit: Some(limit),
                cursor: None,
                proposal_id: proposal_id.clone(),
                execution_id: execution_id.clone(),
            })
            .await?;

        // Filter to only Pending approvals (API may return non-pending on the page)
        let pending: Vec<_> = pending
            .items
            .into_iter()
            .filter(|a| is_pending_state(&a.state))
            .collect();

        // Fail if count doesn't match expectation
        let actual_count = pending.len() as u32;
        if actual_count != expect_count {
            bail!(
                "bulk mode: expected {} pending approvals but found {} \
                 (use --expect-count to match the actual count or re-list with --limit to adjust)",
                expect_count,
                actual_count
            );
        }

        if pending.is_empty() {
            println!("Bulk resolve: no pending approvals match the filter. Nothing to do.");
            return Ok(());
        }

        println!(
            "Bulk resolve: {} approval(s) (limit={}, expect_count={})\n",
            pending.len(),
            limit,
            expect_count
        );

        // Resolve each approval and classify the outcome
        let mut outcomes: Vec<BulkResolutionOutcome> = Vec::new();
        let mut hard_failure_count = 0u32;

        for approval in &pending {
            let approval_id = &approval.approval_id;

            // Attempt resolve — let any panics propagate; handle error classification below
            let outcome = match client.resolve_approval(approval_id, &req).await {
                Ok(updated) => {
                    // 2xx: classify as Resolved if terminal state, else MutationConflicted
                    let state = updated.state.clone();
                    if state == "Approved" || state == "Denied" {
                        BulkResolutionOutcome::Resolved {
                            approval_id: approval_id.clone(),
                            decision: updated.state.clone(),
                            state,
                        }
                    } else {
                        // Should not happen on 2xx with a valid response, but guard anyway
                        BulkResolutionOutcome::Resolved {
                            approval_id: approval_id.clone(),
                            decision: updated.state.clone(),
                            state,
                        }
                    }
                }
                Err(err) => {
                    // Non-2xx or network error — classify via follow-up read
                    let http_status = extract_http_status(&err);
                    classify_resolve_outcome(&client, approval_id, http_status).await
                }
            };

            // Check for hard failures that should cause non-zero exit
            let is_hard_failure = matches!(
                outcome,
                BulkResolutionOutcome::MutationRejected { .. }
                    | BulkResolutionOutcome::Unreadable { .. }
            );
            if is_hard_failure {
                hard_failure_count += 1;
            }

            outcomes.push(outcome);
        }

        // Output per-item results
        if as_json {
            println!("{}", serde_json::to_string_pretty(&outcomes)?);
        } else {
            println!("Bulk resolution results:");
            for outcome in &outcomes {
                println!("  {}", format_bulk_outcome(outcome));
            }
            println!();
        }

        // Summary
        let resolved_count = outcomes
            .iter()
            .filter(|o| matches!(o, BulkResolutionOutcome::Resolved { .. }))
            .count() as u32;
        let conflicted_count = outcomes
            .iter()
            .filter(|o| matches!(o, BulkResolutionOutcome::MutationConflicted { .. }))
            .count() as u32;
        let rejected_count = outcomes
            .iter()
            .filter(|o| matches!(o, BulkResolutionOutcome::MutationRejected { .. }))
            .count() as u32;
        let unreadable_count = outcomes
            .iter()
            .filter(|o| matches!(o, BulkResolutionOutcome::Unreadable { .. }))
            .count() as u32;

        println!(
            "Summary: {} resolved, {} conflicted, {} rejected, {} unreadable",
            resolved_count, conflicted_count, rejected_count, unreadable_count
        );

        // Exit non-zero if any hard failures remain (rejected, unreadable)
        if hard_failure_count > 0 {
            bail!(
                "{} hard failure(s) (rejected/unreadable); \
                 review output above and retry individual approvals",
                hard_failure_count
            );
        }

        Ok(())
    } else {
        // Fallback: single-approval mode — delegate to the existing handler
        // This path should not be reached via CLI because ResolveApprovalBulk
        // always has at least one bulk flag set. But kept for safety.
        bail!(
            "bulk mode: missing required flags (--proposal-id, --execution-id, \
             --limit, --yes, --expect-count)"
        );
    }
}

/// Extracts the HTTP status code from an anyhow error that wraps a reqwest error.
fn extract_http_status(err: &anyhow::Error) -> u16 {
    err.chain()
        .find_map(|e| {
            e.downcast_ref::<reqwest::Error>()
                .and_then(|re| re.status())
                .map(|s| s.as_u16())
        })
        .unwrap_or(0)
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

#[allow(clippy::too_many_arguments)]
async fn run_inspect_lineage_query(
    execution_id: String,
    event_id: String,
    ancestry: bool,
    descendants: bool,
    max_hops: Option<u32>,
    edge_type: Option<Vec<String>>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    // Fail-closed: require at least one direction
    if !ancestry && !descendants {
        bail!("at least one of --ancestry or --descendants must be set");
    }

    // Validate max_hops locally before making any network call
    let max_hops = validate_max_hops(max_hops)?;

    // Parse edge_type strings to ProvenanceEdgeType
    let edge_types = parse_edge_types(edge_type)?;

    let req = LineageQueryRequest {
        execution_id: ferrum_proto::ExecutionId(
            uuid::Uuid::parse_str(&execution_id)
                .map_err(|e| anyhow::anyhow!("invalid execution_id: {}", e))?,
        ),
        event_id: ferrum_proto::EventId(
            uuid::Uuid::parse_str(&event_id)
                .map_err(|e| anyhow::anyhow!("invalid event_id: {}", e))?,
        ),
        ancestry,
        descendants,
        max_hops,
        edge_types,
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.lineage_query(&req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("{}", format_lineage_query_text(&resp));
    }

    Ok(())
}

async fn run_replay(
    execution_id: String,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let req = ProvenanceReplayRequest {
        execution_id: ferrum_proto::ExecutionId(
            uuid::Uuid::parse_str(&execution_id)
                .map_err(|e| anyhow::anyhow!("invalid execution_id: {}", e))?,
        ),
    };

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let resp = client.replay_provenance(&req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        // Human-readable output: show execution_id and event count
        println!("Replay for execution: {}", resp.execution_id);
        println!("Total events: {}", resp.events.len());
        for (i, event) in resp.events.iter().enumerate() {
            println!("  [{}] {} ({:?})", i + 1, event.event_id, event.kind);
        }
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

/// Validates max_hops locally, returning an error if outside the 1..32 range.
fn validate_max_hops(max_hops: Option<u32>) -> Result<Option<u32>> {
    match max_hops {
        None => Ok(None),
        Some(v) if (1..=32).contains(&v) => Ok(Some(v)),
        Some(v) => bail!("--max-hops must be between 1 and 32, got {}", v),
    }
}

/// Parses a list of edge type strings into ProvenanceEdgeType enum variants.
fn parse_edge_types(
    edge_types: Option<Vec<String>>,
) -> Result<Option<Vec<ferrum_proto::ProvenanceEdgeType>>> {
    match edge_types {
        None => Ok(None),
        Some(types) => {
            let mut parsed = Vec::new();
            for s in types {
                let edge_type = match s.as_str() {
                    "DerivedFrom" => ferrum_proto::ProvenanceEdgeType::DerivedFrom,
                    "AuthorizedBy" => ferrum_proto::ProvenanceEdgeType::AuthorizedBy,
                    "ApprovedBy" => ferrum_proto::ProvenanceEdgeType::ApprovedBy,
                    "TaintedBy" => ferrum_proto::ProvenanceEdgeType::TaintedBy,
                    "UsesManifest" => ferrum_proto::ProvenanceEdgeType::UsesManifest,
                    "EvaluatedByPolicy" => ferrum_proto::ProvenanceEdgeType::EvaluatedByPolicy,
                    "Caused" => ferrum_proto::ProvenanceEdgeType::Caused,
                    "Compensates" => ferrum_proto::ProvenanceEdgeType::Compensates,
                    "Verifies" => ferrum_proto::ProvenanceEdgeType::Verifies,
                    "References" => ferrum_proto::ProvenanceEdgeType::References,
                    "ObservedBy" => ferrum_proto::ProvenanceEdgeType::ObservedBy,
                    other => bail!("unknown edge type: {}", other),
                };
                parsed.push(edge_type);
            }
            Ok(Some(parsed))
        }
    }
}

/// Formats a `LineageQueryResponse` as a deterministic human-readable summary.
fn format_lineage_query_text(resp: &ferrum_proto::LineageQueryResponse) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "LineageQuery: {} event(s), {} edge(s)",
        resp.events.len(),
        resp.edges.len()
    ));

    // Build event lookup for edge rendering (keyed by event_id string)
    let event_map: std::collections::HashMap<String, &ferrum_proto::ProvenanceEvent> = resp
        .events
        .iter()
        .map(|e| (e.event_id.0.to_string(), e))
        .collect();

    // Sort edges deterministically by (from_event_id, to_event_id, edge_type)
    let mut edges: Vec<(String, String, String)> = resp
        .edges
        .iter()
        .map(|e| {
            (
                e.from_event_id.0.to_string(),
                e.to_event_id.0.to_string(),
                format!("{:?}", e.edge_type),
            )
        })
        .collect();
    edges.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    if !edges.is_empty() {
        lines.push("\nEdges:".to_string());
        for (from, to, edge_type) in &edges {
            let from_kind = event_map
                .get(from)
                .map(|e| kind_label(&e.kind))
                .unwrap_or_else(|| "?".to_string());
            let to_kind = event_map
                .get(to)
                .map(|e| kind_label(&e.kind))
                .unwrap_or_else(|| "?".to_string());
            lines.push(format!("  {} --[{}]--> {}", from_kind, edge_type, to_kind));
        }
    }

    // Sort events deterministically by (occurred_at, event_id)
    let mut events: Vec<&ferrum_proto::ProvenanceEvent> = resp.events.iter().collect();
    events.sort_by(|a, b| {
        a.occurred_at
            .to_rfc3339()
            .cmp(&b.occurred_at.to_rfc3339())
            .then_with(|| a.event_id.0.to_string().cmp(&b.event_id.0.to_string()))
    });

    lines.push("\nEvents:".to_string());
    for event in &events {
        let kind_str = kind_label(&event.kind);
        lines.push(format!(
            "  [{}] {}  {}",
            event.occurred_at.to_rfc3339(),
            kind_str,
            event.event_id.0
        ));
        if let Some(ref eid) = event.execution_id {
            lines.push(format!("    execution: {}", eid.0));
        }
    }

    lines.join("\n")
}

/// Returns a human-readable label for a ProvenanceEventKind.
fn kind_label(kind: &ferrum_proto::ProvenanceEventKind) -> String {
    use ferrum_proto::ProvenanceEventKind as PK;
    match kind {
        PK::UserGoalReceived => "UserGoalReceived".to_string(),
        PK::IntentCompiled => "IntentCompiled".to_string(),
        PK::IntentRevoked => "IntentRevoked".to_string(),
        PK::ActionProposalSubmitted => "ActionProposalSubmitted".to_string(),
        PK::PolicyEvaluated => "PolicyEvaluated".to_string(),
        PK::CapabilityMinted => "CapabilityMinted".to_string(),
        PK::CapabilityRevoked => "CapabilityRevoked".to_string(),
        PK::ApprovalRequested => "ApprovalRequested".to_string(),
        PK::ApprovalGranted => "ApprovalGranted".to_string(),
        PK::ApprovalDenied => "ApprovalDenied".to_string(),
        PK::ToolCallPrepared => "ToolCallPrepared".to_string(),
        PK::ToolCallIntercepted => "ToolCallIntercepted".to_string(),
        PK::ToolCallExecuted => "ToolCallExecuted".to_string(),
        PK::ToolOutputReceived => "ToolOutputReceived".to_string(),
        PK::ToolOutputSanitized => "ToolOutputSanitized".to_string(),
        PK::DlpBlocked => "DlpBlocked".to_string(),
        PK::SideEffectPrepared => "SideEffectPrepared".to_string(),
        PK::SideEffectVerified => "SideEffectVerified".to_string(),
        PK::SideEffectCommitted => "SideEffectCommitted".to_string(),
        PK::SideEffectCompensated => "SideEffectCompensated".to_string(),
        PK::SideEffectRolledBack => "SideEffectRolledBack".to_string(),
        PK::Quarantined => "Quarantined".to_string(),
        PK::ErrorRaised => "ErrorRaised".to_string(),
        PK::ExecutionCancelled => "ExecutionCancelled".to_string(),
        PK::ExecutionPaused => "ExecutionPaused".to_string(),
        PK::ExternalEventObserved => "ExternalEventObserved".to_string(),
    }
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
        mut query,
        url,
        token,
        as_json,
        all_pages,
    } = options;

    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    if all_pages {
        // Export mode: follow cursors until exhaustion, emit JSONL to stdout.
        // Each line is one event as a separate JSON object.
        loop {
            let response = client.query_provenance(&query).await?;
            let next_cursor = response.next_cursor.clone();
            for event in response.events {
                println!("{}", serde_json::to_string(&event)?);
            }
            match next_cursor {
                Some(cursor) => {
                    query.cursor = Some(cursor);
                }
                None => break,
            }
        }
    } else if as_json {
        let response = client.query_provenance(&query).await?;
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        let response = client.query_provenance(&query).await?;
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
        if let Some(next_cursor) = response.next_cursor {
            println!("Next cursor: {}", next_cursor);
        }
    }
    Ok(())
}

/// Parse an optional UUID string into an IntentId.
fn parse_intent_id(s: Option<String>) -> Result<Option<IntentId>> {
    match s {
        Some(s) => Ok(Some(IntentId(Uuid::parse_str(&s)?))),
        None => Ok(None),
    }
}

/// Parse an optional UUID string into a ProposalId.
fn parse_proposal_id(s: Option<String>) -> Result<Option<ProposalId>> {
    match s {
        Some(s) => Ok(Some(ProposalId(Uuid::parse_str(&s)?))),
        None => Ok(None),
    }
}

/// Parse an optional UUID string into an ExecutionId.
fn parse_execution_id(s: Option<String>) -> Result<Option<ExecutionId>> {
    match s {
        Some(s) => Ok(Some(ExecutionId(Uuid::parse_str(&s)?))),
        None => Ok(None),
    }
}

/// Parse an optional UUID string into a CapabilityId.
fn parse_capability_id(s: Option<String>) -> Result<Option<CapabilityId>> {
    match s {
        Some(s) => Ok(Some(CapabilityId(Uuid::parse_str(&s)?))),
        None => Ok(None),
    }
}

/// Parse an optional ISO 8601 timestamp string into a DateTime<Utc>.
fn parse_timestamp(s: Option<String>) -> Result<Option<DateTime<Utc>>> {
    match s {
        Some(s) => {
            // Try RFC3339 format first, then ISO 8601
            let dt = DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    // Try ISO 8601 without timezone (assume UTC)
                    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S")
                        .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                })
                .with_context(|| format!("invalid timestamp format: {}", s))?;
            Ok(Some(dt))
        }
        None => Ok(None),
    }
}

/// Parse an optional event kind string into ProvenanceEventKind.
fn parse_event_kind(s: Option<String>) -> Result<Option<ProvenanceEventKind>> {
    match s {
        Some(s) => {
            let kind = match s.as_str() {
                "UserGoalReceived" => ProvenanceEventKind::UserGoalReceived,
                "IntentCompiled" => ProvenanceEventKind::IntentCompiled,
                "IntentRevoked" => ProvenanceEventKind::IntentRevoked,
                "ActionProposalSubmitted" => ProvenanceEventKind::ActionProposalSubmitted,
                "PolicyEvaluated" => ProvenanceEventKind::PolicyEvaluated,
                "CapabilityMinted" => ProvenanceEventKind::CapabilityMinted,
                "CapabilityRevoked" => ProvenanceEventKind::CapabilityRevoked,
                "ApprovalRequested" => ProvenanceEventKind::ApprovalRequested,
                "ApprovalGranted" => ProvenanceEventKind::ApprovalGranted,
                "ApprovalDenied" => ProvenanceEventKind::ApprovalDenied,
                "ToolCallPrepared" => ProvenanceEventKind::ToolCallPrepared,
                "ToolCallIntercepted" => ProvenanceEventKind::ToolCallIntercepted,
                "ToolCallExecuted" => ProvenanceEventKind::ToolCallExecuted,
                "ToolOutputReceived" => ProvenanceEventKind::ToolOutputReceived,
                "ToolOutputSanitized" => ProvenanceEventKind::ToolOutputSanitized,
                "DlpBlocked" => ProvenanceEventKind::DlpBlocked,
                "SideEffectPrepared" => ProvenanceEventKind::SideEffectPrepared,
                "SideEffectVerified" => ProvenanceEventKind::SideEffectVerified,
                "SideEffectCommitted" => ProvenanceEventKind::SideEffectCommitted,
                "SideEffectCompensated" => ProvenanceEventKind::SideEffectCompensated,
                "SideEffectRolledBack" => ProvenanceEventKind::SideEffectRolledBack,
                "Quarantined" => ProvenanceEventKind::Quarantined,
                "ErrorRaised" => ProvenanceEventKind::ErrorRaised,
                "ExternalEventObserved" => ProvenanceEventKind::ExternalEventObserved,
                _ => anyhow::bail!("unknown event kind: {}", s),
            };
            Ok(Some(kind))
        }
        None => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_export_provenance(
    intent_id: Option<String>,
    proposal_id: Option<String>,
    execution_id: Option<String>,
    capability_id: Option<String>,
    event_kind: Option<String>,
    terminal_only: bool,
    since: Option<String>,
    until: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    let req = ProvenanceExportRequest {
        intent_id: parse_intent_id(intent_id)?,
        proposal_id: parse_proposal_id(proposal_id)?,
        execution_id: parse_execution_id(execution_id)?,
        capability_id: parse_capability_id(capability_id)?,
        event_kind: parse_event_kind(event_kind)?,
        terminal_only: if terminal_only { Some(true) } else { None },
        since: parse_timestamp(since)?,
        until: parse_timestamp(until)?,
        limit,
        cursor,
    };

    let response = client.export_provenance(&req).await?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Provenance Export");
        println!("=================");
        println!("Exported at: {}", response.export_info.exported_at);
        println!("Total matched: {}", response.total_matched);
        println!("Exported count: {}", response.exported_count);
        if response.events.is_empty() {
            println!("No events found.");
            return Ok(());
        }
        println!("\nEvents ({}):", response.events.len());
        for event in response.events {
            println!(
                "  [{}] {:?}  {}",
                event.occurred_at, event.kind, event.event_id
            );
        }
        if let Some(next_cursor) = response.next_cursor {
            println!("Next cursor: {}", next_cursor);
        }
    }
    Ok(())
}

fn parse_edge_type(s: &str) -> Result<ProvenanceEdgeType, String> {
    match s {
        "DerivedFrom" => Ok(ProvenanceEdgeType::DerivedFrom),
        "AuthorizedBy" => Ok(ProvenanceEdgeType::AuthorizedBy),
        "ApprovedBy" => Ok(ProvenanceEdgeType::ApprovedBy),
        "TaintedBy" => Ok(ProvenanceEdgeType::TaintedBy),
        "UsesManifest" => Ok(ProvenanceEdgeType::UsesManifest),
        "EvaluatedByPolicy" => Ok(ProvenanceEdgeType::EvaluatedByPolicy),
        "Caused" => Ok(ProvenanceEdgeType::Caused),
        "Compensates" => Ok(ProvenanceEdgeType::Compensates),
        "Verifies" => Ok(ProvenanceEdgeType::Verifies),
        "References" => Ok(ProvenanceEdgeType::References),
        "ObservedBy" => Ok(ProvenanceEdgeType::ObservedBy),
        _ => Err(format!(
            "unknown edge type '{}': valid values are DerivedFrom, AuthorizedBy, \
             ApprovedBy, TaintedBy, UsesManifest, EvaluatedByPolicy, Caused, \
             Compensates, Verifies, References, ObservedBy",
            s
        )),
    }
}

/// Converts a ProvenanceEdgeType to its string representation for query parameters.
/// This is the reverse of parse_edge_type.
fn edge_type_to_string(et: &ProvenanceEdgeType) -> &'static str {
    match et {
        ProvenanceEdgeType::DerivedFrom => "DerivedFrom",
        ProvenanceEdgeType::AuthorizedBy => "AuthorizedBy",
        ProvenanceEdgeType::ApprovedBy => "ApprovedBy",
        ProvenanceEdgeType::TaintedBy => "TaintedBy",
        ProvenanceEdgeType::UsesManifest => "UsesManifest",
        ProvenanceEdgeType::EvaluatedByPolicy => "EvaluatedByPolicy",
        ProvenanceEdgeType::Caused => "Caused",
        ProvenanceEdgeType::Compensates => "Compensates",
        ProvenanceEdgeType::Verifies => "Verifies",
        ProvenanceEdgeType::References => "References",
        ProvenanceEdgeType::ObservedBy => "ObservedBy",
    }
}

/// Joins multiple ProvenanceEdgeType values into a comma-separated string
/// for use as a single query parameter value.
/// E.g., [DerivedFrom, AuthorizedBy] -> "DerivedFrom,AuthorizedBy"
fn edge_types_to_query_string(types: &[ProvenanceEdgeType]) -> String {
    types
        .iter()
        .map(edge_type_to_string)
        .collect::<Vec<_>>()
        .join(",")
}

async fn run_inspect_event(
    event_id: &str,
    ancestry: bool,
    descendants: bool,
    edge_types: Option<Vec<ProvenanceEdgeType>>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
) -> Result<()> {
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);
    let response = client
        .get_event(event_id, ancestry, descendants, edge_types)
        .await?;

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

/// Configuration for provenance stats inspection.
struct InspectProvenanceStatsConfig {
    intent_id: Option<String>,
    proposal_id: Option<String>,
    execution_id: Option<String>,
    capability_id: Option<String>,
    event_kind: Option<String>,
    since: Option<String>,
    until: Option<String>,
    max_events: Option<u32>,
    url: Option<String>,
    token: Option<String>,
    as_json: bool,
}

async fn run_inspect_provenance_stats(config: InspectProvenanceStatsConfig) -> Result<()> {
    let InspectProvenanceStatsConfig {
        intent_id,
        proposal_id,
        execution_id,
        capability_id,
        event_kind,
        since,
        until,
        max_events,
        url,
        token,
        as_json,
    } = config;
    let url = resolve_server_url(url)?;
    let client = ServerClient::new(&url, token);

    // Build stats request with properly parsed UUIDs
    let stats_request = ProvenanceStatsRequest {
        intent_id: parse_intent_id(intent_id)?,
        proposal_id: parse_proposal_id(proposal_id)?,
        execution_id: parse_execution_id(execution_id)?,
        capability_id: parse_capability_id(capability_id)?,
        event_kind: parse_event_kind(event_kind)?,
        since: parse_timestamp(since)?,
        until: parse_timestamp(until)?,
        max_events,
    };

    // Call server-side stats endpoint
    let stats_response = client.get_provenance_stats(&stats_request).await?;

    if as_json {
        // For JSON output, use the server response directly
        println!("{}", serde_json::to_string_pretty(&stats_response)?);
    } else {
        // Format as text to match existing output semantics
        println!("Total events: {}", stats_response.total_events);
        println!("Terminal events: {}", stats_response.terminal_count);
        println!(
            "Issue events (error/denied/quarantined/rolledback): {}",
            stats_response.issue_count
        );
        println!(
            "Events missing execution_id: {}",
            stats_response.events_without_execution_id
        );
        println!(
            "Unique intents: {}, proposals: {}, executions: {}",
            stats_response.unique_intents,
            stats_response.unique_proposals,
            stats_response.unique_executions
        );

        // Sort kinds by count descending for readability
        let mut kinds: Vec<(String, u64)> = stats_response
            .kinds
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1));
        println!("\nEvents by kind:");
        for (kind, count) in kinds {
            println!("  {}: {}", kind, count);
        }

        if !stats_response.flagged_events.is_empty() {
            println!(
                "\nFlagged events ({}):",
                stats_response.flagged_events.len()
            );
            for flagged in &stats_response.flagged_events {
                println!(
                    "  [{:?}] {}  {}",
                    flagged.kind, flagged.event_id, flagged.reason
                );
            }
        } else {
            println!("\nNo flagged events.");
        }
    }

    Ok(())
}

/// Configuration for external event ingestion.
struct IngestExternalEventConfig {
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
}

async fn run_ingest_external_event(config: IngestExternalEventConfig) -> Result<()> {
    let IngestExternalEventConfig {
        execution_id,
        parent_event_id,
        source_system,
        source_event_id,
        observed_at,
        summary,
        payload_digest,
        metadata_json,
        url,
        token,
        as_json,
    } = config;
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
            ServerCommand::InspectLineageQuery {
                execution_id,
                event_id,
                ancestry,
                descendants,
                max_hops,
                edge_type,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_lineage_query(
                    execution_id,
                    event_id,
                    ancestry,
                    descendants,
                    max_hops,
                    edge_type,
                    server_url,
                    bearer_token,
                    json,
                )
                .await?;
            }
            ServerCommand::Replay {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_replay(execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::InspectProvenanceStats {
                intent_id,
                proposal_id,
                execution_id,
                capability_id,
                event_kind,
                since,
                until,
                max_events,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_provenance_stats(InspectProvenanceStatsConfig {
                    intent_id,
                    proposal_id,
                    execution_id,
                    capability_id,
                    event_kind,
                    since,
                    until,
                    max_events,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                })
                .await?;
            }
            ServerCommand::InspectProvenance {
                intent_id,
                proposal_id,
                execution_id,
                execution_ids,
                capability_id,
                event_kind,
                terminal_only,
                since,
                until,
                limit,
                cursor,
                all_pages,
                server_url,
                bearer_token,
                json,
            } => {
                let query = ProvenanceQueryRequest {
                    intent_id,
                    proposal_id,
                    execution_id,
                    execution_ids,
                    capability_id,
                    event_kind,
                    terminal_only: terminal_only.then_some(true),
                    since,
                    until,
                    limit,
                    cursor,
                };
                run_inspect_provenance(InspectProvenanceOptions {
                    query,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                    all_pages,
                })
                .await?;
            }
            ServerCommand::ExportProvenance {
                intent_id,
                proposal_id,
                execution_id,
                capability_id,
                event_kind,
                terminal_only,
                since,
                until,
                limit,
                cursor,
                server_url,
                bearer_token,
                json,
            } => {
                run_export_provenance(
                    intent_id,
                    proposal_id,
                    execution_id,
                    capability_id,
                    event_kind,
                    terminal_only,
                    since,
                    until,
                    limit,
                    cursor,
                    server_url,
                    bearer_token,
                    json,
                )
                .await?;
            }
            ServerCommand::InspectEvent {
                event_id,
                ancestry,
                descendants,
                edge_type,
                server_url,
                bearer_token,
                json,
            } => {
                run_inspect_event(
                    &event_id,
                    ancestry,
                    descendants,
                    Some(edge_type),
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
                run_ingest_external_event(IngestExternalEventConfig {
                    execution_id,
                    parent_event_id,
                    source_system,
                    source_event_id,
                    observed_at,
                    summary,
                    payload_digest,
                    metadata_json,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                })
                .await?;
            }
            ServerCommand::ResolveApproval {
                approval_id,
                approve,
                deny,
                actor_type,
                actor_id,
                actor_display_name,
                reason,
                server_url,
                bearer_token,
                json,
            } => {
                run_resolve_approval(ResolveApprovalConfig {
                    approval_id: &approval_id,
                    approve,
                    deny,
                    actor_type,
                    actor_id: &actor_id,
                    actor_display_name,
                    reason,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                })
                .await?;
            }
            ServerCommand::ResolveApprovalBulk {
                all_pending,
                proposal_id,
                execution_id,
                limit,
                yes,
                expect_count,
                approve,
                deny,
                actor_type,
                actor_id,
                actor_display_name,
                reason,
                server_url,
                bearer_token,
                json,
            } => {
                run_resolve_approval_bulk(ResolveApprovalBulkConfig {
                    all_pending,
                    proposal_id,
                    execution_id,
                    limit,
                    yes,
                    expect_count,
                    approve,
                    deny,
                    actor_type,
                    actor_id: &actor_id,
                    actor_display_name,
                    reason,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                })
                .await?;
            }
            ServerCommand::WatchApprovals {
                proposal_id,
                execution_id,
                limit,
                cursor,
                poll_interval_ms,
                iterations,
                server_url,
                bearer_token,
                json,
            } => {
                run_watch_approvals(WatchApprovalsConfig {
                    proposal_id,
                    execution_id,
                    limit,
                    cursor,
                    poll_interval_ms,
                    iterations,
                    url: server_url,
                    token: bearer_token,
                    as_json: json,
                })
                .await?;
            }
            ServerCommand::WatchExecution {
                execution_id,
                poll_interval_ms,
                iterations,
                server_url,
                bearer_token,
                json,
                require_terminal,
            } => {
                run_watch_execution(
                    &execution_id,
                    poll_interval_ms,
                    iterations,
                    server_url,
                    bearer_token,
                    json,
                    require_terminal,
                )
                .await?;
            }
            ServerCommand::CompensateExecution {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_compensate_execution(&execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::RollbackExecution {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_rollback_execution(&execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::CancelExecution {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_cancel_execution(&execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::PauseExecution {
                execution_id,
                server_url,
                bearer_token,
                json,
            } => {
                run_pause_execution(&execution_id, server_url, bearer_token, json).await?;
            }
            ServerCommand::VerifyLedger {
                server_url,
                bearer_token,
                json,
            } => {
                run_verify_ledger(server_url, bearer_token, json).await?;
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

    // -------------------------------------------------------------------------
    // Provenance stats aggregation tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_aggregate_provenance_stats_empty() {
        let events: Vec<ProvenanceEvent> = vec![];
        let stats = aggregate_provenance_stats(&events);
        assert_eq!(stats.total_events, 0);
        assert_eq!(stats.terminal_count, 0);
        assert_eq!(stats.issue_count, 0);
        assert!(stats.flagged_events.is_empty());
    }

    #[test]
    fn test_aggregate_provenance_stats_counts_terminal() {
        let events = vec![
            ProvenanceEvent {
                event_id: "evt-1".to_string(),
                kind: "IntentCompiled".to_string(),
                occurred_at: "2024-01-01T00:00:00Z".to_string(),
                intent_id: Some("intent-1".to_string()),
                proposal_id: None,
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![],
            },
            ProvenanceEvent {
                event_id: "evt-2".to_string(),
                kind: "SideEffectCommitted".to_string(),
                occurred_at: "2024-01-01T00:00:01Z".to_string(),
                intent_id: Some("intent-1".to_string()),
                proposal_id: None,
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![ProvenanceEdge {
                    from_event_id: "evt-1".to_string(),
                }],
            },
        ];
        let stats = aggregate_provenance_stats(&events);
        assert_eq!(stats.total_events, 2);
        assert_eq!(stats.terminal_count, 1); // SideEffectCommitted is terminal
        assert_eq!(stats.issue_count, 0);
        assert_eq!(stats.kinds.get("IntentCompiled"), Some(&1));
        assert_eq!(stats.kinds.get("SideEffectCommitted"), Some(&1));
    }

    #[test]
    fn test_aggregate_provenance_stats_flags_terminal_without_execution_id() {
        let events = vec![ProvenanceEvent {
            event_id: "evt-1".to_string(),
            kind: "SideEffectCommitted".to_string(),
            occurred_at: "2024-01-01T00:00:00Z".to_string(),
            intent_id: None,
            proposal_id: None,
            execution_id: None, // missing execution_id
            parent_edges: vec![],
        }];
        let stats = aggregate_provenance_stats(&events);
        assert_eq!(stats.terminal_count, 1);
        assert_eq!(stats.events_without_execution_id, 1);
        assert_eq!(stats.flagged_events.len(), 1);
        assert_eq!(
            stats.flagged_events[0].reason,
            "terminal event missing execution_id"
        );
    }

    #[test]
    fn test_aggregate_provenance_stats_issues() {
        let events = vec![
            ProvenanceEvent {
                event_id: "evt-1".to_string(),
                kind: "ErrorRaised".to_string(),
                occurred_at: "2024-01-01T00:00:00Z".to_string(),
                intent_id: None,
                proposal_id: None,
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![],
            },
            ProvenanceEvent {
                event_id: "evt-2".to_string(),
                kind: "ApprovalDenied".to_string(),
                occurred_at: "2024-01-01T00:00:01Z".to_string(),
                intent_id: None,
                proposal_id: None,
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![],
            },
            ProvenanceEvent {
                event_id: "evt-3".to_string(),
                kind: "Quarantined".to_string(),
                occurred_at: "2024-01-01T00:00:02Z".to_string(),
                intent_id: None,
                proposal_id: None,
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![],
            },
            ProvenanceEvent {
                event_id: "evt-4".to_string(),
                kind: "SideEffectRolledBack".to_string(),
                occurred_at: "2024-01-01T00:00:03Z".to_string(),
                intent_id: None,
                proposal_id: None,
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![],
            },
        ];
        let stats = aggregate_provenance_stats(&events);
        assert_eq!(stats.issue_count, 4);
        assert_eq!(stats.terminal_count, 4); // all are terminal
    }

    #[test]
    fn test_aggregate_provenance_stats_tracks_unique_entities() {
        let events = vec![
            ProvenanceEvent {
                event_id: "evt-1".to_string(),
                kind: "IntentCompiled".to_string(),
                occurred_at: "2024-01-01T00:00:00Z".to_string(),
                intent_id: Some("intent-1".to_string()),
                proposal_id: Some("prop-1".to_string()),
                execution_id: Some("exec-1".to_string()),
                parent_edges: vec![],
            },
            ProvenanceEvent {
                event_id: "evt-2".to_string(),
                kind: "IntentCompiled".to_string(),
                occurred_at: "2024-01-01T00:00:01Z".to_string(),
                intent_id: Some("intent-1".to_string()), // same intent
                proposal_id: Some("prop-1".to_string()), // same proposal
                execution_id: Some("exec-2".to_string()), // different exec
                parent_edges: vec![],
            },
        ];
        let stats = aggregate_provenance_stats(&events);
        assert_eq!(stats.events_by_intent.len(), 1); // 1 unique intent
        assert_eq!(stats.events_by_proposal.len(), 1); // 1 unique proposal
        assert_eq!(stats.events_by_execution.len(), 2); // 2 unique executions
        assert_eq!(stats.events_by_intent.get("intent-1"), Some(&2));
    }

    #[test]
    fn test_format_provenance_stats_text_empty() {
        let stats = ProvenanceStats::default();
        let output = format_provenance_stats_text(&stats);
        assert!(output.contains("Total events: 0"));
        assert!(output.contains("No flagged events"));
    }

    #[test]
    fn test_format_provenance_stats_text_with_data() {
        let mut stats = ProvenanceStats::default();
        stats.total_events = 5;
        stats.terminal_count = 2;
        stats.issue_count = 1;
        stats.events_without_execution_id = 0;
        stats.kinds.insert("IntentCompiled".to_string(), 3);
        stats.kinds.insert("SideEffectCommitted".to_string(), 2);
        stats.flagged_events.push(FlaggedEvent {
            event_id: "evt-flagged".to_string(),
            kind: "ErrorRaised".to_string(),
            reason: "terminal event missing execution_id".to_string(),
        });

        let output = format_provenance_stats_text(&stats);
        assert!(output.contains("Total events: 5"));
        assert!(output.contains("Terminal events: 2"));
        assert!(output.contains("Issue events (error/denied/quarantined/rolledback): 1"));
        assert!(output.contains("IntentCompiled: 3"));
        assert!(output.contains("SideEffectCommitted: 2"));
        assert!(output.contains("Flagged events (1)"));
        assert!(output.contains("evt-flagged"));
    }

    #[test]
    fn test_provenance_stats_json_conversion() {
        let mut stats = ProvenanceStats::default();
        stats.total_events = 10;
        stats.terminal_count = 5;
        stats.issue_count = 2;
        stats.events_by_intent.insert("intent-x".to_string(), 3);
        stats.events_by_proposal.insert("prop-y".to_string(), 4);
        stats.events_by_execution.insert("exec-z".to_string(), 5);

        let json: ProvenanceStatsJson = stats.into();
        assert_eq!(json.total_events, 10);
        assert_eq!(json.terminal_count, 5);
        assert_eq!(json.issue_count, 2);
        assert_eq!(json.events_by_intent_count, 1);
        assert_eq!(json.events_by_proposal_count, 1);
        assert_eq!(json.events_by_execution_count, 1);
    }

    // =============================================================================
    // ResolveApproval tests
    // =============================================================================

    #[test]
    fn test_actor_type_all_variants() {
        // Verify all ActorType variants exist and can be constructed
        let _ = ActorType::User;
        let _ = ActorType::Agent;
        let _ = ActorType::PolicyEngine;
        let _ = ActorType::Gateway;
        let _ = ActorType::Adapter;
        let _ = ActorType::Operator;
        let _ = ActorType::System;
    }

    #[test]
    fn test_approval_resolve_request_serialization() {
        let actor = ActorRef {
            actor_type: ActorType::Operator,
            actor_id: "test-op".to_string(),
            display_name: None,
        };
        let req = ApprovalResolveRequest {
            actor,
            approve: true,
            reason: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"approve\":true"));
        assert!(json.contains("\"actor_id\":\"test-op\""));
        assert!(json.contains("\"actor_type\":\"Operator\""));
    }

    #[test]
    fn test_approval_resolve_request_deny_with_reason() {
        let actor = ActorRef {
            actor_type: ActorType::User,
            actor_id: "alice".to_string(),
            display_name: Some("Alice".to_string()),
        };
        let req = ApprovalResolveRequest {
            actor,
            approve: false,
            reason: Some("Not authorized for this action".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"approve\":false"));
        assert!(json.contains("\"reason\":\"Not authorized for this action\""));
    }

    // -------------------------------------------------------------------------
    // Bulk approval resolution tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_pending_state() {
        assert!(is_pending_state("Pending"));
        assert!(!is_pending_state("Approved"));
        assert!(!is_pending_state("Denied"));
        assert!(!is_pending_state("Expired"));
        assert!(!is_pending_state("Cancelled"));
    }

    #[test]
    fn test_format_bulk_outcome_resolved() {
        let outcome = BulkResolutionOutcome::Resolved {
            approval_id: "approval-abc".to_string(),
            decision: "Approved".to_string(),
            state: "Approved".to_string(),
        };
        let formatted = format_bulk_outcome(&outcome);
        assert!(formatted.contains("RESOLVED"));
        assert!(formatted.contains("approval-abc"));
        assert!(formatted.contains("decision=Approved"));
        assert!(formatted.contains("state=Approved"));
    }

    #[test]
    fn test_format_bulk_outcome_rejected() {
        let outcome = BulkResolutionOutcome::MutationRejected {
            approval_id: "approval-xyz".to_string(),
            http_status: 409,
            state: "Pending".to_string(),
        };
        let formatted = format_bulk_outcome(&outcome);
        assert!(formatted.contains("REJECTED"));
        assert!(formatted.contains("approval-xyz"));
        assert!(formatted.contains("HTTP 409"));
        assert!(formatted.contains("state=Pending"));
    }

    #[test]
    fn test_format_bulk_outcome_conflicted() {
        let outcome = BulkResolutionOutcome::MutationConflicted {
            approval_id: "approval-conf".to_string(),
            http_status: 500,
            decision: "Approved".to_string(),
            state: "Approved".to_string(),
        };
        let formatted = format_bulk_outcome(&outcome);
        assert!(formatted.contains("CONFLICT"));
        assert!(formatted.contains("approval-conf"));
        assert!(formatted.contains("HTTP 500"));
        assert!(formatted.contains("decision=Approved"));
    }

    #[test]
    fn test_format_bulk_outcome_unreadable() {
        let outcome = BulkResolutionOutcome::Unreadable {
            approval_id: "approval-unr".to_string(),
            http_status: 503,
            read_error: "connection refused".to_string(),
        };
        let formatted = format_bulk_outcome(&outcome);
        assert!(formatted.contains("UNREADABLE"));
        assert!(formatted.contains("approval-unr"));
        assert!(formatted.contains("HTTP 503"));
        assert!(formatted.contains("connection refused"));
    }

    #[test]
    fn test_bulk_resolution_outcome_serialize_resolved() {
        let outcome = BulkResolutionOutcome::Resolved {
            approval_id: "approval-s".to_string(),
            decision: "Approved".to_string(),
            state: "Approved".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"Resolved\""));
        assert!(json.contains("\"approval_id\":\"approval-s\""));
        assert!(json.contains("\"decision\":\"Approved\""));
    }

    #[test]
    fn test_bulk_resolution_outcome_serialize_rejected() {
        let outcome = BulkResolutionOutcome::MutationRejected {
            approval_id: "approval-r".to_string(),
            http_status: 409,
            state: "Pending".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"MutationRejected\""));
        assert!(json.contains("\"approval_id\":\"approval-r\""));
        assert!(json.contains("\"http_status\":409"));
    }

    #[test]
    fn test_bulk_resolution_outcome_serialize_conflicted() {
        let outcome = BulkResolutionOutcome::MutationConflicted {
            approval_id: "approval-c".to_string(),
            http_status: 500,
            decision: "Denied".to_string(),
            state: "Denied".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"MutationConflicted\""));
        assert!(json.contains("\"http_status\":500"));
    }

    #[test]
    fn test_bulk_resolution_outcome_serialize_unreadable() {
        let outcome = BulkResolutionOutcome::Unreadable {
            approval_id: "approval-u".to_string(),
            http_status: 503,
            read_error: "timeout".to_string(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"Unreadable\""));
        assert!(json.contains("\"read_error\":\"timeout\""));
    }

    #[test]
    fn test_extract_http_status_from_non_reqwest_error() {
        // A regular anyhow error with no reqwest in the chain
        let err = anyhow::Error::msg("some other error");
        let status = extract_http_status(&err);
        assert_eq!(status, 0);
    }

    // -------------------------------------------------------------------------
    // Lineage query validation and formatting tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_max_hops_none() {
        let result = validate_max_hops(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_validate_max_hops_valid_values() {
        for v in [1u32, 8, 16, 32] {
            let result = validate_max_hops(Some(v));
            assert!(result.is_ok(), "max_hops={} should be valid", v);
            assert_eq!(result.unwrap(), Some(v));
        }
    }

    #[test]
    fn test_validate_max_hops_too_low() {
        let result = validate_max_hops(Some(0));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("between 1 and 32"));
    }

    #[test]
    fn test_validate_max_hops_too_high() {
        let result = validate_max_hops(Some(33));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("between 1 and 32"));
    }

    #[test]
    fn test_kind_label_all_variants() {
        use ferrum_proto::ProvenanceEventKind as PK;
        let variants: Vec<(PK, &str)> = vec![
            (PK::UserGoalReceived, "UserGoalReceived"),
            (PK::IntentCompiled, "IntentCompiled"),
            (PK::IntentRevoked, "IntentRevoked"),
            (PK::ActionProposalSubmitted, "ActionProposalSubmitted"),
            (PK::PolicyEvaluated, "PolicyEvaluated"),
            (PK::CapabilityMinted, "CapabilityMinted"),
            (PK::CapabilityRevoked, "CapabilityRevoked"),
            (PK::ApprovalRequested, "ApprovalRequested"),
            (PK::ApprovalGranted, "ApprovalGranted"),
            (PK::ApprovalDenied, "ApprovalDenied"),
            (PK::ToolCallPrepared, "ToolCallPrepared"),
            (PK::ToolCallIntercepted, "ToolCallIntercepted"),
            (PK::ToolCallExecuted, "ToolCallExecuted"),
            (PK::ToolOutputReceived, "ToolOutputReceived"),
            (PK::ToolOutputSanitized, "ToolOutputSanitized"),
            (PK::DlpBlocked, "DlpBlocked"),
            (PK::SideEffectPrepared, "SideEffectPrepared"),
            (PK::SideEffectVerified, "SideEffectVerified"),
            (PK::SideEffectCommitted, "SideEffectCommitted"),
            (PK::SideEffectCompensated, "SideEffectCompensated"),
            (PK::SideEffectRolledBack, "SideEffectRolledBack"),
            (PK::Quarantined, "Quarantined"),
            (PK::ErrorRaised, "ErrorRaised"),
            (PK::ExternalEventObserved, "ExternalEventObserved"),
        ];
        for (kind, expected) in variants {
            let label = kind_label(&kind);
            assert_eq!(
                label,
                expected,
                "variant {:?}",
                std::mem::discriminant(&kind)
            );
        }
    }

    #[test]
    fn test_format_lineage_query_text_empty() {
        let resp = ferrum_proto::LineageQueryResponse {
            events: vec![],
            edges: vec![],
        };
        let text = format_lineage_query_text(&resp);
        assert!(text.contains("LineageQuery: 0 event(s), 0 edge(s)"));
        assert!(text.contains("Events:"));
    }

    #[test]
    fn test_format_lineage_query_text_edge_rendering() {
        // Build a minimal LineageQueryResponse by deserializing from JSON
        // to avoid constructing full ProvenanceEvent with all nested types.
        let json = r#"{
            "events": [
                {
                    "event_id": "00000000-0000-0000-0000-000000000001",
                    "kind": "IntentCompiled",
                    "occurred_at": "2024-01-01T00:00:00Z",
                    "actor": {"actor_type": "System", "actor_id": "sys"},
                    "object": {"object_type": "Intent", "object_id": "obj1"},
                    "intent_id": null,
                    "proposal_id": null,
                    "execution_id": null,
                    "capability_id": null,
                    "rollback_contract_id": null,
                    "policy_bundle_id": null,
                    "trust_labels": [],
                    "sensitivity_labels": [],
                    "parent_edges": [],
                    "hash_chain": {"content_hash": ""},
                    "metadata": {}
                }
            ],
            "edges": []
        }"#;
        let resp: ferrum_proto::LineageQueryResponse =
            serde_json::from_str(json).expect("valid test fixture");
        let text = format_lineage_query_text(&resp);
        assert!(text.contains("LineageQuery: 1 event(s), 0 edge(s)"));
        assert!(text.contains("IntentCompiled"));
    }

    #[test]
    fn test_lineage_query_request_serialization() {
        let req = LineageQueryRequest {
            execution_id: ferrum_proto::ExecutionId(
                uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            ),
            event_id: ferrum_proto::EventId(
                uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap(),
            ),
            ancestry: true,
            descendants: false,
            max_hops: Some(8),
            edge_types: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"execution_id\":\"11111111-1111-1111-1111-111111111111\""));
        assert!(json.contains("\"event_id\":\"22222222-2222-2222-2222-222222222222\""));
        assert!(json.contains("\"ancestry\":true"));
        assert!(json.contains("\"descendants\":false"));
        assert!(json.contains("\"max_hops\":8"));
    }

    #[test]
    fn test_lineage_query_request_minimal() {
        // Only required fields
        let req = LineageQueryRequest {
            execution_id: ferrum_proto::ExecutionId(
                uuid::Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
            ),
            event_id: ferrum_proto::EventId(
                uuid::Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap(),
            ),
            ancestry: false,
            descendants: true,
            max_hops: None,
            edge_types: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"ancestry\":false"));
        assert!(json.contains("\"descendants\":true"));
        assert!(json.contains("\"max_hops\":null"));
    }

    // -------------------------------------------------------------------------
    // WatchApprovals validation and formatting tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_poll_interval_ms_none() {
        // None should return default
        let result = validate_poll_interval_ms(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5_000);
    }

    #[test]
    fn test_validate_poll_interval_ms_valid_values() {
        for v in [100u64, 1_000, 5_000, 60_000, 300_000] {
            let result = validate_poll_interval_ms(Some(v));
            assert!(result.is_ok(), "poll_interval_ms={} should be valid", v);
            assert_eq!(result.unwrap(), v);
        }
    }

    #[test]
    fn test_validate_poll_interval_ms_too_low() {
        let result = validate_poll_interval_ms(Some(99));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("between 100 and 300000"));
    }

    #[test]
    fn test_validate_poll_interval_ms_too_high() {
        let result = validate_poll_interval_ms(Some(300_001));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("between 100 and 300000"));
    }

    #[test]
    fn test_format_watch_iteration_text_empty() {
        let envelope = ApprovalListEnvelope {
            items: vec![],
            next_cursor: None,
        };
        let text = format_watch_iteration_text(&envelope, 1);
        assert!(text.contains("--- iteration 1 (0 approval(s), next_cursor=none) ---"));
    }

    #[test]
    fn test_format_watch_iteration_text_single_approval() {
        let envelope = ApprovalListEnvelope {
            items: vec![ApprovalRequest {
                approval_id: "approval-1".to_string(),
                intent_id: "intent-1".to_string(),
                proposal_id: "proposal-1".to_string(),
                execution_id: Some("exec-1".to_string()),
                reason: "test reason".to_string(),
                action_digest: "action-digest-1".to_string(),
                expires_at: "2024-01-01T00:15:00Z".to_string(),
                state: "Pending".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
            }],
            next_cursor: Some("cursor-abc".to_string()),
        };
        let text = format_watch_iteration_text(&envelope, 3);
        assert!(text.contains("--- iteration 3 (1 approval(s), next_cursor=cursor-abc) ---"));
        assert!(text.contains("Approval: approval-1"));
        assert!(text.contains("State:    Pending"));
        assert!(text.contains("Intent:   intent-1"));
        assert!(text.contains("Proposal: proposal-1"));
        assert!(text.contains("Execution:exec-1"));
        assert!(text.contains("Reason:   test reason"));
    }

    #[test]
    fn test_format_watch_iteration_text_deterministic_order() {
        // Two approvals with different states - Pending should sort first
        let envelope = ApprovalListEnvelope {
            items: vec![
                ApprovalRequest {
                    approval_id: "approval-2".to_string(),
                    intent_id: "intent-x".to_string(),
                    proposal_id: "proposal-x".to_string(),
                    execution_id: None,
                    reason: "second".to_string(),
                    action_digest: "digest-2".to_string(),
                    expires_at: "2024-01-01T00:15:00Z".to_string(),
                    state: "Approved".to_string(),
                    created_at: "2024-01-01T00:00:01Z".to_string(),
                },
                ApprovalRequest {
                    approval_id: "approval-1".to_string(),
                    intent_id: "intent-x".to_string(),
                    proposal_id: "proposal-x".to_string(),
                    execution_id: None,
                    reason: "first".to_string(),
                    action_digest: "digest-1".to_string(),
                    expires_at: "2024-01-01T00:15:00Z".to_string(),
                    state: "Pending".to_string(),
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                },
            ],
            next_cursor: None,
        };
        let text = format_watch_iteration_text(&envelope, 1);
        // Pending approval should appear first despite being created earlier
        let lines: Vec<&str> = text.lines().collect();
        let pending_pos = lines.iter().position(|l| l.contains("approval-1")).unwrap();
        let approved_pos = lines.iter().position(|l| l.contains("approval-2")).unwrap();
        assert!(
            pending_pos < approved_pos,
            "Pending approval should sort before Approved"
        );
    }

    #[test]
    fn test_format_watch_iteration_text_next_cursor_display() {
        let envelope_with_cursor = ApprovalListEnvelope {
            items: vec![],
            next_cursor: Some("cursor-xyz".to_string()),
        };
        let text = format_watch_iteration_text(&envelope_with_cursor, 5);
        assert!(text.contains("next_cursor=cursor-xyz"));

        let envelope_no_cursor = ApprovalListEnvelope {
            items: vec![],
            next_cursor: None,
        };
        let text_no_cursor = format_watch_iteration_text(&envelope_no_cursor, 5);
        assert!(text_no_cursor.contains("next_cursor=none"));
    }

    // -------------------------------------------------------------------------
    // WatchExecution terminal state and formatting tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_execution_terminal_state_terminal() {
        for state in &[
            "Completed",
            "Committed",
            "Approved",
            "Denied",
            "RolledBack",
            "Error",
            "Quarantined",
            "Cancelled",
            "TimedOut",
        ] {
            assert!(
                is_execution_terminal_state(state),
                "state '{}' should be terminal",
                state
            );
        }
    }

    #[test]
    fn test_is_execution_terminal_state_non_terminal() {
        for state in &[
            "Pending",
            "Running",
            "Executing",
            "AwaitingApproval",
            "Paused",
        ] {
            assert!(
                !is_execution_terminal_state(state),
                "state '{}' should not be terminal",
                state
            );
        }
    }

    #[test]
    fn test_format_execution_record_text_non_terminal() {
        let record = ExecutionRecord {
            execution_id: "exec-123".to_string(),
            proposal_id: "proposal-456".to_string(),
            intent_id: "intent-789".to_string(),
            capability_id: "cap-abc".to_string(),
            rollback_contract_id: None,
            decision: "Pending".to_string(),
            state: "Running".to_string(),
            started_at: "2024-01-01T12:00:00Z".to_string(),
            finished_at: None,
            result_digest: None,
        };
        let text = format_execution_record_text(&record, 3);
        assert!(text.contains("--- iteration 3 (execution_id=exec-123, state=Running) ---"));
        assert!(text.contains("  Decision:  Pending"));
        assert!(text.contains("  Intent:    intent-789"));
        assert!(text.contains("  Proposal:  proposal-456"));
        assert!(text.contains("  Capability:cap-abc"));
        assert!(text.contains("  Started:   2024-01-01T12:00:00Z"));
        // No [TERMINAL] marker for non-terminal state
        assert!(!text.contains("[TERMINAL]"));
    }

    #[test]
    fn test_format_execution_record_text_terminal() {
        let record = ExecutionRecord {
            execution_id: "exec-abc".to_string(),
            proposal_id: "proposal-def".to_string(),
            intent_id: "intent-ghi".to_string(),
            capability_id: "cap-xyz".to_string(),
            rollback_contract_id: Some("rollback-123".to_string()),
            decision: "Approved".to_string(),
            state: "Completed".to_string(),
            started_at: "2024-01-01T12:00:00Z".to_string(),
            finished_at: Some("2024-01-01T12:05:00Z".to_string()),
            result_digest: Some("sha256:abc123".to_string()),
        };
        let text = format_execution_record_text(&record, 1);
        assert!(
            text.contains(
                "--- iteration 1 (execution_id=exec-abc, state=Completed [TERMINAL]) ---"
            )
        );
        assert!(text.contains("  Decision:  Approved"));
        assert!(text.contains("  Rollback:  rollback-123"));
        assert!(text.contains("  Digest:    sha256:abc123"));
        assert!(text.contains("  Finished:  2024-01-01T12:05:00Z"));
    }

    #[test]
    fn test_format_execution_record_text_shows_all_fields() {
        let record = ExecutionRecord {
            execution_id: "exec-full".to_string(),
            proposal_id: "prop-full".to_string(),
            intent_id: "intent-full".to_string(),
            capability_id: "cap-full".to_string(),
            rollback_contract_id: None,
            decision: "Approved".to_string(),
            state: "Committed".to_string(),
            started_at: "2024-06-15T10:30:00Z".to_string(),
            finished_at: None,
            result_digest: None,
        };
        let text = format_execution_record_text(&record, 5);
        // Verify all standard fields are present
        assert!(text.contains("execution_id=exec-full"));
        assert!(text.contains("state=Committed"));
        assert!(text.contains("Decision:"));
        assert!(text.contains("Intent:"));
        assert!(text.contains("Proposal:"));
        assert!(text.contains("Capability:"));
        assert!(text.contains("Started:"));
    }

    // -------------------------------------------------------------------------
    // Edge type parsing and encoding tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_edge_type_to_string_all_variants() {
        use ProvenanceEdgeType::*;
        let cases = [
            (DerivedFrom, "DerivedFrom"),
            (AuthorizedBy, "AuthorizedBy"),
            (ApprovedBy, "ApprovedBy"),
            (TaintedBy, "TaintedBy"),
            (UsesManifest, "UsesManifest"),
            (EvaluatedByPolicy, "EvaluatedByPolicy"),
            (Caused, "Caused"),
            (Compensates, "Compensates"),
            (Verifies, "Verifies"),
            (References, "References"),
            (ObservedBy, "ObservedBy"),
        ];
        for (et, expected) in cases {
            let result = edge_type_to_string(&et);
            assert_eq!(
                result, expected,
                "edge_type_to_string({:?}) should be '{}', got '{}'",
                et, expected, result
            );
        }
    }

    #[test]
    fn test_edge_type_to_string_not_json_quoted() {
        // Verify edge_type_to_string produces plain string, not JSON-quoted
        let et = ProvenanceEdgeType::DerivedFrom;
        let result = edge_type_to_string(&et);
        // Should NOT contain quotes - JSON serialization would produce "\"DerivedFrom\""
        assert!(
            !result.starts_with('"'),
            "edge_type_to_string should not produce JSON-quoted string, got '{}'",
            result
        );
        assert!(
            !result.ends_with('"'),
            "edge_type_to_string should not produce JSON-quoted string, got '{}'",
            result
        );
    }

    #[test]
    fn test_parse_edge_type_and_to_string_are_inverses() {
        // Verify parse_edge_type and edge_type_to_string are inverses
        let variants = [
            "DerivedFrom",
            "AuthorizedBy",
            "ApprovedBy",
            "TaintedBy",
            "UsesManifest",
            "EvaluatedByPolicy",
            "Caused",
            "Compensates",
            "Verifies",
            "References",
            "ObservedBy",
        ];
        for variant_str in variants {
            let parsed = parse_edge_type(variant_str).expect("valid edge type");
            let back_to_string = edge_type_to_string(&parsed);
            assert_eq!(
                back_to_string, variant_str,
                "parse_edge_type and edge_type_to_string should be inverses for '{}'",
                variant_str
            );
        }
    }

    #[test]
    fn test_edge_types_to_query_string_single() {
        use ProvenanceEdgeType::*;
        // Single edge type produces no commas
        let result = edge_types_to_query_string(&[DerivedFrom]);
        assert_eq!(result, "DerivedFrom");
    }

    #[test]
    fn test_edge_types_to_query_string_multiple() {
        use ProvenanceEdgeType::*;
        // Multiple edge types joined with commas
        let result = edge_types_to_query_string(&[DerivedFrom, AuthorizedBy]);
        assert_eq!(result, "DerivedFrom,AuthorizedBy");
    }

    #[test]
    fn test_edge_types_to_query_string_three() {
        use ProvenanceEdgeType::*;
        // Three edge types
        let result = edge_types_to_query_string(&[DerivedFrom, AuthorizedBy, TaintedBy]);
        assert_eq!(result, "DerivedFrom,AuthorizedBy,TaintedBy");
    }

    #[test]
    fn test_edge_types_to_query_string_empty() {
        // Empty slice produces empty string
        let result = edge_types_to_query_string(&[]);
        assert_eq!(result, "");
    }

    #[test]
    fn test_edge_types_to_query_string_ascii_only() {
        use ProvenanceEdgeType::*;
        // Result should be pure ASCII (no special chars)
        let result = edge_types_to_query_string(&[DerivedFrom, AuthorizedBy, ApprovedBy]);
        assert!(result.is_ascii());
        assert!(!result.contains(' '));
    }

    // ---------------------------------------------------------------------------
    // Ledger verification type tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_ledger_verification_response_valid_empty() {
        use ferrum_proto::api::LedgerVerificationResponse;

        let json = r#"{"valid":true,"entry_count":0,"verified_at":"2026-03-30T12:00:00Z"}"#;
        let resp: LedgerVerificationResponse = serde_json::from_str(json).unwrap();
        assert!(resp.valid);
        assert_eq!(resp.entry_count, 0);
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_ledger_verification_response_valid_with_entries() {
        use ferrum_proto::api::LedgerVerificationResponse;

        let json = r#"{"valid":true,"entry_count":5,"verified_at":"2026-03-30T12:00:00Z"}"#;
        let resp: LedgerVerificationResponse = serde_json::from_str(json).unwrap();
        assert!(resp.valid);
        assert_eq!(resp.entry_count, 5);
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_ledger_verification_response_invalid_broken_chain() {
        use ferrum_proto::api::{LedgerVerificationError, LedgerVerificationResponse};

        let json = r#"{"valid":false,"entry_count":3,"verified_at":"2026-03-30T12:00:00Z","error":{"type":"BrokenChain","detail":{"expected":"abc123","actual":"def456"}}}"#;
        let resp: LedgerVerificationResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.valid);
        assert_eq!(resp.entry_count, 3);
        assert!(resp.error.is_some());

        if let Some(LedgerVerificationError::BrokenChain { expected, actual }) = resp.error {
            assert_eq!(expected, "abc123");
            assert_eq!(actual, "def456");
        } else {
            panic!("expected BrokenChain error");
        }
    }

    #[test]
    fn test_ledger_verification_response_invalid_tamper_detected() {
        use ferrum_proto::api::{LedgerVerificationError, LedgerVerificationResponse};

        let json = r#"{"valid":false,"entry_count":2,"verified_at":"2026-03-30T12:00:00Z","error":{"type":"TamperDetected","detail":{"sequence":1,"recorded":"xyz","recomputed":"abc"}}}"#;
        let resp: LedgerVerificationResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.valid);

        if let Some(LedgerVerificationError::TamperDetected {
            sequence,
            recorded,
            recomputed,
        }) = resp.error
        {
            assert_eq!(sequence, 1);
            assert_eq!(recorded, "xyz");
            assert_eq!(recomputed, "abc");
        } else {
            panic!("expected TamperDetected error");
        }
    }

    #[test]
    fn test_ledger_verification_response_invalid_sequence_mismatch() {
        use ferrum_proto::api::{LedgerVerificationError, LedgerVerificationResponse};

        let json = r#"{"valid":false,"entry_count":1,"verified_at":"2026-03-30T12:00:00Z","error":{"type":"SequenceMismatch","detail":{"event_seq":5,"ledger_len":3}}}"#;
        let resp: LedgerVerificationResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.valid);

        if let Some(LedgerVerificationError::SequenceMismatch {
            event_seq,
            ledger_len,
        }) = resp.error
        {
            assert_eq!(event_seq, 5);
            assert_eq!(ledger_len, 3);
        } else {
            panic!("expected SequenceMismatch error");
        }
    }

    #[test]
    fn test_ledger_verification_response_invalid_empty_ledger() {
        use ferrum_proto::api::{LedgerVerificationError, LedgerVerificationResponse};

        // EmptyLedger is a unit variant - JSON is {"type": "EmptyLedger"} without detail
        let json = r#"{"valid":false,"entry_count":0,"verified_at":"2026-03-30T12:00:00Z","error":{"type":"EmptyLedger"}}"#;
        let resp: LedgerVerificationResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.valid);
        assert!(matches!(
            resp.error,
            Some(LedgerVerificationError::EmptyLedger)
        ));
    }
}
