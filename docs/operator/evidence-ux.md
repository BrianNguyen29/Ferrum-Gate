# Operational Snapshot UI

> **Parent**: [`guides/README.md`](../guides/README.md)

---

## 1. Overview

This document describes the operator snapshot UI for FerrumGate via CLI (`ferrumctl`) and TUI (`ferrum-tui`).

- **Scope**: Read-only, point-in-time operational evidence collection and display. No mutation operations are exposed through the evidence commands or the TUI.
- **Nature**: Everything produced by these tools is a **point-in-time snapshot or report**, not a sustained certification.
- **Goal**: Give operators a single command to understand system state without writing custom scripts.

---

## 2. Prerequisites

Before using the evidence workflow:

1. A running `ferrumd` instance (local or remote).
2. A valid bearer token if auth mode is `Bearer`.
3. An evidence directory (default: current working directory) for snapshot files.

---

## 3. CLI Evidence Workflow

All commands below are implemented in `bins/ferrumctl/src/main.rs`.

### 3.1 Capture a point-in-time snapshot

```bash
ferrumctl evidence snapshot --output-dir ./evidence/
```

- Connects to the server and aggregates health, deep readiness, audit chain verification, Merkle roots summary, checkpoints summary, pending approvals count, policy bundle summary, intents summary, and metrics summary.
- Writes a timestamped JSON file named `evidence-snapshot-YYYY-MM-DDTHH-MM-SSZ.json`.
- Individual probe failures are captured as errors inside their respective sections; the snapshot itself still succeeds.

### 3.2 Generate a readiness report

```bash
ferrumctl readiness report --snapshot ./evidence/evidence-snapshot-...json --json
```

- Aggregates live server probes (unless `--offline`) with local snapshot.
- Outputs: health, readiness, deep readiness, functional readiness, metrics summary, evidence snapshot, and an `overall` assessment.

### 3.3 Verify audit chain integrity

```bash
ferrumctl admin audit verify
```

- Calls `GET /v1/admin/audit/verify` and reports whether the audit log hash chain is valid.
- Output formats: text (default) or `--format json`.

---

## 4. TUI Evidence Readiness View

The TUI is implemented in `bins/ferrum-tui/src/main.rs`, `app.rs`, and `client.rs`.

### 4.1 Launch

```bash
# Live mode
ferrum-tui --server-url http://127.0.0.1:8080 --evidence-dir ./evidence/

# Dry-run mode (synthetic OKs, no HTTP calls)
ferrum-tui --dry-run
```

Environment variables:

| Variable | Purpose | Fallback |
|----------|---------|----------|
| `FERRUM_TUI_SERVER_URL` | Base URL | `FERRUMCTL_SERVER_URL`, then `http://127.0.0.1:8080` |
| `FERRUM_TUI_BEARER_TOKEN` | Auth token | `FERRUMCTL_BEARER_TOKEN` |
| `FERRUM_TUI_EVIDENCE_DIR` | Evidence snapshot directory | `.` |

### 4.2 Overview tab

The Overview tab displays:

- **Last audit verify**: valid/invalid/error/unauthorized with entry counts.
- **Latest snapshot**: timestamp and filename of the most recent `evidence-snapshot-*.json`.
- **Operational errors**: list of current errors (probe errors, API errors, local evidence read errors).
- **Endpoint status table**: Health, Readiness, Readiness Deep with status badge, latency, and path.

### 4.3 Readiness card states

The top summary bar shows a **Readiness** badge with three possible states:

| State | Condition | Color |
|-------|-----------|-------|
| `DRY-RUN` | `--dry-run` is active | Cyan |
| `BLOCKED` | One or more errors detected | Red |
| `HEALTHY` | No errors, not in dry-run | Green |

> `HEALTHY` means "all probes pass at this point in time" and **does not** imply any external certification.

### 4.4 Read-only behavior

- The TUI does **not** perform mutations. No approvals can be resolved, no tokens can be created or revoked, and no policies can be changed from the TUI.
- It is a **read-only convenience** for operators.

### 4.5 Data sources

- **Live API**: `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/approvals`, `/v1/metrics`, `/v1/admin/audit/verify`.
- **Local files**: `evidence-snapshot-*.json` from the configured directory.

### 4.6 Keyboard shortcuts

| Key | Action |
|-----|--------|
| `Tab` / `→` | Next tab |
| `Shift+Tab` / `←` | Previous tab |
| `1` | Overview tab |
| `2` | Approvals tab |
| `3` | Metrics tab |
| `4` | Help tab |
| `a` | Jump to Approvals |
| `r` | Refresh now |
| `?` / `h` | Toggle help overlay |
| `q` | Quit |

---

## 5. Demo Flow

End-to-end operator workflow from snapshot to TUI:

```bash
# 1. Ensure ferrumd is running and you have a bearer token
export FERRUMCTL_BEARER_TOKEN="<your-token>"
export FERRUMCTL_SERVER_URL="http://127.0.0.1:8080"

# 2. Capture a point-in-time evidence snapshot
ferrumctl evidence snapshot --output-dir ./evidence/
# Output: ./evidence/evidence-snapshot-2026-05-29T12-00-00Z.json

# 3. Open the TUI to monitor in real time
ferrum-tui --server-url http://127.0.0.1:8080 --evidence-dir ./evidence/
# Observe Overview tab: latest snapshot, endpoint status.
# Readiness card will show HEALTHY if all probes pass, or BLOCKED if errors exist.
# Press 'q' to quit.

# 4. Generate a readiness report
ferrumctl readiness report --snapshot ./evidence/evidence-snapshot-...json --json
# Report includes live probes + local snapshot.

# 5. Verify audit chain integrity independently
ferrumctl admin audit verify
# Output: Audit chain verification: VALID (or INVALID with details)
```

---

## 6. References

- [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) — Runtime configuration notes
- [`docs/guides/operator.md`](../guides/operator.md) — General operator procedures
- `bins/ferrumctl/src/main.rs` — CLI commands and report generation
- `bins/ferrum-tui/src/main.rs` — TUI entry point, local evidence readers, CLI args
- `bins/ferrum-tui/src/app.rs` — TUI rendering, Overview tab, readiness card, operational errors
- `bins/ferrum-tui/src/client.rs` — TUI HTTP client, audit verify call

---

*End of operational snapshot UI document.*
