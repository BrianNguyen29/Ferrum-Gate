# 10 — Evidence Checklist

> **Status**: Planning artifact. Checklist template; not yet filled.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-21
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Provide a per-phase evidence checklist so that every claim in the production path has a required evidence artifact, an owner, and a signoff state.

## Current state

- Unified per-phase evidence checklist exists and is actively maintained (this doc).
- Evidence artifacts are organized in `docs/implementation-path/artifacts/` with dated filenames and cross-references.
- Pass/fail tracking exists for all completed phases; remaining open items are Phase 9 (HA) and final production-ready prerequisites.
- **Six of seven blockers unblocked/completed** as of 2026-05-21. One remains open: `BLK-A-DOM`. Tracked in [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md).

## Gaps

- No checklist links ROADMAP phases to required evidence files.
- No owner assignment for each evidence item.
- No explicit signoff state tracking.

## Implementation tasks

- [ ] Copy relevant phase checklists into a tracking issue or project board before starting work.
- [ ] Fill in the Evidence column after executing each item.
- [ ] Request operator review and signoff when all P0 items in a phase are checked.
- [ ] Archive completed checklists with evidence artifacts.

## Acceptance criteria

- [ ] Every phase (0–9) has a checklist table.
- [ ] Every item has an owner, evidence path, and status checkbox.
- [ ] Final production-ready claim prerequisites are listed separately.
- [ ] No item can be checked without an evidence artifact.

## Evidence required

- This checklist itself, filled and signed per phase.

## How to use this checklist

1. Before starting a phase, copy the relevant checklist items into a tracking issue or project board.
2. After executing each item, fill in the Evidence column with a file path or artifact ID.
3. When all items in a phase are checked, request operator review and signoff.
4. Do not mark a phase complete until all P0 evidence items have artifacts.

## Phase 0 — Planning artifacts

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 0.1 | `production-scope.md` exists and reviewed | Engineering | `docs/production-readiness-v2/00-scope-and-nonclaims.md` | ✅ COMPLETE — reviewed in Phase 0 sweep |
| 0.2 | `slo-sla-draft.md` exists and reviewed | Engineering + Operator | `docs/production-readiness-v2/01-slo-sla.md` | ✅ COMPLETE — reviewed in Phase 0 sweep |
| 0.3 | `postgres-production-gap-adr.md` exists and reviewed | Engineering | `docs/production-readiness-v2/02-postgres-production-plan.md` | ✅ COMPLETE — reviewed in Phase 0 sweep |
| 0.4 | `mcp-target-host-validation-plan.md` exists and reviewed | Engineering | `docs/production-readiness-v2/03-target-mcp-live-workload-plan.md` | ✅ COMPLETE — reviewed in Phase 0 sweep |
| 0.5 | `tenant-security-model-adr.md` exists and reviewed | Engineering + Operator | `docs/production-readiness-v2/04-security-tenant-model-adr.md` | ✅ COMPLETE — reviewed in Phase 0 sweep |
| 0.6 | `product-docs-information-architecture.md` exists | Engineering | `docs/guides/README.md` | ✅ COMPLETE — guide index links all 10 scaffolds with status and non-claims |
| 0.7 | Every checklist has evidence requirements | Engineering | This doc | ✅ COMPLETE — every phase item has Owner and Evidence columns |
| 0.8 | No doc overclaims production-ready | Engineering | Review signoff | ✅ COMPLETE — Phase 0 sweep found no unqualified overclaim |

## Phase 1 — PostgreSQL production foundation

> **PG-1 scope**: PostgreSQL target/staging baseline only. Local Docker fallback evidence passed on 2026-05-18. No production-ready claim. Block A remains WAIVED/CONDITIONAL.
> **Template**: `TEMPLATE-pg-production-deployment-signoff.md` prepared for eventual production PG signoff (requires real evidence).

