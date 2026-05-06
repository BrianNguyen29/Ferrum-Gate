# 67 — Production-Readiness Roadmap

> **Status**: In-tree documentation. Bounded todo list for reaching RC-ready/conditional posture.
> **Purpose**: Durable, complete roadmap of pre-production items with priorities, blockers, owners, evidence, and non-claims.
> **Scope**: Single-node SQLite v1 only. No PostgreSQL/multi-node/HA. No production-ready claim.
> **Constraint**: Do not claim G2 complete, do not sign doc59/doc54, do not authorize pilot.

---

## Purpose

This document is the authoritative production-readiness roadmap for FerrumGate v1 single-node SQLite.
It consolidates all pre-production blockers, hardening items, and operational readiness gaps into a
single prioritized list with owners, evidence requirements, and explicit non-claims.

**This is NOT a plan to reach "production-ready."** FerrumGate v1 is RC-ready/conditional.
Reaching full production posture requires operator signoff (Path 2 G2 gates) and optional
Phase 3 PostgreSQL (Path 3) — both are outside the scope of this roadmap.

---

## Explicit Non-Claims

- **No production-ready claim**: This roadmap does not make FerrumGate "production-ready."
  FerrumGate v1 is RC-ready/conditional. Full production posture requires Path 2 operator
  signoff and optional Phase 3 PostgreSQL.
- **No G2 complete**: G2.1–G2.8 remain pending until operator signs `59-pilot-readiness-evidence-packet.md`
  and `54-operator-signoff-packet.md`.
- **No pilot authorized**: Pilot is not authorized until doc 54 is signed.
- **No PostgreSQL**: PostgreSQL/multi-node/HA is Path 3 — not in scope for Phase 1.
- **No target-host evidence**: All target-host execution evidence (D1–D6 drills, restore drill,
  probe evidence) requires operator action on target environment.
- **Do not sign doc59/doc54 on behalf of operator**: All signature fields remain blank.

---

## Priority Definitions

| Priority | Meaning |
|----------|---------|
| **P0** | Must fix before any production pilot. Blocking item that prevents bounded RC use. |
| **P1** | Should fix before production pilot. Affects operational posture or safety. |
| **P2** | Fix before production pilot if practical. Improves operability. |
| **P3** | Post-pilot or deferred. Not a pilot blocker. |

---

## P0 — Must-Fix Before Any Production Pilot

| # | Item | Owner | Evidence Required | Status |
|---|---|---|---|---|
| P0.1 | **CI must not swallow cargo check** | Engineering | CI pipeline runs `cargo check --workspace` without `\|\| true` | ✅ Done (CI hardened 2026-05-03) |
| P0.1b | **CI dependency scanning deferred** | N/A | Security scanning (cargo-deny, cargo-audit) not in CI due to cost; local/manual alternatives documented in `70-security-hardening-local-only-plan.md` | ✅ Done (doc only) |
| P0.2 | **Target-host execution evidence missing** | Operator | D1–D6 drill evidence on target host; `readyz/deep` returns HTTP 200 on target | ☐ Pending (operator-owned) |
| P0.3 | **Restore drill not executed on target** | Operator | Restore drill log with `PRAGMA integrity_check` passing on target | ☐ Pending (operator-owned) |
| P0.4 | **Backup automation not configured** | Operator | External scheduler (cron/systemd timer) configured; `ferrumctl backup verify` passes | ☐ Pending (operator-owned) |
| P0.5 | **G2.1–G2.8 not signed** | Operator | `59-pilot-readiness-evidence-packet.md` G2.1–G2.8 filled and signed | ☐ Pending (operator-owned) |
| P0.6 | **Operator signoff not obtained** | Operator | `54-operator-signoff-packet.md` signed | ☐ Pending (operator-owned) |

### P0 Notes

- P0.1 is a repo-side blocker fixed by CI hardening (2026-05-03).
- P0.2–P0.6 are **operator-owned** and cannot be completed by the engineering team.
  They require operator action on the target host.
- See [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) §Step 1–5 for the ordered
  execution checklist.
- See [`66-path-2-operator-handoff.md`](./66-path-2-operator-handoff.md) §Phase B for blockers.

---

