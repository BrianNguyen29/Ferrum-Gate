# 98 — Phase 3D G2 Readiness Checklist

**Prepared for**: BrianNguyen

**Status**: SUPERSEDED — Doc 59 and doc 54 were signed by BrianNguyen on 09/05/2026 for conditional single-node SQLite pilot scope. B3/B4/B5 closed via delegated authority on 2026-05-15. This document is retained as historical rehearsal evidence.

> **Important**: This document did **not** complete any G2 gate at the time of preparation. It maps historical rehearsal evidence to gate readiness. Operator signoff was later completed via docs 54/59 on 09/05/2026. See canonical docs 54/59 and 115/66 for current status.

---

## Non-Claims

> **IMPORTANT**: Phase 3D carries the following explicit non-claims:
> - NOT production-ready
> - NOT G2 complete
> - NOT pilot authorized
> - NOT operator signoff
> - Evidence is from GCP non-prod rehearsal only (nip.io TLS, SQLite single-node)
> - nip.io is temporary; not suitable for production
> - PostgreSQL/multi-node/HA not validated
> - Canonical docs 54/58/59/63/65 still required for production pilot signoff

---

## G2 Gate Mapping Summary

| Gate | Name | Status | Evidence Basis |
|------|------|--------|----------------|
| G2.1 | Target workload model | **operator-required** | No production workload model provided |
| G2.2 | Bearer auth + TLS + firewall | **ready** | GCP non-prod TLS via nip.io + Caddy; 401/200 auth confirmed |
| G2.3 | Backup schedule evidence | **partial** | Timer enabled; manual backup confirmed; operator must define production schedule |
| G2.4 | Restore drill | **ready** | Restore drill passed; INTEGRITY=ok; 14 tables; copy removed |
| G2.5 | RPO/RTO acceptance | **operator-required** | Operator must accept RPO/RTO for target workload |
| G2.6 | Production evaluation framework | **partial** | Local evaluation passed; operator-completed framework pending |
| G2.7 | Accepted-risk review | **partial** | Repo-side evidence present; operator signature pending on doc 54 |
| G2.8 | Compensate noop risk acceptance | **partial** | Compensate flow exercised; operator acceptance of noop-backed adapters pending |

**Conservative conclusion**: G2 is not complete. Phase 3D evidence suggests the GCP non-prod target is **ready for operator review only** if the evidence packet is acceptable to the operator. All G2 gates were open pending operator signoff at that time.

> **Superseded (2026-05-15)**: Doc 59 and doc 54 were subsequently signed by BrianNguyen on 09/05/2026 for conditional single-node SQLite pilot scope. B3/B4/B5 were closed via delegated authority on 2026-05-15. Conditional single-node SQLite pilot readiness is ACCEPTABLE/YES (scoped only). Production-ready remains NO.

---

## G2.1 — Target Workload Model

**Status**: `operator-required`

### Gate Description
Operator must provide a production workload model showing sustained write rate ≤300 writes/s for SQLite single-node, or confirm single-node topology is acceptable for the target workload.

### Current Evidence

No production workload model has been provided. This gate cannot be assessed without operator-provided workload data.

### What Is Needed

- Operator-defined expected sustained write rate (must be ≤300 writes/s for Phase 1 SQLite)
- Operator-defined expected peak write rate
- Operator-defined daily write volume
- Operator confirmation of single-node topology fit

### Action

**Operator action required**: Complete workload model and confirm single-node fit before G2.1 can be marked complete.

---

## G2.2 — Bearer Auth + TLS + Firewall

**Status**: `ready`

### Gate Description
Operator confirms bearer auth mode with operator-managed token, TLS termination at reverse proxy, and firewall configuration appropriate for the target environment.

### Current Evidence (GCP Non-Prod)