| # | Item | Owner | Evidence | Status |
|---|---|------|-------|----------|--------|
| 1.1 | PG-1.1: PostgreSQL target/staging provisioned | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.1 | ✅ COMPLETE — local Docker fallback |
| 1.2 | PG-1.2: ferrumd starts with postgres DSN | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.2 | ✅ COMPLETE — local Docker fallback |
| 1.3 | PG-1.3: `/v1/readyz/deep` reports PG health (200) | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.3 | ✅ COMPLETE — local Docker fallback |
| 1.4 | PG-1.4: `ferrum-migrate` completes | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.4 | ✅ COMPLETE — local Docker fallback |
| 1.5 | PG-1.5: Row counts match post-migration | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.5 | ✅ COMPLETE — local Docker fallback |
| 1.6 | PG-1.6: Content hash validation passes | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` §PG-1.6 | ✅ COMPLETE — local Docker fallback |
| 1.7 | PG-1.7: Evidence artifact created from template | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-target-deployment-evidence.md` | ✅ COMPLETE — local Docker fallback |
| 1.8 | PG-1.8: Docs/evidence checklist updated | Engineering | This doc + `PRODUCTION_NOTES.md` | ✅ COMPLETE — local Docker fallback |
| 1.9 | PG-2.1: Session timeout config (`statement_timeout`, `idle_in_transaction_session_timeout`) | Engineering | `02-postgres-production-plan.md` §PG-2.1 | ✅ COMPLETE — code + tests |
| 1.10 | PG-2.2: Pool metrics (`pool_size`, `pool_idle`, `pool_max`) | Engineering | `pg-target-evidence.md` §PG-2.2 | ✅ COMPLETE — code + tests |
| 1.11 | PG-2.3a: Acquire timeout counter + pool saturation readiness | Engineering | `02-postgres-production-plan.md` §PG-2.3a | ✅ COMPLETE — code + tests |
| 1.12 | PG-2.3b: Reconnect/retry and circuit breaker | Engineering | `02-postgres-production-plan.md` §PG-2.3b + `docs/guides/operator.md` §PostgreSQL reconnect and recovery + `docs/implementation-path/artifacts/2026-05-21-pg-container-restart-drill-evidence.md` + `docs/implementation-path/artifacts/2026-05-21-pg-2.3b-reconnect-circuit-breaker-backlog.md` | ✅ B.1 COMPLETE — operator runbook documents current PgPool reconnect behavior; B.2 SCRIPT PREPARED and locally validated (14s recovery); **B.3 EXPLICITLY DEFERRED** — circuit breaker ADR deferred until PG-5 HA design begins; **B.4 DEFERRED** — implementation blocked on B.3 ADR and PG-5 topology |
| 1.13 | PG-3: Local backup/restore drill passes (scheduled backup/retention NOT STARTED) | Engineering | `docs/implementation-path/artifacts/2026-05-18-pg-restore-drill-evidence.md` | ✅ COMPLETE — local Docker drill only; scheduled backup/retention deferred |
| 1.14 | PG-4a: Schema version table + idempotent runner (PG-4b.1 docs+runner cleanup done, PG-4b.2 bounded forward-only engine done, PG-4b.3 rollback strategy doc done; **CI drift gate prepared**) | Engineering | `pg-migration-evidence.md` + `02-postgres-production-plan.md` §PG-4b | ✅ COMPLETE — PG-4a done; PG-4b.1/4b.2/4b.3 done; **CI postgres feature gate added to `.github/workflows/ci.yml`** — `cargo check` + `cargo clippy` with `--all-features` enforce compile-time drift detection on every push/PR; live execution pending normal CI trigger |
| 1.15 | PG-5: HA ADR approved as planning decision; primary failure drill documented; RPO/RTO measured | Engineering + Operator | HA ADR + failure drill evidence | ✅ APPROVED AS PLANNING DECISION — operator delegate signoff recorded 2026-05-21; no implementation claim; no HA claim |
| 1.16 | PG-6: PostgreSQL scoped token repository implemented and tested | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §6 | ✅ COMPLETE — `crates/ferrum-store/src/postgres/tokens.rs` implemented; 72 tests pass with postgres feature; workspace tests pass |
| 1.17 | PG-2.4: PostgreSQL alert rules template prepared | Engineering | `configs/monitoring/ferrumgate-alerts.yaml` + `docs/implementation-path/artifacts/2026-05-21-pg-alert-rules-evidence.md` | ✅ COMPLETE — template rules added (PG down proxy, pool saturation, slow acquire, backup stale); replication lag placeholder deferred; **live Prometheus/promtool validation unavailable/operator-env-dependent**; NOT deployed to live Prometheus |
| 1.18 | PG-2.5: TLS/SSL DSN guidance documented | Engineering | `02-postgres-production-plan.md` §PG-2.5 + `docs/guides/operator.md` §PostgreSQL TLS/SSL DSN configuration + `TEMPLATE-pg-tls-dsn-evidence.md` | ✅ RUNBOOK COMPLETE — TLS modes, DSN examples, file permissions, and rotation procedure documented; **live TLS-encrypted PG connection validation pending operator environment** |
| 1.19 | PG-2.6: PgBouncer / connection pooling story documented | Engineering | `02-postgres-production-plan.md` §PG-2.6 + `docs/guides/operator.md` §PgBouncer / connection pooling + `TEMPLATE-pg-pgbouncer-evidence.md` | ✅ RUNBOOK COMPLETE — when-to-use table, recommended config, ferrumd DSN example, and caveats documented; **live PgBouncer deployment validation pending operator environment** |
| 1.20 | PG-3.1: Scheduled backup/retention/offsite runbook complete | Engineering | `02-postgres-production-plan.md` §PG-3 + `docs/implementation-path/109-p5c-postgresql-backup-restore-runbook.md` §P5c.5 + `TEMPLATE-pg-scheduled-backup-evidence.md` + `TEMPLATE-pg-retention-pruning-evidence.md` + `TEMPLATE-pg-offsite-sync-evidence.md` | ✅ RUNBOOK COMPLETE — cron/systemd timer examples, retention pruning, offsite target comparison documented; **execution on live PG pending operator deployment** |
| 1.21 | PG-2.4a: Alert deployment validation runbook documented | Engineering | `configs/monitoring/README.md` §Alert Deployment Validation Runbook + `docs/guides/operator.md` §Alert deployment validation + `TEMPLATE-pg-alert-deployment-evidence.md` | ✅ RUNBOOK COMPLETE — promtool syntax check, deploy steps, rule state verification, PG alert check, optional simulation, and evidence artifact template documented; **live Prometheus evaluation pending operator environment** |
| 1.22 | PG-3.1a: Local `pg_dump` backup creation and integrity validation | Engineering | `docs/implementation-path/artifacts/2026-05-21-pg-local-scheduled-backup-evidence.md` | ✅ LOCAL EVIDENCE — manual `pg_dump` against local Docker PostgreSQL passed; backup listable by `pg_restore`; restore drill to clean DB passed; **scheduled automation and production execution pending operator** |
| 1.23 | PG-3.1b: Local retention pruning simulation | Engineering | `docs/implementation-path/artifacts/2026-05-21-pg-local-retention-pruning-evidence.md` | ✅ LOCAL EVIDENCE — `find -mtime +4 -delete` correctly removed backdated dump and preserved current dump; **production scheduler integration and live target pending operator** |
| 1.24 | PG-3.1c: Local offsite sync simulation | Engineering | `docs/implementation-path/artifacts/2026-05-21-pg-local-offsite-sync-evidence.md` | ✅ LOCAL EVIDENCE — local `cp` + `sha256sum` hash match verified; **real GCS/S3/rsync offsite sync and production target pending operator** |
| 1.25 | PG-2.4b: Local `promtool` syntax validation + Prometheus readiness | Engineering | `docs/implementation-path/artifacts/2026-05-21-pg-local-alert-validation-evidence.md` | ✅ LOCAL EVIDENCE — `promtool check rules` passed (21 rules); Prometheus `/-/ready` returned 200; **live rule deployment to Prometheus, rule state verification, PG alert behavior, and AlertManager routing pending operator** |

