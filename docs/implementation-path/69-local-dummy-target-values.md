# 69 — Local Dummy Target Values (NOT OPERATOR EVIDENCE)

> **Status**: LOCAL-TEST ONLY — NOT G2 Evidence, NOT Production Ready, NOT Operator Evidence
> **Purpose**: Provide artificial/local-only dummy values for safe Path 2 rehearsal, runbook practice, and tooling validation without requiring real target environment access.
> **Scope**: Local host only. Single-node SQLite. No target host, SSH, domain, or TLS required.
> **Constraint**: Do not modify [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md), [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md), [`59-pilot-readiness-evidence-packet.md`](./59-pilot-readiness-evidence-packet.md), or [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md) with these values. Do not claim G2/pilot/production readiness from local evidence.

---

## ⚠️ CRITICAL WARNINGS — READ BEFORE USE

**THIS DOCUMENT IS LOCAL-TEST ONLY (DUMMY VALUES):**

- All values in this document are **ARTIFICIAL/LOCAL-ONLY** — they do NOT represent any real target environment
- Evidence generated using these values is **LABELED "LOCAL-TEST/DUMMY"** and does NOT constitute G2 completion
- No production-ready, pilot-accepted, G2-gate-complete, or operator-signed claim is made or implied
- This document is for **local rehearsal, runbook practice, and tooling validation ONLY**

**NOT OPERATOR EVIDENCE:**
- Completing local scripts (`run_local_auth_smoke.sh`, `run_local_restore_drill.sh`) validates that tooling works locally
- It does NOT validate target environment readiness
- Bridging to G2 requires completing [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) with **real** target values

**EXPLICITLY NOT CLAIMED:**
- ☐ G2 complete (any gate)
- ☐ Pilot authorized
- ☐ Production-ready
- ☐ Operator signed
- ☐ HTTP workload trigger active
- ☐ PostgreSQL/multi-node/HA operational

---

## Purpose

This document provides a **local-test-only** Path 2 non-prod target profile using safe artificial values. It enables:
- Local auth smoke checks (`run_local_auth_smoke.sh`)
- Local restore drills (`run_local_restore_drill.sh`)
- Operator rehearsal without requiring target environment access
- Runbook command-sequence practice
- Tooling validation before target deployment

Real target deployment requires completing [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) with actual infrastructure values provided by the operator.

---

## Dummy Values Table

The following table provides **LOCAL-TEST ONLY** artificial values. These are NOT real secrets, NOT real infrastructure, and NOT operator evidence.

| Category | Field | Dummy Value | Provenance | Notes |
|----------|-------|-------------|------------|-------|
| **Target URL** | Base URL | `http://127.0.0.1:18080` | LOCAL-TEST-GENERATED | Loopback only; no TLS |
| **Target Host** | FQDN/IP | `localhost` | LOCAL-TEST-GENERATED | Loopback placeholder |
| **SSH** | SSH host | `n/a-local-test-dummy` | LOCAL-TEST-GENERATED | Not applicable for local |
| **SSH** | SSH user | `local-test-user` | LOCAL-TEST-GENERATED | Dev user placeholder |
| **SSH** | SSH key | `n/a-local-test-dummy` | LOCAL-TEST-GENERATED | No SSH for local |
| **Service** | Service name | `ferrumd` | REPO-DERIVED | From binary name |
| **Storage** | Store path | `/tmp/ferrumgate-dummy-target/store/ferrumgate.db` | LOCAL-TEST-GENERATED | Temp dir; recreate each run |
| **Storage** | Store DSN | `sqlite::memory:` or file path above | LOCAL-TEST-GENERATED | In-memory or temp file |
| **Backup** | Backup dir | `/tmp/ferrumgate-dummy-target/backups` | LOCAL-TEST-GENERATED | Temp dir; recreate each run |
| **Domain** | FQDN | `n/a-local-test-dummy.example.com` | LOCAL-TEST-GENERATED | No real domain |
| **TLS** | Cert path | `n/a-local-test-dummy` | LOCAL-TEST-GENERATED | No TLS for local |
| **TLS** | Key path | `n/a-local-test-dummy` | LOCAL-TEST-GENERATED | No TLS for local |
| **TLS** | Nginx N/A | `true` | LOCAL-TEST-GENERATED | No reverse proxy for local |
| **Auth** | Auth mode | `bearer` | LOCAL-TEST-GENERATED | Matches production config |
| **Auth** | Bearer token | Auto-generated per session | LOCAL-TEST-GENERATED | Never committed; generated via `openssl rand -hex 32` |
| **Auth** | Token env var | `FERRUMD_BEARER_TOKEN` | REPO-DERIVED | Standard env var name |
| **Network** | Bind address | `127.0.0.1:18080` | LOCAL-TEST-GENERATED | Loopback + dynamic port |
| **Network** | Firewall | `localhost-only` | LOCAL-TEST-GENERATED | Loopback restriction |
| **Backup** | Scheduler | `manual` | LOCAL-TEST-GENERATED | For local rehearsal only |
| **RPO/RTO** | RPO | `n/a-local-test` | LOCAL-TEST-GENERATED | No SLA for local |
| **RTO** | RTO | `n/a-local-test` | LOCAL-TEST-GENERATED | No SLA for local |
| **Operators** | Owner | `local-test-operator` | LOCAL-TEST-GENERATED | Placeholder only |
| **Evidence** | Evidence dir | `/tmp/ferrumgate-dummy-target/evidence` | LOCAL-TEST-GENERATED | Temp dir |
| **Workload** | HTTP trigger | `no` | LOCAL-TEST-GENERATED | No HTTP workload |
| **G2 Flags** | G2 complete | `no` | NOT APPLICABLE | G2 requires operator action |
| **G2 Flags** | Pilot authorized | `no` | NOT APPLICABLE | Pilot requires doc 54 signoff |
| **G2 Flags** | Production-ready | `no` | NOT APPLICABLE | FerrumGate v1 is RC-ready/conditional |

