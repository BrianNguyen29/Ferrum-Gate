# Phased Visualization Improvements — Proposal (no code changes yet)

Status: proposal only. No files outside this document are modified by this proposal.

Scope reviewed:
- `configs/monitoring/ferrumgate-grafana-dashboard.json` — 8 panels, 2-column grid (w=12, h=8), uid `ferrumgate-overview`, 30s refresh.
- `configs/monitoring/ferrumgate-alerts.yaml` — 20 rules in 4 groups (`ferrumgate`, `ferrumgate_instance`, `ferrumgate_postgres`, `ferrumgate_worm_sink`); every rule has a `runbook_url`; none link back to a dashboard.
- `bins/ferrum-tui` — 4 tabs (Overview, Approvals, Metrics, Help) built from `Paragraph`/`Table`/`Tabs`/`Block` only; no `Gauge`, `Sparkline`, or `Chart` widgets are used today.
- `docs/diagrams/` — 4 Mermaid sources (`.mmd`) that render only when pasted into a viewer; the Zola site (`site/`) has no Mermaid integration.
- Metrics observed in-repo: HTTP rate/duration, governance success/error counters, write queue depth, store health, PG pool gauges, WORM sink counters/timestamp, lifecycle outbox review gauge (plus instance-level `node_*` metrics used by alerts).

Grounding rule for all copy below: describe only what the metric measures and its current coverage. Mirror the repo's existing cautious wording (e.g. the WORM alert text "best-effort … does not prove compliance immutability", the TUI "NOT COMPLETE" non-claims). No readiness, compliance, or HA claims.

---

## P0 — Config/docs-only, zero code risk

These changes touch only the Grafana JSON, alert annotations, and the Zola site content. Each is independently revertible.

### P0-1. Fill dashboard gaps with existing metrics
File: `configs/monitoring/ferrumgate-grafana-dashboard.json`

Add a third row of panels (keep the existing 2-column w=12/h=8 grid; append at `y: 32`, do not move or resize existing panels):

| Panel | Type | Query (existing metrics only) | Grounded title |
|---|---|---|---|
| Write Queue Depth | timeseries | `ferrumgate_write_queue_depth` | "Write Queue Depth" |
| Governance Error Rate by Route | timeseries | `sum by (route) (rate(ferrumgate_governance_errors_total[5m]))` | "Governance Errors by Route" |
| Governance Success Rate by Route | timeseries | `sum by (route) (rate(ferrumgate_governance_success_total[5m]))` | "Governance Successes by Route" |
| Store Health | stat | `ferrumgate_store_health_up` | "Store Health (1 = healthy)" |

Layout direction: unchanged — same grid, same uid, same refresh. New panels append below the existing ones so the current overview row order ("what an operator checks first") is preserved.

### P0-2. Alert → dashboard links
File: `configs/monitoring/ferrumgate-alerts.yaml`

Add one annotation per rule (no rule logic changes):

```yaml
dashboard_url: "https://<grafana-host>/d/ferrumgate-overview"
```

This is the standard alert-panel link pattern; the placeholder host matches the file's existing template posture (it already documents placeholder thresholds and relative `runbook_url`s). Keep wording in `description` fields as-is — they are already grounded.

### P0-3. Render the 4 Mermaid diagrams into the site
Files: `site/content/guides/diagrams.md` (new page), `site/static/mermaid.min.js` (vendored, pinned version), small template hook in `site/templates/guide.html` (load the vendored script only on the diagrams page).