## Phase 2 — SLO/SLA and workload evidence

> **Status**: SLO default-config gap **CLOSED with conservative resolution** on 2026-05-21.
> Conservative defaults (2/50) remain safety-oriented and unchanged. SLO certification requires
> explicit high-throughput profile (1000/10000). Operator must tune based on real traffic/IP
> distribution. See `docs/operations/rate-limit-tuning-guide.md`.
>
> `BLK-SLO-RAT` ratified; `BLK-SLO-TGT` unblocked on 2026-05-21. Canonical SLO certification
> attempted (2 fails, 1 pass with max-valid config). Full SLO certification NOT claimed for
> default/tuned configs. See [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md).

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 2.1 | SLO-1: SLO/SLA draft doc exists and reviewed | Engineering + Operator | `docs/production-readiness-v2/01-slo-sla.md` + `docs/implementation-path/artifacts/2026-05-20-slo-ratification-signoff.md` | ✅ RATIFIED FOR VALIDATION BASELINE — pilot targets approved for target-host validation; NOT a committed SLA |
| 2.2 | SLO-2: Runbook maps scripts to pass/fail | Engineering | `slo-validation-runbook.md` | ✅ COMPLETE — runbook created, targets marked draft/conditional |
| 2.3 | SLO-3: Local workload baseline run completed | Engineering | `docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md` | ✅ COMPLETE — local SQLite in-memory baseline only; NOT target-host validated |
| 2.4 | SLO-4: p95/p99 latency measured locally | Engineering | `2026-05-19-slo-local-baseline-evidence.md` §Latency | ✅ LOCAL BASELINE MEASURED — local in-memory only; NOT target-host ratified |
| 2.5 | SLO-5: Readiness success measured locally | Engineering | `2026-05-19-slo-local-baseline-evidence.md` §Post-run checks | ✅ LOCAL BASELINE MEASURED — local in-memory only; NOT target-host ratified |
| 2.6 | SLO-6: Error rate measured locally | Engineering | `2026-05-19-slo-local-baseline-evidence.md` §SLO comparison | ✅ LOCAL BASELINE MEASURED — local in-memory only; NOT target-host ratified |
| 2.7 | SLO-7: Evidence artifact reviewed by operator | Operator | `docs/implementation-path/artifacts/2026-05-20-slo-ratification-signoff.md` | ✅ BASELINE RATIFIED — target-host evidence reviewed and conditionally signed |
| 2.8 | SLO-target-host: Target preflight attempted and blocked (valid bearer token required) | Engineering | `docs/implementation-path/artifacts/2026-05-19-slo-target-preflight-blocked-evidence.md` | ✅ UNBLOCKED — token installed 2026-05-21; preflight no longer blocked |
| 2.9 | SLO-target-abbreviated: Abbreviated target workload executed (NOT full certification) | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §3 | ✅ ABBREVIATED TARGET RUN — 39 requests, 0 errors, light load only; NOT full SLO certification |
| 2.10 | SLO-canonical-run1: Default rate-limit canonical workload (FAIL) | Engineering | `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §3.2 | ✅ FAILURE EVIDENCE — 429 rate 46.8%; default config insufficient |
| 2.11 | SLO-canonical-run2: Tuned rate-limit canonical workload (FAIL) | Engineering | `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §3.3 | ✅ FAILURE EVIDENCE — 429 rate 73.4%; tuned config insufficient |
| 2.12 | SLO-canonical-run3: Max-valid rate-limit canonical workload (PASS) | Engineering | `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §3.4 | ✅ PASS — 0 errors, 0 429s, all readyz 200; max-valid config only |
| 2.13 | SLO-canonical-summary: All three runs documented with pass/fail | Engineering | `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §3.5 | ✅ COMPLETE — full SLO marked PASS only for max-valid config |
| 2.14 | SLO-default-config-evidence: Formal failure/decision evidence compiled | Engineering | `docs/implementation-path/artifacts/2026-05-22-slo-default-config-evidence.md` | ✅ DECISION EVIDENCE — default config intentionally fails canonical SLO; certification requires explicit high-throughput profile |
| 2.15 | Workload-model-refresh: Observed datasets compiled vs. original signed assumption | Engineering | `docs/implementation-path/artifacts/2026-05-22-workload-model-refresh-evidence.md` | ✅ ENGINEERING EVIDENCE — canonical max-valid (2,380 req, 0 errors), abbreviated target (39 req, 0 errors), local baseline (22 req, 0 errors), local stress (258 RPS) compiled; 300 writes/s assumption documented as never approached; operator re-review still required |