## P1 — Should-Fix Before Production Pilot

| # | Item | Owner | Evidence Required | Status |
|---|---|---|---|---|
| P1.1 | **Readiness semantics: `/v1/readyz/deep` functional probe** | Engineering | Load balancers and Kubernetes should use `/v1/readyz/deep` as functional readiness probe; `/v1/healthz` and `/v1/readyz` are shallow and always return 200 | ✅ Done — documented in `PRODUCTION_NOTES.md` §Health and Readiness Endpoints; `/v1/readyz/deep` returns 200 when store healthy and write queue depth <= 100, 503 when store unhealthy or write queue depth > 100 |
| P1.2 | **Configurable rate limit** | Engineering | Rate limit configurable via CLI/env/config file (2 req/s, burst 50 default); operator confirms fit for target workload | ✅ Done — CLI: `--rate-limit-per-second` and `--rate-limit-burst`; env: `FERRUMD_RATE_LIMIT_PER_SECOND` and `FERRUMD_RATE_LIMIT_BURST`; config file: `rate_limit_per_second` and `rate_limit_burst` under `[server]` |
| P1.3 | **Structured logging (JSON)** | Engineering | Logs are unstructured text; production debugging and log aggregation benefit from JSON structured output | ✅ Done — CLI: `--log-format`; env: `FERRUMD_LOG_FORMAT`; config file: `log_format` under `[server]`; default is "text" (human-readable); accepted values: "text", "compact", "json"; documented in `PRODUCTION_NOTES.md` |
| P1.4 | **Full metrics/observability** | Engineering | `/v1/metrics` with method labels on request/governance counters and latency histograms for public endpoints | ✅ Done — `/v1/metrics` provides: request counters per endpoint with HTTP method labels (healthz, readyz, readyz/deep, metrics), bounded HTTP status labels for public endpoints (status="200" for healthz/readyz/metrics; status="200"/"503" for readyz/deep), store health gauge (`ferrumgate_store_health_up`), SQLite write queue depth gauge (`ferrumgate_write_queue_depth`), governance error counters per route with HTTP method labels (26 routes), governance success counters per route with HTTP method labels (26 routes), and latency histogram (`ferrumgate_request_duration_seconds`) for public endpoints with bounded labels (route, method, status, le) emitting _bucket/_sum/_count lines |
| P1.5 | **RPO/RTO formally accepted** | Operator | Backup/restore objectives formally accepted per `27-production-evaluation-plan.md` §Operator Signoff Packet §3 | ☐ Pending (operator-owned) |
| P1.6 | **Compensate noop risk accepted** | Operator | Operator acknowledges compensate may be noop-backed for target adapters per G2.8 | ☐ Pending (operator-owned) |

### P1 Notes

- P1.1–P1.4 are engineering items. P1.5–P1.6 are operator-owned.
- P1.2: Rate limiting is built-in via `tower_governor` with per-IP enforcement. Configurable via
  CLI flags (`--rate-limit-per-second`, `--rate-limit-burst`), environment variables
  (`FERRUMD_RATE_LIMIT_PER_SECOND`, `FERRUMD_RATE_LIMIT_BURST`), or config file fields
  (`rate_limit_per_second`, `rate_limit_burst` under `[server]`). Defaults remain 2 req/s and burst 50.
  Validation rejects 0 and values >10000 for burst. CLI > env > config file > defaults precedence.
- P1.1: `/v1/readyz/deep` is the functional readiness probe. Returns HTTP 200 when store is healthy
  AND write queue depth <= 100; returns HTTP 503 when store is unhealthy OR write queue depth > 100.
  Use for load balancers and Kubernetes readiness probes.
  `/v1/healthz` and `/v1/readyz` are shallow checks — always return 200, do NOT check store health.
- P1.3: Configurable log format via CLI (`--log-format`), env (`FERRUMD_LOG_FORMAT`), or config file
  (`log_format` under `[server]`). Default is "text" (human-readable). Accepted values: "text",
  "compact" (both are the same human-readable format), "json" (structured JSON for log aggregation).
  Config precedence: CLI > env > config file > defaults. Documented in `PRODUCTION_NOTES.md`.
