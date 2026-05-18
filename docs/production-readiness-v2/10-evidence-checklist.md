# 10 — Evidence Checklist

> **Status**: Planning artifact. Checklist template; not yet filled.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-18
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)

---

## Goal

Provide a per-phase evidence checklist so that every claim in the production path has a required evidence artifact, an owner, and a signoff state.

## Current state

- No unified per-phase evidence checklist exists.
- Evidence artifacts are scattered across `docs/implementation-path/artifacts/`.
- No systematic pass/fail tracking for production-path gates.

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
| 0.1 | `production-scope.md` exists and reviewed | Engineering | `docs/production-readiness-v2/00-scope-and-nonclaims.md` | ☐ |
| 0.2 | `slo-sla-draft.md` exists and reviewed | Engineering + Operator | `docs/production-readiness-v2/01-slo-sla.md` | ☐ |
| 0.3 | `postgres-production-gap-adr.md` exists and reviewed | Engineering | `docs/production-readiness-v2/02-postgres-production-plan.md` | ☐ |
| 0.4 | `mcp-target-host-validation-plan.md` exists and reviewed | Engineering | `docs/production-readiness-v2/03-target-mcp-live-workload-plan.md` | ☐ |
| 0.5 | `tenant-security-model-adr.md` exists and reviewed | Engineering + Operator | `docs/production-readiness-v2/04-security-tenant-model-adr.md` | ☐ |
| 0.6 | `product-docs-information-architecture.md` exists | Engineering | `docs/guides/README.md` or equivalent | ☐ |
| 0.7 | Every checklist has evidence requirements | Engineering | This doc | ☐ |
| 0.8 | No doc overclaims production-ready | Engineering | Review signoff | ☐ |

## Phase 1 — PostgreSQL production foundation

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 1.1 | PG-1: ferrumd starts with postgres DSN | Engineering | `pg-target-evidence.md` §PG-1 | ☐ |
| 1.2 | PG-2: `/v1/readyz/deep` reports PG health | Engineering | `pg-target-evidence.md` §PG-2 | ☐ |
| 1.3 | PG-3: Migration succeeds with hash/count validation | Engineering | `pg-migration-evidence.md` | ☐ |
| 1.4 | PG-4: Backup/restore drill passes | Engineering | `pg-restore-drill-evidence.md` | ☐ |
| 1.5 | PG-5: PG metrics visible in `/v1/metrics` | Engineering | Metrics scrape diff | ☐ |
| 1.6 | PG-6: PG target evidence artifact created | Engineering | `pg-target-evidence.md` | ☐ |

## Phase 2 — SLO/SLA and workload evidence

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 2.1 | SLO-1: SLO/SLA doc exists | Engineering + Operator | `slo-sla-draft.md` ratified | ☐ |
| 2.2 | SLO-2: Runbook maps scripts to pass/fail | Engineering | `slo-validation-runbook.md` | ☐ |
| 2.3 | SLO-3: Target workload run completed | Engineering | `slo-target-evidence-{date}.md` | ☐ |
| 2.4 | SLO-4: p95/p99 latency under threshold | Engineering | Latency histograms | ☐ |
| 2.5 | SLO-5: Readiness success meets target | Engineering | `/v1/readyz/deep` scrape | ☐ |
| 2.6 | SLO-6: Error rate under threshold | Engineering | Error counters | ☐ |
| 2.7 | SLO-7: Evidence artifact reviewed | Operator | Review signoff | ☐ |

## Phase 3 — Target-host MCP/live workload

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 3.1 | MCP-1: Target `tools/list` returns 19 tools | Engineering | `mcp-target-smoke-evidence.md` | ☐ |
| 3.2 | MCP-2: Read-only tools pass against target | Engineering | `mcp-target-smoke-evidence.md` | ☐ |
| 3.3 | MCP-3: Mutating tools fail closed without auth | Engineering | `mcp-target-smoke-evidence.md` | ☐ |
| 3.4 | MCP-4: Lifecycle flow passes with auth | Engineering | `mcp-lifecycle-evidence.md` | ☐ |
| 3.5 | MCP-5: Provenance chain exists | Engineering | `mcp-lifecycle-evidence.md` | ☐ |
| 3.6 | MCP-6: Redaction/sanitization verified | Engineering | `mcp-lifecycle-evidence.md` | ☐ |
| 3.7 | MCP-7: Target evidence artifact created | Engineering | `mcp-live-workload-evidence.md` | ☐ |

## Phase 4 — Security and tenant model

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 4.1 | SEC-1: Read-only token cannot mutate | Engineering | Test output | ☐ |
| 4.2 | SEC-2: Agent token cannot approve | Engineering | Test output | ☐ |
| 4.3 | SEC-3: Auditor token cannot execute | Engineering | Test output | ☐ |
| 4.4 | SEC-4: Revoked token fails | Engineering | Test output | ☐ |
| 4.5 | SEC-5: Expired token fails | Engineering | Test output | ☐ |
| 4.6 | SEC-6: Audit log records admin/policy/approval/token actions | Engineering | Audit log sample | ☐ |
| 4.7 | SEC-7: Tenant ADR approved before implementation | Operator | ADR signoff | ☐ |

