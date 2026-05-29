# Phase 7 Operator Evidence UX

> **Status**: Phase 7 documentation artifact
> **Owner**: Dev
> **Last updated**: 2026-05-29
> **Parent**: [`docs/plan.md`](../plan.md)

---

## 1. Overview

Phase 7 provides the operator evidence UX for FerrumGate via CLI (`ferrumctl`) and TUI (`ferrum-tui`).

- **Scope**: Read-only, point-in-time operational evidence collection and display. No mutation operations are exposed through the evidence commands or the TUI.
- **Nature**: Everything produced by these tools is a **point-in-time snapshot or report**, not a sustained certification.
- **Goal**: Give operators a single command to understand system readiness state without writing custom scripts.

---

## 2. Prerequisites

Before using the evidence workflow:

1. A running `ferrumd` instance (local or remote).
2. A valid bearer token if auth mode is `Bearer`.
3. An evidence directory (default: current working directory) for snapshot files.
4. Optional: an SLO window directory (default: current working directory) for `slo-window-state.json`.

---

## 3. Non-claims

The following non-claims are hard-coded into all evidence outputs and must not be interpreted as readiness certification.

| Boundary | Status |
|----------|--------|
| `production-ready` | **NO** |
| `Tier 2` | **NOT COMPLETE** |
| `Sustained SLO` | **NOT COMPLETE** |
| `HA-4 automated failover` | **NOT COMPLETE** |
| `Block A` | **WAIVED / CONDITIONAL** |

Additional required exact qualifiers:

- `production-ready = NO`
- `Tier 2 = NOT COMPLETE`
- `Sustained SLO = NOT COMPLETE`
- `HA-4 automated failover = NOT COMPLETE`
- `Block A = WAIVED / CONDITIONAL`
- `domainless production-candidate only`
- A point-in-time snapshot or report is **not** production-ready, Tier 2, GA, compliance, or SLO proof.

> These exact strings appear in CLI output, JSON reports, TUI help text, and snapshot metadata. Any document or dashboard that consumes these artifacts must preserve the qualifiers.

---

## 4. CLI Evidence Workflow

All commands below are implemented in `bins/ferrumctl/src/main.rs`.

### 4.1 Capture a point-in-time snapshot

```bash
ferrumctl evidence snapshot --output-dir ./evidence/
```

- Connects to the server and aggregates health, deep readiness, audit chain verification, Merkle roots summary, checkpoints summary, pending approvals count, policy bundle summary, intents summary, and metrics summary.
- Writes a timestamped JSON file named `evidence-snapshot-YYYY-MM-DDTHH-MM-SSZ.json`.
- Individual probe failures are captured as errors inside their respective sections; the snapshot itself still succeeds.
- Each snapshot includes `non_claims_reference: docs/security/non-claims.md` and the exact non-claims notice.

### 4.2 Manage an SLO evidence window

```bash
# Start a new window
ferrumctl evidence slo-window start --window-dir ./evidence/ --notes "Pilot window"

# Check status
ferrumctl evidence slo-window status --window-dir ./evidence/ --json

# Finalize (rejects before 7 days unless --allow-early)
ferrumctl evidence slo-window finalize --window-dir ./evidence/ --notes "Pilot concluded"
```

- The window state is stored locally in `slo-window-state.json`.
- Default target duration: 30 days. Minimum duration: 7 days.
- Starting a window when an active one already exists is rejected.
- Finalizing an already-finalized window is idempotent.
- The state file includes the exact non-claims notice.

### 4.3 Generate a readiness report

```bash
ferrumctl readiness report --snapshot ./evidence/evidence-snapshot-...json --window-dir ./evidence/ --json
```

- Aggregates live server probes (unless `--offline`) with local snapshot and SLO window state.
- Outputs: health, readiness, deep readiness, functional readiness, metrics summary, SLO window, evidence snapshot, and an `overall` assessment.
- The `overall` assessment hardcodes:
  - `label: "Cautious / Point-in-time only"`
  - `production_ready: "NO"`
  - `tier_2: "NOT COMPLETE"`
  - `ha4_automated_failover: "NOT COMPLETE"`
  - `sustained_slo: "NOT COMPLETE"`

### 4.4 Verify audit chain integrity

```bash
ferrumctl admin audit verify
```

- Calls `GET /v1/admin/audit/verify` and reports whether the audit log hash chain is valid.
- Output formats: text (default) or `--format json`.

---

## 5. TUI Evidence Readiness View

The TUI is implemented in `bins/ferrum-tui/src/main.rs`, `app.rs`, and `client.rs`.

### 5.1 Launch

```bash
# Live mode
ferrum-tui --server-url http://127.0.0.1:8080 --evidence-dir ./evidence/ --window-dir ./evidence/

# Dry-run mode (synthetic OKs, no HTTP calls)
ferrum-tui --dry-run
```

Environment variables:

| Variable | Purpose | Fallback |
|----------|---------|----------|
| `FERRUM_TUI_SERVER_URL` | Base URL | `FERRUMCTL_SERVER_URL`, then `http://127.0.0.1:8080` |
| `FERRUM_TUI_BEARER_TOKEN` | Auth token | `FERRUMCTL_BEARER_TOKEN` |
| `FERRUM_TUI_WINDOW_DIR` | SLO window state directory | `.` |
| `FERRUM_TUI_EVIDENCE_DIR` | Evidence snapshot directory | `.` |