- P1.4: `/v1/metrics` provides: request counters per endpoint with HTTP method labels (healthz,
  readyz, readyz/deep, metrics), bounded HTTP status labels for public endpoints (status="200" for
  healthz/readyz/metrics; status="200"/"503" for readyz/deep based on health),
  `ferrumgate_store_health_up` gauge, `ferrumgate_write_queue_depth` gauge for accepted SQLite write
  operations not yet processed by the writer loop, `ferrumgate_metrics_scrapes_total`,
  `ferrumgate_governance_errors_total` per route with HTTP method labels (26 governance endpoints), and
  `ferrumgate_governance_success_total` per route with HTTP method labels (26 governance endpoints).
  Latency histograms (`ferrumgate_request_duration_seconds`) implemented for public endpoints only
  with bounded labels (route, method, status, le). Governance route latency instrumentation
  is out of scope. HTTP status labels are implemented for public endpoints only
  (bounded change, per-handler instrumentation).
- See [`27-production-evaluation-plan.md`](./27-production-evaluation-plan.md) for the full
  production evaluation framework.

---

## P2 — Fix Before Production Pilot If Practical

| # | Item | Owner | Evidence Required | Status |
|---|---|---|---|---|
| P2.1 | **Adapter hardening beyond bounded slices** | Engineering | Target adapter surface verified for target workload; compensate behavior confirmed | 🟡 Partial — adapters have verified local slices; remaining surface is post-v1 |
| P2.2 | **`ferrumctl` expanded surface** | Engineering | `ferrumctl` includes health/inspect/backup/restore plus list-intents and cancel-execution API coverage | ✅ Done — `GET /v1/intents` implemented for existing `ferrumctl list-intents`; `POST /v1/executions/{execution_id}/cancel` was already implemented and is now documented in OpenAPI |
| P2.3 | **Deep health check** | Engineering | Functional readiness probe documented and operational | ✅ Done — `/v1/readyz/deep` documented in `PRODUCTION_NOTES.md` §Health and Readiness Endpoints; returns 200 when store healthy, 503 when unhealthy |
| P2.4 | **TLS/reverse proxy not configured in-tree** | Operator | TLS termination at reverse proxy (nginx/etc.); ferrumd does not terminate TLS | ☐ Pending (operator-owned) |
| P2.5 | **Bearer token not generated** | Operator | Real bearer token generated by operator via `openssl rand -hex 32` | ☐ Pending (operator-owned) |

### P2 Notes

- P2.1–P2.3 are engineering items. P2.4–P2.5 are operator-owned.
- P2.1: Adapters have verified local slices (fs: 146 tests, git: 86 tests, http: 103 tests,
  sqlite: 16 tests, maildraft: 16 tests). Remaining surface (permissions/symlinks for fs,
  remote push/pull for git, broader replay for http) is post-v1 scope.
- P2.2: `GET /v1/intents` supports `intent_id`, repeated `state`, `cursor`, and `limit`
  parameters and returns the JSON shape expected by `ferrumctl list-intents`. `exec_state` is
  populated from the latest execution state when one exists and remains `null` when no execution
  exists. `POST /v1/executions/{execution_id}/cancel`
  remains available via `ferrumctl cancel-execution --confirm` and is documented in OpenAPI.
- P2.4: FerrumGate v1 does not include TLS termination. Deploy behind a TLS-terminating
  reverse proxy. Example nginx config in `configs/examples/nginx-ferrumgate.conf`.
- See [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md)
  for v1 support boundaries.

---

## P3 — Post-Pilot / Deferred

| # | Item | Owner | Status | Notes |
|---|---|---|---|---|
| P3.1 | **PostgreSQL implementation** | Engineering | ☐ Pending | Path 3; ADR-50 Phase P1–P4; ~2000–3000 LOC + migrations + container tests |
| P3.2 | **Multi-node / HA / read-replica** | Engineering | ☐ Pending | Not implemented; out of v1 scope |
| P3.3 | **Target-host execution beyond local slices** | Operator | ☐ Pending | D1–D6 drills require operator execution on target host |
| P3.4 | **Phase 2 transaction batching** | Engineering | ✅ Reverted | Benchmark regression; Phase 1 write queue remains production target |
| P3.5 | **Outcome-aware Governance (U1)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |
| P3.6 | **Reversible Execution Planner (U2)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |
| P3.7 | **Cross-runtime Provenance Fabric (U3)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |
| P3.8 | **Runtime Integrations — MCP/local/NemoClaw (U4)** | Engineering | ✅ Done (post-v1) | Implemented but outside v1 single-node support baseline |

