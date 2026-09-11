use std::str::FromStr;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Sparkline, Table, TableState, Tabs,
        Wrap,
    },
};

use crate::client::{ApprovalRequest, AuditVerifyResult};

/// Non-claims lines shared verbatim by the readiness summary, help page, and
/// quick-help overlay so the wording cannot drift between views.
const NON_CLAIMS: [&str; 4] = [
    "  production-ready = NO",
    "  Tier 2 = NOT COMPLETE",
    "  sustained SLO = NOT COMPLETE",
    "  HA-4 = NOT COMPLETE",
];

fn non_claims_lines() -> impl Iterator<Item = Line<'static>> {
    NON_CLAIMS.into_iter().map(Line::from)
}

/// Latency sparkline history length in refresh samples (in-memory only; no
/// history across restarts).
const LATENCY_HISTORY_CAP: usize = 30;

/// Display cap for the write-queue-depth gauge. Mirrors the server's default
/// `write_queue_threshold` (ferrum-gateway `state.rs` default: 100), so a
/// full bar means the depth reached that default degraded threshold; the raw
/// depth stays in the label.
const QUEUE_DEPTH_GAUGE_CAP: f64 = 100.0;

/// Selectable TUI theme. `Ansi` is the default and keeps the terminal's
/// 16-color palette; `Rgb` opts into the truecolor palette shared with the
/// site and SVG assets (`assets/README.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Ansi,
    Rgb,
}

impl FromStr for ThemeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ansi" => Ok(ThemeMode::Ansi),
            "rgb" => Ok(ThemeMode::Rgb),
            other => Err(format!("expected `ansi` or `rgb`, got `{other}`")),
        }
    }
}

/// Semantic color slots for the active theme. `Theme::ansi()` reproduces the
/// named 16-color values the TUI always used; `Theme::rgb()` substitutes the
/// truecolor tokens. Layout and text are identical in both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Primary accent: brand, section headings, highlighted values.
    pub accent: Color,
    /// Success: healthy probes, approved state, audit VALID.
    pub ok: Color,
    /// Warning: loading, pending, transient messages.
    pub warn: Color,
    /// Error: probe failures, rejected approvals, blockers.
    pub err: Color,
    /// Secondary text: hints, base URL, unavailable values.
    pub muted: Color,
    /// Tertiary text: timestamps, paths, dismiss hints.
    pub dim: Color,
    /// Primary text: body copy and metric names.
    pub text: Color,
    /// Block borders.
    pub border: Color,
    /// Ink on colored badges; also the modal backdrop.
    pub badge_fg: Color,
}

impl Theme {
    /// Terminal 16-color palette (default; safe on non-truecolor terminals).
    pub const fn ansi() -> Self {
        Self {
            accent: Color::Cyan,
            ok: Color::Green,
            warn: Color::Yellow,
            err: Color::Red,
            muted: Color::Gray,
            dim: Color::DarkGray,
            text: Color::White,
            border: Color::Blue,
            badge_fg: Color::Black,
        }
    }

    /// Truecolor palette. Canonical tokens from `assets/README.md`: iron blue
    /// `#7aa2f7` borders, violet `#bb9af7` accents, iron-oxide rust `#d97757`
    /// errors/recovery, mute `#d8dce6` text, muted `#8b92a8` secondary text,
    /// and the site `#0f1115` background as badge ink. Green and yellow have
    /// no canonical token, so the status slots use their terminal-safe hues.
    pub const fn rgb() -> Self {
        Self {
            accent: Color::Rgb(0xbb, 0x9a, 0xf7),
            ok: Color::Rgb(0x9e, 0xce, 0x6a),
            warn: Color::Rgb(0xe0, 0xaf, 0x68),
            err: Color::Rgb(0xd9, 0x77, 0x57),
            muted: Color::Rgb(0x8b, 0x92, 0xa8),
            dim: Color::Rgb(0x56, 0x5f, 0x89),
            text: Color::Rgb(0xd8, 0xdc, 0xe6),
            border: Color::Rgb(0x7a, 0xa2, 0xf7),
            badge_fg: Color::Rgb(0x0f, 0x11, 0x15),
        }
    }

    /// Resolve the palette for the selected mode.
    pub const fn for_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Ansi => Self::ansi(),
            ThemeMode::Rgb => Self::rgb(),
        }
    }
}

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