### 5.2 Overview tab

The Overview tab displays:

- **Non-claims block**: `production-ready = NO`, `Tier 2 = NOT COMPLETE`, `sustained SLO = NOT COMPLETE`, `HA-4 = NOT COMPLETE`.
- **SLO window state**: window ID, status, elapsed days, target days.
- **Last audit verify**: valid/invalid/error/unauthorized with entry counts.
- **Latest snapshot**: timestamp and filename of the most recent `evidence-snapshot-*.json`.
- **Readiness blockers**: list of operational blockers (probe errors, API errors, local evidence read errors).
- **Endpoint status table**: Health, Readiness, Readiness Deep with status badge, latency, and path.

### 5.3 Readiness card states

The top summary bar shows a **Readiness** badge with three possible states:

| State | Condition | Color |
|-------|-----------|-------|
| `DRY-RUN` | `--dry-run` is active | Cyan |
| `BLOCKED` | One or more errors/blockers detected | Red |
| `RC-READY` | No errors, not in dry-run | Yellow |

> `RC-READY` means "release-candidate ready at this point in time" and **does not** imply production-ready, Tier 2, or sustained SLO completion.

### 5.4 Read-only behavior

- The TUI does **not** perform mutations. No approvals can be resolved, no tokens can be created or revoked, and no policies can be changed from the TUI.
- It is a **read-only convenience** for operators.

### 5.5 Data sources

- **Live API**: `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/approvals`, `/v1/metrics`, `/v1/admin/audit/verify`.
- **Local files**: `slo-window-state.json` and `evidence-snapshot-*.json` from the configured directories.

### 5.6 Keyboard shortcuts

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

## 6. Demo Flow

End-to-end operator workflow from snapshot to TUI to finalize:

```bash
# 1. Ensure ferrumd is running and you have a bearer token
export FERRUMCTL_BEARER_TOKEN="<your-token>"
export FERRUMCTL_SERVER_URL="http://127.0.0.1:8080"

# 2. Capture a point-in-time evidence snapshot
ferrumctl evidence snapshot --output-dir ./evidence/
# Output: ./evidence/evidence-snapshot-2026-05-29T12-00-00Z.json

# 3. Start an SLO evidence window
ferrumctl evidence slo-window start --window-dir ./evidence/ --notes "Phase 7 pilot"
# Output: SLO window started: slo-window-20260529T120000Z

# 4. Open the TUI to monitor in real time
ferrum-tui --server-url http://127.0.0.1:8080 --evidence-dir ./evidence/ --window-dir ./evidence/
# Observe Overview tab: non-claims, SLO window, latest snapshot, endpoint status.
# Readiness card will show RC-READY if all probes pass, or BLOCKED if errors exist.
# Press 'q' to quit.

# 5. Generate a readiness report (can be done while the window is active)
ferrumctl readiness report --window-dir ./evidence/ --json
# Report includes live probes + local SLO window + latest snapshot.
# overall.production_ready is always "NO".

# 6. After the observation period (minimum 7 days), finalize the window
ferrumctl evidence slo-window finalize --window-dir ./evidence/ --notes "Pilot concluded"
# Rejects early finalization unless --allow-early is passed.

# 7. Verify audit chain integrity independently
ferrumctl admin audit verify
# Output: Audit chain verification: VALID (or INVALID with details)
```

---

## 7. Phase 7 Closure Checklist

| Item | Status | Evidence |
|------|--------|----------|
| 7.1 `ferrumctl evidence snapshot` | Complete | `bins/ferrumctl/src/main.rs` `run_evidence_snapshot` |
| 7.2 `ferrumctl evidence slo-window start/status/finalize` | Complete | `bins/ferrumctl/src/main.rs` `SloWindowState` + lifecycle |
| 7.3 `ferrumctl readiness report` | Complete | `bins/ferrumctl/src/main.rs` `build_readiness_report` |
| 7.4 TUI evidence readiness view | Complete | `bins/ferrum-tui/src/app.rs` Overview + Readiness card |
| 7.5 `docs/operator/evidence-ux.md` | Complete | This document |

> **Important**: Phase 7 closure means the CLI/TUI evidence workflow is implemented and documented. It does **not** mean FerrumGate is production-ready, Tier 2 complete, or that a sustained SLO window has been achieved. See [`docs/security/non-claims.md`](../security/non-claims.md) for the canonical readiness boundaries.

---

## 8. References

- [`docs/plan.md`](../plan.md) — Strategic execution checklist, Phase 7
- [`docs/security/non-claims.md`](../security/non-claims.md) — Canonical non-claims and readiness boundaries
- [`docs/guides/operator.md`](../guides/operator.md) — General operator procedures
- `bins/ferrumctl/src/main.rs` — CLI commands and report generation
- `bins/ferrum-tui/src/main.rs` — TUI entry point, local evidence readers, CLI args
- `bins/ferrum-tui/src/app.rs` — TUI rendering, Overview tab, readiness card, blockers
- `bins/ferrum-tui/src/client.rs` — TUI HTTP client, audit verify call

---

*End of Phase 7 evidence UX document.*