### P3 Notes

- P3.1–P3.2 are Phase 3 / Path 3 items. They are blocked until G2.1–G2.8 are signed
  and Phase 3 go/no-go gates (G3.1–G3.4) are satisfied.
- P3.3 is operator-owned target execution evidence.
- P3.5–P3.8 are implemented but explicitly outside the v1 single-node support baseline.
  They do not contribute to the production-ready claim.
- See [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 3 for G3 gates.

---

## Consolidated Blocker Summary

| Blocker | Type | Owner | Resolution |
|---------|------|-------|------------|
| CI swallows cargo check | Repo-side | Engineering | ✅ Fixed — CI now runs fmt/check/clippy/test without `\|\| true` |
| Readiness probe semantics undocumented | Engineering | Engineering | ✅ Fixed — `/v1/readyz/deep` documented as functional probe in `PRODUCTION_NOTES.md` |
| Target-host execution evidence | Operator | Operator | Complete D1–D6 drills on target host |
| G2.1–G2.8 not signed | Operator | Operator | Fill and sign `59-pilot-readiness-evidence-packet.md` |
| Operator signoff not obtained | Operator | Operator | Sign `54-operator-signoff-packet.md` |
| Backup automation not configured | Operator | Operator | Configure external scheduler (cron/systemd timer) |
| Restore drill not executed | Operator | Operator | Execute non-prod restore drill per `61-path-2-execution-plan.md` §3 |
| TLS/reverse proxy not configured | Operator | Operator | Deploy behind TLS-terminating reverse proxy |
| PostgreSQL not implemented | Out of scope | N/A | Path 3; blocked until G2 complete |

---

## Cross-Reference Index

| From | To | Purpose |
|---|---|---|
| This doc | [`31-release-paths-todo.md`](./31-release-paths-todo.md) | Path 2 G2 gates and checklists |
| This doc | [`66-path-2-operator-handoff.md`](./66-path-2-operator-handoff.md) | Phase A/B handoff; operator-owned blockers |
| This doc | [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Ordered execution checklist |
| This doc | [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md) | G2.1–G2.8 evidence packet |
| This doc | [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) | Operator signoff form |
| This doc | [`27-production-evaluation-plan.md`](./27-production-evaluation-plan.md) | Production evaluation framework |
| This doc | [`19-v1-single-node-support-contract.md`](../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md) | v1 support boundaries and constraints |
| This doc | [`30-production-roadmap.md`](./30-production-roadmap.md) | Phase 1/2/3 production roadmap |
| This doc | [`PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) | SQLite configuration and stress test baseline |
| This doc | [`70-security-hardening-local-only-plan.md`](./70-security-hardening-local-only-plan.md) | Security hardening proposals, local-only audit commands, token rotation procedure |
| This doc | [`71-mcp-server-feasibility-and-design.md`](./71-mcp-server-feasibility-and-design.md) | MCP server design and todo-list (post-v1 scope; v1.4 MCP Governance Beta; U4 bridge exists, MCP server is next step) |

---

## Evidence Sources

| Check | Command | Pass Criteria |
|-------|---------|---------------|
| CI hardening | `.github/workflows/ci.yml` | `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all run without `\|\| true` |
| Local pretarget gate | `bash scripts/run_pre_target_gate.sh` | All checks pass; Tier 0 smoke validation |
| Layout validation | `bash scripts/validate_repo_layout.sh` | "Repository layout looks OK" |
| Contract consistency | `python3 scripts/check_contract_consistency.py` | "VALIDATION PASSED" |

---

*Document created: 2026-05-03. Production-readiness roadmap — no production-ready claim, no G2 complete, no operator signature pre-populated.*

*Next update: Operator-owned items remain pending until target execution evidence is provided. Engineering items tracked in `11-remaining-tasks.md`.*