| Check | Expected | Observed |
|-------|----------|----------|
| TLS termination | Caddy reverse proxy | Caddy v2.11.2 active |
| TLS domain | nip.io (temporary) | `34-158-51-8.nip.io` |
| Auth: no token | 401 | 401 |
| Auth: with token | 200 | 200 |
| Firewall: SSH 22 | From allowlist only | From `118.69.4.63/32` |
| Firewall: app 19080 | From allowlist only | From `118.69.4.63/32` |
| Firewall: HTTP 80 | Public for ACME | From `0.0.0.0/0` |
| Firewall: HTTPS 443 | Public for HTTPS | From `0.0.0.0/0` |

Token handling: Full token retrieved on-VM via `sudo`; only 8-char prefix ever printed.

### Caveats

- nip.io is **temporary and not for production** — a real domain with proper DNS is required for production
- GCP non-prod firewall allows public 80/443 — production firewall design may differ
- No production bearer token management (rotation, expiry) validated

### Action

**Operator acknowledgment required**: Operator confirms TLS/reverse proxy setup and firewall design are acceptable for the target environment. Production deployment requires a real domain, not nip.io.

---

## G2.3 — Backup Schedule Evidence

**Status**: `partial`

### Gate Description
Operator implements and confirms backup schedule external to FerrumGate, with evidence of scheduled backup runs.

### Current Evidence (GCP Non-Prod)

| Check | Expected | Observed |
|-------|----------|----------|
| Backup timer | Enabled | `ferrumgate-backup.timer enabled` |
| Manual backup trigger | Success | Backup file created: `ferrumgate_20260508_154446.db` |
| Backup file location | `/var/lib/ferrumgate/backups/` | Confirmed |
| Backup timer next run | Listed | Confirmed via `systemctl list-timers` |

### Gap

The current evidence shows:
- Timer is enabled (hourly + 5 min after boot)
- Manual backup works
- But no **operator-defined production backup schedule** has been established
- Retention policy not defined
- Offsite backup not addressed

### Action

**Operator action required**: Define production backup schedule (frequency, retention, offsite), implement it, and provide evidence. This is an operator responsibility, not a tooling gap.

---

## G2.4 — Restore Drill

**Status**: `ready`

### Gate Description
Operator performs restore drill in non-production environment and verifies `PRAGMA integrity_check` passes on restored database.

### Current Evidence (GCP Non-Prod Restore Drill)

```
LATEST_BACKUP=ferrumgate_20260508_154446.db
RESTORE_COPY=ferrumgate_restore_drill_20260508_165658.db
INTEGRITY=ok
TABLE_COUNT=14
RESTORE_COPY_REMOVED=yes
```

### Drill Steps Performed

1. Backup file identified: `ferrumgate_20260508_154446.db`
2. Restore to temporary copy: `ferrumgate_restore_drill_20260508_165658.db`
3. `PRAGMA integrity_check` on restored DB: **ok**
4. Table count verified: **14 tables**
5. Restore copy removed after verification: **yes**

### Caveats

- This drill was performed on the **GCP non-prod SQLite store**, not a production store
- Production restore drill should be performed in a **production-adjacent environment**
- Operator must verify the same restore procedure works in their target environment

### Action

**Operator confirmation recommended**: Operator should verify the restore drill procedure works for the target production environment and confirm RPO/RTO fit (see G2.5).

---

## G2.5 — RPO/RTO Acceptance

**Status**: `operator-required`

### Gate Description
Operator formally accepts Recovery Point Objective (RPO) and Recovery Time Objective (RTO) for the target workload. RPO = time since last backup. RTO = restore time + restart + verification.

### Current Evidence

No formal RPO/RTO acceptance has been provided. This gate requires operator-defined SLAs.

### FerrumGate Constraints

- **RPO**: Equals time since last backup. Any writes after the last backup are lost on restore.
- **RTO**: Includes backup restore time + service restart + verification. No automated recovery in FerrumGate.
- FerrumGate has **no automated failover or HA** in Phase 1 (single-node SQLite only).

