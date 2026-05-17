# Bridge L1–L3 Live Evidence — 2026-05-17

> **Status**: Live validation evidence. No production-ready claim. No full G2 completion claimed.  
> **Purpose**: Record live target-host validation results after DuckDNS conditional pilot waiver acknowledgment.  
> **Scope**: Single-node SQLite v1 conditional pilot only.  
> **Constraint**: `production-ready = NO` throughout. L2 with-token verification blocked by token/SSH access constraints.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Blockers remain open; operator signoff incomplete; L2 with-token auth not fully verified |
| **G2 / operator signoff** | **NOT complete** | Path 2 pilot requires Block A closure or conditional waiver plus full target-host evidence; L2 with-token gate not closed |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **L2 full pass** | **NO** | No-token deny PASS; with-token allow NOT verified due to token/SSH access blocker |
| **HA / multi-node / PostgreSQL** | **NO** | Single-node SQLite is the only supported runtime |

---

## Live Execution Context

- **Target host**: `ferrumgate.duckdns.org`
- **Expected IP**: `34.158.51.8`
- **Execution date**: `2026-05-17`
- **Waiver commit**: `19986c0 docs: record DuckDNS conditional pilot waiver` (pushed)
- **Operator acknowledgment**: Recorded 2026-05-17; Block A WAIVED/CONDITIONAL for single-node SQLite pilot only

> **Note**: All live commands were executed by the operator or in an authorized environment. This artifact records results only; no live commands were executed during artifact creation.

---

## L1 — Target Reachability & TLS (LIVE)

**Command executed**:
```bash
python3 scripts/validate_bridge_readiness.py --execute \
  --target-host ferrumgate.duckdns.org \
  --expected-ip 34.158.51.8 \
  --check-readiness-live \
  --output-dir /tmp/opencode/ferrum-bridge-l1-l3-20260517
```

**Results** (included in combined L1/L3 output):

| # | Gate | Status | Message |
|---|------|--------|---------|
| 1 | L1_dns_resolution | PASS | DNS resolved ferrumgate.duckdns.org -> 34.158.51.8 |
| 2 | L1_port_443 | PASS | Port 443 open on ferrumgate.duckdns.org |
| 3 | L1_tls_handshake | PASS | TLS handshake OK (TLSv1.3) |

- **L1 overall**: **PASS** (3/3)
- **Owner**: Engineering / Operator

---

## L2 — Authentication & Authorization (LIVE — PARTIAL)

**No-token denial command executed**:
```bash
python3 scripts/validate_bridge_readiness.py --execute \
  --target-host ferrumgate.duckdns.org \
  --expected-ip 34.158.51.8 \
  --check-auth-live \
  --output-dir /tmp/opencode/ferrum-bridge-l2-auth-no-token-20260517
```

**Results**:

| # | Gate | Status | Message |
|---|------|--------|---------|
| 1 | L1_dns_resolution | PASS | DNS resolved ferrumgate.duckdns.org -> 34.158.51.8 |
| 2 | L1_port_443 | PASS | Port 443 open on ferrumgate.duckdns.org |
| 3 | L1_tls_handshake | PASS | TLS handshake OK (TLSv1.3) |
| 4 | L2_auth_no_token_denies | PASS | No-token GET /v1/approvals returned HTTP 401 |
| 5 | L2_auth_with_token_allows | FAIL | With-token GET /v1/approvals returned HTTP 401 |

- **No-token deny**: **PASS** — unauthenticated requests are correctly rejected with HTTP 401
- **With-token allow**: **NOT VERIFIED** — no valid bearer token was available in the local execution environment
- **L2 overall**: **PARTIAL / BLOCKED** (4 passed, 1 failed, 5 total)
- **Owner**: Engineering / Operator

### L2 with-token blocker details

A remote attempt to obtain or use a valid bearer token was blocked:

