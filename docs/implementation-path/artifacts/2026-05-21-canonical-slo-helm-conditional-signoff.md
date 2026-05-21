# Canonical SLO + Helm Live + Conditional Signoff — 2026-05-21

> **Status**: Engineering evidence artifact. Final gates for conditional pilot. No production-ready claim.
> **Purpose**: Record canonical SLO certification attempts (pass and fail), live Helm install on kind, and conditional signoff under BrianNguyen authorization.
> **Scope**: Single-node SQLite v1 conditional pilot only.
> **Constraint**: `production-ready = NO` throughout. Block A remains WAIVED/CONDITIONAL. Full G2 remains NOT COMPLETE.
> **Authorized by**: BrianNguyen in current session, 2026-05-21.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Block A remains conditional; no real owned domain; single-node SQLite pilot only |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Conditional re-signoff documented below; full closure requires real domain + revalidation |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **HA / multi-node / PostgreSQL production** | **NO** | PostgreSQL token repo is code-complete; target host remains SQLite single-node |
| **Helm production K8s / HA** | **NO** | Live install verified on local kind cluster only; not production K8s or HA |
| **SLO certification for default rate limits** | **NO** | Runs #1 and #2 failed SLO under default/tuned rate limits; pass required max-valid rate-limit config |

---

## 1. Target Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-21 |
| VM name | `ferrumgate-nonprod` |
| Project | `fairy-b13f4` |
| Zone | `asia-southeast1-a` |
| IP | `34.158.51.8` |
| HTTPS domain | `ferrumgate.duckdns.org` |
| Store backend | SQLite (on-disk) |
| Auth mode | `bearer` |

---

## 2. Token Handling (Sanitized)

- A new target bearer token was generated locally with `openssl rand -hex 32`.
- The token was installed on the VM via temporary startup-script metadata.
- Startup-script metadata was removed after installation and verified empty.
- **No token value, prefix, or hash appears in this artifact or any repository.**
- Local temporary token files were deleted after final documentation/verification.

---

## 3. Canonical SLO Certification Runs

### 3.1 Runbook Reference

Validation performed per `docs/production-readiness-v2/slo-validation-runbook.md`.
Canonical phases: baseline → low → target → spike → cooldown.
Acceptance criteria (pilot baseline):

- 5xx rate < 1%
- 429 rate < 5% (pilot baseline; production target may be stricter)
- `/v1/readyz/deep` all HTTP 200
- p99 latencies recorded per phase

### 3.2 Run #1 — Default Rate Limits (FAIL)

| Parameter | Value |
|-----------|-------|
| Output dir | `/tmp/opencode/ferrumgate-target-slo-canonical-20260521` |
| rate_limit_per_second | default (not tuned) |
| rate_limit_burst | default (not tuned) |
| Exit code | 0 (script completed) |
| SLO result | **FAIL** |

**Results:**

| Metric | Value |
|--------|-------|
| Total requests | 2382 |
| HTTP 429 | 1114 |
| 429 rate | **46.767%** |
| Target phase p99 | 403.424 ms |
| Spike phase p99 | 378.343 ms |
| Readyz probes | All HTTP 200 |

**Assessment**: SLO failed due to excessive 429 responses under default rate limits. This is **failure evidence**, not a pass.

### 3.3 Run #2 — Tuned Rate Limits (FAIL)

| Parameter | Value |
|-----------|-------|
| Output dir | `/tmp/opencode/ferrumgate-target-slo-canonical-tuned-20260521` |
| rate_limit_per_second | 20 |
| rate_limit_burst | 500 |
| Exit code | 0 (script completed) |
| SLO result | **FAIL** |

**Results:**

| Metric | Value |
|--------|-------|
| Total requests | 2444 |
| HTTP 429 | 1795 |
| 429 rate | **73.445%** |
| Target phase p99 | 382.939 ms |
| Spike phase p99 | 305.626 ms |
| Readyz probes | All HTTP 200 |