## Phase 5 — Policy authoring UX

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 5.1 | POL-1: Invalid policy returns useful error | Engineering | Test output | ☐ |
| 5.2 | POL-2: Simulate returns decision without side effect | Engineering | Test output | ☐ |
| 5.3 | POL-3: Template produces valid policy | Engineering | Test output | ☐ |
| 5.4 | POL-4: Policy switch is auditable | Engineering | Audit log sample | ☐ |
| 5.5 | POL-5: Rollback to previous policy works | Engineering | Test output | ☐ |

## Phase 6 — Admin/operator UX

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 6.1 | UX-1: Operator can view current health/status | Engineering | Demo recording or test | ☐ |
| 6.2 | UX-2: Operator can approve/reject without curl | Engineering | Demo recording or test | ☐ |
| 6.3 | UX-3: Operator can inspect execution lineage | Engineering | Demo recording or test | ☐ |
| 6.4 | UX-4: Operator can rotate/revoke token | Engineering | Demo recording or test | ☐ |
| 6.5 | UX-5: Operator can validate/apply policy | Engineering | Demo recording or test | ☐ |
| 6.6 | UX-6: Operator can run/verify backup | Engineering | Demo recording or test | ☐ |

## Phase 7 — Product-facing docs and demo flows

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 7.1 | DOC-1: New user can complete quickstart in <30 min | Operator / UX tester | Timer + completion signoff | ☐ |
| 7.2 | DOC-2: Every demo runs without secrets | Engineering | Demo run log | ☐ |
| 7.3 | DOC-3: Docs state production-ready limitations correctly | Engineering | Doc review | ☐ |
| 7.4 | DOC-4: MCP client config example exists | Engineering | `docs/guides/mcp-integration.md` | ☐ |
| 7.5 | DOC-5: Policy guide has at least 5 templates/examples | Engineering | `docs/guides/policy-authoring.md` | ☐ |

## Phase 8 — Hosted deployment story

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 8.1 | DEP-1: Docker Compose demo starts ferrumd | Engineering | `docker-compose.demo.yml` + run log | ☐ |
| 8.2 | DEP-2: Healthz passes after compose up | Engineering | `curl` output | ☐ |
| 8.3 | DEP-3: Postgres deployment mode documented and tested | Engineering | `deployment/postgres-self-hosted.md` + run log | ☐ |
| 8.4 | DEP-4: Systemd unit works with env file | Engineering | `systemctl status` output | ☐ |
| 8.5 | DEP-5: Helm install produces ready pod | Engineering | `kubectl get pods` output | ☐ |
| 8.6 | DEP-6: Backup/restore procedure works in hosted mode | Engineering | Restore drill log | ☐ |

## Phase 9 — HA/multi-node

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| 9.1 | HA-1: HA ADR approved | Engineering + Operator | `ha-adr.md` signoff | ☐ |
| 9.2 | HA-2: Manual failover drill pass | Engineering + Operator | Failover drill log | ☐ |
| 9.3 | HA-3: Read replica behavior documented | Engineering | `ha-read-replica-plan.md` | ☐ |
| 9.4 | HA-4: Automated failover drill pass (deferred) | Engineering + Operator | Failover drill log | ☐ |
| 9.5 | RPO/RTO measured for HA scenario | Engineering | Measurement log | ☐ |

## Final production-ready claim prerequisites

| # | Item | Owner | Evidence | Status |
|---|------|-------|----------|--------|
| F.1 | Real domain acquired and DNS configured | Operator | `dig` + HTTPS 200 | ☐ |
| F.2 | L1–L5 re-run with real domain | Operator | Live evidence artifact | ☐ |
| F.3 | G2 re-signoff with new evidence | Operator | `54-operator-signoff-packet.md` updated | ☐ |
| F.4 | Final evidence pack reviewed | Operator | Review signoff | ☐ |
| F.5 | Operator signs final production posture | Operator | Signed doc | ☐ |

## Non-claims

- **NOT a guarantee**: This checklist is a template. Execution and evidence creation are required to check boxes.
- **NOT production-ready**: Checking boxes in this doc does not make FerrumGate production-ready.
- **NOT self-executing**: Each item requires engineering work or operator action.

## Related docs

- [`docs/ROADMAP.md`](../../ROADMAP.md) — Full phase descriptions and acceptance gates.
- [`docs/implementation-path/67-production-readiness-roadmap.md`](../../implementation-path/67-production-readiness-roadmap.md) — Prior blocker tracker.
- [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) — Operator signoff form.