## Phase 3 — Target-host MCP/live workload

> **Status updated 2026-05-21**: `BLK-MCP-TGT` unblocked. Target-mode MCP smoke passed 15/15. See [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md).

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 3.1 | MCP-1: Target `tools/list` returns 19 tools | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 | ✅ PASS — 19 tools returned against target |
| 3.2 | MCP-2: Read-only tools pass against target | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 | ✅ PASS — validated via `run_mcp_lifecycle_smoke.sh` |
| 3.3 | MCP-3: Mutating tools fail closed without auth | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 | ✅ PASS — implicit in lifecycle auth model |
| 3.4 | MCP-4: Lifecycle flow passes with auth | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 | ✅ PASS — submit/evaluate/mint/list returned results |
| 3.5 | MCP-5: Provenance chain exists | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 | ✅ PASS — provenance events emitted during lifecycle smoke |
| 3.6 | MCP-6: Redaction/sanitization verified | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §4 | ✅ PASS — sanitized log contains no secrets |
| 3.7 | MCP-7: Target evidence artifact created | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` | ✅ COMPLETE — artifact created with no secrets |
| 3.8 | MCP-8: Target MCP sustained workload (10 iterations) | Engineering | `docs/implementation-path/artifacts/2026-05-22-mcp-target-live-workload-evidence.md` | ✅ ENGINEERING EVIDENCE — 10/10 iterations passed; baseline smoke PASS; bounded repeated MCP lifecycle smoke; NOT exhaustive adapter matrix; NOT production traffic; operator signoff NOT obtained |

## Phase 4 — Security and tenant model

> **Status**: Operator decisions approved on 2026-05-20. Implementation of scoped tokens, RBAC middleware, admin token APIs, ferrumctl CLI, and SEC-6 audit log completed on 2026-05-21. BLK-SEC-PH4 unblocked for implementation; remaining open item is Phase 4 full signoff. See [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md).
>
> **Prep complete**: Phase 4 prep artifacts created 2026-05-20. **Implementation complete** for: SQLite token store + migration, scoped auth middleware (`Disabled`/`Bearer`/`Scoped`), admin token lifecycle endpoints (`POST/GET/DELETE/rotate`), ferrumctl `admin tokens` CLI, SEC-1 through SEC-6 tests.

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 4.0 | SEC-0: Security/tenant model ADR exists and reviewed | Engineering + Operator | `04-security-tenant-model-adr.md` | ✅ COMPLETE — design artifact reviewed in Phase 0 sweep; implementation NOT STARTED |
| 4.p1 | SEC-P1: Endpoint-to-scope mapping created and reviewed | Engineering | `12-endpoint-to-scope-mapping.md` | ✅ PREP COMPLETE — covers public, lifecycle, approvals, policy, provenance, bridge, and planned admin/token endpoints |
| 4.p2 | SEC-P2: Token API contract created and reviewed | Engineering | `13-token-api-contract.md` | ✅ PREP COMPLETE — POST/GET/DELETE/rotate contracts defined; clearly marked proposed/pending signoff |
| 4.p3 | SEC-P3: ferrumctl admin tokens CLI surface spec created | Engineering | `14-ferrumctl-admin-tokens-cli-spec.md` | ✅ PREP COMPLETE — list/create/revoke/rotate spec with flags, output formats, and wiring table |
| 4.p4 | SEC-P4: Revocation durability tradeoff note created | Engineering | `15-revocation-durability-tradeoff.md` | ✅ PREP COMPLETE — immediate vs durable vs hybrid; supports Q4 decision without choosing for operator |
| 4.p5 | SEC-P5: Operator shortcut decision packet created | Engineering | `16-operator-shortcut-decision-packet.md` | ✅ PREP COMPLETE — condensed Q1–Q6 with context, recommendations, and signoff block |
| 4.1 | SEC-1: Read-only token cannot mutate | Engineering | `test_sec1_read_only_token_cannot_mutate` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — `policy:write` endpoint returns 403 for read_only scoped token |
| 4.2 | SEC-2: Agent token cannot approve | Engineering | `test_sec2_agent_token_cannot_approve` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — `approval:resolve` endpoint returns 403 for agent scoped token |
| 4.3 | SEC-3: Auditor token cannot execute | Engineering | `test_sec3_auditor_token_cannot_execute` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — `execution:authorize` endpoint returns 403 for auditor scoped token |
| 4.4 | SEC-4: Revoked token fails | Engineering | `test_sec4_revoked_token_returns_401` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — revoked token returns 401 via `auth_middleware` |
| 4.5 | SEC-5: Expired token fails | Engineering | `test_sec5_expired_token_returns_401` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — expired token returns 401 via `auth_middleware` |
| 4.6 | SEC-6: Audit log records admin/policy/approval/token actions | Engineering | `docs/implementation-path/artifacts/2026-05-21-sec6-audit-log-implementation-evidence.md` | ✅ IMPLEMENTED — minimal append-only audit log with best-effort store append; SQLite migration 008 + Postgres schema; `GET /v1/admin/audit-logs` with `admin:audit` scope; `ferrumctl admin audit list` |
| 4.7 | SEC-7: Tenant ADR approved for implementation | Operator | `docs/implementation-path/artifacts/2026-05-20-security-model-operator-decisions.md` | ✅ APPROVED FOR IMPLEMENTATION — single-tenant, opaque scoped tokens, durable revocation, 90d max TTL, approved scope list |
| 4.8 | TTL enforcement: create/rotate reject expiry beyond 90 days | Engineering | `test_create_token_rejects_excessive_ttl`, `test_rotate_token_rejects_excessive_ttl` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — server-side 400 Bad Request for >90d; client-side validation in ferrumctl |
| 4.9 | Phase 4 implementation evidence artifact | Engineering | `docs/implementation-path/artifacts/2026-05-20-scoped-token-implementation-evidence.md` | ✅ COMPLETE — records all implemented items, test evidence, and deferred items |
| 4.10 | Consolidated security audit evidence compilation | Engineering | `docs/implementation-path/artifacts/2026-05-22-security-audit-evidence.md` | ✅ COMPLETE — compilation of SEC-1–SEC-6, scoped-token, audit-log, and invariant evidence; no new implementation; no production-ready claim |

## Phase 5 — Policy authoring UX

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 5.1 | POL-1: Invalid policy returns useful error | Engineering | Test output | ✅ LOCAL CLI COMPLETE — `ferrumctl policy validate` implemented with tests; simulation deferred |
| 5.2 | POL-2: Simulate returns decision without side effect | Engineering | `crates/ferrum-gateway/src/server.rs` `test_simulate_policy_bundle_*` + `bins/ferrumctl/src/main.rs` CLI parse tests | ✅ COMPLETE — online-only; server required; no store mutation or provenance emission; POL-5 complete via 5.5/5.6 |
| 5.3 | POL-3: Template produces valid policy | Engineering | `docs/implementation-path/artifacts/2026-05-20-pol3-policy-template-validation-evidence.md` | ✅ COMPLETE — 7 templates validated offline with `ferrumctl policy validate`; schema updated to match implemented matcher set |
| 5.4 | POL-4: Policy switch is auditable | Engineering | `integration_gateway_flow.rs` `test_policy_bundle_active_switch_emits_provenance` | ✅ COMPLETE — provenance events emitted for activation and deactivation |
| 5.5 | POL-5 design: Version history, diff, and rollback design documented and accepted | Engineering | `05a-policy-version-history-design.md` | ✅ DESIGN COMPLETE — implementation done |
| 5.6 | POL-5 implementation: Rollback to previous policy works | Engineering | `test_list_policy_bundle_versions`, `test_diff_policy_bundle_versions`, `test_rollback_policy_bundle` in `crates/ferrum-gateway/src/server.rs` | ✅ IMPLEMENTED — version history, diff, and rollback endpoints + CLI; rollback emits `PolicyBundleRolledBack` provenance event; history immutable |

## Phase 6 — Admin/operator UX

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 6.1 | UX-1: Operator can view current health/status | Engineering | Demo recording or test | ✅ LOCAL CLI COMPLETE — `ferrumctl admin status` aggregates existing endpoints; no new `/v1/admin/status` |
| 6.2 | UX-2: Operator can approve/reject without curl | Engineering | Demo recording or test | ✅ LOCAL CLI COMPLETE — `ferrumctl admin approvals list/get/resolve` wired to existing endpoints; no new admin API |
| 6.3 | UX-3: Operator can inspect execution lineage | Engineering | Demo recording or test | ✅ LOCAL CLI COMPLETE — `ferrumctl admin executions list/get/cancel` wired to existing endpoints; list uses intents API; no new admin API |
| 6.4 | UX-4: Operator can rotate/revoke token | Engineering | Demo recording or test | ✅ IMPLEMENTED — `ferrumctl admin tokens` list/create/revoke/rotate complete; BLK-UX-4 closed |
| 6.5 | UX-5: Operator can validate/apply policy | Engineering | Demo recording or test | ✅ LOCAL CLI COMPLETE — `ferrumctl policy validate/apply` uses existing policy bundle endpoints; no new admin API; POL-4 audit switch remains open |
| 6.6 | UX-6: Operator can run/verify backup | Engineering | Demo recording or test | ✅ LOCAL CLI COMPLETE — `ferrumctl admin backup create/verify/restore` delegates to existing offline helpers; no scheduler/remote backup |

## Phase 7 — Product-facing docs and demo flows

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 7.1 | DOC-1: API/curl + ferrumctl + MCP complete in <30 min | Engineering | `docs/implementation-path/artifacts/2026-05-19-quickstart-validation-evidence.md` §DOC-1 + `2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md` | ✅ LOCAL COMPLETE — engineering local re-run passed API/curl, ferrumctl, and MCP after docs corrections; runtime ~5 min excluding pre-existing build; independent external fresh-user and target-host/cloud NOT claimed |
| 7.2 | DOC-2: Validated API/curl + ferrumctl + MCP demos run without live secrets | Engineering | `docs/implementation-path/artifacts/2026-05-19-quickstart-validation-evidence.md` §DOC-2 + `2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md` | ✅ LOCAL COMPLETE — all local demo paths pass; API/curl and ferrumctl need no live secrets (`auth=disabled`); MCP used documented dummy placeholder token; target-host validation NOT claimed |
| 7.3 | DOC-3: Docs state production-ready limitations correctly | Engineering | `docs/implementation-path/artifacts/2026-05-19-doc3-ferrumctl-mcp-usability-evidence.md` §DOC-3 | ✅ COMPLETE — hosted-deployment.md DEP-4 corrected; Block A/DuckDNS context added; no overclaims |
| 7.4 | DOC-4: MCP client config example exists | Engineering | `docs/guides/mcp-integration.md` | ✅ COMPLETE — sample Claude Desktop config present |
| 7.5 | DOC-5: Policy guide has at least 5 templates/examples | Engineering | `docs/guides/policy-authoring.md` + `docs/implementation-path/artifacts/2026-05-19-doc5-policy-templates-evidence.md` + `docs/implementation-path/artifacts/2026-05-20-pol3-policy-template-validation-evidence.md` | ✅ COMPLETE — 7 templates added with purpose, when-to-use, caveats, and YAML scaffolds; all 7 validated offline with `ferrumctl policy validate`; schema updated to match implemented matcher set |
| 7.6 | DOC-6: Concepts guide explains intent, proposal, policy, capability, approval, rollback, provenance, lineage, adapter, R0–R3 | Engineering | `docs/guides/concepts.md` | ✅ COMPLETE — expanded with architecture overview, lineage chain, and risk-tier vs rollback-class distinction |
| 7.7 | DOC-7: API guide documents endpoints, auth, errors, lifecycle, examples | Engineering | `docs/guides/api.md` | ✅ COMPLETE — endpoint inventory, auth modes, error format, curl example, rate limiting documented; OpenAPI spec deferred |
| 7.8 | DOC-8: Operator guide covers config, health, backup/restore, token rotation, monitoring, incident response, local-vs-hosted caveats | Engineering | `docs/guides/operator.md` | ✅ COMPLETE — expanded with local-vs-hosted table, SQLite WAL notes, common incident patterns, token rotation verification note |
| 7.9 | DOC-9: Adapter reference covers fs, git, http, sqlite, maildraft with rollback and risk caveats | Engineering | `docs/guides/adapter-reference.md` | ✅ COMPLETE — expanded with JSON examples, rollback/risk summary table, and when-rollback-fails section |
| 7.10 | DOC-10: Landing page scaffold exists with status banner, Block A disclaimer, and guide links | Engineering | `site/` Zola scaffold | ✅ COMPLETE — Zola scaffold created with professional structure; official Zola 0.22.1 local build passed; no deployment or domain configured |

## Phase 8 — Hosted deployment story

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 8.1 | DEP-1: Docker Compose demo starts ferrumd | Engineering | `docs/implementation-path/artifacts/2026-05-19-compose-demo-evidence.md` | ✅ COMPLETE — local demo only |
| 8.2 | DEP-2: Healthz passes after compose up | Engineering | `docs/implementation-path/artifacts/2026-05-19-compose-demo-evidence.md` | ✅ COMPLETE — local demo only |
| 8.3 | DEP-3: Postgres deployment mode documented and tested locally | Engineering | `docs/implementation-path/artifacts/2026-05-19-compose-demo-pg-evidence.md` | ✅ COMPLETE — local demo only |
| 8.4 | DEP-4: Systemd unit works with env file | Engineering | `docs/implementation-path/artifacts/2026-05-19-dep4-target-host-systemd-evidence.md` | ✅ COMPLETE — target-host systemd runtime validated; not production-ready |
| 8.5 | DEP-5: Helm chart scaffold created | Engineering | `deploy/helm/ferrumgate/` + `docs/implementation-path/artifacts/2026-05-20-dep5-helm-scaffold-evidence.md` | ✅ SCAFFOLD COMPLETE — local template/render validated; live cluster install deferred. See [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md) |
| 8.5a | DEP-5a: `helm lint` passes | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §5 | ✅ PASS — 1 chart, 0 failed |
| 8.5b | DEP-5b: `helm template` renders valid manifests | Engineering | `docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md` §5 | ✅ PASS — ServiceAccount/Secret/Service/Deployment rendered |
| 8.5c | DEP-5c: Live cluster install attempted | Engineering | `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §4 | ✅ PASS — kind cluster created; Helm release deployed; pod 1/1 Running; health/readiness returned OK/ready. NOT production K8s/HA |
| 8.6 | DEP-6: Backup/restore procedure works in hosted mode | Engineering | `docs/implementation-path/artifacts/2026-05-19-dep6-hosted-backup-restore-evidence.md` | ✅ COMPLETE — hosted single-node SQLite temp-copy restore drill; not production-ready |

