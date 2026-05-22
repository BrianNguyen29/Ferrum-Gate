# Target MCP Live Workload Evidence — 2026-05-22

> **Status**: Engineering evidence artifact (bounded). No production-ready claim. No full G2 closure claimed.
> **Purpose**: Record target-host MCP sustained live workload run (10 iterations) with baseline smoke confirmation.
> **Scope**: Single-node SQLite conditional pilot on DuckDNS. Not production traffic. Not exhaustive adapter-matrix validation.
> **Constraint**: `production-ready = NO` throughout. Block A remains WAIVED/CONDITIONAL.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | DuckDNS conditional pilot; real domain still required |
| **Full G2 completion** | **NOT COMPLETE** | Requires Block A closure and all P.1–P.7 prerequisites |
| **Exhaustive adapter matrix validation** | **NO** | Only MCP lifecycle smoke repeated 10×; individual adapter flows not individually validated |
| **Production traffic validation** | **NO** | Engineering-run workload against DuckDNS target; not live production traffic |
| **Block A closed** | **WAIVED/CONDITIONAL** | DuckDNS accepted for pilot only; real domain required for full G2 |
| **PostgreSQL production** | **NO** | SQLite single-node only |
| **HA/multi-node** | **NO** | Not implemented |

---

## Source / Sanitization Note

All data in this artifact is derived from user-provided sanitized run summary. No secrets, tokens, or sensitive values appear. The user confirmed the data is sanitized prior to submission. Iteration log files (`iteration-1.log` through `iteration-10.log`) are referenced by name only; full log contents were not provided and are not included here.

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Artifact date | 2026-05-22 |
| Run ID (smoke) | `20260522T173804Z-mcp-smoke` |
| Run ID (sustained) | `20260522T174203Z-mcp-sustained` |
| Target URL | `https://ferrumgate.duckdns.org` |
| Environment | DuckDNS conditional pilot — `ferrumgate-nonprod` VM, `asia-southeast1-a` |
| Store backend | SQLite (on-disk) |
| Auth mode | `bearer` |
| Baseline smoke | PASS — `MCP LIFECYCLE SMOKE: ALL CHECKS PASSED`, Failed: 0 |
| Sustained iterations | 10 / 10 PASS |
| Operator signoff | **NOT obtained** — engineering evidence only |

---

## 2. Baseline Smoke Result

| Field | Value |
|-------|-------|
| Run ID | `20260522T173804Z-mcp-smoke` |
| Target | `https://ferrumgate.duckdns.org` |
| Status | **PASS** |
| Failed | 0 |
| Summary | `MCP LIFECYCLE SMOKE: ALL CHECKS PASSED` |

**Note**: This confirms the target gateway is reachable over HTTPS with bearer auth and the MCP lifecycle smoke test passes. This is the same smoke test used in the 2026-05-21 run (15/15 checks passed). The baseline smoke is a prerequisite for the sustained run.

---

## 3. Sustained Workload — 10 Iteration Results

| Iteration | Status | Start (UTC) | End (UTC) | Duration (s) | Log file |
|-----------|--------|-------------|-----------|-------------|----------|
| 1 | PASS | 2026-05-22T17:42:27Z | 2026-05-22T17:42:30Z | ~3 | `iteration-1.log` |
| 2 | PASS | 2026-05-22T17:42:40Z | 2026-05-22T17:42:43Z | ~3 | `iteration-2.log` |
| 3 | PASS | 2026-05-22T17:42:53Z | 2026-05-22T17:42:59Z | ~6 | `iteration-3.log` |
| 4 | PASS | 2026-05-22T17:43:09Z | 2026-05-22T17:43:11Z | ~2 | `iteration-4.log` |
| 5 | PASS | 2026-05-22T17:43:21Z | 2026-05-22T17:43:24Z | ~3 | `iteration-5.log` |
| 6 | PASS | 2026-05-22T17:43:37Z | 2026-05-22T17:43:40Z | ~3 | `iteration-6.log` |
| 7 | PASS | 2026-05-22T17:43:50Z | 2026-05-22T17:43:52Z | ~2 | `iteration-7.log` |
| 8 | PASS | 2026-05-22T17:44:05Z | 2026-05-22T17:44:07Z | ~2 | `iteration-8.log` |
| 9 | PASS | 2026-05-22T17:44:17Z | 2026-05-22T17:44:20Z | ~3 | `iteration-9.log` |
| 10 | PASS | 2026-05-22T17:44:30Z | 2026-05-22T17:44:32Z | ~2 | `iteration-10.log` |