### Action

**Operator action required**: Define RPO and RTO SLAs for the target workload. Confirm whether FerrumGate's backup/restore capabilities meet those SLAs. Sign RPO/RTO acceptance in doc 54.

---

## G2.6 — Production Evaluation Framework

**Status**: `partial`

### Gate Description
Operator completes the production evaluation framework (performance, security, reliability, operations, release confidence) and confirms all critical items are SATISFIED or CONDITIONAL with controls.

### Current Evidence

Local repo-side tooling validation has been performed:

| Check | Result |
|-------|--------|
| `cargo test --package ferrumctl -- backup` | PASS (8 tests) |
| `cargo test --package ferrumd -- test_resolve_config_rejects_bearer_mode_without_token` | PASS |
| `cargo test --package ferrumd -- test_resolve_config_rejects_postgres_dsn` | PASS |
| `cargo test --package ferrum-integration-tests -- test_scope_mismatch_deny_on_empty_scope_with_mutation` | PASS |
| `cargo test --package ferrum-integration-tests -- test_r3_contracts_have_auto_commit_false` | PASS |
| `cargo test --package ferrum-integration-tests -- compensate_execution_flow` | PASS |
| `cargo test --test integration_lineage_chain -- test_lineage_chain_full_provenance_events` | PASS |

### Gap

The **operator-completed** production evaluation framework (Dimension 1-5 assessment) has not been performed. This requires operator review of `27-production-evaluation-plan.md` and formal signoff.

### Action

**Operator action required**: Complete the production evaluation framework in doc 54, dimension by dimension, and sign the evaluation framework section.

---

## G2.7 — Accepted-Risk Review

**Status**: `partial`

