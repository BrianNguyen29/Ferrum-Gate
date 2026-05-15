# 2026-05-12 — SQLite Path 2 Target-Host Blocked Attempt Evidence

> **Status**: BLOCKED — operator-owned. No production-ready claim.  
> **Purpose**: Record the 2026-05-12 attempted target-host execution and G3.6 monitoring probe, the blockers encountered, and the exact remediation required.  
> **Scope**: Single-node SQLite pilot target host only. No PostgreSQL/multi-node/HA.  
> **Constraint**: This artifact does NOT close any blockers. Do not mark checklist boxes complete based on this evidence. No secret values are recorded.

---

## 1. Context

This artifact documents an attempted execution against the live SQLite Path 2 target host after the operator selected Option A (continue SQLite) in `113-operator-path-selection-packet.md`. The attempt was made from a CI/runner environment with the intent of progressing B1–B5 (doc 115) and B8 / full G3.6 (doc 116). **The attempt was blocked by network access and authentication gaps.**

Pre-attempt commit: `b320f5c docs: record SQLite pilot path selection`.

---

## 2. Evidence — Public Endpoint Probes (Passed)

Public HTTPS probes were executed from the runner against `https://ferrumgate.duckdns.org`. These verify that the reverse proxy and `ferrumd` process are externally reachable, but they **do not** constitute target-host shell access, bearer-auth validation, or adapter-path execution.

| Endpoint | Method | Result | Note |
|---|---|---|---|
| `/v1/healthz` | `curl` | HTTP 200 | Public health probe passes |
| `/v1/readyz` | `curl` | HTTP 200 | Public readiness probe passes |
| `/v1/readyz/deep` | `curl` | HTTP 200 | Deep readiness probe passes |
| `/v1/metrics` | `curl` | HTTP 200 | Metrics endpoint reachable |

**Interpretation**: The VM process is alive and responding to unauthenticated public probes. This is expected behavior for the current configuration.

---

## 3. Evidence — Target Host Status

| Check | Command / Observation | Result |
|---|---|---|
| VM status | `gcloud compute instances describe ... --format='value(status)'` | `RUNNING` |
| External IP | `gcloud compute instances describe` | `34.158.51.8` |
| Domain | DNS A record | `ferrumgate.duckdns.org` |

---

## 4. Evidence — Firewall Rules

| Rule | Protocol | Port | Source | Status |
|---|---|---|---|---|
| SSH | tcp | 22 | `118.69.4.63/32` | Active |
| HTTP | tcp | 80 | `0.0.0.0/0` | Active |
| HTTPS | tcp | 443 | `0.0.0.0/0` | Active |
| App direct | tcp | 19080 | `118.69.4.63/32` | Active |

**Runner public IP**: `118.68.117.136`

**Gap**: The runner IP (`118.68.117.136`) is **not** in the SSH source range (`118.69.4.63/32`). Therefore direct SSH and IAP SSH are expected to fail from this runner.

---

## 5. Evidence — SSH Access Attempts (Blocked)

### 5.1 Direct SSH

```text
ssh: connect to host 34.158.51.8 port 22: Connection timed out
```

**Root cause**: Firewall rule restricts SSH source to `118.69.4.63/32`; runner IP `118.68.117.136` is outside this range.

### 5.2 IAP SSH

```text
failed to connect to backend
Failed to connect to port 22
```

**Root cause**: IAP tunnel requires the backend port to be reachable from the IAP proxy. The same firewall restriction blocks the backend connection.

---

## 6. Evidence — Bearer Token / Authentication

| Check | Observation | Implication |
|---|---|---|
| Local token env | Absent (`FERRUMD_BEARER_TOKEN` not set in runner env) | No authenticated requests possible |
| Compile workload probe | 60 s target, 1 rps, compile-only | 55 requests sent; all returned HTTP 401 |
| Status distribution | `{'401': 55}` | 100 % unauthorized — expected without valid token |

**No authenticated workload was executed.** The 401 responses confirm the API is enforcing bearer auth; they do not indicate a service error.

---

## 7. Evidence — Metrics Snapshot (Post-Attempt)

Metrics scraped from `/v1/metrics` after the attempt:

| Metric | Value | Interpretation |
|---|---|---|
| `ferrumgate_store_health_up` | 1 | Store healthy |
| `ferrumgate_write_queue_depth` | 0 | No backlog |
| `ferrumgate_governance_errors_total{route="/v1/intents/compile"}` | 0 | No governance errors on compile route |
| `ferrumgate_governance_success_total{route="/v1/intents/compile"}` | 1805 | Prior successful compiles unchanged |

