use anyhow::Result;
use chrono::Local;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
};
use std::{
    io::{self, IsTerminal},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

mod app;
mod client;

use app::{
    App, ApprovalsView, AuditVerifyView, MetricsView, ProbeResult, ProbeStatus, SloWindowView, Tab,
};
use client::{ApprovalRequest, Client};

#[derive(Debug, Parser)]
#[command(name = "ferrum-tui")]
#[command(about = "FerrumGate operator TUI dashboard")]
struct Args {
    /// Server base URL.
    /// Env: FERRUM_TUI_SERVER_URL (falls back to FERRUMCTL_SERVER_URL)
    #[arg(long)]
    server_url: Option<String>,

    /// Bearer token for authentication.
    /// Env: FERRUM_TUI_BEARER_TOKEN (falls back to FERRUMCTL_BEARER_TOKEN)
    #[arg(long)]
    bearer_token: Option<String>,

    /// Auto-refresh interval in seconds.
    #[arg(long, default_value = "5")]
    interval: u64,

    /// Dry-run mode: show synthetic OKs without making HTTP calls.
    #[arg(long)]
    dry_run: bool,

    /// Directory containing slo-window-state.json.
    /// Env: FERRUM_TUI_WINDOW_DIR
    #[arg(long)]
    window_dir: Option<String>,

    /// Directory containing evidence-snapshot-*.json.
    /// Env: FERRUM_TUI_EVIDENCE_DIR
    #[arg(long)]
    evidence_dir: Option<String>,
}

fn resolve_env(primary: &str, fallback: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .or_else(|| std::env::var(fallback).ok())
}

#[derive(Debug, Clone)]
struct LocalEvidence {
    slo: Option<SloWindowState>,
    snapshot_path: Option<std::path::PathBuf>,
    snapshot_timestamp: Option<String>,
}

enum AppEvent {
    Key(event::KeyEvent),
    Probes(Vec<ProbeResult>),
    Approvals(Result<Vec<ApprovalRequest>, String>),
    Metrics(Result<String, String>),
    AuditVerify(Result<client::AuditVerifyResult, String>),
    LocalEvidence(Result<LocalEvidence, String>),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let base_url = args
        .server_url
        .or_else(|| resolve_env("FERRUM_TUI_SERVER_URL", "FERRUMCTL_SERVER_URL"))
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

    let bearer_token = args
        .bearer_token
        .or_else(|| resolve_env("FERRUM_TUI_BEARER_TOKEN", "FERRUMCTL_BEARER_TOKEN"));

    let token_present = bearer_token.is_some();
    let client = Client::new(base_url.clone(), bearer_token)?;
    let interval_secs = args.interval;

    let window_dir = args
        .window_dir
        .or_else(|| std::env::var("FERRUM_TUI_WINDOW_DIR").ok())
        .unwrap_or_else(|| ".".to_string());

    let evidence_dir = args
        .evidence_dir
        .or_else(|| std::env::var("FERRUM_TUI_EVIDENCE_DIR").ok())
        .unwrap_or_else(|| ".".to_string());

    // Setup terminal
    if !io::stdout().is_terminal() {
        anyhow::bail!(
            "ferrum-tui requires an interactive terminal (TTY). Use --help for usage information."
        );
    }
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(
        &mut terminal,
        client,
        base_url,
        token_present,
        args.dry_run,
        interval_secs,
        window_dir,
        evidence_dir,
    )
    .await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    client: Client,
    base_url: String,
    token_present: bool,
    dry_run: bool,
    interval_secs: u64,
    window_dir: String,
    evidence_dir: String,
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<AppEvent>(32);

    // Spawn refresh task
    let refresh_tx = tx.clone();
    let refresh_client = client;
    let refresh_window_dir = window_dir.clone();
    let refresh_evidence_dir = evidence_dir.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        interval.tick().await; // first tick immediately

        loop {
            let results = if dry_run {
                vec![
                    ProbeResult {
                        name: "Health".to_string(),
                        endpoint: "/v1/healthz".to_string(),
                        status: ProbeStatus::DryRun,
                        latency_ms: Some(0),
                    },
                    ProbeResult {
                        name: "Readiness".to_string(),
                        endpoint: "/v1/readyz".to_string(),
                        status: ProbeStatus::DryRun,
                        latency_ms: Some(0),
                    },
                    ProbeResult {
                        name: "Readiness Deep".to_string(),
                        endpoint: "/v1/readyz/deep".to_string(),
                        status: ProbeStatus::DryRun,
                        latency_ms: Some(0),
                    },
                ]
            } else {
                let mut results = Vec::with_capacity(3);

                let start = Instant::now();
                let health_status = match refresh_client.health().await {
                    Ok(r) => ProbeStatus::Ok(r.status),
                    Err(e) => ProbeStatus::Err(format!("{:#}", e)),
                };
                results.push(ProbeResult {
                    name: "Health".to_string(),
                    endpoint: "/v1/healthz".to_string(),
                    status: health_status,
                    latency_ms: Some(start.elapsed().as_millis()),
                });

                let start = Instant::now();
                let ready_status = match refresh_client.readiness().await {
                    Ok(r) => ProbeStatus::Ok(r.status),
                    Err(e) => ProbeStatus::Err(format!("{:#}", e)),
                };
                results.push(ProbeResult {
                    name: "Readiness".to_string(),
                    endpoint: "/v1/readyz".to_string(),
                    status: ready_status,
                    latency_ms: Some(start.elapsed().as_millis()),
                });

                let start = Instant::now();
                let deep_status = match refresh_client.readiness_deep().await {
                    Ok(r) => ProbeStatus::Ok(r.status),
                    Err(e) => ProbeStatus::Err(format!("{:#}", e)),
                };
                results.push(ProbeResult {
                    name: "Readiness Deep".to_string(),
                    endpoint: "/v1/readyz/deep".to_string(),
                    status: deep_status,
                    latency_ms: Some(start.elapsed().as_millis()),
                });

                results
            };

            let approvals = if dry_run {
                Ok(vec![
                    ApprovalRequest {
                        approval_id: "dry-run-001".to_string(),
                        proposal_id: "dry-run-prop-001".to_string(),
                        requested_by: serde_json::json!("dry-run-user"),
                        reason: "Synthetic approval for dry-run mode".to_string(),
                        state: "pending".to_string(),
                        created_at: "2024-01-01T00:00:00Z".to_string(),
                        expires_at: "2024-01-02T00:00:00Z".to_string(),
                    },
                    ApprovalRequest {
                        approval_id: "dry-run-002".to_string(),
                        proposal_id: "dry-run-prop-002".to_string(),
                        requested_by: serde_json::json!({"user": "dry-run-admin", "role": "operator"}),
                        reason: "Another synthetic approval".to_string(),
                        state: "approved".to_string(),
                        created_at: "2024-01-01T12:00:00Z".to_string(),
                        expires_at: "2024-01-02T12:00:00Z".to_string(),
                    },
                ])
            } else {
                match refresh_client.list_approvals().await {
                    Ok(resp) => Ok(resp.items),
                    Err(e) => Err(format!("{:#}", e)),
                }
            };

            let metrics = if dry_run {
                let synthetic = r#"# HELP store_health Store health status
# TYPE store_health gauge
store_health 1
# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{method="GET",path="/v1/healthz"} 42
http_requests_total{method="GET",path="/v1/readyz"} 17
"#;
                Ok(synthetic.to_string())
            } else {
                match refresh_client.metrics().await {
                    Ok(text) => Ok(text),
                    Err(e) => Err(format!("{:#}", e)),
                }
            };

            let audit_verify = if dry_run {
                Ok(client::AuditVerifyResult {
                    valid: true,
                    total_entries: 0,
                    hashed_entries: 0,
                    error: None,
                })
            } else {
                match refresh_client.verify_audit_chain().await {
                    Ok(r) => Ok(r),
                    Err(e) => Err(format!("{:#}", e)),
                }
            };

            let local_evidence: Result<LocalEvidence, String> = (|| {
                let mut slo = None;
                let slo_path =
                    std::path::PathBuf::from(&refresh_window_dir).join("slo-window-state.json");
                if slo_path.exists() {
                    let content = std::fs::read_to_string(&slo_path)
                        .map_err(|e| format!("read slo state: {}", e))?;
                    let state: SloWindowState = serde_json::from_str(&content)
                        .map_err(|e| format!("parse slo state: {}", e))?;
                    slo = Some(state);
                }
                let mut snap = None;
                if let Some(path) = latest_evidence_snapshot_path(&refresh_evidence_dir) {
                    let content = std::fs::read_to_string(&path)
                        .map_err(|e| format!("read snapshot: {}", e))?;
                    let meta: EvidenceSnapshotMeta = serde_json::from_str(&content)
                        .map_err(|e| format!("parse snapshot: {}", e))?;
                    snap = Some((path, meta));
                }
                let (snapshot_path, snapshot_timestamp) = snap
                    .map(|(p, m)| (Some(p), Some(m.snapshot_timestamp)))
                    .unwrap_or((None, None));
                Ok(LocalEvidence {
                    slo,
                    snapshot_path,
                    snapshot_timestamp,
                })
            })();

            if refresh_tx.send(AppEvent::Probes(results)).await.is_err() {
                break;
            }
            if refresh_tx
                .send(AppEvent::Approvals(approvals))
                .await
                .is_err()
            {
                break;
            }
            if refresh_tx.send(AppEvent::Metrics(metrics)).await.is_err() {
                break;
            }
            if refresh_tx
                .send(AppEvent::AuditVerify(audit_verify))
                .await
                .is_err()
            {
                break;
            }
            if refresh_tx
                .send(AppEvent::LocalEvidence(local_evidence))
                .await
                .is_err()
            {
                break;
            }

            interval.tick().await;
        }
    });

    // Spawn key event reader (blocking I/O)
    let event_tx = tx.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(Event::Key(key)) = event::read() {
                    if event_tx.blocking_send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut app = App::new(base_url, token_present, dry_run, interval_secs);

    loop {
        terminal.draw(|f| app::draw(f, &app))?;

        if let Some(event) = rx.recv().await {
            match event {
                AppEvent::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') => {
                                app.quit = true;
                            }
                            KeyCode::Char('r') => {
                                app.message = "Refreshing…".to_string();
                            }
                            KeyCode::Char('a') => {
                                app.current_tab = Tab::Approvals;
                            }
                            KeyCode::Char('?') | KeyCode::Char('h') => {
                                app.help_visible = !app.help_visible;
                            }
                            KeyCode::Tab | KeyCode::Right => {
                                app.current_tab = app.current_tab.next();
                            }
                            KeyCode::BackTab | KeyCode::Left => {
                                app.current_tab = app.current_tab.prev();
                            }
                            KeyCode::Char('1') => app.current_tab = Tab::Overview,
                            KeyCode::Char('2') => app.current_tab = Tab::Approvals,
                            KeyCode::Char('3') => app.current_tab = Tab::Metrics,
                            KeyCode::Char('4') => app.current_tab = Tab::Help,
                            _ => {}
                        }
                    }
                }
                AppEvent::Probes(probes) => {
                    app.probes = probes;
                    app.last_refresh = Some(Local::now().format("%H:%M:%S").to_string());
                    app.compute_readiness_state();
                    if app.message == "Refreshing…" {
                        app.message.clear();
                    }
                }
                AppEvent::Approvals(result) => {
                    app.approvals = match result {
                        Ok(items) => ApprovalsView::Loaded(items),
                        Err(e) => ApprovalsView::Error(e),
                    };
                    app.compute_readiness_state();
                }
                AppEvent::Metrics(result) => {
                    app.metrics = match result {
                        Ok(text) => {
                            let pairs = parse_metrics(&text);
                            if pairs.is_empty() {
                                MetricsView::Skipped
                            } else {
                                MetricsView::Loaded(pairs)
                            }
                        }
                        Err(e) => MetricsView::Error(e),
                    };
                    app.compute_readiness_state();
                }
                AppEvent::AuditVerify(result) => {
                    app.audit_verify = match result {
                        Ok(r) => AuditVerifyView::Verified(r),
                        Err(e) => AuditVerifyView::Error(e),
                    };
                    app.compute_readiness_state();
                }
                AppEvent::LocalEvidence(result) => {
                    match result {
                        Ok(ev) => {
                            app.slo_window = ev
                                .slo
                                .map(|s| {
                                    SloWindowView::Loaded(app::SloWindowState {
                                        window_id: s.window_id,
                                        status: s.status,
                                        elapsed_seconds: s.elapsed_duration_seconds,
                                        target_days: s.target_duration_days,
                                        minimum_days: s.minimum_duration_days,
                                        notes: s.notes,
                                    })
                                })
                                .unwrap_or(SloWindowView::Missing);
                            app.latest_snapshot_path = ev.snapshot_path;
                            app.latest_snapshot_timestamp = ev.snapshot_timestamp;
                            app.local_evidence_error = None;
                        }
                        Err(e) => {
                            app.local_evidence_error = Some(e);
                        }
                    }
                    app.compute_readiness_state();
                }
            }
        }

        if app.quit {
            break;
        }
    }

    Ok(())
}