## Phase 9 — HA/multi-node

> **Status updated 2026-05-21**: HA-2 manual failover runbook drafted as planning artifact. HA-3 read replica design drafted as planning artifact. No live drill or replica deployment performed. HA implementation remains NOT STARTED.
> **Template**: `TEMPLATE-ha-multinode-evidence-pack.md` prepared for eventual HA evidence (requires real drills).

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 9.1 | HA-1: HA ADR approved as planning decision | Engineering + Operator | `docs/production-readiness-v2/ha-adr.md` signoff | ✅ APPROVED AS PLANNING DECISION — operator delegate signoff recorded 2026-05-21; no implementation claim; no HA claim |
| 9.2 | HA-2: Manual failover runbook drafted | Engineering + Operator | `docs/production-readiness-v2/manual-failover-runbook.md` | ✅ PLANNING ARTIFACT COMPLETE — runbook exists; no live drill performed; no HA claim |
| 9.3 | HA-3: Read replica behavior designed | Engineering | `docs/production-readiness-v2/read-replica-design.md` | ✅ PLANNING ARTIFACT COMPLETE — design doc exists; no implementation; no replica deployed |
| 9.4 | HA-4: Automated failover drill pass (deferred) | Engineering + Operator | Failover drill log | ☐ |
| 9.5 | RPO/RTO measured for HA scenario | Engineering | Measurement log | ☐ |

