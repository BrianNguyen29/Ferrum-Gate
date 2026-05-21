# Target Evidence — 2026-05-21

> **Status**: Target-host validation evidence (abbreviated / partial). No production-ready claim. No full G2 completion claimed.  
> **Purpose**: Record target token rotation, abbreviated SLO workload, target-mode MCP smoke, Helm static validation, PostgreSQL token repo completion, and domain posture.  
> **Scope**: Single-node SQLite v1 conditional pilot only.  
> **Constraint**: `production-ready = NO` throughout. Block A remains WAIVED/CONDITIONAL.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Block A remains conditional; full SLO long-run not executed; live Helm install not performed |
| **G2 / operator signoff** | **NOT complete** | Requires Block A closure or full conditional waiver plus complete target-host evidence |
| **Block A — Real owned domain** | **WAIVED/CONDITIONAL** | DuckDNS accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure |
| **Full SLO certification** | **NO** | Only abbreviated target workload executed; not a full SLO certification run |
| **HA / multi-node / PostgreSQL production** | **NO** | PostgreSQL token repo is code-complete; target host remains SQLite single-node |
| **Helm live cluster validated** | **NO** | `helm lint` and `helm template` passed; live cluster install blocked by kind timeout |

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

## 2. Target Bearer Token Rotation

### 2.1 Token generation

A new bearer token was generated locally with `openssl rand -hex 32`.  
The token value was **never printed, logged, or committed** to any repository or artifact.

### 2.2 Token installation

The token was installed on the VM through a temporary `startup-script` metadata path.  
After installation was confirmed working, the startup-script metadata was removed.

### 2.3 Startup metadata removal verification

Command:

```bash
gcloud compute instances describe ferrumgate-nonprod \
  --project=fairy-b13f4 \
  --zone=asia-southeast1-a \
  --format='value(metadata.items.startup-script)'
```

Result: **empty string** — startup-script metadata successfully removed.

### 2.4 Metrics endpoint confirmation

`GET https://ferrumgate.duckdns.org/v1/metrics` with the installed token returned **HTTP 200**.  
This confirms the token is active and the target gateway is reachable over HTTPS with bearer auth.

### 2.5 SSH availability caveat

SSH to the VM remained unavailable / timed out after reset.  
Service status could not be confirmed via SSH. The HTTP 200 from the metrics endpoint is the operational confirmation signal.

### 2.6 Pilot readiness script result (informational)

`scripts/check_pilot_readiness.py` was executed locally against the target:

| Probe | Result |
|-------|--------|
| metrics | PASS (HTTP 200) |
| shallow readiness | FAIL — `ferrumctl` binary missing from local PATH |
| deep readiness | FAIL — `ferrumctl` binary missing from local PATH |
| functional readiness | FAIL — `ferrumctl` binary missing from local PATH |

**Root cause**: `ferrumctl` was not built locally; only metrics probe could run.  
This result is **not used as final success evidence**. The metrics HTTP 200 and MCP smoke (below) are the primary operational confirmations.

---

## 3. Target SLO Abbreviated Live Workload

### 3.1 Scope

This is an **abbreviated** target-host workload run, not a full SLO certification.  
Phases were shortened for feasibility. Results establish that the target gateway can serve authenticated requests under light load without errors.

### 3.2 Workload parameters

| Phase | Duration | Rate (rps) |
|-------|----------|------------|
| baseline | 5 s | 0 |
| target | 30 s | 1.0 |
| spike | 10 s | 2.0 |
| cooldown | 5 s | 0 |

### 3.3 Results summary

| Metric | Value |
|--------|-------|
| Total requests | 39 |
| HTTP 200 | 39 |
| Errors | 0 |

**Target phase latency** (30 s @ 1 rps):

| Metric | Value |
|--------|-------|
| p50 | 191.385 ms |
| p95 | 384.841 ms |
| p99 | 1000.073 ms |
| max | 1183.25 ms |

**Spike phase latency** (10 s @ 2 rps):

| Metric | Value |
|--------|-------|
| p50 | 191.21 ms |
| p95 | 418.251 ms |
| p99 | 527.098 ms |
| max | 554.31 ms |

**Readyz / deep records**: 4 probes, all HTTP 200.

### 3.4 Caveats

- **Abbreviated run**: Phase durations are much shorter than the canonical runbook (300–1800 s). This is evidence that the target responds correctly under light load, not a performance certification.
- **Network path**: Latencies include public internet round-trip from local workstation to `asia-southeast1-a`.
- **No operator signoff**: This is an engineering-run abbreviated workload; operator has not reviewed or ratified.
- **NOT full SLO certification**.

### 3.5 Workload generator fix

During the run, `scripts/run_real_workload_generator.py` encountered a bug when mid-run `readyz` records lacked a `probe_number` field. The script was fixed to handle missing `probe_number` gracefully.  
Verification: `py_compile` passed; local simulation passed.

---

## 4. Target-Mode MCP Smoke

### 4.1 Execution

Command:

```bash
FERRUM_GATEWAY_BEARER_TOKEN=<temporary local token file> \
bash scripts/run_mcp_lifecycle_smoke.sh \
  --gateway-url https://ferrumgate.duckdns.org
```

> The temporary token file is local-only, outside the repository, and is deleted after validation. No token value appears in this artifact.

### 4.2 Results