**Assessment**: SLO failed despite tuned rate limits. 429 rate increased versus Run #1, indicating the tuned config was insufficient for the canonical workload profile. This is **failure evidence**, not a pass.

### 3.4 Run #3 — Max-Valid Rate Limits (PASS)

| Parameter | Value |
|-----------|-------|
| Output dir | `/tmp/opencode/ferrumgate-target-slo-canonical-maxvalid-20260521` |
| rate_limit_per_second | 1000 |
| rate_limit_burst | 10000 |
| Exit code | 0 (script completed) |
| SLO result | **PASS** |

**Results:**

| Metric | Value |
|--------|-------|
| Total requests | 2380 |
| HTTP 200 | 2380 |
| HTTP 429 | 0 |
| Errors | 0 |
| Error rate | 0.0% |
| 429 rate | 0.0% |

**Latency by phase:**

| Phase | Requests | p99 | Max |
|-------|----------|-----|-----|
| Low | 59 | 372.533 ms | 387.92 ms |
| Target | 1521 | 394.054 ms | 1159.73 ms |
| Spike | 800 | 379.684 ms | 1196.95 ms |

| Metric | Value |
|--------|-------|
| Readyz probes | 47 records, all HTTP 200 |

**Assessment**: SLO **PASSED** under max-valid rate-limit configuration. Zero errors, zero 429s, all readyz probes healthy. This is the **only passing SLO run** of the three attempts.

**Sanitized log**: `/tmp/opencode/ferrumgate-target-slo-canonical-maxvalid-20260521.log`

### 3.5 SLO Certification Summary

| Run | Config | Result | 429 Rate | Key Finding |
|-----|--------|--------|----------|-------------|
| #1 | Default | **FAIL** | 46.767% | Default rate limits insufficient for canonical workload |
| #2 | Tuned (20/500) | **FAIL** | 73.445% | Tuned config still insufficient; 429 rate worsened |
| #3 | Max-valid (1000/10000) | **PASS** | 0.0% | Max-valid config required to pass canonical SLO |

**Claim**: Full SLO certification is **PASS only for the max-valid rate-limit config**. Earlier failed attempts are documented as failure evidence. Default/tuned configs do not meet pilot SLO under canonical workload.

---

## 4. Live Helm Install (kind Cluster)

### 4.1 Scope

Verified on a **local kind cluster only**. NOT a production K8s or HA deployment.

### 4.2 Tooling

| Tool | Version |
|------|---------|
| kind | v0.23.0 |
| Helm | v3.15.4 |
| Docker image | `ferrumgate/ferrumd:0.1.0` (local build) |

### 4.3 Cluster Creation

- Cluster name: `ferrumgate-helm-live`
- Creation result: **SUCCESS**

### 4.4 Image Load

- Local Docker image `ferrumgate/ferrumd:0.1.0` loaded into kind cluster.

### 4.5 Helm Install

- Release name: `ferrumgate`
- Namespace: `ferrumgate`
- Chart path: `deploy/helm/ferrumgate`
- Install result: **SUCCESS**

### 4.6 Rollout Verification

| Check | Command / Method | Result |
|-------|------------------|--------|
| Pod status | `kubectl get pods -n ferrumgate` | `ferrumgate-5cf6c87fb5-nr5hj 1/1 Running 0 restarts` |
| Helm status | `helm status ferrumgate -n ferrumgate` | `deployed` |
| Health (port-forward) | `GET /v1/healthz` | `{"status":"ok"}` |
| Readiness (port-forward) | `GET /v1/readyz` | `{"status":"ready"}` |

### 4.7 Helm Live Assessment

| Claim | Status |
|-------|--------|
| Helm install on local kind | **PASS** |
| Production K8s / HA | **NO** — not claimed |
| Ingress / TLS termination | **NO** — not configured or tested |
| Multi-node / rolling update | **NO** — single-node kind only |

---

## 5. Conditional Signoff

### 5.1 Authorization

> **Signed by**: BrianNguyen
> **Authorization statement**: Authorized by BrianNguyen in current session, 2026-05-21.
> **Nature of signature**: Session authorization, not cryptographic signature.

