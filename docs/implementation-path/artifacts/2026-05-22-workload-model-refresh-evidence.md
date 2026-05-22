# Workload Model Refresh Evidence — 2026-05-22

> **Status**: Engineering evidence artifact. Refreshes G2.1 workload model with observed metrics from DuckDNS canonical run, abbreviated target run, and local baseline. No production-ready claim.
> **Owner**: Engineering
> **Date**: 2026-05-22
> **Scope**: Single-node SQLite v1 conditional pilot only
> **Constraint**: `production-ready = NO` throughout. Block A remains WAIVED/CONDITIONAL. Full G2 re-signoff still requires operator review and Block A/domain context.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Block A remains conditional; no real owned domain; single-node SQLite pilot only |
| **Full G2 / operator signoff** | **NOT COMPLETE** | This artifact provides engineering evidence only; operator re-review and re-signoff are still required |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **300 writes/s tested** | **NO** | The 300 writes/s signed assumption was **never approached** in any observed run. This artifact documents how far below the ceiling observed traffic was, not that the ceiling was validated |
| **PostgreSQL production / HA** | **NO** | Single-node SQLite is the only supported runtime |
| **Sustained multi-day workload** | **NO** | All observed runs are bounded-duration tests (seconds to minutes). No 7–30 day sustained window exists |
| **Target-host full SLO certification** | **NO** | Only abbreviated target workload and canonical max-valid run exist; operator ratification pending |

---

## 1. Metadata

| Field | Value |
|-------|-------|
| Artifact date | 2026-05-22 |
| Author | Engineering |
| Review status | Pending operator review |
| G2 item | G2.1 — Workload Model Refreshed |

---

## 2. Original Signed Assumption (2026-05-09)

Source: [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) §1, §5

| Parameter | Signed Assumption |
|-----------|-------------------|
| Sustained write rate | ≤300 writes/s |
| Peak write rate | ≤300 writes/s |
| Daily write volume | ≤1,000,000 writes/day |
| SQLite single-node fit | CONFIRMED by operator (BrianNguyen, 2026-05-09) |

**Context**: This assumption was accepted by the operator based on a workload analysis document (not reproduced here) and the Phase 1 SQLite constraint. The operator explicitly checked "Fits within ≤300 writes/s sustained" at signing time.

---

## 3. Observed Datasets

All datasets below are drawn from prior evidence artifacts. No new live workload was executed for this artifact.

### 3.1 Dataset A — Canonical Max-Valid DuckDNS Run (2026-05-21)

Source: [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](./2026-05-21-canonical-slo-helm-conditional-signoff.md) §3.4

| Parameter | Value |
|-----------|-------|
| Environment | GCP VM `ferrumgate-nonprod` (`asia-southeast1-a`) |
| Store | SQLite on-disk |
| Domain | `ferrumgate.duckdns.org` (conditional pilot only) |
| Rate-limit config | 1000/10000 (max-valid, explicit opt-in) |
| Total requests | 2380 |
| HTTP 200 | 2380 |
| Errors | 0 |
| Error rate | 0.0% |
| 429 rate | 0.0% |
| Target phase requests | 1521 |
| Spike phase requests | 800 |
| Target phase p99 | 394.054 ms |
| Spike phase p99 | 379.684 ms |
| Readyz probes | 47 records, all HTTP 200 |

**Note**: This is the only canonical SLO run that passed. Default and tuned configs failed with 46.8% and 73.4% 429 rates respectively. See [`2026-05-22-slo-default-config-evidence.md`](./2026-05-22-slo-default-config-evidence.md).

### 3.2 Dataset B — Abbreviated DuckDNS Run (2026-05-21)

Source: [`docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md`](./2026-05-21-target-slo-mcp-helm-domain-evidence.md) §3