The 401 responses from the compile workload probe are **not** reflected in governance error counters because the requests failed at the auth layer before reaching governance.

---

## 8. Blocker Status

### 8.1 Doc 115 — SQLite Path 2 Target-Host Checklist

| Blocker | Status | Reason |
|---|---|---|
| B1 — Target-host D1–D6 evidence | **BLOCKED** | No SSH access; cannot run drills on target host |
| B2 — SQLite restore drill | **BLOCKED** | No SSH access; cannot execute `ferrumctl backup` on target host |
| B3 — Backup automation | **BLOCKED** | No SSH access; cannot configure systemd timer or cron |
| B4 — TLS/reverse proxy configuration | **BLOCKED** | Probes pass, but operator cannot verify config on host or confirm cert paths |
| B5 — Bearer token generation | **BLOCKED** | No shell access to generate/store token securely on target host |

> **Note**: Public HTTPS probes (healthz/readyz/deep/metrics HTTP 200) do **not** satisfy any B1–B5 checklist item. They only confirm the process is externally reachable.

### 8.2 Doc 116 — G3.6 Monitoring Execution Plan

| Item | Status | Reason |
|---|---|---|
| Full G3.6 acceptance | **BLOCKED** | No authenticated workload executed; no adapter-path exercise; no phase sequence (baseline/low/target/spike/cooldown) |
| B8 — G3.6 real workload / post-deploy monitoring | **BLOCKED** | No bearer token → no authenticated requests → no real workload |
| R3 prerequisite (metrics with auth) | **BLOCKED** | Token absent; `/v1/metrics` returned 200 without auth, but subsequent authenticated workloads impossible |

**G3.6 remains conditionally accepted only** (compile-only/light workload basis from 2026-05-11). This attempt does **not** upgrade G3.6 to full acceptance.

---

## 9. Remediation Required (Operator-Owned)

| # | Remediation | Owner | Unblocks |
|---|---|---|---|
| 1 | Add runner IP `118.68.117.136/32` to GCP firewall SSH source range, OR establish a dedicated operator bastion/jump host in `118.69.4.63/32` | Operator / Infra | SSH access |
| 2 | Verify operator workstation IP matches `118.69.4.63/32`, or update firewall to match operator actual IP | Operator | SSH access |
| 3 | Generate bearer token via `openssl rand -hex 32` and set in target host env file (`/etc/ferrumgate/ferrumd.env`) with `chmod 600` | Operator | B5, authenticated probes |
| 4 | Re-attempt SSH (direct or IAP) after firewall correction | Operator | All B1–B5 |
| 5 | Execute authenticated compile workload to confirm 200 responses | Operator | G3.6 baseline |
| 6 | Run full G3.6 phase sequence (baseline → low → target → spike → cooldown) with adapter mix | Operator | G3.6 full acceptance |
| 7 | Record evidence in `58-workload-compensation-drill-evidence-template.md` and `106-g3-6-pilot-metrics-evidence-packet.md` | Operator | B1, B8 |

---

## 10. Explicit Non-Claims

- **No production-ready claim**: This artifact does not make FerrumGate production-ready.
- **No blocker closure**: B1–B5 and B8 were open and operator-owned at that time.
- **No G3.6 full acceptance**: G3.6 remains conditionally accepted only.
- **No PostgreSQL deployment**: PostgreSQL/multi-node/HA remains out of scope.
- **No secret recording**: No bearer token, password, or private key path is recorded in this artifact.
- **No fabricated evidence**: All observations are from real probes and commands executed on 2026-05-12.

---

## 11. Cross-References

| Artifact | Links To | Purpose |
|---|---|---|
| This artifact | `115-sqlite-path2-target-host-checklist.md` | Blocker definitions B1–B5 |
| This artifact | `116-g36-monitoring-execution-plan.md` | G3.6 execution plan and acceptance criteria |
| This artifact | `112-post-p5c-completion-execution-plan.md` | Track 4 and Phase 3–5 context |
| This artifact | `66-path-2-operator-handoff.md` §B.0 | Consolidated operator blockers B1–B8 |
| This artifact | `106-g3-6-pilot-metrics-evidence-packet.md` | G3.6 conditional acceptance baseline |

---

## 12. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial blocked-attempt evidence artifact | Engineering |

---

*Artifact created: 2026-05-12. SQLite Path 2 Target-Host Blocked Attempt — evidence only. No blocker closed. No production-ready claim. Operator-owned remediation required.*
