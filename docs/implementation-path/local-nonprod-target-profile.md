# Local Non-Prod Target Profile (LOCAL-TEST ONLY)

> **Status**: LOCAL-TEST ONLY — Not G2 Evidence, Not Production Ready
> **Purpose**: Provide a local-test-only Path 2 non-prod target profile using safe local values for rehearsal and drill execution.
> **Scope**: Local host only. Single-node SQLite. No target host, SSH, domain, or TLS required.
> **Constraint**: Do not modify [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) or [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md). Do not commit real secrets. Do not claim G2/pilot/production readiness from local evidence.

---

## ⚠️ IMPORTANT WARNINGS

**THIS DOCUMENT IS LOCAL-TEST ONLY:**
- This profile uses artificial/local-only values that do NOT represent a real target environment
- Local evidence from this profile does NOT constitute G2 completion
- No production-ready, pilot-accepted, or G2-gate-complete claim is made or implied
- This document is for local rehearsal/drills only

**NOT G2 EVIDENCE:**
- Completion of local scripts (`run_local_auth_smoke.sh`, `run_local_restore_drill.sh`) validates tooling works locally
- It does NOT validate target environment readiness
- Bridging to G2 requires completing [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) with real target values

---

## Purpose

This document provides a LOCAL-TEST ONLY Path 2 non-prod target profile using safe local values.
It enables:
- Local auth smoke checks (`run_local_auth_smoke.sh`)
- Local restore drills (`run_local_restore_drill.sh`)
- Operator rehearsal without requiring target environment access

Real target deployment requires completing [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) with actual infrastructure values.

---

## Local-Test Field Table

The following table distinguishes local-test values from values required for real target deployment.

| Field | Value (Local-Test) | Provenance | Notes |
|-------|-------------------|------------|-------|
| Target URL | `http://127.0.0.1:8080` | LOCAL-TEST-GENERATED | Loopback only; no TLS |
| SSH host | `n/a-local-test` | LOCAL-TEST-GENERATED | Not applicable for local |
| SSH user | `local-dev` | LOCAL-TEST-GENERATED | Dev user for local drills |
| SSH key | `n/a-local-test` | LOCAL-TEST-GENERATED | No SSH for local |
| Service name | `ferrumd` | REPO-DERIVED | From binary name |
| Store path | `/tmp/ferrumgate-local-nonprod/ferrumgate.db` | LOCAL-TEST-GENERATED | Temp dir; recreate each run |
| Backup dir | `/tmp/ferrumgate-local-nonprod/backups` | LOCAL-TEST-GENERATED | Temp dir; recreate each run |
| Domain | `n/a-local-test` | LOCAL-TEST-GENERATED | No real domain |
| TLS cert path | `n/a-local-test` | LOCAL-TEST-GENERATED | No TLS for local |
| TLS key path | `n/a-local-test` | LOCAL-TEST-GENERATED | No TLS for local |
| Auth mode | `bearer` | LOCAL-TEST-GENERATED | Matches production config |
| Bearer token | Auto-generated locally | LOCAL-TEST-GENERATED | Never committed; generated per session |
| Scheduler | `manual` | LOCAL-TEST-GENERATED | For local rehearsal only |
| RPO/RTO | `n/a-local-test` | LOCAL-TEST-GENERATED | No SLA for local |
| Operator owner | `local-dev` | LOCAL-TEST-GENERATED | Local rehearsal owner placeholder |
| Evidence dir | `/tmp/ferrumgate-local-nonprod/evidence` | LOCAL-TEST-GENERATED | Temp dir |
| Network/firewall | `localhost only` | LOCAL-TEST-GENERATED | Loopback restriction |

**Provenance Key:**
- **LOCAL-TEST-GENERATED**: Safe artificial values created for local testing only
- **REPO-DERIVED**: Values obtained from repository files/templates
- **MANUAL-REQUIRED-FOR-REAL-TARGET**: Values the operator must provide from their infrastructure

---

## Field Values for Real Target (Reference)

For actual Path 2 deployment, the following fields must be completed in [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md):

| Field | Manual-Required | Reference |
|-------|-----------------|-----------|
| Target FQDN/IP | PROVIDE | Must have DNS A record |
| SSH user | PROVIDE | Dedicated deployment user |
| TLS certificate path | PROVIDE | From CA or certbot |
| TLS private key path | PROVIDE | From CA or certbot |
| SQLite store path | PROVIDE | Must be writable |
| Backup output directory | PROVIDE | Must be writable |
| Bearer token | OPERATOR-GENERATED | `openssl rand -hex 32` |

---

## Local Test Commands

The following commands were executed to validate local tooling:

### Auth Smoke Check

```bash
cd /home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify
bash scripts/run_local_auth_smoke.sh
```

This script:
1. Builds `ferrumd` if not found
2. Creates a temp config with `auth_mode=bearer` and auto-generated token
3. Starts `ferrumd` on a dynamic free port (18080-18180)
4. Tests public endpoints (no auth): `/v1/healthz`, `/v1/readyz`, `/v1/readyz/deep`, `/v1/metrics`
5. Tests protected endpoint (with auth): `/v1/approvals` with correct and incorrect tokens
6. Reports pass/fail counts
7. Cleans up temp files on exit

**Expected output:** `AUTH SMOKE: ALL CHECKS PASSED`

### Restore Drill

```bash
cd /home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify
bash scripts/run_local_restore_drill.sh
```

This script:
1. Builds `ferrumctl` if not found
2. Creates a temp store directory with test SQLite database
3. Verifies source store integrity (`ferrumctl backup verify`)
4. Creates backup (`ferrumctl backup create`)
5. Verifies backup integrity
6. Restores backup to new location (`ferrumctl backup restore --confirm`)
7. Verifies restored database integrity
8. Compares original and restored data (if `sqlite3` available)
9. Cleans up temp files on exit

**Expected output:** `LOCAL RESTORE DRILL COMPLETE`

---

## Local Evidence Output

Local evidence is written to temp directories that are cleaned up on script exit. The scripts use their own temp directories (`mktemp -d`) which may differ from the values in the field table above, but follow the same safety assumptions:

| Evidence Type | Temp Dir Pattern | Notes |
|---------------|------------------|-------|
| Auth smoke | `/tmp/ferrumgate-auth-smoke-*` | Auto-cleaned |
| Restore drill | `/tmp/ferrumgate-restore-drill-*` | Auto-cleaned |

---

## Relationship to Other Docs

| Doc | Relationship |
|-----|--------------|
| [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) | Real target profile; do not modify via this doc |
| [`65-path-2-target-questionnaire.md`](./65-path-2-target-questionnaire.md) | Target questionnaire; do not modify via this doc |
| [`64-local-staging-simulation-guide.md`](./64-local-staging-simulation-guide.md) | Local staging simulation guide (broader scope) |
| [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md) | Path 2 execution plan context |

---

## Disclaimer

**LOCAL-TEST ONLY — NOT G2 EVIDENCE — NOT PRODUCTION READY**

- No G2 complete claim is made by completing local drills
- No pilot accepted or production-ready claim is made
- Local evidence is labeled "local-test" and cannot substitute for target environment evidence
- FerrumGate v1 is RC-ready/conditional for single-node SQLite only
- PostgreSQL/multi-node/HA are not implemented
- For G2 completion, operator must complete [`63-path-2-target-environment-spec.md`](./63-path-2-target-environment-spec.md) with real target values and execute target-environment drills per [`61-path-2-execution-plan.md`](./61-path-2-execution-plan.md)

---

*Created: 2026-05-03. LOCAL-TEST ONLY documentation — no G2 claim, no production-ready claim.*