| Parameter | Value |
|-----------|-------|
| Environment | GCP VM `ferrumgate-nonprod` (`asia-southeast1-a`) |
| Store | SQLite on-disk |
| Domain | `ferrumgate.duckdns.org` (conditional pilot only) |
| Total requests | 39 |
| HTTP 200 | 39 |
| Errors | 0 |
| Target phase p50 | 191.385 ms |
| Target phase p95 | 384.841 ms |
| Target phase p99 | 1000.073 ms |
| Spike phase p50 | 191.21 ms |
| Spike phase p95 | 418.251 ms |
| Spike phase p99 | 527.098 ms |
| Readyz probes | 4 probes, all HTTP 200 |

**Note**: Abbreviated run with shortened phases (5–30 s). Light-load evidence only; not a performance certification.

### 3.3 Dataset C — Local Baseline (2026-05-19)

Source: [`docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md`](./2026-05-19-slo-local-baseline-evidence.md)

| Parameter | Value |
|-----------|-------|
| Environment | Local development workstation |
| Store | `sqlite::memory:` (in-memory) |
| Auth mode | `disabled` |
| Rate limit | 2/50 (default) |
| Total requests | 22 |
| HTTP 200 | 22 |
| Errors | 0 |
| Global p99 | 638.249 ms (driven by single warm-up outlier) |
| Target phase p99 | 2.0291 ms |
| Spike phase p99 | 2.07 ms |
| Readyz probes | 5/5 HTTP 200 |

**Note**: Local in-memory baseline. Latencies are not representative of target-host or persistent-store behavior.

### 3.4 Dataset D — Local Stress Suite (2026-05-19)

Source: [`docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md`](./2026-05-19-slo-local-baseline-evidence.md) §Stress baseline

| Scenario | Requests | RPS | Errors |
|----------|----------|-----|--------|
| s1-health | 2368 | 236.80 | 0% |
| s2-auth | 2583 | **258.30** | 0% |
| s4-intent-compile | 1459 | 145.90 | 0% |
| s7-sqlite-contention | 2093 | 209.30 | 0% |
| s8-rate-limit | 2480 | 248.00 | 0% |

**Note**: Local stress suite measures raw handler throughput against in-memory SQLite with `auth=disabled`. Rate limiting may not be fully effective in this configuration. These numbers represent local ceiling, not target-host capacity.

---

## 4. Assumed vs. Observed Comparison

| Metric | Original Signed Assumption | Highest Observed (Target Host) | Highest Observed (Local) | Gap Analysis |
|--------|---------------------------|-------------------------------|--------------------------|--------------|
| Sustained throughput | ≤300 writes/s | 1,521 requests (canonical target phase); phase duration not recorded in source | 258.30 RPS (stress s2-auth; in-memory, no auth) | Target-host evidence is far below the untested 300 writes/s ceiling. Local stress is not representative of target-host behavior |
| Peak throughput | ≤300 writes/s | 800 requests (canonical spike phase); phase duration not recorded in source | 258.30 RPS (stress s2-auth; in-memory, no auth) | Target-host evidence is far below the untested 300 writes/s ceiling |
| Daily volume | ≤1,000,000 writes/day | Not measured over 24 h | Not measured over 24 h | No 24 h observation exists. Canonical run total was 2,380 requests across all phases; daily volume cannot be derived from bounded test data |
| Error rate | <1% (5xx) | 0.0% | 0% | Within assumption |
| p99 latency | <500 ms (draft pilot target) | 394 ms (canonical target phase) | ~2 ms (local steady-state) | Within draft pilot target |

**Key finding**: The signed 300 writes/s ceiling was a **capacity planning assumption**, not an observed limit. All observed target-host runs generated request counts far below what would be needed to validate that ceiling. The local stress suite reached 258 RPS, which is still below the 300 writes/s ceiling, but it was run with in-memory SQLite and disabled auth, so it does not validate the 300 writes/s claim for a realistic deployment.

**Important**: This comparison does **not** prove that 300 writes/s is safe. It only documents that the assumption was never stress-tested and that observed traffic was far below it.

