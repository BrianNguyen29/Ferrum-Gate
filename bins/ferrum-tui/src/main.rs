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

use app::{App, ApprovalsView, ProbeResult, ProbeStatus};
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
}

fn resolve_env(primary: &str, fallback: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .or_else(|| std::env::var(fallback).ok())
}

enum AppEvent {
    Key(event::KeyEvent),
    Probes(Vec<ProbeResult>),
    Approvals(Result<Vec<ApprovalRequest>, String>),
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
) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<AppEvent>(32);

    // Spawn refresh task
    let refresh_tx = tx.clone();
    let refresh_client = client;
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
                                app.message = "Refreshing...".to_string();
                            }
                            KeyCode::Char('a') => {
                                app.approvals_visible = !app.approvals_visible;
                            }
                            KeyCode::Char('?') | KeyCode::Char('h') => {
                                app.help_visible = !app.help_visible;
                            }
                            _ => {}
                        }
                    }
                }
                AppEvent::Probes(probes) => {
                    app.probes = probes;
                    app.last_refresh = Some(Local::now().format("%H:%M:%S").to_string());
                    if app.message == "Refreshing..." {
                        app.message.clear();
                    }
                }
                AppEvent::Approvals(result) => {
                    app.approvals = match result {
                        Ok(items) => ApprovalsView::Loaded(items),
                        Err(e) => ApprovalsView::Error(e),
                    };
                }
            }
        }

        if app.quit {
            break;
        }
    }

    Ok(())
}
