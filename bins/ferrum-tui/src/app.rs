use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};

use crate::client::ApprovalRequest;

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

    fn color(&self) -> Color {
        match self {
            ProbeStatus::Loading => Color::Yellow,
            ProbeStatus::Ok(_) => Color::Green,
            ProbeStatus::Err(_) => Color::Red,
            ProbeStatus::DryRun => Color::Cyan,
        }
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

pub struct App {
    pub base_url: String,
    pub token_redacted: String,
    pub probes: Vec<ProbeResult>,
    pub last_refresh: Option<String>,
    pub dry_run: bool,
    pub help_visible: bool,
    pub refresh_interval_secs: u64,
    pub quit: bool,
    pub message: String,
    pub approvals_visible: bool,
    pub approvals: ApprovalsView,
}

impl App {
    pub fn new(base_url: String, token_present: bool, dry_run: bool, interval_secs: u64) -> Self {
        let token_redacted = if token_present {
            "***present***".to_string()
        } else {
            "***not set***".to_string()
        };

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
            token_redacted,
            probes,
            last_refresh: None,
            dry_run,
            help_visible: false,
            refresh_interval_secs: interval_secs,
            quit: false,
            message: String::new(),
            approvals_visible: false,
            approvals: ApprovalsView::Loading,
        }
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(f.area());

    draw_header(f, app, main_layout[0]);
    if app.approvals_visible {
        draw_approvals_table(f, app, main_layout[1]);
    } else {
        draw_probe_table(f, app, main_layout[1]);
    }
    draw_footer(f, app, main_layout[2]);

    if app.help_visible {
        draw_help(f, app);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let header_block = Block::default()
        .title(" FerrumGate Operator TUI ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let dry_run_span = if app.dry_run {
        Span::styled(
            " [DRY-RUN] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let text = Text::from(vec![
        Line::from(vec![
            Span::styled("Base URL: ", Style::default().fg(Color::Gray)),
            Span::styled(&app.base_url, Style::default().fg(Color::White)),
            dry_run_span,
        ]),
        Line::from(vec![
            Span::styled("Token:    ", Style::default().fg(Color::Gray)),
            Span::styled(&app.token_redacted, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("Interval: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}s", app.refresh_interval_secs),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("Message:  ", Style::default().fg(Color::Gray)),
            Span::styled(&app.message, Style::default().fg(Color::Yellow)),
        ]),
    ]);

    let paragraph = Paragraph::new(text)
        .block(header_block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_probe_table(f: &mut Frame, app: &App, area: Rect) {
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
            let status_style = Style::default().fg(p.status.color());
            let latency_text = p
                .latency_ms
                .map(|ms| format!("{} ms", ms))
                .unwrap_or_else(|| "-".to_string());

            Row::new(vec![
                Cell::from(p.name.clone()),
                Cell::from(Span::styled(status_text, status_style)),
                Cell::from(latency_text),
                Cell::from(p.endpoint.clone()).style(Style::default().fg(Color::DarkGray)),
            ])
            .height(1)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    f.render_widget(table, area);
}

fn draw_approvals_table(f: &mut Frame, app: &App, area: Rect) {
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
            let text = Paragraph::new("Loading approvals...")
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
                        "pending" => Style::default().fg(Color::Yellow),
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

            let table = Table::new(
                rows,
                [
                    Constraint::Length(18),
                    Constraint::Length(18),
                    Constraint::Length(10),
                    Constraint::Min(8),
                    Constraint::Length(14),
                    Constraint::Length(18),
                    Constraint::Length(18),
                ],
            )
            .header(header)
            .block(block)
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_widget(table, area);
        }
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let refresh_info = app
        .last_refresh
        .as_ref()
        .map(|t| format!("Last refresh: {}", t))
        .unwrap_or_else(|| "Last refresh: —".to_string());

    let help_hint = "Press ? for help  |  q to quit  |  r to refresh  |  a approvals";

    let text = Text::from(vec![Line::from(vec![
        Span::styled(help_hint, Style::default().fg(Color::Gray)),
        Span::raw("  •  "),
        Span::styled(refresh_info, Style::default().fg(Color::DarkGray)),
    ])]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut Frame, _app: &App) {
    let area = centered_rect(60, 50, f.area());

    let block = Block::default()
        .title(" Keyboard Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let text = Text::from(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("     Refresh probes now"),
        ]),
        Line::from(vec![
            Span::styled("a", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("     Toggle approvals panel"),
        ]),
        Line::from(vec![
            Span::styled("? / h", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("     Quit TUI"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Env:",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  FERRUM_TUI_SERVER_URL    Base URL fallback"),
        Line::from("  FERRUM_TUI_BEARER_TOKEN  Token fallback"),
        Line::from("  FERRUMCTL_SERVER_URL     Alternate base URL fallback"),
        Line::from("  FERRUMCTL_BEARER_TOKEN   Alternate token fallback"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Non-claims:",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  Operator convenience only; not production-ready."),
        Line::from("  No mutation operations in this MVP."),
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