### 5.2 Block A — Domain

| Item | Status |
|------|--------|
| Real owned domain provided | **NO** |
| DuckDNS domain | `ferrumgate.duckdns.org` (conditional pilot only) |
| Block A | **WAIVED/CONDITIONAL** |

**Rationale**: No real owned domain was provided by the operator. DuckDNS remains accepted only for the conditional single-node SQLite pilot. Full G2 closure and any production-ready claim require a real domain.

### 5.3 Production-Ready Posture

| Item | Status |
|------|--------|
| Production-ready | **NO** |
| Full G2 complete | **NOT COMPLETE** |
| Conditional pilot / RC-ready | **YES** — single-node SQLite with DuckDNS |

### 5.4 Full G2 Status

G2.1–G2.8 were originally signed for conditional single-node SQLite pilot (2026-05-17). The evidence added on 2026-05-21 (canonical SLO max-valid pass, Helm live kind install, target MCP smoke) supports a **conditional re-signoff** for the same pilot scope, **not** a full production G2 closure.

**G2 items refreshed with new evidence:**

| G2 Item | Evidence | Status |
|---------|----------|--------|
| G2.1 Workload model | Canonical SLO Run #3 (max-valid) | Refreshed — conditional |
| G2.2 Auth/TLS/security | Target token rotation + MCP smoke pass | Refreshed — conditional |
| G2.3 Backup schedule | Prior evidence (2026-05-15 retention run) | Unchanged — conditional |
| G2.4 Restore drill | Prior evidence (2026-05-15 restore drill) | Unchanged — conditional |
| G2.5 RPO/RTO | Prior evidence (local Docker) | Unchanged — conditional |
| G2.6 Production evaluation | Canonical SLO pass (max-valid only) | Refreshed — conditional |
| G2.7 Accepted-risk review | Block A waived | Conditional only |
| G2.8 Compensate/noop risk | Prior evidence | Unchanged — conditional |

**Conclusion**: G2 is **conditionally re-signed** for the single-node SQLite pilot scope only. Full G2 closure remains blocked on Block A (real domain) and is **NOT claimed**.

---

## 6. Cross-References

| Document | Purpose |
|----------|---------|
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Phase-by-phase evidence checklist |
| [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md) | Blocker tracking |
| [`2026-05-21-target-slo-mcp-helm-domain-evidence.md`](./2026-05-21-target-slo-mcp-helm-domain-evidence.md) | Prior target evidence (abbreviated SLO, MCP smoke, Helm static validation) |
| [`2026-05-20-slo-ratification-signoff.md`](./2026-05-20-slo-ratification-signoff.md) | SLO baseline ratification |
| [`2026-05-19-slo-local-baseline-evidence.md`](./2026-05-19-slo-local-baseline-evidence.md) | Local SLO baseline |
| [`2026-05-19-slo-target-preflight-blocked-evidence.md`](./2026-05-19-slo-target-preflight-blocked-evidence.md) | Prior blocked preflight |
| [`2026-05-20-dep5-helm-scaffold-evidence.md`](./2026-05-20-dep5-helm-scaffold-evidence.md) | Helm scaffold evidence |
| [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) | SLO validation procedure |

---

## 7. Engineering Review Statement

> This artifact accurately records canonical SLO certification results, live Helm install results, and conditional signoff posture as of 2026-05-21. Three SLO runs were executed: Run #1 failed (46.8% 429), Run #2 failed (73.4% 429), Run #3 passed (0% 429, 0% error) under max-valid rate-limit configuration. Helm install passed on a local kind cluster. No token values or secrets appear in this artifact. Production-ready remains **NO**. Block A remains **WAIVED/CONDITIONAL**. Full G2 remains **NOT COMPLETE**. Conditional re-signoff is authorized by BrianNguyen in the current session, 2026-05-21.

---

*Artifact created: 2026-05-21. Canonical SLO + Helm live + conditional signoff — records observed results only. No production-ready claim.*