| Metric | Value |
|--------|-------|
| Passed | 15 / 15 |
| Failed | 0 |

Validations performed:

- Target gateway reachable over HTTPS.
- `tools/list` returned 19 tools.
- Required tools validated against current registry (`bash scripts/validate_mcp_required_tools.sh` passed).
- Lifecycle flow: submit → evaluate → mint → list returned results.

### 4.3 Sanitized log location

`/tmp/opencode/ferrumgate-target-mcp-smoke-20260521.log`  
(Contains no secrets; token redacted by script design.)

### 4.4 MCP script fixes applied

- Target gateway mode added to `run_mcp_lifecycle_smoke.sh`.
- `REQUIRED_TOOLS` updated to match current registry.
- `validate_mcp_required_tools.sh` passed.

---

## 5. Helm Static Validation

### 5.1 Tooling

| Tool | Version | Path |
|------|---------|------|
| Helm | v3.15.4 | `/tmp/opencode/linux-amd64/helm` |

### 5.2 `helm lint`

Command:

```bash
/tmp/opencode/linux-amd64/helm lint deploy/helm/ferrumgate
```

Result: **PASS** — 1 chart linted, 0 charts failed.

### 5.3 `helm template`

Command:

```bash
/tmp/opencode/linux-amd64/helm template ferrumgate deploy/helm/ferrumgate
```

Result: **PASS** — rendered ServiceAccount, Secret, Service, and Deployment manifests without syntax errors.

### 5.4 Live cluster attempt

| Tool | Version | Path |
|------|---------|------|
| kind | v0.23.0 | downloaded to `/tmp/opencode/` |

Command:

```bash
kind create cluster --name ferrumgate-helm-smoke
```

Result: **TIMEOUT at 300 s** — cluster creation did not complete.  
Post-attempt check: `kind get clusters` returned no clusters.

**Conclusion**: Live Helm install is **blocked/deferred** — no successful local cluster available for install. Static chart evidence (`lint` + `template`) passes.

---

## 6. PostgreSQL Scoped Token Repository

### 6.1 Implementation

- `crates/ferrum-store/src/postgres/tokens.rs` — implemented.
- `crates/ferrum-store/src/postgres/mod.rs` — compile-time test added.

### 6.2 Validation performed

| Check | Command | Result |
|-------|---------|--------|
| Format | `cargo fmt --all` | PASS |
| Check (workspace) | `cargo check --workspace` | PASS |
| Check (postgres feature) | `cargo check --package ferrum-store --features postgres` | PASS |
| Clippy (postgres feature) | `cargo clippy --package ferrum-store --features postgres` | PASS |
| Tests (postgres feature) | `cargo test --package ferrum-store --features postgres` | PASS (72 tests) |
| Workspace tests | `cargo test --workspace` | PASS |

### 6.3 Status

Code-complete and passing all automated checks. PostgreSQL token store is ready for integration when a PostgreSQL-backed target host is deployed.

---

## 7. Domain / Block A Posture

| Item | Status |
|------|--------|
| Real owned domain provided | NO |
| DuckDNS domain | `ferrumgate.duckdns.org` (conditional pilot only) |
| DNS A record | Resolves to `34.158.51.8` |
| HTTPS | Active (TLSv1.3) |
| Block A | **WAIVED/CONDITIONAL** |

**Rationale**: No real owned domain was provided by the operator. DuckDNS remains accepted only for the conditional single-node pilot. Full G2 closure and any production-ready claim require a real domain.

---

## 8. Cross-References

| Document | Purpose |
|----------|---------|
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Phase-by-phase evidence checklist |
| [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md) | Blocker tracking |
| [`docs/production-readiness-v2/03-target-mcp-live-workload-plan.md`](../../production-readiness-v2/03-target-mcp-live-workload-plan.md) | MCP target-host plan |
| [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) | SLO validation procedure |
| [`2026-05-19-slo-target-preflight-blocked-evidence.md`](./2026-05-19-slo-target-preflight-blocked-evidence.md) | Prior blocked preflight (2026-05-19) |
| [`2026-05-19-slo-local-baseline-evidence.md`](./2026-05-19-slo-local-baseline-evidence.md) | Local baseline evidence |
| [`2026-05-20-dep5-helm-scaffold-evidence.md`](./2026-05-20-dep5-helm-scaffold-evidence.md) | Helm scaffold evidence |
| [`2026-05-20-scoped-token-implementation-evidence.md`](./2026-05-20-scoped-token-implementation-evidence.md) | Scoped token implementation evidence |

---

## 9. Operator / Engineering Review Statement

> This artifact accurately records target-host validation results as of 2026-05-21. A new bearer token was generated, installed via temporary metadata, and the metadata was subsequently removed and verified empty. An abbreviated SLO workload executed successfully against the target (39 requests, 0 errors). Target-mode MCP smoke passed 15/15. Helm static validation (`lint` + `template`) passed; live cluster install remains blocked by kind timeout. PostgreSQL scoped token repository is code-complete and passing all checks. No secrets or token values are present in this artifact. Production-ready remains **NO**. Full G2 operator signoff remains **NOT COMPLETE**. Block A remains **WAIVED/CONDITIONAL**.

---

*Artifact created: 2026-05-21. Target SLO / MCP / Helm / Domain / PG token repo evidence — records observed results only. No production-ready claim.*
