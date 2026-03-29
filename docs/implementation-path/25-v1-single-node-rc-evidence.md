# 25 — v1 Single-Node RC Evidence

**Branch:** `docs/v1-single-node-scope-freeze`
**Date:** 2026-03-28
**Scope:** Single-node only (v1 scope freeze)

## RC Status: READY TO CLOSE

All release checklist gates pass. Single-node v1 is ready for RC sign-off.

---

## 1. Release Checklist Gate Status

### PASSED — All Items Green

| Check | Command | Result |
|-------|---------|--------|
| Contract consistency | `python3 scripts/check_contract_consistency.py` | VALIDATION PASSED |
| Cargo check | `cargo check --workspace` | all crates compile |
| Cargo fmt | `cargo fmt --all --check` | No formatting differences |
| Cargo clippy | `cargo clippy --workspace -- -D warnings` | passed |
| Cargo test | `cargo test --workspace` | passed (all tests pass) |

---

## 2. Smoke Test Evidence

### 2.1 Startup Guard Preflight

```
startup_guard: ok
```
Verified with production config; startup guard preflight passes.

### 2.2 Server Startup

```
ferrumd listening on 127.0.0.1:18080
```
Local file-backed smoke server starts and binds successfully.

### 2.3 Readiness Endpoint

```
GET /v1/readyz => 200 OK
Body: {"status":"ready"}
```

### 2.4 Metrics Endpoint — Auth Enforcement

```
GET /metrics (no bearer) => 401 Unauthorized
Body: PolicyDenied

GET /metrics with Authorization: Bearer rc-token => 200 OK
Body: Prometheus text format metrics
```

### 2.5 SQLite Backup / Integrity

```
SQLite backup + integrity check via Python sqlite3 module => ok
```

**Note:** The local `sqlite3` CLI is not installed in this environment. The
backup and integrity verification were performed using Python's stdlib
`sqlite3` module rather than the `sqlite3` CLI form documented in the
runbook. The operational procedure is equivalent; only the tooling differs.

---

## 3. What IS Working (Positive Evidence)

### 3.1 Contract Integrity
`python3 scripts/check_contract_consistency.py` => `VALIDATION PASSED`

### 3.2 Workspace Compilation
`cargo check --workspace` => all crates compile successfully

### 3.3 Code Formatting
`cargo fmt --all --check` => no formatting diffs

### 3.4 Clippy
`cargo clippy --workspace -- -D warnings` => passed (0 errors; previously 11 mechanical errors in ferrum-ledger and ferrumctl, all fixed).

### 3.5 Tests
`cargo test --workspace` => all tests pass (previously 3 missing-field errors in integration_capability_restart.rs, all fixed)

### 3.6 Startup Guard
`startup_guard: ok` on production config

### 3.7 Smoke Server
`ferrumd listening on 127.0.0.1:18080` — local file-backed server starts cleanly

### 3.8 Readiness Endpoint
`GET /v1/readyz` => `200 {"status":"ready"}`

### 3.9 Metrics Auth
`GET /metrics` without bearer => `401 PolicyDenied`
`GET /metrics` with `Authorization: Bearer rc-token` => `200 Prometheus text`

### 3.10 SQLite Backup/Integrity
Python `sqlite3` module backup and integrity check => `ok`

---

## 4. v1 Non-Goals (Not Blockers)

The following are explicitly out of scope for v1 and are NOT blockers:

| Item | Reason |
|------|--------|
| Multi-node sync write-path | Out of scope; Sync-3a probe done; write-path P3 |
| HA / multi-leader | Out of scope; single-node only |
| In-process TLS | Out of scope; external terminator required |
| Distributed trace context | Out of scope; single-node v1 |
| Alerting rules | Out of scope; P2 exploratory |
| Generic provenance replay fabric | Out of scope; core query surface done |

Source: `docs/16-release-checklist.md` lines 3-14, `docs/implementation-path/23-production-readiness-assessment.md` Section 4

---

## 5. Verdict

**Single-node v1 RC: READY TO CLOSE**

All release checklist items pass:
1. `cargo clippy --workspace -- -D warnings` -- passed (0 errors; 11 mechanical errors previously fixed)
2. `cargo test --workspace` -- passed (3 missing-field errors previously fixed)
3. startup guard preflight -- `startup_guard: ok`
4. smoke server -- `ferrumd listening on 127.0.0.1:18080`
5. `/v1/readyz` -- `200 {"status":"ready"}`
6. `/metrics` auth -- `401 PolicyDenied` without bearer; `200 Prometheus text` with `Authorization: Bearer rc-token`
7. SQLite backup/integrity -- `ok` (Python sqlite3 module; sqlite3 CLI not available in environment)

**Recommended next action (owner: orchestrator):**
- Branch is ready to commit and open the final PR for single-node v1 RC sign-off

---

## 6. Cross-References

- Release checklist: `docs/16-release-checklist.md`
- Production readiness: `docs/implementation-path/23-production-readiness-assessment.md`
- Execution plan: `docs/implementation-path/24-p1-p2-p3-execution-plan.md`
- Runbooks: `docs/runbooks/README.md`