/// Input mode. `Normal` is the default routing; `Detail` shows the approval
/// detail modal (Esc/q closes); `Filter` types into the metrics filter
/// (Enter accepts, Esc clears).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Detail,
    Filter,
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

    fn badge_style(&self, theme: &Theme) -> Style {
        let (fg, bg) = match self {
            ProbeStatus::Loading => (theme.badge_fg, theme.warn),
            ProbeStatus::Ok(_) => (theme.badge_fg, theme.ok),
            ProbeStatus::Err(_) => (theme.badge_fg, theme.err),
            ProbeStatus::DryRun => (theme.badge_fg, theme.accent),
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

/// PostgreSQL pool snapshot extracted from the latest `/v1/metrics` scrape
/// (`ferrumgate_store_pg_pool_size` / `_idle` / `_max`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoolSnapshot {
    pub size: f64,
    pub idle: f64,
    pub max: f64,
}

#[derive(Debug, Clone)]
pub enum MetricsView {
    Loading,
    /// Display rows plus the total number of matching metrics before the
    /// display cap.
    Loaded(Vec<(String, String)>, usize),
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
    /// Selected row index in the Approvals table (`j`/`k`).
    pub approvals_selected: Option<usize>,
    /// Current input mode (Normal / Detail / Filter).
    pub mode: Mode,
    /// Active metrics filter text (`/` on the Metrics tab; Esc clears).
    pub metrics_filter: String,
    /// Selected row index in the filtered Metrics table (`j`/`k`).
    pub metrics_selected: Option<usize>,
    /// Approvals page currently displayed (0-based, offset paging).
    pub approvals_page: usize,
    /// Whether the last approvals fetch indicated a further page exists.
    pub approvals_has_more: bool,
    /// Highest probe latency (ms) per refresh, capped sparkline history for
    /// the latency card.
    pub latency_history: Vec<u64>,
    /// Write-queue depth from the latest metrics scrape (None = unavailable).
    pub queue_depth: Option<f64>,
    /// PG pool snapshot from the latest metrics scrape (None = unavailable,
    /// e.g. non-PostgreSQL stores).
    pub pool: Option<PoolSnapshot>,
    /// Active color theme (`--theme` / `FERRUM_TUI_THEME`); ANSI by default.
    pub theme: Theme,
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
            approvals_selected: None,
            mode: Mode::Normal,
            metrics_filter: String::new(),
            metrics_selected: None,
            approvals_page: 0,
            approvals_has_more: false,
            latency_history: Vec::new(),
            queue_depth: None,
            pool: None,
            theme: Theme::ansi(),
        }
    }

    pub fn healthy_count(&self) -> usize {
        self.probes
            .iter()
            .filter(|p| matches!(p.status, ProbeStatus::Ok(_) | ProbeStatus::DryRun))
            .count()
    }

    /// The approval row currently selected with `j`/`k`, if any.
    pub fn selected_approval(&self) -> Option<&ApprovalRequest> {
        let idx = self.approvals_selected?;
        match &self.approvals {
            ApprovalsView::Loaded(items) => items.get(idx),
            _ => None,
        }
    }

    /// Move the Approvals table selection by `delta` rows, clamped to the
    /// loaded list. No-op when no approvals are loaded.
    pub fn move_approvals_selection(&mut self, delta: isize) {
        let count = match &self.approvals {
            ApprovalsView::Loaded(items) => items.len(),
            _ => return,
        };
        if count == 0 {
            return;
        }
        let next = match self.approvals_selected {
            None => {
                if delta >= 0 {
                    0
                } else {
                    count - 1
                }
            }
            Some(idx) => (idx as isize + delta).clamp(0, count as isize - 1) as usize,
        };
        self.approvals_selected = Some(next);
    }

    /// Metric rows surviving the active metrics filter (case-insensitive
    /// substring match on the metric name). An empty filter keeps every row.
    pub fn filtered_metric_rows(&self) -> Vec<&(String, String)> {
        match &self.metrics {
            MetricsView::Loaded(pairs, _) => pairs
                .iter()
                .filter(|(name, _)| {
                    self.metrics_filter.is_empty()
                        || name
                            .to_lowercase()
                            .contains(&self.metrics_filter.to_lowercase())
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Move the Metrics table selection by `delta` rows over the filtered
    /// rows, clamped. No-op when no metrics are loaded.
    pub fn move_metrics_selection(&mut self, delta: isize) {
        let count = self.filtered_metric_rows().len();
        if count == 0 {
            return;
        }
        let next = match self.metrics_selected {
            None => {
                if delta >= 0 {
                    0
                } else {
                    count - 1
                }
            }
            Some(idx) => (idx as isize + delta).clamp(0, count as isize - 1) as usize,
        };
        self.metrics_selected = Some(next);
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

    /// Record the highest probe latency of the current refresh into the
    /// capped sparkline history. Called once per refresh cycle (Probes
    /// event), not per frame.
    pub fn record_probe_latency_sample(&mut self) {
        let max_ms = self
            .probes
            .iter()
            .filter_map(|p| p.latency_ms)
            .max()
            .unwrap_or(0);
        // Saturate at u64::MAX (probe latencies are elapsed milliseconds and
        // stay far below that); the sparkline data type is u64.
        let max_ms = max_ms.min(u128::from(u64::MAX)) as u64;
        self.latency_history.push(max_ms);
        if self.latency_history.len() > LATENCY_HISTORY_CAP {
            let overflow = self.latency_history.len() - LATENCY_HISTORY_CAP;
            self.latency_history.drain(..overflow);
        }
    }

    /// Refresh the summary-gauge inputs (write-queue depth, PG pool) from a
    /// raw `/v1/metrics` body. `None` (failed scrape) clears them so the
    /// cards render as unavailable. Reads the raw body because the Metrics
    /// tab's display rows are a curated, capped subset.
    pub fn update_metric_gauges(&mut self, metrics_text: Option<&str>) {
        self.queue_depth =
            metrics_text.and_then(|text| metric_gauge_value(text, "ferrumgate_write_queue_depth"));
        self.pool = metrics_text.and_then(|text| {
            let size = metric_gauge_value(text, "ferrumgate_store_pg_pool_size");
            let idle = metric_gauge_value(text, "ferrumgate_store_pg_pool_idle");
            let max = metric_gauge_value(text, "ferrumgate_store_pg_pool_max");
            match (size, idle, max) {
                (Some(size), Some(idle), Some(max)) if max > 0.0 => {
                    Some(PoolSnapshot { size, idle, max })
                }
                _ => None,
            }
        });
    }
}

/// Value of a label-free Prometheus gauge line (exact metric-name match) in
/// a `/v1/metrics` body. `# HELP`/`# TYPE` lines cannot match because their
/// first token is `#`.
fn metric_gauge_value(text: &str, name: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some(first) if first == name => parts.next()?.parse::<f64>().ok(),
            _ => None,
        }
    })
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
    if app.mode == Mode::Detail {
        draw_approval_detail(f, app);
    }
}

fn draw_title_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let mode_span = if app.dry_run {
        Span::styled(
            " DRY-RUN ",
            Style::default()
                .fg(theme.badge_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " LIVE ",
            Style::default()
                .fg(theme.badge_fg)
                .bg(theme.ok)
                .add_modifier(Modifier::BOLD),
        )
    };

    let auth_span = if app.token_present {
        Span::styled(
            " AUTH ",
            Style::default()
                .fg(theme.badge_fg)
                .bg(theme.ok)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " NO-AUTH ",
            Style::default()
                .fg(theme.badge_fg)
                .bg(theme.warn)
                .add_modifier(Modifier::BOLD),
        )
    };

    let line = Line::from(vec![
        Span::styled(
            " FerrumGate ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Operator Console", Style::default().fg(theme.text)),
        Span::raw("  "),
        mode_span,
        Span::raw("  "),
        Span::styled(&app.base_url, Style::default().fg(theme.muted)),
        Span::raw("  "),
        auth_span,
        Span::raw("  "),
        Span::styled(
            format!("{}s", app.refresh_interval_secs),
            Style::default().fg(theme.dim),
        ),
    ]);

    let paragraph = Paragraph::new(line).alignment(Alignment::Left);
    f.render_widget(paragraph, area);
}

fn draw_summary_cards(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let healthy = app.healthy_count();
    let total = app.probes.len();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    // Healthy card: gauge at healthy/total probes, same green/yellow rule as
    // before; the border and bar turn red when the readiness computation
    // recorded errors.
    let healthy_color = if app.error_count > 0 {
        theme.err
    } else if healthy == total {
        theme.ok
    } else {
        theme.warn
    };
    let healthy_ratio = if total > 0 {
        healthy as f64 / total as f64
    } else {
        0.0
    };
    let healthy_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Healthy ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(healthy_color)),
        )
        .ratio(healthy_ratio.clamp(0.0, 1.0))
        .label(format!("{}/{}", healthy, total))
        .gauge_style(Style::default().fg(healthy_color));
    f.render_widget(healthy_gauge, chunks[0]);

    // Write-queue depth card: gauge saturates at the server's default
    // write_queue_threshold; the label carries the raw depth. Without a
    // metrics scrape (or without the metric) the bar stays empty and the
    // label reads "—".
    let depth = app.queue_depth.unwrap_or(0.0);
    let depth_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" Queue Depth ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        )
        .ratio((depth / QUEUE_DEPTH_GAUGE_CAP).clamp(0.0, 1.0))
        .label(if app.queue_depth.is_some() {
            format!("{}", depth)
        } else {
            "—".to_string()
        })
        .gauge_style(Style::default().fg(if depth >= QUEUE_DEPTH_GAUGE_CAP {
            theme.err
        } else {
            theme.ok
        }));
    f.render_widget(depth_gauge, chunks[1]);

    // Latency card: sparkline of the highest probe latency per refresh
    // (capped in-memory history; per-probe numbers remain in the Endpoint
    // Status table on the Overview tab).
    let latency_sparkline = Sparkline::default()
        .block(
            Block::default()
                .title(" Latency (ms) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        )
        .data(app.latency_history.as_slice())
        .style(Style::default().fg(theme.accent));
    f.render_widget(latency_sparkline, chunks[2]);

    // PG pool card: in-use (size - idle) connections over the configured
    // maximum. Absent on non-PostgreSQL stores; renders empty with "—".
    let (pool_ratio, pool_label) = match app.pool {
        Some(p) if p.max > 0.0 => {
            let in_use = (p.size - p.idle).max(0.0);
            (
                (in_use / p.max).clamp(0.0, 1.0),
                format!("{}/{} used", in_use, p.max),
            )
        }
        _ => (0.0, "—".to_string()),
    };
    let pool_gauge = Gauge::default()
        .block(
            Block::default()
                .title(" PG Pool ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border)),
        )
        .ratio(pool_ratio)
        .label(pool_label)
        .gauge_style(Style::default().fg(if pool_ratio >= 1.0 {
            theme.err
        } else {
            theme.ok
        }));
    f.render_widget(pool_gauge, chunks[3]);
}

fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
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
                .border_style(Style::default().fg(theme.border)),
        )
        .highlight_style(
            Style::default()
                .fg(theme.accent)
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
    let theme = &app.theme;
    let block = Block::default()
        .title(" Readiness Summary ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

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
            // Uppercase matches the `ferrumctl admin audit verify` text output.
            let label = if r.valid { "VALID" } else { "INVALID" };
            let color = if r.valid { theme.ok } else { theme.err };
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
                ("unauthorized", theme.warn)
            } else {
                ("error", theme.err)
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

    let mut lines: Vec<Line> = vec![Line::from(vec![Span::styled(
        "Non-claims:",
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme.accent),
    )])];
    lines.extend(non_claims_lines());
    lines.extend([
        Line::from(""),
        slo_line,
        audit_line,
        snapshot_line,
        Line::from(""),
    ]);
    lines.push(Line::from(vec![Span::styled(
        "Readiness blockers:",
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(theme.accent),
    )]));

    if app.blockers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No operational blockers detected.",
            Style::default().fg(theme.ok),
        )));
    } else {
        for b in &app.blockers {
            lines.push(Line::from(Span::styled(
                format!("  • {}", b),
                Style::default().fg(theme.err),
            )));
        }
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_endpoint_status(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    // Row position is not selectable here; the title reports the list size.
    let title = format!(" Endpoint Status ({} probes) ", app.probes.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    let header = Row::new(vec!["Endpoint", "Status", "Latency", "Path"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = app
        .probes
        .iter()
        .map(|p| {
            let status_text = p.status.label();
            let status_style = p.status.badge_style(theme);
            let latency_text = p
                .latency_ms
                .map(|ms| format!("{} ms", ms))
                .unwrap_or_else(|| "—".to_string());

            Row::new(vec![
                Cell::from(p.name.clone()),
                Cell::from(Span::styled(format!(" {} ", status_text), status_style)),
                Cell::from(latency_text),
                Cell::from(p.endpoint.clone()).style(Style::default().fg(theme.dim)),
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

/// Truncate `s` to at most `max` characters, replacing the last rendered
/// character with `…` when the value is cut. The approval detail modal shows
/// the full values, so the ellipsis only marks the cell as clipped.
fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn draw_approvals(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let title = match &app.approvals {
        ApprovalsView::Loaded(items) => {
            let page_part = if app.approvals_page > 0 {
                format!("page {}, ", app.approvals_page + 1)
            } else {
                String::new()
            };
            // 1-based selected row within the loaded page, shown in the title
            // so the clamped `j`/`k` position is visible.
            let row_part = match app.approvals_selected.filter(|i| *i < items.len()) {
                Some(i) => format!(", row {} of {}", i + 1, items.len()),
                None => String::new(),
            };
            if app.dry_run {
                format!(
                    " Pending Approvals [DRY-RUN] ({}showing {}{}) ",
                    page_part,
                    items.len(),
                    row_part
                )
            } else {
                format!(
                    " Pending Approvals ({}showing {}{}) ",
                    page_part,
                    items.len(),
                    row_part
                )
            }
        }
        _ => {
            if app.dry_run {
                " Pending Approvals [DRY-RUN] ".to_string()
            } else {
                " Pending Approvals ".to_string()
            }
        }
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    match &app.approvals {
        ApprovalsView::Loading => {
            let text = Paragraph::new(Span::styled(
                "Loading approvals…",
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            ))
            .block(block)
            .alignment(Alignment::Center);
            f.render_widget(text, area);
        }
        ApprovalsView::Error(err) => {
            let text = Paragraph::new(Span::styled(
                format!("Error loading approvals: {}", err),
                Style::default().fg(theme.err),
            ))
            .block(block)
            .wrap(Wrap { trim: true });
            f.render_widget(text, area);
        }
        ApprovalsView::Loaded(items) => {
            if items.is_empty() {
                let text = Paragraph::new(Span::styled(
                    "No approvals found.",
                    Style::default().fg(theme.ok),
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
                    let by_truncated = truncate_with_ellipsis(&by_text, 12);

                    let state_style = match a.state.to_lowercase().as_str() {
                        "pending" => Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
                        "approved" => Style::default().fg(theme.ok),
                        "rejected" | "denied" => Style::default().fg(theme.err),
                        _ => Style::default().fg(theme.muted),
                    };

                    Row::new(vec![
                        Cell::from(truncate_with_ellipsis(&a.approval_id, 16)),
                        Cell::from(truncate_with_ellipsis(&a.proposal_id, 16)),
                        Cell::from(Span::styled(&a.state, state_style)),
                        Cell::from(truncate_with_ellipsis(&a.reason, 20)),
                        Cell::from(by_truncated),
                        Cell::from(truncate_with_ellipsis(&a.created_at, 16)),
                        Cell::from(truncate_with_ellipsis(&a.expires_at, 16)),
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

            let mut table_state = TableState::new()
                .with_selected(app.approvals_selected.filter(|i| *i < items.len()));
            f.render_stateful_widget(table, area, &mut table_state);
        }
    }
}

fn draw_metrics(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let filtered = app.filtered_metric_rows();
    // 1-based selected row within the filtered rows, shown in the title so
    // the clamped `j`/`k` position is visible.
    let row_part = match app.metrics_selected.filter(|i| *i < filtered.len()) {
        Some(i) => format!(", row {} of {}", i + 1, filtered.len()),
        None => String::new(),
    };
    let title = match &app.metrics {
        MetricsView::Loaded(pairs, total) => {
            if app.metrics_filter.is_empty() {
                if *total > pairs.len() {
                    format!(
                        " Metrics Summary (showing {} of {}{}) ",
                        pairs.len(),
                        total,
                        row_part
                    )
                } else {
                    format!(" Metrics Summary ({}{}) ", pairs.len(), row_part)
                }
            } else {
                format!(
                    " Metrics Summary ({} of {} match \"{}\"{}) ",
                    filtered.len(),
                    pairs.len(),
                    app.metrics_filter,
                    row_part
                )
            }
        }
        _ => " Metrics Summary ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));

    match &app.metrics {
        MetricsView::Loading => {
            let text = Paragraph::new(Span::styled(
                "Loading metrics…",
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
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
                    Style::default().fg(theme.muted),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "The /v1/metrics endpoint returned data but no recognised numeric metrics.",
                    Style::default().fg(theme.dim),
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
                Style::default().fg(theme.err),
            ))
            .block(block)
            .wrap(Wrap { trim: true });
            f.render_widget(text, area);
        }
        MetricsView::Loaded(pairs, _) => {
            if pairs.is_empty() {
                let text = Paragraph::new(Span::styled(
                    "No numeric metrics found.",
                    Style::default().fg(theme.muted),
                ))
                .block(block)
                .alignment(Alignment::Center);
                f.render_widget(text, area);
                return;
            }
            if filtered.is_empty() {
                let text = Paragraph::new(Span::styled(
                    format!("No metrics match filter \"{}\".", app.metrics_filter),
                    Style::default().fg(theme.muted),
                ))
                .block(block)
                .alignment(Alignment::Center);
                f.render_widget(text, area);
                return;
            }

            let header = Row::new(vec!["Metric", "Value"])
                .style(Style::default().add_modifier(Modifier::BOLD))
                .height(1);

            let filtered_count = filtered.len();
            let rows: Vec<Row> = filtered
                .into_iter()
                .map(|(k, v)| {
                    Row::new(vec![
                        Cell::from(k.clone()).style(Style::default().fg(theme.text)),
                        Cell::from(v.clone()).style(Style::default().fg(theme.accent)),
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

            let mut table_state = TableState::new()
                .with_selected(app.metrics_selected.filter(|i| *i < filtered_count));
            f.render_stateful_widget(table, area, &mut table_state);
        }
    }
}

fn draw_help_page(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let block = Block::default()
        .title(" Keyboard Shortcuts & Information ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn));

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.accent),
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
        Line::from(vec![
            Span::styled("j / k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Select approval row / scroll metrics (Approvals, Metrics tabs)"),
        ]),
        Line::from(vec![
            Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Open approval detail modal (Approvals tab)"),
        ]),
        Line::from(vec![
            Span::styled("n / p", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Next / previous approvals page (Approvals tab)"),
        ]),
        Line::from(vec![
            Span::styled("/", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Filter metrics (Metrics tab)"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Actions",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.accent),
        )]),
        Line::from(vec![
            Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("          Refresh data now"),
        ]),
        Line::from(vec![
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("        Close detail modal / clear metrics filter"),
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
                .fg(theme.accent),
        )]),
        Line::from("  FERRUM_TUI_SERVER_URL    Base URL fallback"),
        Line::from("  FERRUM_TUI_BEARER_TOKEN  Token fallback"),
        Line::from("  FERRUMCTL_SERVER_URL     Alternate base URL fallback"),
        Line::from("  FERRUMCTL_BEARER_TOKEN   Alternate token fallback"),
        Line::from("  FERRUM_TUI_WINDOW_DIR    Directory for slo-window-state.json"),
        Line::from("  FERRUM_TUI_EVIDENCE_DIR  Directory for evidence-snapshot-*.json"),
        Line::from("  FERRUM_TUI_THEME         Color theme: ansi (default) or rgb"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Non-claims",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(theme.accent),
        )]),
    ];
    lines.extend(non_claims_lines());
    lines.push(Line::from(
        "  Operator convenience only; no mutation operations in this MVP.",
    ));
    lines.push(Line::from("  Token values are redacted in the UI."));
    let text = Text::from(lines);

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(paragraph, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    // Full IDs of the selected approval row; the table cells truncate IDs to
    // fit column widths, so the footer is where the operator reads them in
    // full. While a row is selected the IDs take the hint's place so the line
    // fits on one row.
    let selected = if app.current_tab == Tab::Approvals {
        app.selected_approval()
    } else {
        None
    };

    // While the metrics filter is being typed there is no visible cursor or
    // input line; the footer carries the live input and the accept/clear keys.
    let hint = if app.mode == Mode::Filter {
        format!(
            " filter: {}▏  Enter accept · Esc clear ",
            app.metrics_filter
        )
    } else {
        match app.current_tab {
            Tab::Overview => "Tab →  |  r refresh  |  ? help  |  q quit".to_string(),
            Tab::Approvals => {
                "Tab →  |  j/k select  |  Enter detail  |  n/p page  |  r refresh  |  ? help  |  q quit"
                    .to_string()
            }
            Tab::Metrics => {
                "Tab →  |  / filter  |  j/k scroll  |  r refresh  |  ? help  |  q quit".to_string()
            }
            Tab::Help => "Tab →  |  q quit".to_string(),
        }
    };

    let msg_span = if app.message.is_empty() {
        Span::raw("")
    } else {
        Span::styled(
            format!("  {}  ", app.message),
            Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
        )
    };

    let line = if let Some(a) = selected {
        vec![
            Span::styled(
                format!("approval {} · proposal {}", a.approval_id, a.proposal_id),
                Style::default().fg(theme.accent),
            ),
            msg_span,
        ]
    } else {
        vec![
            Span::styled(hint, Style::default().fg(theme.muted)),
            msg_span,
        ]
    };

    let text = Text::from(vec![Line::from(line)]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border));
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

/// Full-field view of the selected approval: the table cells truncate IDs and
/// reason to fit column widths, so this modal is where the operator reads the
/// complete values. Reuses the help overlay's Clear + centered_rect pattern.
fn draw_approval_detail(f: &mut Frame, app: &App) {
    let theme = &app.theme;
    let Some(a) = app.selected_approval() else {
        return;
    };
    let area = centered_rect(70, 60, f.area());

    let by_text = match &a.requested_by {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let label = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let text = Text::from(vec![
        Line::from(vec![
            Span::styled("Approval ID:  ", label),
            Span::raw(a.approval_id.as_str()),
        ]),
        Line::from(vec![
            Span::styled("Proposal ID:  ", label),
            Span::raw(a.proposal_id.as_str()),
        ]),
        Line::from(vec![
            Span::styled("State:        ", label),
            Span::raw(a.state.as_str()),
        ]),
        Line::from(vec![
            Span::styled("Requested by: ", label),
            Span::raw(by_text.as_str()),
        ]),
        Line::from(vec![
            Span::styled("Created:      ", label),
            Span::raw(a.created_at.as_str()),
        ]),
        Line::from(vec![
            Span::styled("Expires:      ", label),
            Span::raw(a.expires_at.as_str()),
        ]),
        Line::from(""),
        Line::from(Span::styled("Reason:", label)),
        Line::from(a.reason.as_str()),
        Line::from(""),
        Line::from(Span::styled(
            "Esc / q close",
            Style::default().fg(theme.dim),
        )),
    ]);

    let block = Block::default()
        .title(" Approval Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.badge_fg));
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    f.render_widget(Clear, area);
    f.render_widget(paragraph, area);
}

fn draw_help_overlay(f: &mut Frame, app: &App) {
    let theme = &app.theme;
    let area = centered_rect(60, 55, f.area());

    let block = Block::default()
        .title(" Quick Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warn))
        .style(Style::default().bg(theme.badge_fg));

    let mut lines = vec![
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
            Span::styled("j / k", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("      Select approval row / scroll metrics"),
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
                .fg(theme.accent),
        )]),
    ];
    lines.extend(non_claims_lines());
    lines.push(Line::from(
        "  Operator convenience only; not production-ready.",
    ));
    let text = Text::from(lines);

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
    fn test_readiness_healthy_when_no_errors() {
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
        // The full-page reference grew with the P1 shortcuts (Enter, n/p, /,
        // Esc); 36 rows keep the whole page incl. the non-claims block visible.
        let backend = TestBackend::new(80, 36);
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

    fn sample_approval(id: &str) -> ApprovalRequest {
        ApprovalRequest {
            approval_id: id.to_string(),
            proposal_id: format!("prop-{}", id),
            requested_by: serde_json::json!("operator"),
            reason: "test".to_string(),
            state: "pending".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2026-01-02T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_approvals_selection_movement_and_clamping() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.approvals = ApprovalsView::Loaded(vec![
            sample_approval("a1"),
            sample_approval("a2"),
            sample_approval("a3"),
        ]);

        app.move_approvals_selection(1);
        assert_eq!(app.approvals_selected, Some(0));
        app.move_approvals_selection(1);
        app.move_approvals_selection(1);
        assert_eq!(app.approvals_selected, Some(2));
        app.move_approvals_selection(1);
        assert_eq!(app.approvals_selected, Some(2)); // clamped at last row
        app.move_approvals_selection(-1);
        assert_eq!(app.approvals_selected, Some(1));

        // k with no prior selection jumps to the last row.
        let mut fresh = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        fresh.approvals = ApprovalsView::Loaded(vec![sample_approval("a1")]);
        fresh.move_approvals_selection(-1);
        assert_eq!(fresh.approvals_selected, Some(0));
    }

    #[test]
    fn test_selection_noop_without_loaded_approvals() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.move_approvals_selection(1);
        assert_eq!(app.approvals_selected, None);
        assert!(app.selected_approval().is_none());
    }

    #[test]
    fn test_summary_cards_render_widgets_without_rc_ready_claim() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.probes = vec![ProbeResult {
            name: "Health".to_string(),
            endpoint: "/v1/healthz".to_string(),
            status: ProbeStatus::Ok("ok".to_string()),
            latency_ms: Some(3),
        }];
        app.compute_readiness_state();
        terminal
            .draw(|f| draw_summary_cards(f, &app, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        // The strip is now widget cards (Gauge/Sparkline); it still never
        // claims RC-READY.
        assert!(text.contains("Healthy"));
        assert!(text.contains("Queue Depth"));
        assert!(text.contains("Latency"));
        assert!(text.contains("PG Pool"));
        assert!(text.contains("1/1"));
        assert!(!text.contains("RC-READY"));
    }

    #[test]
    fn test_metric_gauge_value_exact_name_match() {
        let text = "# HELP ferrumgate_write_queue_depth pending writes\n\
                    # TYPE ferrumgate_write_queue_depth gauge\n\
                    ferrumgate_write_queue_depth 42\n\
                    other_metric 7\n\
                    ferrumgate_write_queue_depth_extra{a=\"b\"} 3\n";
        assert_eq!(
            metric_gauge_value(text, "ferrumgate_write_queue_depth"),
            Some(42.0)
        );
        assert_eq!(metric_gauge_value(text, "missing_metric"), None);
        assert_eq!(metric_gauge_value("", "ferrumgate_write_queue_depth"), None);
    }

    #[test]
    fn test_update_metric_gauges_extracts_queue_and_pool() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.update_metric_gauges(Some(
            "ferrumgate_write_queue_depth 5\n\
             ferrumgate_store_pg_pool_size 3\n\
             ferrumgate_store_pg_pool_idle 2\n\
             ferrumgate_store_pg_pool_max 10\n",
        ));
        assert_eq!(app.queue_depth, Some(5.0));
        let pool = app.pool.unwrap();
        assert_eq!((pool.size, pool.idle, pool.max), (3.0, 2.0, 10.0));

        // A failed scrape clears the gauge inputs.
        app.update_metric_gauges(None);
        assert_eq!(app.queue_depth, None);
        assert_eq!(app.pool, None);

        // A partial pool report (no max) renders as unavailable, not 0.
        app.update_metric_gauges(Some(
            "ferrumgate_store_pg_pool_size 3\n\
             ferrumgate_store_pg_pool_idle 2\n",
        ));
        assert_eq!(app.pool, None);
    }

    #[test]
    fn test_latency_history_records_max_and_caps() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.probes = vec![
            ProbeResult {
                name: "Health".to_string(),
                endpoint: "/v1/healthz".to_string(),
                status: ProbeStatus::Ok("ok".to_string()),
                latency_ms: Some(3),
            },
            ProbeResult {
                name: "Readiness".to_string(),
                endpoint: "/v1/readyz".to_string(),
                status: ProbeStatus::Ok("ok".to_string()),
                latency_ms: Some(9),
            },
        ];
        for _ in 0..(LATENCY_HISTORY_CAP + 10) {
            app.record_probe_latency_sample();
        }
        assert_eq!(app.latency_history.len(), LATENCY_HISTORY_CAP);
        assert!(app.latency_history.iter().all(|&ms| ms == 9));
    }

    #[test]
    fn test_footer_per_tab_hints() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.current_tab = Tab::Approvals;
        app.approvals = ApprovalsView::Loaded(vec![sample_approval("a1")]);
        terminal.draw(|f| draw_footer(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("j/k select"));
        assert!(text.contains("Enter detail"));
        assert!(text.contains("n/p page"));

        app.current_tab = Tab::Help;
        terminal.draw(|f| draw_footer(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(!text.contains("refresh"));
    }

    #[test]
    fn test_footer_shows_full_ids_for_selected_approval() {
        let approval_id = "0f7d2c8e-1a4b-4c9d-9e2f-3b5a6c7d8e9f";
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.current_tab = Tab::Approvals;
        app.approvals = ApprovalsView::Loaded(vec![ApprovalRequest {
            approval_id: approval_id.to_string(),
            proposal_id: "1a8e5f2c-9b3d-4e7f-8a1c-2d4e6f8a0b3c".to_string(),
            requested_by: serde_json::json!("operator"),
            reason: "test".to_string(),
            state: "pending".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2026-01-02T00:00:00Z".to_string(),
        }]);
        app.move_approvals_selection(1);

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw_footer(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains(approval_id));
        assert!(text.contains("1a8e5f2c-9b3d-4e7f-8a1c-2d4e6f8a0b3c"));
        assert!(!text.contains("q quit"));
    }

    #[test]
    fn test_metrics_filter_matches_name_case_insensitively() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.metrics = MetricsView::Loaded(
            vec![
                ("http_requests_total".to_string(), "1".to_string()),
                ("store_health".to_string(), "1".to_string()),
            ],
            2,
        );

        assert_eq!(app.filtered_metric_rows().len(), 2); // empty filter keeps all
        app.metrics_filter = "HTTP".to_string();
        let rows = app.filtered_metric_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "http_requests_total");
        app.metrics_filter = "zzz".to_string();
        assert!(app.filtered_metric_rows().is_empty());
    }

    #[test]
    fn test_metrics_selection_clamps_to_filtered_rows() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.metrics = MetricsView::Loaded(
            vec![
                ("http_requests_total".to_string(), "1".to_string()),
                ("http_errors_total".to_string(), "2".to_string()),
                ("store_health".to_string(), "1".to_string()),
            ],
            3,
        );

        app.move_metrics_selection(1);
        assert_eq!(app.metrics_selected, Some(0));
        app.move_metrics_selection(9);
        assert_eq!(app.metrics_selected, Some(2)); // clamped at last row
        app.move_metrics_selection(-9);
        assert_eq!(app.metrics_selected, Some(0));

        // A narrowing filter re-clamps the selection on the next move.
        app.metrics_filter = "store_health".to_string();
        app.move_metrics_selection(1);
        assert_eq!(app.metrics_selected, Some(0));
    }

    #[test]
    fn test_metrics_title_shows_filter_match_count() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.current_tab = Tab::Metrics;
        app.metrics = MetricsView::Loaded(
            vec![
                ("http_requests_total".to_string(), "1".to_string()),
                ("http_errors_total".to_string(), "2".to_string()),
                ("store_health".to_string(), "1".to_string()),
            ],
            3,
        );
        app.metrics_filter = "http".to_string();
        terminal.draw(|f| draw_metrics(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("2 of 3 match \"http\""));

        // With a filter matching nothing the empty state is grounded.
        app.metrics_filter = "zzz".to_string();
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw_metrics(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("No metrics match filter \"zzz\"."));
    }

    #[test]
    fn test_metrics_table_renders_with_selection() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.metrics = MetricsView::Loaded(
            vec![
                ("http_requests_total".to_string(), "1".to_string()),
                ("store_health".to_string(), "1".to_string()),
            ],
            2,
        );
        app.move_metrics_selection(1);
        terminal.draw(|f| draw_metrics(f, &app, f.area())).unwrap();
        // Rendering with an active selection must not panic; the highlight
        // itself is styling-only, so only the row content is asserted.
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("http_requests_total"));
    }

    #[test]
    fn test_approval_detail_modal_shows_full_fields() {
        let approval_id = "0f7d2c8e-1a4b-4c9d-9e2f-3b5a6c7d8e9f";
        let proposal_id = "1a8e5f2c-9b3d-4e7f-8a1c-2d4e6f8a0b3c";
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.current_tab = Tab::Approvals;
        app.approvals = ApprovalsView::Loaded(vec![ApprovalRequest {
            approval_id: approval_id.to_string(),
            proposal_id: proposal_id.to_string(),
            requested_by: serde_json::json!({"user": "operator", "role": "ops"}),
            reason: "Grant read-only audit access for the pilot".to_string(),
            state: "pending".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2026-01-02T00:00:00Z".to_string(),
        }]);
        app.move_approvals_selection(1);
        app.mode = Mode::Detail;

        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("Approval Detail"));
        assert!(text.contains(approval_id));
        assert!(text.contains(proposal_id));
        assert!(text.contains("operator"));
        assert!(text.contains("Grant read-only audit access for the pilot"));
        assert!(text.contains("2026-01-01T00:00:00Z"));
        assert!(text.contains("2026-01-02T00:00:00Z"));
        assert!(text.contains("Esc / q close"));
    }

    #[test]
    fn test_approvals_title_shows_page_number_past_first_page() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.approvals = ApprovalsView::Loaded(vec![sample_approval("a1")]);

        terminal
            .draw(|f| draw_approvals(f, &app, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(!text.contains("page "));

        app.approvals_page = 2; // third page
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_approvals(f, &app, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("page 3"));
    }

    #[test]
    fn test_table_titles_show_selected_row_position() {
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.approvals = ApprovalsView::Loaded(vec![
            sample_approval("a1"),
            sample_approval("a2"),
            sample_approval("a3"),
        ]);
        app.move_approvals_selection(1);
        app.move_approvals_selection(1);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| draw_approvals(f, &app, f.area()))
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("row 2 of 3"));

        app.current_tab = Tab::Metrics;
        app.metrics = MetricsView::Loaded(
            vec![
                ("metric_one".to_string(), "1".to_string()),
                ("metric_two".to_string(), "2".to_string()),
            ],
            2,
        );
        app.move_metrics_selection(1);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw_metrics(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("row 1 of 2"));
    }

    #[test]
    fn test_truncate_with_ellipsis_marks_only_cut_values() {
        assert_eq!(truncate_with_ellipsis("short", 16), "short");
        assert_eq!(
            truncate_with_ellipsis("0123456789abcdef", 16),
            "0123456789abcdef"
        );
        assert_eq!(
            truncate_with_ellipsis("0123456789abcdefg", 16),
            "0123456789abcde…"
        );
        assert_eq!(
            truncate_with_ellipsis("0123456789abcdefghij", 20),
            "0123456789abcdefghij"
        );
        assert_eq!(
            truncate_with_ellipsis("0123456789abcdefghijk", 20),
            "0123456789abcdefghi…"
        );
    }

    #[test]
    fn test_footer_shows_filter_input_in_filter_mode() {
        let backend = TestBackend::new(120, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.mode = Mode::Filter;
        app.metrics_filter = "http".to_string();
        terminal.draw(|f| draw_footer(f, &app, f.area())).unwrap();
        let buf = terminal.backend().buffer();
        let text: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(text.contains("filter: http"));
        assert!(text.contains("Enter accept"));
        assert!(text.contains("Esc clear"));
        assert!(!text.contains("r refresh"));
    }

    #[test]
    fn test_theme_mode_parse_and_ansi_default() {
        assert_eq!("rgb".parse::<ThemeMode>().unwrap(), ThemeMode::Rgb);
        assert_eq!("ANSI".parse::<ThemeMode>().unwrap(), ThemeMode::Ansi);
        assert!("bogus".parse::<ThemeMode>().is_err());
        assert_eq!(Theme::for_mode(ThemeMode::Ansi), Theme::ansi());
        assert_eq!(Theme::for_mode(ThemeMode::Rgb), Theme::rgb());

        // New apps start on ANSI; RGB is opt-in.
        let app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        assert_eq!(app.theme, Theme::ansi());
    }

    #[test]
    fn test_theme_ansi_default_has_no_truecolor_and_rgb_opt_in_does() {
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("http://127.0.0.1:8080".to_string(), false, false, 5);
        app.probes = vec![ProbeResult {
            name: "Health".to_string(),
            endpoint: "/v1/healthz".to_string(),
            status: ProbeStatus::Ok("ok".to_string()),
            latency_ms: Some(3),
        }];
        app.compute_readiness_state();

        let has_truecolor = |buf: &ratatui::buffer::Buffer| {
            buf.content
                .iter()
                .any(|c| matches!(c.fg, Color::Rgb(..)) || matches!(c.bg, Color::Rgb(..)))
        };

        // Default (ANSI) render stays on named 16-color values.
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(!has_truecolor(terminal.backend().buffer()));

        // Opting into RGB switches the same layout to truecolor slots.
        app.theme = Theme::rgb();
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(has_truecolor(terminal.backend().buffer()));
    }

    #[test]
    fn test_rgb_theme_keeps_borders_and_accents_distinct() {
        assert_ne!(Theme::rgb().border, Theme::rgb().accent);
        assert_ne!(Theme::ansi().border, Theme::ansi().accent);
    }
}