**Provenance Key:**
- **LOCAL-TEST-GENERATED**: Safe artificial values created for local testing only
- **REPO-DERIVED**: Values obtained from repository files/templates
- **NOT APPLICABLE**: Requires operator action on real target environment

---

## Local-Only Command Sequence

The following sequence uses dummy values for local rehearsal only.

### Prerequisites

```bash
# Verify binaries exist
which ferrumd || cargo build --release --bin ferrumd
which ferrumctl || cargo build --release --bin ferrumctl

# Create temp directories
mkdir -p /tmp/ferrumgate-dummy-target/store
mkdir -p /tmp/ferrumgate-dummy-target/backups
mkdir -p /tmp/ferrumgate-dummy-target/evidence
```

### Step 1 — Generate Dummy Token

```bash
# Generate a local-only dummy token (DO NOT USE IN PRODUCTION)
DUMMY_TOKEN="dummy-local-test-$(openssl rand -hex 16)"
echo "Dummy token generated (local-test only): ${DUMMY_TOKEN:0:20}..."
```

### Step 2 — Create Dummy Config

```bash
cat > /tmp/ferrumgate-dummy-target/dummy-ferrumgate.toml << EOF
[server]
bind_addr = "127.0.0.1:18080"
store_dsn = "sqlite:/tmp/ferrumgate-dummy-target/store/ferrumgate.db"
auth_mode = "bearer"
bearer_token = "$DUMMY_TOKEN"
allow_insecure_nonlocal_bind = false
log_filter = "info"
EOF

echo "Dummy config created at: /tmp/ferrumgate-dummy-target/dummy-ferrumgate.toml"
```

### Step 3 — Start ferrumd (Dummy Target)

```bash
# Start ferrumd with dummy config
./target/release/ferrumd --config /tmp/ferrumgate-dummy-target/dummy-ferrumgate.toml &
FERRUMD_PID=$!
echo "ferrumd started with PID: $FERRUMD_PID"

# Wait for startup
sleep 3

# Verify running
pgrep -f "ferrumd" || echo "WARNING: ferrumd not running"
```

### Step 4 — Probe Sequence (Expected Results)

```bash
DUMMY_BASE="http://127.0.0.1:18080"

echo "=== PROBE SEQUENCE (Dummy Target) ==="

# healthz — expect 200
curl -s -o /dev/null -w "%{http_code}" "${DUMMY_BASE}/v1/healthz"
# Expected: 200

# readyz — expect 200
curl -s -o /dev/null -w "%{http_code}" "${DUMMY_BASE}/v1/readyz"
# Expected: 200

# readyz/deep — expect 200
curl -s -o /dev/null -w "%{http_code}" "${DUMMY_BASE}/v1/readyz/deep"
# Expected: 200

# metrics — expect 200 + prometheus format
curl -s "${DUMMY_BASE}/v1/metrics" | head -3
# Expected: Prometheus text format

# approvals (no auth) — expect 401
curl -s -o /dev/null -w "%{http_code}" "${DUMMY_BASE}/v1/approvals"
# Expected: 401

# approvals (wrong token) — expect 401
curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer wrong-token" \
    "${DUMMY_BASE}/v1/approvals"
# Expected: 401

# approvals (correct token) — expect 200
curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $DUMMY_TOKEN" \
    "${DUMMY_BASE}/v1/approvals"
# Expected: 200
```

### Step 5 — Cleanup

```bash
# Stop ferrumd
kill $FERRUMD_PID 2>/dev/null || pkill -f "ferrumd"

# Remove temp directories
rm -rf /tmp/ferrumgate-dummy-target

echo "Dummy target cleanup complete"
```

---

## Expected Probe Results (Dummy Target)