- **Direct SSH to VM** (`34.158.51.8:22`): Connection timeout. SSH port not reachable from local environment.
- **IAP SSH**: `failed to connect to backend` port 22. IAP tunnel could not establish.
- **Token handling**: No bearer token value was printed, logged, or stored in this artifact.

**Implication**: The positive-path L2 auth check (valid token → HTTP 200) remains unverified. This does not indicate a service misconfiguration; it indicates an **access constraint** preventing token retrieval from the VM.

---

## L3 — Health & Readiness Probes (LIVE)

**Command executed**: Same L1/L3 combined command (see L1 section).

**Results**:

| # | Gate | Status | Message |
|---|------|--------|---------|
| 4 | L3_v1_healthz | PASS | GET /v1/healthz returned HTTP 200 |
| 5 | L3_v1_readyz | PASS | GET /v1/readyz returned HTTP 200 |
| 6 | L3_v1_readyz_deep | PASS | GET /v1/readyz/deep returned HTTP 200 |
| 7 | L3_metrics_required_counters | PASS | Required counters present |

- **L3 overall**: **PASS** (4/4; 7/7 combined with L1)
- **Owner**: Engineering / Operator

---

## Combined L1/L3 Summary

| Level | Checks | Passed | Failed | Status |
|-------|--------|--------|--------|--------|
| L1 — Reachability & TLS | 3 | 3 | 0 | **PASS** |
| L3 — Health & Readiness | 4 | 4 | 0 | **PASS** |
| **L1 + L3 combined** | **7** | **7** | **0** | **PASS** |
| L2 — Auth (no-token) | 1 | 1 | 0 | **PASS** |
| L2 — Auth (with-token) | 1 | 0 | 1 | **BLOCKED / NOT VERIFIED** |

---

## Remaining Blockers with Owners

| Blocker | Owner | Status | Next Action |
|---------|-------|--------|-------------|
| **Block A — Real owned domain** | Operator | WAIVED/CONDITIONAL | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **L2 with-token verification** | Operator | BLOCKED | Requires either: (a) local env with valid bearer token, or (b) SSH/IAP access to VM to generate/retrieve token. SSH timeout and IAP backend failure prevent remote token access. |
| **Path 2 full G2 signoff** | Operator | NOT COMPLETE | Requires Block A closure + target-host evidence + closed L2 auth gate |
| **Production-ready claim** | — | **NO** | Requires all G2/G3 gates + operator signoff + live validation + real domain |

---

## Cross-References

| Document | Purpose |
|----------|---------|
| [`2026-05-17-bridge-to-live-runbook.md`](./2026-05-17-bridge-to-live-runbook.md) | L1–L5 live gate runbook (safe-by-default, dry-run default) |
| [`2026-05-17-bridge-l0-preflight-evidence.md`](./2026-05-17-bridge-l0-preflight-evidence.md) | L0 local-only pre-flight evidence |
| [`2026-05-17-all-paths-execution-evidence.md`](./2026-05-17-all-paths-execution-evidence.md) | Path 1/2/3 execution evidence |
| [`2026-05-17-block-a-duckdns-conditional-pilot-waiver.md`](./2026-05-17-block-a-duckdns-conditional-pilot-waiver.md) | Block A DuckDNS conditional pilot waiver |
| [`../01-current-state.md`](../01-current-state.md) | Current state and completion tracker |
| [`../54-operator-signoff-packet.md`](../54-operator-signoff-packet.md) | Formal G2 signoff form |

---

## Operator / Engineering Review Statement

> This artifact accurately records live validation results as of 2026-05-17. L1 and L3 passed against `ferrumgate.duckdns.org`. L2 no-token denial passed. L2 with-token allow was not verified due to SSH/IAP access blockers, not due to service misconfiguration. No secrets or token values are present in this artifact. Production-ready remains **NO**. Full G2 operator signoff remains **NOT COMPLETE**.

---

*Artifact created: 2026-05-17. Bridge L1–L3 live evidence — records observed results only. No production-ready claim.*