### Gate Description
Operator reviews and accepts all accepted risks documented in `19-v1-single-node-support-contract.md` §4 and the Weak Spots in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`.

### Current Evidence

Repo-side verification of weak spots has been performed:

| Weak Spot | Status |
|-----------|--------|
| Weak Spot 1 — Rollback class handling | Resolved (R3 `auto_commit=false` verified) |
| Weak Spot 2 — Draft-only revalidation | Resolved (scope-mismatch deny verified) |
| Weak Spot 3 — Scope-bounds enforcement | Resolved (single-use capability verified) |
| Weak Spot 4 — Provenance completeness | Resolved (full lineage chain verified) |

### Gap

Operator has not signed the accepted-risk verification checklist in doc 54. Operator must review Weak Spots 1-4 and all §4 accepted risks, then sign the checklist.

### Action

**Operator action required**: Review `19-v1-single-node-support-contract.md` §4 and `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`, then sign the accepted-risk verification checklist in doc 54.

---

## G2.8 — Compensate Noop Risk Acceptance

**Status**: `partial`

### Gate Description
Operator acknowledges that `POST /v1/executions/{execution_id}/compensate` may return 200 without performing external undo for noop-backed adapters, and defines a manual verification procedure.

### Current Evidence

Compensate flow has been exercised in integration tests:

```
cargo test --package ferrum-integration-tests -- compensate_execution_flow
Result: PASS
```

The compensate flow works, but the **operator must formally accept** that certain adapters may be noop-backed and that manual verification is their responsibility.

### Gap

No operator-signed compensate behavior matrix (listing which adapters are verified real-undo vs noop-backed) has been provided.

### Action

**Operator action required**: Complete the compensate behavior matrix in doc 54, identify which (if any) target adapters are noop-backed, and sign the compensate noop risk acceptance section.

---

## Phase 3D Metrics Snapshot

Collected from GCP non-prod FerrumGate `/v1/metrics` endpoint:

| Metric | Value |
|--------|-------|
| `ferrumgate_store_health_up` | 1 |
| `ferrumgate_write_queue_depth` | 0 |
| `/v1/healthz` request count | 7 |
| `/v1/readyz` request count | 4 |
| `/v1/readyz/deep` request count | 3 |
| `/v1/metrics` request count | 5 |
| `readyz/deep` 503 count | 0 |

Store health is `up=1`, write queue is `0` (empty), no 503 errors on deep readyz.

---

## Light Workload Smoke Test

Sequential single-request smoke test (5 rounds each):

| Endpoint | Success Rate |
|----------|--------------|
| `/v1/healthz` | 5/5 ok |
| `/v1/readyz` | 5/5 ok |
| `/v1/readyz/deep` | 5/5 ok |
| `/v1/metrics` | 5/5 ok |

All endpoints returned HTTP 200 in all 5 sequential requests.

---

## Conservative Conclusion

**G2 is NOT complete (historical).** The evidence collected in Phase 3D suggests:

1. **G2.2 (auth/TLS)**: Evidence is `ready` for operator review — GCP non-prod TLS via nip.io + Caddy works, auth returns 401/200 as expected.

2. **G2.4 (restore drill)**: Evidence is `ready` — restore drill passed with `PRAGMA integrity_check=ok`, 14 tables, copy removed.

3. **G2.3, G2.6, G2.7, G2.8**: Evidence is `partial` — tooling/integration evidence exists but operator-defined acceptance is pending.

4. **G2.1, G2.5**: Evidence is `operator-required` — no workload model or RPO/RTO acceptance provided.

**The GCP non-prod target is ready for operator review** — meaning the evidence packet suggests the implementation is functioning correctly in a non-prod context, but all G2 gates were open pending operator signoff via canonical doc 54 at that time.

> **Superseded (2026-05-15)**: Doc 59 and doc 54 were subsequently signed by BrianNguyen on 09/05/2026 for conditional single-node SQLite pilot scope. B3/B4/B5 were closed via delegated authority on 2026-05-15. Conditional single-node SQLite pilot readiness is ACCEPTABLE/YES (scoped only). Production-ready remains NO.

**No production-ready claim is made. No G2 completion is claimed. No pilot authorization is made.**

---

## What's Needed to Complete G2

| Gate | Action |
|------|--------|
| G2.1 | Operator provides workload model; confirms single-node fit |
| G2.2 | Operator acknowledges TLS/reverse proxy setup for target environment |
| G2.3 | Operator defines and implements production backup schedule |
| G2.4 | Operator performs restore drill in production-adjacent environment |
| G2.5 | Operator formally accepts RPO/RTO for target workload |
| G2.6 | Operator completes and signs production evaluation framework |
| G2.7 | Operator reviews and signs accepted-risk checklist |
| G2.8 | Operator completes and signs compensate behavior matrix |

---

## Signature Section (Operator Use)

> **This document is UNSIGNED. It is prepared for operator review.**

### Phase 3D Evidence Review

Operator name: _______________________________

Date reviewed: _______________

G2 gate statuses reviewed: [ ] Yes  [ ] No

Notes: _______________________________

Operator signature: _______________________________ Date: _______________

---

## References

- Phase 3A artifact: [2026-05-08-gcp-phase3a-nonprod-target.md](./artifacts/2026-05-08-gcp-phase3a-nonprod-target.md)
- Phase 3B artifact: [2026-05-08-gcp-phase3b-domain-tls.md](./artifacts/2026-05-08-gcp-phase3b-domain-tls.md)
- Phase 3C artifact: [2026-05-08-gcp-phase3c-live-rehearsal.md](./artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md)
- Phase 3D artifact: [artifacts/2026-05-08-gcp-phase3d-g2-readiness.md](./artifacts/2026-05-08-gcp-phase3d-g2-readiness.md)
- Operator signoff packet: [54-operator-signoff-packet.md](./54-operator-signoff-packet.md)
- Pilot readiness evidence: [59-pilot-readiness-evidence-packet.md](./59-pilot-readiness-evidence-packet.md)

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial Phase 3D G2 readiness checklist. UNSIGNED. Operator review required. |
