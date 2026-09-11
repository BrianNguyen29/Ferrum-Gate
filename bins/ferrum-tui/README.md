# ferrum-tui

Lightweight terminal dashboard for FerrumGate operator endpoints. No mutation operations.

## Usage

```bash
# Build
cargo build --release --bin ferrum-tui

# Run with defaults (http://127.0.0.1:8080)
./target/release/ferrum-tui

# Run against a specific server
./target/release/ferrum-tui --server-url https://ferrumgate.example.com:8080

# Run with bearer token (token is redacted in UI)
./target/release/ferrum-tui --bearer-token $TOKEN

# Dry-run mode (synthetic OKs, no HTTP calls)
./target/release/ferrum-tui --dry-run

# Opt into the RGB truecolor theme (ANSI 16-color is the default)
./target/release/ferrum-tui --theme rgb

# Custom refresh interval (seconds)
./target/release/ferrum-tui --interval 10
```

## Environment variables

| Variable | Purpose | Fallback |
|----------|---------|----------|
| `FERRUM_TUI_SERVER_URL` | Base URL | `FERRUMCTL_SERVER_URL`, then `http://127.0.0.1:8080` |
| `FERRUM_TUI_BEARER_TOKEN` | Bearer token | `FERRUMCTL_BEARER_TOKEN`, then unset |
| `FERRUM_TUI_WINDOW_DIR` | Directory for `slo-window-state.json` | `.` |
| `FERRUM_TUI_EVIDENCE_DIR` | Directory for `evidence-snapshot-*.json` | `.` |
| `FERRUM_TUI_THEME` | Color theme: `ansi` (default) or `rgb` | `ansi` |

## Themes

The default ANSI theme uses the terminal's 16-color palette and is safe on
non-truecolor terminals. `--theme rgb` (or `FERRUM_TUI_THEME=rgb`) opts into
the truecolor palette shared with the site and SVG assets (`assets/README.md`):
iron blue borders, violet accents, rust errors, and the mute/muted text tones.
The `--theme` flag wins over the environment variable. Layout and text are
identical in both themes.

## Keyboard shortcuts

### Navigation

| Key | Action |
|-----|--------|
| `Tab` / `→` | Next tab |
| `Shift+Tab` / `←` | Previous tab |
| `1` | Overview tab |
| `2` | Approvals tab |
| `3` | Metrics tab |
| `4` | Help tab |
| `a` | Jump to Approvals tab |
| `j` / `k` | Select next / previous row (Approvals table; Metrics table scroll) |
| `Enter` | Open the approval detail modal (Approvals tab): full approval/proposal IDs, state, requested_by, created/expires, and the untruncated reason |
| `n` / `p` | Next / previous approvals page (Approvals tab) |
| `/` | Filter metrics by name, applied live while typing (Metrics tab) |

### Actions

| Key | Action |
|-----|--------|
| `r` | Refresh data now (immediate fetch; restarts the auto-refresh timer) |
| `Esc` | Close the approval detail modal / clear the metrics filter |
| `?` / `h` | Toggle help overlay |
| `q` | Quit (in the detail modal, `q` closes the modal instead) |

While the metrics filter input is open, typed characters go into the filter,
`Enter` accepts it and `Esc` clears it. While the detail modal is open,
`Esc` / `q` close it and other keys are ignored.

## Layout

- **Title bar** — App name, mode badge (LIVE / DRY-RUN), base URL, auth badge, refresh interval.
- **Summary cards** — Healthy endpoints, pending approvals, errors, last refresh time.
- **Tab bar** — Overview · Approvals · Metrics · Help.
- **Content area** — Tab-specific data.
- **Footer** — Context-aware shortcut hints and status messages.

## Tabs

### Overview
Endpoint status table showing health, readiness, and deep-readiness probes with semantic status badges and latency.

### Approvals
Read-only list of pending approvals with state badges, truncation for narrow terminals, and empty-state messaging. The title shows how many rows were fetched and, past the first page, the page number; `j` / `k` select a row (the footer then displays the selected approval's full approval ID and proposal ID), and `Enter` opens a detail modal with the full IDs, state, requested_by, created/expires timestamps, and untruncated reason (`Esc` / `q` closes).

Approvals are fetched in pages of 50 rows (`?limit=51&offset=…` — one extra row is requested to detect whether a further page exists). `n` / `p` move between pages. Offset paging is used deliberately: the server currently emits `next_cursor` only for cursor-path requests, and a first-page request cannot enter that path without the client fabricating cursors (duplicating server-internal encoding and mixing the offset path's `created_at`-only ordering with the cursor path's `created_at, approval_id` ordering). More than 50 pending approvals is not expected in the pilot posture, but paging is supported; note that offset paging over a mutable pending list can shift rows between refreshes.

### Metrics
Parses `/v1/metrics` (Prometheus text format) and displays a curated subset of numeric metrics (health, totals, counts, pool stats, latency, etc.). The title shows how many curated metrics are displayed out of the total matched ("showing X of Y"). `/` opens the filter input: rows are filtered live by case-insensitive substring match on the metric name and the title shows the match count ("N of M match \"filter\""); `Enter` accepts the filter, `Esc` clears it. `j` / `k` scroll the table selection. Filtering applies to the curated rows (the display cap stays). If parsing yields no recognised metrics or the endpoint is unavailable, a friendly skip message is shown.

### Help
Full-page keyboard reference and notes reminder.

## Endpoints monitored

- `GET /v1/healthz`
- `GET /v1/readyz`
- `GET /v1/readyz/deep`
- `GET /v1/approvals` (read-only approvals view; paged via `limit`/`offset`, 50 rows shown per page)
- `GET /v1/metrics` (optional Prometheus metrics summary)

## Notes

- **Operator convenience only**: This TUI is a lightweight dashboard for observing endpoint health. It is not an admin tool.
- **Not a security boundary**: The TUI itself does not enforce auth; it forwards the bearer token to the server.
- **No mutation operations**: MVP is read-only. No approve/reject, token rotation, or policy changes via TUI.
- **Token redaction**: The bearer token is never rendered to the terminal surface or logs.
- **Metrics are best-effort**: Prometheus metric parsing is heuristic and may skip metrics it does not recognise.
