# ferrum-tui

> **Status**: D.2 TUI MVP — operator convenience only; not production-ready; domainless/waiver scope.
> **Scope**: Lightweight terminal dashboard for existing operator endpoints. No mutation operations.

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

# Custom refresh interval (seconds)
./target/release/ferrum-tui --interval 10
```

## Environment variables

| Variable | Purpose | Fallback |
|----------|---------|----------|
| `FERRUM_TUI_SERVER_URL` | Base URL | `FERRUMCTL_SERVER_URL` → `http://127.0.0.1:8080` |
| `FERRUM_TUI_BEARER_TOKEN` | Bearer token | `FERRUMCTL_BEARER_TOKEN` → unset |

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

### Actions

| Key | Action |
|-----|--------|
| `r` | Refresh data now |
| `?` / `h` | Toggle help overlay |
| `q` | Quit |

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
Read-only list of pending approvals with state badges, truncation for narrow terminals, and empty-state messaging.

### Metrics
Parses `/v1/metrics` (Prometheus text format) and displays a curated subset of numeric metrics (health, totals, counts, pool stats, latency, etc.). If parsing yields no recognised metrics or the endpoint is unavailable, a friendly skip message is shown.

### Help
Full-page keyboard reference and non-claims reminder.

## Endpoints monitored

- `GET /v1/healthz`
- `GET /v1/readyz`
- `GET /v1/readyz/deep`
- `GET /v1/approvals?limit=20` (read-only approvals view)
- `GET /v1/metrics` (optional Prometheus metrics summary)

## Non-claims

- **Operator convenience only**: This TUI is a lightweight dashboard for observing endpoint health. It is not a production-ready admin tool.
- **Not a security boundary**: The TUI itself does not enforce auth; it forwards the bearer token to the server.
- **No mutation operations**: MVP is read-only. No approve/reject, token rotation, or policy changes via TUI.
- **Token redaction**: The bearer token is never rendered to the terminal surface or logs.
- **Domainless/waiver scope**: Implemented under the same domainless/waiver scope as the rest of the pilot-tier operator UX.
- **Metrics are best-effort**: Prometheus metric parsing is heuristic and may skip metrics it does not recognise.
