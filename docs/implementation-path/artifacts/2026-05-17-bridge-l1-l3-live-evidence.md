# Bridge L1–L3 Live Evidence — 2026-05-17

> **Status**: Live validation evidence. No production-ready claim. No full G2 completion claimed.  
> **Purpose**: Record live target-host validation results after DuckDNS conditional pilot waiver acknowledgment.  
> **Scope**: Single-node SQLite v1 conditional pilot only.  
> **Constraint**: `production-ready = NO` throughout. L2 with-token verification initially blocked by token/SSH access constraints, then unblocked via temporary firewall change, and ultimately remediated after root-cause fix on the VM.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Blockers remain open; operator signoff incomplete |
| **G2 / operator signoff** | **NOT complete** | Path 2 pilot requires Block A closure or conditional waiver plus full target-host evidence |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **L2 full pass** | **PASS (after remediation)** | No-token deny PASS; with-token allow PASS after root-cause fix on VM (see L2 Recovery section below). Historical initial state: blocked by SSH/firewall. |
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

## L2 — Authentication & Authorization (LIVE)

### L2 Initial State (Historical)

**No-token denial command executed**:
```bash
python3 scripts/validate_bridge_readiness.py --execute \
  --target-host ferrumgate.duckdns.org \
  --expected-ip 34.158.51.8 \
  --check-auth-live \
  --output-dir /tmp/opencode/ferrum-bridge-l2-auth-no-token-20260517
```

**Initial results**:

| # | Gate | Status | Message |
|---|------|--------|---------|
| 1 | L1_dns_resolution | PASS | DNS resolved ferrumgate.duckdns.org -> 34.158.51.8 |
| 2 | L1_port_443 | PASS | Port 443 open on ferrumgate.duckdns.org |
| 3 | L1_tls_handshake | PASS | TLS handshake OK (TLSv1.3) |
| 4 | L2_auth_no_token_denies | PASS | No-token GET /v1/approvals returned HTTP 401 |
| 5 | L2_auth_with_token_allows | FAIL | With-token GET /v1/approvals returned HTTP 401 |

- **No-token deny**: **PASS** — unauthenticated requests correctly rejected with HTTP 401
- **With-token allow**: **NOT VERIFIED** — no valid bearer token was available in the local execution environment
- **L2 overall (initial)**: **PARTIAL / BLOCKED** (4 passed, 1 failed, 5 total)

#### Initial blocker details

A remote attempt to obtain or use a valid bearer token was blocked:

- **Direct SSH to VM** (`34.158.51.8:22`): Connection timeout. SSH port not reachable from local environment.
- **IAP SSH**: `failed to connect to backend` port 22. IAP tunnel could not establish.
- **Token handling**: No bearer token value was printed, logged, or stored in this artifact.

**Implication (initial)**: The positive-path L2 auth check remained unverified due to an **access constraint**, not a service misconfiguration.

---

### L2 Recovery — Unblocking & Remediation

#### Step 1 — Temporary firewall unblocking

- **Operator IP at time of work**: `1.55.106.164`
- **Firewall rule**: `ferrumgate-nonprod-fw-ssh`
- **Initial source ranges**: `["118.69.4.63/32"]`
- **Temporary change**: Added `1.55.106.164/32` to source ranges to enable SSH access for investigation
- **Restored after work**: Source ranges reverted to `["118.69.4.63/32"]` and verified

#### Step 2 — Safe L2 with-token probe on VM

With temporary SSH access enabled, a safe probe was executed on the VM. Results:

| Signal | Value |
|--------|-------|
| `L2_AUTH_NO_TOKEN_HTTP` | `401` |
| `L2_AUTH_WITH_TOKEN_HTTP` | `500` |
| `L2_AUTH_WITH_TOKEN_BODY` | `database error ... no such table: approvals` |
| `READYZ_DEEP_HTTP` | `200` |
| Service status | active |

**Observation**: The service was running but returned HTTP 500 on authenticated requests because the `approvals` table did not exist.

#### Step 3 — Root-cause analysis

Investigation of `/etc/ferrumgate/ferrumgate.toml` on the VM found:

- `[server] bind_addr = "0.0.0.0:19080"` was present
- `[server] store_dsn` was **missing**
- A `[store]` section existed, but `ferrumd` config precedence is: CLI args > env vars > config file (`[server]`) > defaults
- Environment variable `FERRUMD_STORE_DSN` was **unset**
- Therefore `ferrumd` fell back to the default: `sqlite::memory:`
- In-memory SQLite does not survive service restarts and had no pre-created schema, causing the missing `approvals` table

**Root cause**: Missing `store_dsn` in the `[server]` section of `ferrumgate.toml`, with no env override, causing an in-memory SQLite database to be used instead of the intended on-disk database.

#### Step 4 — Remediation on VM

1. **Backup**: `cp /etc/ferrumgate/ferrumgate.toml /etc/ferrumgate/ferrumgate.toml.pre-l2-20260517T172236Z.bak`
2. **Insert `store_dsn`**: Added `[server] store_dsn = "sqlite:///var/lib/ferrumgate/ferrumgate.db"`
3. **Create database**: `touch /var/lib/ferrumgate/ferrumgate.db`
4. **Set ownership**: `chown ferrumgate:ferrumgate /var/lib/ferrumgate/ferrumgate.db`
5. **Set mode**: `chmod 640 /var/lib/ferrumgate/ferrumgate.db`
6. **Restart**: `systemctl restart ferrumgate.service`

> **Note**: No bearer token values were printed, logged, or committed during this process.

#### Step 5 — Post-remediation verification

| Signal | Value |
|--------|-------|
| Service status | active |
| `.tables` | includes `approvals` |
| `PRAGMA integrity_check` | `ok` |
| `L2_AUTH_NO_TOKEN_HTTP` | `401` |
| `L2_AUTH_WITH_TOKEN_HTTP` | `200` |
| `READYZ_DEEP_HTTP` | `200` |

- **No-token deny**: **PASS**
- **With-token allow**: **PASS**
- **L2 overall (after remediation)**: **PASS**
- **Owner**: Engineering / Operator

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
| L2 — Auth (with-token) | 1 | 1 | 0 | **PASS (after remediation)** |

---

## Remaining Blockers with Owners

| Blocker | Owner | Status | Next Action |
|---------|-------|--------|-------------|
| **Block A — Real owned domain** | Operator | WAIVED/CONDITIONAL | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **L2 with-token verification** | Operator | **CLOSED (remediated)** | Root cause fixed (missing `store_dsn` → in-memory SQLite). No-token deny PASS; with-token allow PASS. Firewall restored after work. |
| **Path 2 full G2 signoff** | Operator | NOT COMPLETE | Requires Block A closure or conditional waiver + target-host evidence + operator signoff |
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

> This artifact accurately records live validation results as of 2026-05-17. L1 and L3 passed against `ferrumgate.duckdns.org`. L2 no-token denial passed. L2 with-token allow initially failed due to SSH/firewall access constraints, was investigated after temporary firewall unblocking, and ultimately passed after root-cause remediation (missing `store_dsn` in config causing in-memory SQLite). No secrets or token values are present in this artifact. Production-ready remains **NO**. Full G2 operator signoff remains **NOT COMPLETE**.

---

*Artifact created: 2026-05-17. Bridge L1–L3 live evidence — records observed results only. No production-ready claim.*