---

## 5. Capacity Ceiling Analysis

### 5.1 What was measured

| Ceiling | Value | Evidence | Caveat |
|---------|-------|----------|--------|
| Target-host canonical pass | 2380 requests, 0 errors | Dataset A | Required max-valid rate-limit config (1000/10000). Per-IP token-bucket enforcement means total server capacity is higher, but single-IP capacity is capped |
| Target-host light-load pass | 39 requests, 0 errors | Dataset B | Abbreviated phases; not a capacity test |
| Local raw handler throughput | ~258 RPS (auth disabled, in-memory) | Dataset D | Not representative of target host with auth, TLS, network RTT, and on-disk SQLite |

### 5.2 What was NOT measured

| Test | Why Missing | Impact on Claim |
|------|-------------|-----------------|
| Sustained 300 writes/s for >1 hour | No test generated this load | Cannot claim ceiling is ≥300 writes/s |
| Multi-IP concurrent load to measure total server throughput | Canonical workload uses 1–2 generator IPs | Per-IP limiter masks true server capacity; total capacity unknown |
| Write-heavy vs. read-heavy mix | All observed runs are mixed governance + health traffic | Cannot isolate write capacity |
| PostgreSQL-backed target | No production PG target deployed | PostgreSQL may raise ceiling, but no evidence exists |
| 24-hour continuous load | No overnight or multi-day run exists | Daily volume assumption untested |
| Degraded conditions (disk full, high CPU, memory pressure) | No degradation tests performed | Ceiling under stress unknown |

### 5.3 Engineering interpretation

The **only verifiable statement** is:

> Under the canonical SLO workload profile with max-valid rate-limit configuration (1000/10000), the target host (`ferrumgate.duckdns.org`, single-node SQLite on-disk) successfully served 2,380 requests with zero errors and p99 latency below 400 ms.

Any statement stronger than that (e.g., "the system can handle 300 writes/s") is **unsupported by the observed evidence**.

---

## 6. Recommended Safe Limits (Engineering Recommendation Only)

These limits are derived from observed data plus conservative engineering margin. They are **recommendations**, not validated ceilings. Operator review is required before adoption.

| Limit | Value | Rationale |
|-------|-------|-----------|
| **Sustained throughput** | ≤10 req/s | Observed canonical target phase served 1,521 requests with zero errors. 10 req/s is a conservative cap that stays well below any unmeasured ceiling without claiming validated headroom. |
| **Burst throughput** | ≤50 req/s | Observed canonical spike phase served 800 requests with zero errors. Local stress reached 258 RPS, but that config (in-memory, no auth) is unrealistic. 50 req/s is a conservative burst cap that stays well below any unmeasured ceiling. |
| **Rate-limit profile for certification** | 1000/10000 (explicit opt-in) | Only config that passed canonical SLO. Must not be used as a default; see [`2026-05-22-slo-default-config-evidence.md`](./2026-05-22-slo-default-config-evidence.md). |
| **Default rate-limit profile** | 2/50 (unchanged) | Safety-oriented default. Protects single-node pilot from accidental overload. |
| **Daily volume** | ≤100,000 writes/day (engineering guess) | Extrapolated from observed runs. No 24 h evidence. Operator must measure real daily volume. |

**Operator action required**:
1. Review these recommendations against actual expected workload.
2. If expected workload exceeds these recommendations, plan migration to PostgreSQL (Phase 3).
3. Re-sign G2.1 after reviewing this evidence and confirming fit.

---

## 7. Gaps and Caveats