| Probe | Endpoint | Expected HTTP Code | Auth Required | Dummy Result |
|-------|----------|-------------------|---------------|--------------|
| Shallow health | `/v1/healthz` | 200 | No | PASS (local-test) |
| Shallow ready | `/v1/readyz` | 200 | No | PASS (local-test) |
| Deep readiness | `/v1/readyz/deep` | 200 | No | PASS (local-test) |
| Prometheus metrics | `/v1/metrics` | 200 | No | PASS (local-test) |
| Approvals (no auth) | `/v1/approvals` | 401 | No | PASS (local-test) |
| Approvals (wrong token) | `/v1/approvals` | 401 | Yes | PASS (local-test) |
| Approvals (correct token) | `/v1/approvals` | 200 | Yes | PASS (local-test) |

**Note**: All results are labeled "LOCAL-TEST/DUMMY" — these do NOT satisfy any G2 gate requirement.

---

## Evidence Layout (Dummy/Local-Test)

Evidence from local dummy target rehearsal is **NOT G2 evidence**. It is stored in temp directories and labeled accordingly.

| Evidence Type | Temp Dir Pattern | Notes |
|---------------|------------------|-------|
| Probe output | `/tmp/ferrumgate-dummy-target/probe_output.txt` | Labeled "LOCAL-TEST" |
| Config file | `/tmp/ferrumgate-dummy-target/*.toml` | Contains dummy token |
| Store database | `/tmp/ferrumgate-dummy-target/store/` | Temp SQLite |
| Backup output | `/tmp/ferrumgate-dummy-target/backups/` | Temp backups |
| Drill logs | `/tmp/ferrumgate-dummy-target/evidence/` | Labeled "LOCAL-TEST/DUMMY" |

**Evidence Label**: All evidence from dummy target rehearsal must be clearly labeled:
```
# FerrumGate v1 — LOCAL-TEST/DUMMY EVIDENCE
# NOT G2 EVIDENCE — NOT OPERATOR EVIDENCE — NOT PRODUCTION READY
```

---

## Boundaries — What This Document Does NOT Provide

| Dimension | What This Doc Provides | What This Doc Does NOT Provide |
|-----------|----------------------|-------------------------------|
| Real target values | Artificial placeholders | Real infrastructure values |
| SSH access | n/a | Real SSH credentials |
| TLS certificates | n/a | Real certificates |
| G2 evidence | None | G2.1–G2.8 evidence |
| Operator signoff | None | Doc 54 signature |
| Pilot authorization | None | Pilot acceptance |
| Production readiness | None | Production-ready claim |
| HTTP workload | No | Real workload trigger |
| Backup schedule | Manual only | Automated scheduler |
| RPO/RTO | n/a | Real SLA acceptance |

---

## Relationship to Other Docs

| Doc | Relationship | Notes |
|-----|--------------|-------|
| [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) | Do NOT modify with dummy values | Real target spec; operator fills |
| [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md) | Do NOT modify with dummy values | Real questionnaire; operator fills |
| [`64-local-staging-simulation-guide.md`](./64-local-staging-simulation-guide.md) | Reference | Broader local simulation guide |
| [`local-nonprod-target-profile.md`](./local-nonprod-target-profile.md) | Reference | Alternative local-test profile |
| [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Reference | Path 2 execution plan context |
| [`68-path-2-operator-handoff-packet.md`](./68-path-2-operator-handoff-packet.md) | Links to this doc | As "optional rehearsal only" |
| [`66-path-2-operator-handoff.md`](./66-path-2-operator-handoff.md) | Links to this doc | As "optional rehearsal only" |

---

## Cross-References

| This Doc | Links To | Purpose |
|----------|---------|---------|
| `69-local-dummy-target-values.md` | [`64-local-staging-simulation-guide.md`](./64-local-staging-simulation-guide.md) | Local simulation context |
| `69-local-dummy-target-values.md` | [`68-path-2-operator-handoff-packet.md`](./68-path-2-operator-handoff-packet.md) | Operator quick-reference |
| `69-local-dummy-target-values.md` | [`run_local_auth_smoke.sh`](../../scripts/run_local_auth_smoke.sh) | Local auth smoke script |
| `69-local-dummy-target-values.md` | [`run_local_restore_drill.sh`](../../scripts/run_local_restore_drill.sh) | Local restore drill script |
| `69-local-dummy-target-values.md` | [`run_pre_target_gate.sh`](../../scripts/run_pre_target_gate.sh) | Pre-target validation gate |

---

## Disclaimer

**LOCAL-TEST ONLY — NOT G2 EVIDENCE — NOT OPERATOR EVIDENCE — NOT PRODUCTION READY**

- No G2 complete claim is made by using these dummy values
- No pilot accepted or production-ready claim is made
- Dummy evidence is labeled "LOCAL-TEST/DUMMY" and cannot substitute for target environment evidence
- FerrumGate v1 is RC-ready/conditional for single-node SQLite only
- PostgreSQL/multi-node/HA are not implemented
- For G2 completion, operator must complete [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) with real target values and execute target-environment drills per [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md)
- Phase 3 remains blocked until G2/operator evidence is complete

---

*Created: 2026-05-06. LOCAL-TEST/DUMMY ONLY documentation — no G2 claim, no production-ready claim, no operator evidence.*