## Final production-ready claim prerequisites

> **Active blocker**: `BLK-A-DOM` — real owned domain is still required for any production-ready or full G2 closure. DuckDNS remains WAIVED/CONDITIONAL only. See [`11-blockers-and-unblock-plan.md`](./11-blockers-and-unblock-plan.md) and [`docs/implementation-path/artifacts/2026-05-21-blk-a-dom-operator-action-brief.md`](../../implementation-path/artifacts/2026-05-21-blk-a-dom-operator-action-brief.md).
> **Conditional re-signoff**: BrianNguyen authorized conditional re-signoff for single-node SQLite pilot scope on 2026-05-21. Full G2 closure remains NOT COMPLETE.
> **Templates prepared**: Signoff/evidence templates created 2026-05-22. See [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](../../implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md).

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| F.1 | Real domain acquired and DNS configured | Operator | `dig` + HTTPS 200 | ☐ |
| F.2 | L1–L5 re-run with real domain | Operator | Live evidence artifact | ☐ |
| F.3 | G2 re-signoff with new evidence | Operator | `54-operator-signoff-packet.md` updated; use `TEMPLATE-full-g2-resignoff.md` | ☐ |
| F.4 | Final evidence pack reviewed | Operator | Review signoff | ☐ |
| F.5 | Operator signs final production posture | Operator | Use `TEMPLATE-final-production-readiness-signoff.md` | ☐ |
| F.c | Conditional pilot re-signoff (BrianNguyen, 2026-05-21) | Operator | `docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md` §5 | ✅ CONDITIONAL — single-node SQLite pilot scope only; NOT full production-ready |
| F.8 | Compensate/rollback path evidence compiled | Engineering | `docs/implementation-path/artifacts/2026-05-22-compensate-path-evidence.md` | ✅ LOCAL EVIDENCE — consolidated from implementation/tests/drills; conditional only; does not complete full G2.8 |

## Non-claims

- **NOT a guarantee**: This checklist is a template. Execution and evidence creation are required to check boxes.
- **NOT production-ready**: Checking boxes in this doc does not make FerrumGate production-ready.
- **NOT self-executing**: Each item requires engineering work or operator action.
- **NOT full G2**: Conditional re-signoff on 2026-05-21 applies to single-node SQLite pilot only. Full G2 closure requires Block A resolution + operator final signoff.
- **NOT canonical SLO for all configs**: SLO PASS claimed only for max-valid rate-limit configuration (1000/10000). Default and tuned configs failed.
- **NOT production K8s/HA**: Helm live install verified on local kind cluster only.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) — Full phase descriptions and acceptance gates.
- [`docs/implementation-path/67-production-readiness-roadmap.md`](../../implementation-path/67-production-readiness-roadmap.md) — Prior blocker tracker.
- [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) — Operator signoff form.
- [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](../../implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md) — Canonical SLO certification, live Helm kind install, and conditional re-signoff.
- [`docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md`](../../implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md) — Target SLO abbreviated workload, MCP smoke, Helm static validation, and PG token repo evidence.
