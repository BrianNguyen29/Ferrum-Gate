use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs, Wrap},
};

use crate::client::{ApprovalRequest, AuditVerifyResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview = 0,
    Approvals = 1,
    Metrics = 2,
    Help = 3,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::Approvals,
            Tab::Approvals => Tab::Metrics,
            Tab::Metrics => Tab::Help,
            Tab::Help => Tab::Overview,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Overview => Tab::Help,
            Tab::Approvals => Tab::Overview,
            Tab::Metrics => Tab::Approvals,
            Tab::Help => Tab::Metrics,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProbeStatus {
    Loading,
    Ok(String),
    Err(String),
    DryRun,
}

impl ProbeStatus {
    fn label(&self) -> &str {
        match self {
            ProbeStatus::Loading => "LOADING",
            ProbeStatus::Ok(s) => s.as_str(),
            ProbeStatus::Err(s) => s.as_str(),
            ProbeStatus::DryRun => "DRY-RUN",
        }
    }

    fn badge_style(&self) -> Style {
        let (fg, bg) = match self {
            ProbeStatus::Loading => (Color::Black, Color::Yellow),
            ProbeStatus::Ok(_) => (Color::Black, Color::Green),
            ProbeStatus::Err(_) => (Color::Black, Color::Red),
            ProbeStatus::DryRun => (Color::Black, Color::Cyan),
        };
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
    }
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub name: String,
    pub endpoint: String,
    pub status: ProbeStatus,
    pub latency_ms: Option<u128>,
}

#[derive(Debug, Clone)]
pub enum ApprovalsView {
    Loading,
    Loaded(Vec<ApprovalRequest>),
    Error(String),
}

#[derive(Debug, Clone)]
pub enum MetricsView {
    Loading,
    Loaded(Vec<(String, String)>),
    Error(String),
    Skipped,
}

#[derive(Debug, Clone)]
pub enum SloWindowView {
    Missing,
    Loaded(SloWindowState),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SloWindowState {
    pub window_id: String,
    pub status: String,
    pub elapsed_seconds: u64,
    pub target_days: u32,
    pub minimum_days: u32,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AuditVerifyView {
    Loading,
    Verified(AuditVerifyResult),
    Error(String),
}

pub struct App {
    pub base_url: String,
    pub token_present: bool,
    pub probes: Vec<ProbeResult>,
    pub last_refresh: Option<String>,
    pub dry_run: bool,
    pub help_visible: bool,
    pub refresh_interval_secs: u64,
    pub quit: bool,
    pub message: String,
    pub approvals: ApprovalsView,
    pub current_tab: Tab,
    pub metrics: MetricsView,
    pub error_count: usize,
    pub slo_window: SloWindowView,
    pub audit_verify: AuditVerifyView,
    pub latest_snapshot_path: Option<std::path::PathBuf>,
    pub latest_snapshot_timestamp: Option<String>,
    pub local_evidence_error: Option<String>,
    pub blockers: Vec<String>,
}

impl App {
    pub fn new(base_url: String, token_present: bool, dry_run: bool, interval_secs: u64) -> Self {
        let probes = vec![
            ProbeResult {
                name: "Health".to_string(),
                endpoint: "/v1/healthz".to_string(),
                status: ProbeStatus::Loading,
                latency_ms: None,
            },
            ProbeResult {
                name: "Readiness".to_string(),
                endpoint: "/v1/readyz".to_string(),
                status: ProbeStatus::Loading,
                latency_ms: None,
            },
            ProbeResult {
                name: "Readiness Deep".to_string(),
                endpoint: "/v1/readyz/deep".to_string(),
                status: ProbeStatus::Loading,
                latency_ms: None,
            },
        ];

        Self {
            base_url,
            token_present,
            probes,
            last_refresh: None,
            dry_run,
            help_visible: false,
            refresh_interval_secs: interval_secs,
            quit: false,
            message: String::new(),
            approvals: ApprovalsView::Loading,
            current_tab: Tab::Overview,
            metrics: MetricsView::Loading,
            error_count: 0,
            slo_window: SloWindowView::Missing,
            audit_verify: AuditVerifyView::Loading,
            latest_snapshot_path: None,
            latest_snapshot_timestamp: None,
            local_evidence_error: None,
            blockers: Vec::new(),
        }
    }

    pub fn healthy_count(&self) -> usize {
        self.probes
            .iter()
            .filter(|p| matches!(p.status, ProbeStatus::Ok(_) | ProbeStatus::DryRun))
            .count()
    }

    pub fn pending_count(&self) -> usize {
        match &self.approvals {
            ApprovalsView::Loaded(items) => items
                .iter()
                .filter(|a| a.state.to_lowercase() == "pending")
                .count(),
            _ => 0,
        }
    }

    pub fn total_approvals(&self) -> usize {
        match &self.approvals {
            ApprovalsView::Loaded(items) => items.len(),
            _ => 0,
        }
    }

    pub fn compute_readiness_state(&mut self) {
        let mut errors = 0;
        let mut blockers = Vec::new();

        for p in &self.probes {
            if let ProbeStatus::Err(ref e) = p.status {
                errors += 1;
                blockers.push(format!("Probe {}: {}", p.name, e));
            }
        }
        if let ApprovalsView::Error(ref e) = self.approvals {
            errors += 1;
            blockers.push(format!("Approvals: {}", e));
        }
        if let MetricsView::Error(ref e) = self.metrics {
            errors += 1;
            blockers.push(format!("Metrics: {}", e));
        }
        if let AuditVerifyView::Error(ref e) = self.audit_verify {
            errors += 1;
            let msg = if e.contains("401") || e.to_lowercase().contains("unauthorized") {
                "Audit verify: unauthorized".to_string()
            } else {
                format!("Audit verify: {}", e)
            };
            blockers.push(msg);
        }
        if let Some(ref e) = self.local_evidence_error {
            errors += 1;
            blockers.push(format!("Local evidence: {}", e));
        }

        self.error_count = errors;
        self.blockers = blockers;
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(f.area());

    draw_title_bar(f, app, main_layout[0]);
    draw_summary_cards(f, app, main_layout[1]);
    draw_tab_bar(f, app, main_layout[2]);
    draw_content(f, app, main_layout[3]);
    draw_footer(f, app, main_layout[4]);

    if app.help_visible {
        draw_help_overlay(f, app);
    }
}

fn draw_title_bar(f: &mut Frame, app: &App, area: Rect) {
    let mode_span = if app.dry_run {
        Span::styled(
            " DRY-RUN ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " LIVE ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    };

    let auth_span = if app.token_present {
        Span::styled(
            " AUTH ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " NO-AUTH ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };

    let line = Line::from(vec![
        Span::styled(
            " FerrumGate ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Operator Console", Style::default().fg(Color::White)),
        Span::raw("  "),
        mode_span,
        Span::raw("  "),
        Span::styled(&app.base_url, Style::default().fg(Color::Gray)),
        Span::raw("  "),
        auth_span,
        Span::raw("  "),
        Span::styled(
            format!("{}s", app.refresh_interval_secs),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let paragraph = Paragraph::new(line).alignment(Alignment::Left);
    f.render_widget(paragraph, area);
}

fn draw_summary_cards(f: &mut Frame, app: &App, area: Rect) {
    let healthy = app.healthy_count();
    let total = app.probes.len();
    let pending = app.pending_count();
    let total_appr = app.total_approvals();
    let errors = app.error_count;

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    // Healthy card
    let healthy_color = if healthy == total {
        Color::Green
    } else {
        Color::Yellow
    };
    let healthy_block = Block::default()
        .title(" Healthy ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(healthy_color));
    let healthy_text = Paragraph::new(Span::styled(
        format!("{}/{}", healthy, total),
        Style::default()
            .fg(healthy_color)
            .add_modifier(Modifier::BOLD),
    ))
    .block(healthy_block)
    .alignment(Alignment::Center);
    f.render_widget(healthy_text, chunks[0]);

    // Pending approvals card
    let pending_color = if pending > 0 {
        Color::Yellow
    } else {
        Color::Green
    };
    let pending_block = Block::default()
        .title(" Pending ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(pending_color));
    let pending_text = Paragraph::new(Span::styled(
        format!("{}/{}", pending, total_appr.max(1)),
        Style::default()
            .fg(pending_color)
            .add_modifier(Modifier::BOLD),
    ))
    .block(pending_block)
    .alignment(Alignment::Center);
    f.render_widget(pending_text, chunks[1]);

    // Errors card
    let error_color = if errors > 0 { Color::Red } else { Color::Gray };
    let error_block = Block::default()
        .title(" Errors ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(error_color));
    let error_text = Paragraph::new(Span::styled(
        format!("{}", errors),
        Style::default()
            .fg(error_color)
            .add_modifier(Modifier::BOLD),
    ))
    .block(error_block)
    .alignment(Alignment::Center);
    f.render_widget(error_text, chunks[2]);

    // Readiness card
    let readiness_label = if app.dry_run {
        "DRY-RUN"
    } else if app.error_count > 0 {
        "BLOCKED"
    } else {
        "RC-READY"
    };
    let readiness_color = if app.dry_run {
        Color::Cyan
    } else if app.error_count > 0 {
        Color::Red
    } else {
        Color::Yellow
    };
    let readiness_block = Block::default()
        .title(" Readiness ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(readiness_color));
    let readiness_text = Paragraph::new(Span::styled(
        readiness_label,
        Style::default()
            .fg(readiness_color)
            .add_modifier(Modifier::BOLD),
    ))
    .block(readiness_block)
    .alignment(Alignment::Center);
    f.render_widget(readiness_text, chunks[3]);
}

fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = vec![
        Line::from(" Overview "),
        Line::from(" Approvals "),
        Line::from(" Metrics "),
        Line::from(" Help "),
    ];

    let tabs = Tabs::new(titles)
        .select(app.current_tab as usize)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED),
        )
        .divider(Span::raw(" │ "));

    f.render_widget(tabs, area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Overview => draw_overview(f, app, area),
        Tab::Approvals => draw_approvals(f, app, area),
        Tab::Metrics => draw_metrics(f, app, area),
        Tab::Help => draw_help_page(f, app, area),
    }
}

fn draw_overview(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_readiness_summary(f, app, chunks[0]);
    draw_endpoint_status(f, app, chunks[1]);
}

fn draw_readiness_summary(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Readiness Summary ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let slo_line = match &app.slo_window {
        SloWindowView::Missing => Line::from("SLO window: No active window"),
        SloWindowView::Loaded(s) => {
            let elapsed_days = s.elapsed_seconds / 86400;
            Line::from(format!(
                "SLO window: {} | {} | {} days elapsed (target {} days)",
                s.window_id, s.status, elapsed_days, s.target_days
            ))
        }
    };

    let audit_line = match &app.audit_verify {
        AuditVerifyView::Loading => Line::from("Last audit verify: Loading…"),
        AuditVerifyView::Verified(r) => {
            let label = if r.valid { "valid" } else { "invalid" };
            let color = if r.valid { Color::Green } else { Color::Red };
            Line::from(vec![
                Span::raw("Last audit verify: "),
                Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    " ({} hashed / {} total)",
                    r.hashed_entries, r.total_entries
                )),
            ])
        }
        AuditVerifyView::Error(e) => {
            let (label, color) = if e.contains("401") || e.to_lowercase().contains("unauthorized") {
                ("unauthorized", Color::Yellow)
            } else {
                ("error", Color::Red)
            };
            Line::from(vec![
                Span::raw("Last audit verify: "),
                Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" ({})", e)),
            ])
        }
    };

    let snapshot_line = match (&app.latest_snapshot_path, &app.latest_snapshot_timestamp) {
        (Some(path), Some(ts)) => Line::from(format!(
            "Latest snapshot: {} ({})",
            ts,
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        )),
        _ => Line::from("Latest snapshot: none found"),
    };

    let mut lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(
            "Non-claims:",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from("  production-ready = NO"),
        Line::from("  Tier 2 = NOT COMPLETE"),
        Line::from("  sustained SLO = NOT COMPLETE"),
        Line::from("  HA-4 = NOT COMPLETE"),
        Line::from(""),
        slo_line,
        audit_line,
        snapshot_line,
        Line::from(""),
        Line::from(vec![Span::styled(
            "Readiness blockers:",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
    ];

    if app.blockers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No operational blockers detected.",
            Style::default().fg(Color::Green),
        )));
    } else {
        for b in &app.blockers {
            lines.push(Line::from(Span::styled(
                format!("  • {}", b),
                Style::default().fg(Color::Red),
            )));
        }
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_endpoint_status(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Endpoint Status ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let header = Row::new(vec!["Endpoint", "Status", "Latency", "Path"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = app
        .probes
        .iter()
        .map(|p| {
            let status_text = p.status.label();
            let status_style = p.status.badge_style();
            let latency_text = p
                .latency_ms
                .map(|ms| format!("{} ms", ms))
                .unwrap_or_else(|| "—".to_string());

            Row::new(vec![
                Cell::from(p.name.clone()),
                Cell::from(Span::styled(format!(" {} ", status_text), status_style)),
                Cell::from(latency_text),
                Cell::from(p.endpoint.clone()).style(Style::default().fg(Color::DarkGray)),
            ])
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(18),
        Constraint::Length(16),
        Constraint::Length(12),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(block)
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_widget(table, area);
}

fn draw_approvals(f: &mut Frame, app: &App, area: Rect) {
    let title = if app.dry_run {
        " Pending Approvals [DRY-RUN] "
    } else {
        " Pending Approvals "
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    match &app.approvals {
        ApprovalsView::Loading => {
            let text = Paragraph::new(Span::styled(
                "Loading approvals…",
                Style::default().fg(Color::Yellow),
            ))
            .block(block)
            .alignment(Alignment::Center);
            f.render_widget(text, area);
        }
        ApprovalsView::Error(err) => {
            let text = Paragraph::new(Span::styled(
                format!("Error loading approvals: {}", err),
                Style::default().fg(Color::Red),
            ))
            .block(block)
            .wrap(Wrap { trim: true });
            f.render_widget(text, area);
        }
        ApprovalsView::Loaded(items) => {
            if items.is_empty() {
                let text = Paragraph::new(Span::styled(
                    "No approvals found.",
                    Style::default().fg(Color::Green),
                ))
                .block(block)
                .alignment(Alignment::Center);
                f.render_widget(text, area);
                return;
            }

            let header = Row::new(vec![
                "Approval ID",
                "Proposal ID",
                "State",
                "Reason",
                "By",
                "Created",
                "Expires",
            ])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .height(1);

            let rows: Vec<Row> = items
                .iter()
                .map(|a| {
                    let by_text = match &a.requested_by {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let by_truncated: String = by_text.chars().take(12).collect();

                    let state_style = match a.state.to_lowercase().as_str() {
                        "pending" => Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                        "approved" => Style::default().fg(Color::Green),
                        "rejected" | "denied" => Style::default().fg(Color::Red),
                        _ => Style::default().fg(Color::Gray),
                    };

                    Row::new(vec![
                        Cell::from(a.approval_id.chars().take(16).collect::<String>()),
                        Cell::from(a.proposal_id.chars().take(16).collect::<String>()),
                        Cell::from(Span::styled(&a.state, state_style)),
                        Cell::from(a.reason.chars().take(20).collect::<String>()),
                        Cell::from(by_truncated),
                        Cell::from(a.created_at.chars().take(16).collect::<String>()),
                        Cell::from(a.expires_at.chars().take(16).collect::<String>()),
                    ])
                    .height(1)
                })
                .collect();

            let widths = [
                Constraint::Length(18),
                Constraint::Length(18),
                Constraint::Length(10),
                Constraint::Min(8),
                Constraint::Length(14),
                Constraint::Length(18),
                Constraint::Length(18),
            ];

            let table = Table::new(rows, widths)
                .header(header)
                .block(block)
                .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_widget(table, area);
        }
    }
}

fn draw_metrics(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Metrics Summary ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    match &app.metrics {
        MetricsView::Loading => {
            let text = Paragraph::new(Span::styled(
                "Loading metrics…",
                Style::default().fg(Color::Yellow),
            ))
            .block(block)
            .alignment(Alignment::Center);
            f.render_widget(text, area);
        }
        MetricsView::Skipped => {
            let text = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Metrics endpoint unavailable or no matching metrics found.",
                    Style::default().fg(Color::Gray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "The /v1/metrics endpoint returned data but no recognised numeric metrics.",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
            f.render_widget(text, area);
        }
        MetricsView::Error(err) => {
            let text = Paragraph::new(Span::styled(
                format!("Error loading metrics: {}", err),
                Style::default().fg(Color::Red),
            ))
            .block(block)
            .wrap(Wrap { trim: true });
            f.render_widget(text, area);
        }
        MetricsView::Loaded(pairs) => {
            if pairs.is_empty() {
                let text = Paragraph::new(Span::styled(
                    "No numeric metrics found.",
                    Style::default().fg(Color::Gray),
                ))
                .block(block)
                .alignment(Alignment::Center);
                f.render_widget(text, area);
                return;
            }

            let header = Row::new(vec!["Metric", "Value"])
                .style(Style::default().add_modifier(Modifier::BOLD))
                .height(1);

            let rows: Vec<Row> = pairs
                .iter()
                .map(|(k, v)| {
                    Row::new(vec![
                        Cell::from(k.clone()).style(Style::default().fg(Color::White)),
                        Cell::from(v.clone()).style(Style::default().fg(Color::Cyan)),
                    ])
                    .height(1)
                })
                .collect();

            let table = Table::new(
                rows,
                [Constraint::Percentage(60), Constraint::Percentage(40)],
            )
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_widget(table, area);
        }
    }
}

fn draw_help_page(f: &mut Frame, _app: &App, area: Rect) {
    let block = Block::default()
        .title(" Keyboard Shortcuts & Information ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from(vec![
            Span::styled("Tab / →", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Next tab"),
        ]),
        Line::from(vec![
            Span::styled(
                "Shift+Tab / ←",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Previous tab"),
        ]),
        Line::from(vec![
            Span::styled("1–4", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("        Jump to tab (Overview / Approvals / Metrics / Help)"),
        ]),
        Line::from(vec![
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Jump to Approvals tab"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Actions",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from(vec![
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Refresh data now"),
        ]),
        Line::from(vec![
            Span::styled("? / h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Toggle help overlay"),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Quit TUI"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Environment",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from("  FERRUM_TUI_SERVER_URL    Base URL fallback"),
        Line::from("  FERRUM_TUI_BEARER_TOKEN  Token fallback"),
        Line::from("  FERRUMCTL_SERVER_URL     Alternate base URL fallback"),
        Line::from("  FERRUMCTL_BEARER_TOKEN   Alternate token fallback"),
        Line::from("  FERRUM_TUI_WINDOW_DIR    Directory for slo-window-state.json"),
        Line::from("  FERRUM_TUI_EVIDENCE_DIR  Directory for evidence-snapshot-*.json"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Non-claims",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from("  production-ready = NO"),
        Line::from("  Tier 2 = NOT COMPLETE"),
        Line::from("  sustained SLO = NOT COMPLETE"),
        Line::from("  HA-4 = NOT COMPLETE"),
        Line::from("  Operator convenience only; no mutation operations in this MVP."),
        Line::from("  Token values are redacted in the UI."),
    ]);

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let hint = match app.current_tab {
        Tab::Overview => "Tab →  |  r refresh  |  ? help  |  q quit",
        Tab::Approvals => "Tab →  |  r refresh  |  ? help  |  q quit",
        Tab::Metrics => "Tab →  |  r refresh  |  ? help  |  q quit",
        Tab::Help => "Tab →  |  q quit",
    };

    let msg_span = if app.message.is_empty() {
        Span::raw("")
    } else {
        Span::styled(
            format!("  {}  ", app.message),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    };

    let text = Text::from(vec![Line::from(vec![
        Span::styled(hint, Style::default().fg(Color::Gray)),
        msg_span,
    ])]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

fn draw_help_overlay(f: &mut Frame, _app: &App) {
    let area = centered_rect(60, 55, f.area());

    let block = Block::default()
        .title(" Quick Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Tab / →", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Next tab"),
        ]),
        Line::from(vec![
            Span::styled(
                "Shift+Tab / ←",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Previous tab"),
        ]),
        Line::from(vec![
            Span::styled("1–4", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("        Jump to tab"),
        ]),
        Line::from(vec![
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Refresh data"),
        ]),
        Line::from(vec![
            Span::styled("? / h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Toggle this overlay"),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Quit"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Non-claims:",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )]),
        Line::from("  production-ready = NO"),
        Line::from("  Tier 2 = NOT COMPLETE"),
        Line::from("  sustained SLO = NOT COMPLETE"),
        Line::from("  HA-4 = NOT COMPLETE"),
        Line::from("  Operator convenience only; not production-ready."),
    ]);

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn test_readiness_blocked_on_probe_error() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.probes = vec![ProbeResult {
            name: "Health".to_string(),
            endpoint: "/v1/healthz".to_string(),
            status: ProbeStatus::Err("fail".to_string()),
            latency_ms: None,
        }];
        app.compute_readiness_state();
        assert!(app.error_count > 0);
        assert!(app.blockers.iter().any(|b| b.contains("Health")));
    }

    #[test]
    fn test_readiness_rc_ready_when_healthy() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.probes = vec![ProbeResult {
            name: "Health".to_string(),
            endpoint: "/v1/healthz".to_string(),
            status: ProbeStatus::Ok("ok".to_string()),
            latency_ms: None,
        }];
        app.compute_readiness_state();
        assert_eq!(app.error_count, 0);
        assert!(app.blockers.is_empty());
    }

    #[test]
    fn test_dry_run_no_blockers() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, true, 5);
        app.compute_readiness_state();
        assert_eq!(app.error_count, 0);
    }

    #[test]
    fn test_blockers_include_audit_unauthorized() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.audit_verify = AuditVerifyView::Error("401 Unauthorized".to_string());
        app.compute_readiness_state();
        assert_eq!(app.error_count, 1);
        assert!(app.blockers.iter().any(|b| b.contains("unauthorized")));
    }

    #[test]
    fn test_help_text_contains_non_claims() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        terminal
            .draw(|f| draw_help_page(f, &app, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("production-ready = NO"));
        assert!(text.contains("Tier 2 = NOT COMPLETE"));
        assert!(text.contains("sustained SLO = NOT COMPLETE"));
        assert!(text.contains("HA-4 = NOT COMPLETE"));
    }

    #[test]
    fn test_readiness_summary_contains_non_claims() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        terminal
            .draw(|f| draw_readiness_summary(f, &app, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("production-ready = NO"));
        assert!(text.contains("Tier 2 = NOT COMPLETE"));
        assert!(text.contains("sustained SLO = NOT COMPLETE"));
        assert!(text.contains("HA-4 = NOT COMPLETE"));
    }
}