| # | Gap | Severity | Mitigation |
|---|-----|----------|------------|
| 7.1 | 300 writes/s assumption never tested | High | Documented here as untested. Operator must either accept the risk or schedule a load test. |
| 7.2 | No 24-hour sustained window | Medium | All runs are bounded. Operator must verify daily volume with real traffic or accepted extrapolation. |
| 7.3 | No multi-IP concurrency test | Medium | Per-IP rate limiting means true server capacity is unknown. Operator must verify if single-IP burst is the limiting factor. |
| 7.4 | DuckDNS is conditional, not production | Medium | Block A remains open. Real domain may introduce CDN, geo-routing, or additional latency not captured here. |
| 7.5 | SQLite on-disk vs. in-memory delta unknown | Low | Local baseline used in-memory; target used on-disk. Latency difference is visible (~2 ms local vs. ~190 ms target p50). |
| 7.6 | No write-isolation test | Medium | Observed runs mix reads and writes. Pure write-stream capacity is unmeasured. |
| 7.7 | Auth disabled for local stress | Low | Local stress throughput is inflated by disabled auth. Target-host auth adds overhead not quantified. |

---

## 8. Operator Signoff / Status

### Engineering Pre-Fill

| Field | Value |
|-------|-------|
| Evidence artifact | `docs/implementation-path/artifacts/2026-05-22-workload-model-refresh-evidence.md` |
| Assumed vs observed throughput | Assumed ≤300 writes/s; observed target-host canonical run total 2,380 requests with zero errors; **assumption never approached** |
| Latency p50/p95/p99 per endpoint | Canonical target p50 ~190 ms, p95 ~380 ms, p99 ~394 ms (Dataset A); local steady-state p99 ~2 ms (Dataset C) |
| Capacity ceiling observed | 2,380 requests, 0 errors, under max-valid rate-limit config (Dataset A) |
| Recommended safe limits | Sustained ≤10 req/s, burst ≤50 req/s (engineering recommendation only; pending operator review) |
| Pass/Fail | ☐ — **Pending operator review** |

### Operator Signoff Block (Intentionally Blank)

> **Operator name**: ________________________
> **Date**: ________________________
> **Signature / Ack**: ________________________
>
> **I confirm that**:
> - I have reviewed the observed datasets in this artifact.
> - I understand the 300 writes/s signed assumption was not tested and remains an assumption.
> - I have reviewed the recommended safe limits and either accept them or plan mitigation.
> - I acknowledge that full G2.1 re-signoff requires Block A closure (real domain) and updated target-host evidence.

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | | |
| Operator (required) | | | |

---

## 9. Cross-References

| Document | Purpose |
|----------|---------|
| [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) | Original G2.1 signed assumption (≤300 writes/s) |
| [`docs/implementation-path/artifacts/2026-05-21-canonical-slo-helm-conditional-signoff.md`](./2026-05-21-canonical-slo-helm-conditional-signoff.md) | Canonical SLO max-valid run (Dataset A) |
| [`docs/implementation-path/artifacts/2026-05-21-target-slo-mcp-helm-domain-evidence.md`](./2026-05-21-target-slo-mcp-helm-domain-evidence.md) | Abbreviated target run (Dataset B) |
| [`docs/implementation-path/artifacts/2026-05-19-slo-local-baseline-evidence.md`](./2026-05-19-slo-local-baseline-evidence.md) | Local baseline and stress suite (Datasets C and D) |
| [`docs/implementation-path/artifacts/2026-05-22-slo-default-config-evidence.md`](./2026-05-22-slo-default-config-evidence.md) | Rate-limit profile decision evidence |
| [`docs/implementation-path/artifacts/2026-05-22-no-to-yes-completion-plan.md`](./2026-05-22-no-to-yes-completion-plan.md) | Updated to reference this artifact (§2.2) |
| [`docs/implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md`](./TEMPLATE-full-g2-resignoff.md) | Updated G2.1 pre-fill (§G2.1) |
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Evidence checklist (Phase 2) |
| [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) | SLO/SLA draft targets |
| [`docs/operations/rate-limit-tuning-guide.md`](../../operations/rate-limit-tuning-guide.md) | Operational rate-limit guidance |

---

*Artifact created: 2026-05-22. Workload model refresh — engineering evidence only. No production-ready claim. Operator re-signoff still required.*