fn parse_metrics(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let metric_part = parts[0];
        let value = parts[1];
        if value.parse::<f64>().is_err() {
            continue;
        }
        let name_end = metric_part.find('{').unwrap_or(metric_part.len());
        let name = &metric_part[..name_end];
        if name.is_empty() {
            continue;
        }
        // Keep a curated set of interesting metric names
        let is_interesting = name.contains("health")
            || name.contains("total")
            || name.contains("count")
            || name.contains("pool")
            || name.contains("requests")
            || name.contains("duration")
            || name.contains("latency")
            || name.contains("active")
            || name.contains("idle")
            || name.contains("connections")
            || name.contains("errors");
        if is_interesting {
            pairs.push((name.to_string(), value.to_string()));
        }
    }
    // Cap to avoid overwhelming the UI
    pairs.truncate(30);
    pairs
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
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

#[derive(Debug, Clone, serde::Deserialize)]
struct EvidenceSnapshotMeta {
    snapshot_timestamp: String,
}

fn latest_evidence_snapshot_path(dir: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("evidence-snapshot-") && name_str.ends_with(".json") {
            candidates.push(entry.path());
        }
    }
    candidates.sort();
    candidates.into_iter().last()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_slo_window_state_deserialization() {
        let json = r#"{
            "window_id": "slo-window-20260101T000000Z",
            "status": "started",
            "window_started_at": "2026-01-01T00:00:00Z",
            "window_ended_at": null,
            "elapsed_duration_seconds": 3600,
            "target_duration_days": 30,
            "minimum_duration_days": 7,
            "notes": null,
            "non_claims_notice": "production-ready = NO",
            "created_by_tool": "ferrumctl evidence slo-window start",
            "finalized_by_tool": null
        }"#;
        let state: SloWindowState = serde_json::from_str(json).unwrap();
        assert_eq!(state.window_id, "slo-window-20260101T000000Z");
        assert_eq!(state.status, "started");
        assert_eq!(state.elapsed_duration_seconds, 3600);
    }

    #[test]
    fn test_evidence_snapshot_latest_selection() {
        let dir = TempDir::new().unwrap();
        let p1 = dir
            .path()
            .join("evidence-snapshot-2026-05-28T12-00-00Z.json");
        let p2 = dir
            .path()
            .join("evidence-snapshot-2026-05-29T12-00-00Z.json");
        std::fs::write(&p1, r#"{"snapshot_timestamp":"2026-05-28T12:00:00Z"}"#).unwrap();
        std::fs::write(&p2, r#"{"snapshot_timestamp":"2026-05-29T12:00:00Z"}"#).unwrap();
        let latest = latest_evidence_snapshot_path(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(
            latest.file_name().unwrap(),
            "evidence-snapshot-2026-05-29T12-00-00Z.json"
        );
    }

    #[test]
    fn test_evidence_snapshot_metadata_parsing() {
        let json = r#"{"snapshot_timestamp":"2026-05-29T12:00:00Z"}"#;
        let meta: EvidenceSnapshotMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.snapshot_timestamp, "2026-05-29T12:00:00Z");
    }
}