**Summary**: 10 iterations executed. 10 iterations passed. 0 failed. Total wall-clock window: ~2 minutes 5 seconds (17:42:27Z to 17:44:32Z, including inter-iteration gaps).

---

## 4. Coverage and Limitations

### What This Evidence Covers

- **Target-host MCP lifecycle smoke repeated 10×**: Each iteration exercises the D1.11 MCP lifecycle smoke subset (submit → evaluate → mint → list) against the DuckDNS target.
- **Baseline smoke confirmation**: Smoke run (`20260522T173804Z-mcp-smoke`) passed before sustained run started.
- **Sustained run baseline_smoke=PASS**: Confirms target remained stable across all 10 iterations.

### What This Evidence Does NOT Cover

- **Individual adapter tool validation**: The sustained run exercises MCP lifecycle; it does not individually validate each adapter (fs, git, http, sqlite, maildraft) exhaustively.
- **Production traffic**: This is an engineering-run synthetic workload, not live production traffic.
- **Extended duration SLO certification**: 10 iterations over ~2 minutes is a repeated smoke test, not a sustained SLO certification window (which requires 7–30 days).
- **Real domain / Block A closure**: DuckDNS remains conditional pilot only.
- **HA / multi-node / PostgreSQL production**: Not in scope.
- **Operator signoff**: Engineering evidence only; no operator review or ratification obtained.

---

## 5. Relationship to Prior Evidence

| Date | Artifact | Key result |
|------|----------|------------|
| 2026-05-21 | [`2026-05-21-target-slo-mcp-helm-domain-evidence.md`](./2026-05-21-target-slo-mcp-helm-domain-evidence.md) | MCP smoke 15/15 PASS; 19 tools confirmed; abbreviated SLO workload (39 req, 0 err) |
| 2026-05-22 (this artifact) | `2026-05-22-mcp-target-live-workload-evidence.md` | MCP smoke PASS; sustained 10-iteration repeated smoke PASS; extends prior evidence with sustained run |

**Distinction**: The 2026-05-21 evidence established that MCP smoke (single run) passes against the target. This artifact extends that by confirming the smoke passes repeatedly (10 iterations) in a sustained run configuration, with baseline smoke confirmed before sustained run started.

---

## 6. Cross-References

| Document | Purpose |
|----------|---------|
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Phase 3 MCP evidence checklist |
| [`docs/production-readiness-v2/03-target-mcp-live-workload-plan.md`](../../production-readiness-v2/03-target-mcp-live-workload-plan.md) | MCP target-host validation plan |
| [`docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md`](./2026-05-21-target-slo-mcp-helm-domain-evidence.md) | Prior MCP smoke evidence (15/15, 19 tools) |
| [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) | NO→YES completion plan; step 2.4 |
| [`docs/implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md`](./TEMPLATE-full-g2-resignoff.md) | Full G2 re-signoff template; P.4 reference |

---

## 7. Engineering Review Statement

> This artifact accurately records target-host MCP sustained workload results as of 2026-05-22. Baseline smoke passed (`MCP LIFECYCLE SMOKE: ALL CHECKS PASSED`, Failed: 0). Sustained run passed 10/10 iterations. No secrets are present. This is engineering evidence of a bounded repeated MCP lifecycle smoke against a DuckDNS target; it is not a production-ready claim, not full G2 closure, and not an exhaustive adapter matrix validation. Block A remains WAIVED/CONDITIONAL. Operator signoff has not been obtained.

---

*Artifact created: 2026-05-22. Target MCP sustained live workload evidence — bounded engineering evidence only. No production-ready claim. No full G2 closure.*