Simplest viable approach: one guide page that embeds each `.mmd` source inside a ` ```mermaid ` fence and renders client-side with a vendored (not CDN) Mermaid build, matching the local-only scaffold posture in `site/config.toml`. Keep `docs/diagrams/*.mmd` as the single source of truth — the page copies or shortcode-includes them, it does not fork them.

### P0-4. WORM sink status panels (metrics already exist)
File: `configs/monitoring/ferrumgate-grafana-dashboard.json`

Append two panels (row at `y: 40`):

- "WORM Sink Export Failures" — timeseries, `rate(ferrumgate_audit_worm_sink_failures_total[5m])`.
- "WORM Sink Last Successful Export (seconds ago)" — stat, `time() - ferrumgate_audit_worm_sink_last_success_timestamp_seconds`.

Panel description (grounded, mirrors ADR 009 / alert wording): *"WORM archive replica is best-effort. The local audit bundle remains the durable record. This panel does not prove compliance immutability."* Panels are empty when the `worm-sink` feature is off; that is expected and matches the alert-group note.

---

## P1 — Small, contained code changes (one binary, no new APIs)

### P1-1. TUI: minimal widget swaps
File: `bins/ferrum-tui/src/app.rs` (rendering only; no client changes)

Keep all 4 tabs, the summary-card strip, and every table. Three narrow swaps:

1. **Healthy card → `Gauge`**: the existing `Healthy N/3` card becomes a ratatui `Gauge` at ratio `healthy/total`, same block title (" Healthy "), same green/yellow color rule. Same information, faster scan.
2. **SLO window line → `LineGauge`**: in the Readiness Summary block, the existing text line "SLO window: … N days elapsed (target M days)" gains a one-row `LineGauge` at ratio `elapsed/target`. Keep the text line verbatim above it — the gauge adds proportion, the text keeps the facts. When `SloWindowView::Missing`, show the gauge empty with the existing "No active window" text. No claim of SLO attainment — it shows elapsed vs target only, matching the current text.
3. **Error-count sparkline**: buffer the last ~30 `error_count` samples in `App` (in-memory Vec, pushed on each refresh tick) and render a `Sparkline` inside the existing " Errors " card. This is the only new state: one `Vec<u64>`, capped. No history across restarts; copy stays "Errors".

Explicitly not changed: the Approvals table, Metrics table, help/overlay modals, footer hints, and the NON_CLAIMS block. No new keybindings.

### P1-2. Governance funnel row on the dashboard
File: `configs/monitoring/ferrumgate-grafana-dashboard.json`

Append a row (4 × w=6 `stat` panels at `y: 48`) showing 5-minute rates across the governance path, using existing route labels on `ferrumgate_governance_success_total`:

1. "Intents Submitted" — `route="/v1/intents"`
2. "Capabilities Minted" — `route="/v1/capabilities/mint"`
3. "Proposals Evaluated" — `route="/v1/proposals/{proposal_id}/evaluate"`
4. "Executions" — `route=~"/v1/executions/.*"`

Title the row group (via panel descriptions or a Grafana `row` object) "Governance Throughput (5m rates)". Do not call it a "funnel" in visible copy — the metrics are independent per-route rates, not a tracked per-request chain, so "funnel" would overclaim.

### P1-3. SLO / error-budget panel (computed, not asserted)
File: `configs/monitoring/ferrumgate-grafana-dashboard.json`

One timeseries panel: `1 - (sum(rate(ferrumgate_http_requests_total{status=~"5.."}[5m])) / sum(rate(ferrumgate_http_requests_total[5m])))` titled "Success Ratio (5m)". Description: *"Ratio of non-5xx responses over the trailing 5 minutes. Not an error-budget statement; FerrumGate has no committed SLO target yet (see TUI non-claims)."* Deferred: true error-budget burn-down needs a chosen SLO target, which is a product decision, not a viz change.

### P1-4. HA/Postgres replication panels — template-gated
File: `configs/monitoring/ferrumgate-grafana-dashboard.json`

Mirror the existing alert-file pattern: add one row, clearly marked in panel descriptions as *"TEMPLATE — requires postgres_exporter; HA/replication is not deployed by this repo."* Panels: replication lag (`pg_stat_replication_pg_wal_lsn_diff`, same expression as the alert). Panels sit empty until an operator wires the exporter — identical posture to `FerrumGatePostgresReplicationLag`.

---

## P2 — Requires new read-only surface; proposed, not scoped for implementation

Flagged explicitly because the constraint says no new APIs unless phased — these are the phases.

### P2-1. Lineage DAG view
The minimum lineage chain (PolicyEvaluated → CapabilityMinted → … → Terminal) is documented in `AGENTS.md` and `docs/diagrams/03-lineage-chain.mmd`, but no endpoint exposes a per-execution event chain for rendering. A lineage DAG (TUI or site) needs a read-only endpoint such as `GET /v1/executions/{execution_id}/lineage` returning ordered lifecycle events. Until that exists, any DAG view would be mock data — out of scope for P0/P1. Proposal: phase the endpoint first, then render the DAG with the same 8 states as `03-lineage-chain.mmd` so docs and UI cannot drift.

### P2-2. Node-graph topology panel
Grafana's `nodeGraph` needs a node/edge data source (traces or a topology API). FerrumGate emits neither. Not proposed at any phase until a tracing or topology source exists.

> **Implementation note (config-only, "P1 Timeline DAG"):** The overview dashboard now ships (a) governance timeline annotations — 2-minute increase markers on the governance error, approval timeout, and quarantine timeout counters — and (b) a template `nodeGraph` panel for the lineage DAG. The panel uses the already-documented read-only lineage APIs (`GET /v1/provenance/lineage/{execution_id}`, `POST /v1/provenance/lineage`), which provide the topology source this section asked for; it stays empty until an operator connects a JSON-capable data source, and edge colors map the `ProvenanceEdgeType` variants from `crates/ferrum-proto/src/provenance.rs`. No new APIs were added, and no claims beyond this template posture are made.

---

## Copy standard applied everywhere

- Panel titles name the metric, not a conclusion ("PG Pool Saturation %", not "Database Health").
- Descriptions state measurement window and known caveats; reuse the WORM alert's caveat sentence verbatim for WORM panels.
- Dashboards/TUI never assert production readiness, Tier-2 completion, SLO attainment, or HA — consistent with the TUI `NON_CLAIMS` lines and the site `status_banner`.
- New TUI widgets follow the operation-label style: short, status-reflecting ("Healthy", "Errors", "SLO window"), no new vocabulary.

## What this proposal deliberately does not do

- No re-layout, recolor, or re-theming of the existing dashboard or TUI (layout intent preserved). **Amended 2026-09-11 (decision D1):** the TUI only may opt into an RGB truecolor theme via `--theme rgb` or `FERRUM_TUI_THEME=rgb`; the ANSI 16-color theme remains the default, and the dashboard and site are unchanged.
- No changes to alert thresholds or rule expressions.
- No new Rust crates, no new dependencies beyond ratatui widgets already in the dependency tree, and one vendored JS file for the site.
- No CI changes; the existing `scripts/validate_monitoring_metrics.py` gate should be re-run after P0 edits, and the TUI's `TestBackend` snapshot tests extended for the new widgets at implementation time.
