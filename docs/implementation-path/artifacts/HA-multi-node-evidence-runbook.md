# HA / Multi-Node Evidence Runbook

> **Status**: PLANNING ARTIFACT — runbook for operator execution. No HA/multi-node implementation. No live drill performed.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-25
> **Parent**: [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md)
> **Template**: [`docs/implementation-path/artifacts/TEMPLATE-ha-multinode-evidence-pack.md`](./TEMPLATE-ha-multinode-evidence-pack.md)
> **Scope**: [`docs/production-readiness-v2/00-scope-and-nonclaims.md`](../../production-readiness-v2/00-scope-and-nonclaims.md)

> **Operator review**: pending
> This is a planning artifact. It does **not** constitute evidence of HA implementation, multi-node deployment, automated failover, or production readiness. Does **not** substitute for missing evidence.

---

## 1. Purpose

This runbook tells an operator **exactly how to capture evidence** for HA/multi-node readiness. It pairs with [`TEMPLATE-ha-multinode-evidence-pack.md`](./TEMPLATE-ha-multinode-evidence-pack.md) and [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md). Every section below contains:

- Concrete shell commands or API calls.
- Expected output format.
- Pass/fail criteria.
- Redaction rules for secrets.
- Evidence file naming convention.

> **Non-claim**: This is a runbook only. No HA infrastructure has been deployed. No failover drill has been executed. No evidence has been captured. Execution is operator-dependent and deferred until a replicated PostgreSQL topology exists.

---

## 2. Evidence naming convention

All evidence artifacts must be date-stamped and stored in `docs/implementation-path/artifacts/`:

| Topic | Filename pattern |
|-------|-----------------|
| Manual failover drill | `YYYY-MM-DD-ha-manual-failover-drill-evidence.md` |
| RPO/RTO measurement | `YYYY-MM-DD-ha-rpo-rto-measurement-evidence.md` |
| Read replica validation | `YYYY-MM-DD-ha-read-replica-validation-evidence.md` |
| Automated failover drill (deferred) | `YYYY-MM-DD-ha-automated-failover-drill-evidence.md` |
| Consolidated HA evidence pack | `YYYY-MM-DD-ha-multinode-evidence-pack.md` (from template) |

**File contents rule**: Every evidence file must contain the exact command, the exact (redacted) output, a pass/fail verdict, timestamps, and the operator initials who ran it.

---

## 3. Redaction rules

Before pasting any output into an evidence artifact, apply these redactions **in this exact order**:

1. **Passwords in DSNs**: Replace `postgres://user:PASSWORD@host` with `postgres://user:__REDACTED__@host`.
2. **Bearer tokens**: Replace `Authorization: Bearer <token>` with `Authorization: Bearer __REDACTED__`.
3. **IP addresses / hostnames**: Replace public IPs with `<PRIMARY_HOST>`, `<STANDBY_HOST>`, `<PG_HOST>`, or `<FERRUMD_HOST>`. Document the mapping in a separate operator-only sheet (not in version control).
4. **Cloud credentials**: Replace any AWS/GCS access keys, API keys, or managed PG console URLs with `__REDACTED__`.
5. **Replication user passwords**: If `pg_stat_replication` or `pg_hba.conf` output contains credentials, redact them.

> **Sanity check**: After redaction, `grep -i -E '(pass|secret|key|token)'` on the artifact should return only the word `__REDACTED__` or innocuous words like "pass/fail".

---

## 4. Prerequisites verification commands

These map directly to the Prerequisites Checklist in [`TEMPLATE-ha-multinode-evidence-pack.md`](./TEMPLATE-ha-multinode-evidence-pack.md).

### P.1 — HA ADR approved as planning decision

**Method**: Confirm [`docs/production-readiness-v2/ha-adr.md`](../../production-readiness-v2/ha-adr.md) §9 contains an operator decision block with date and initials.

**Pass criteria**: ADR shows approved phased approach with operator acknowledgment.

**Evidence to record**: Copy of the ADR signoff block.

---

### P.2 — PostgreSQL production foundation stable

**Method**: Confirm [`TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md) or a dated signoff artifact exists and is signed for the primary PostgreSQL instance.

**Pass criteria**: PG production signoff shows `PASS` for prerequisites P.1–P.11.

**Evidence to record**: Reference to the signed PG production deployment signoff artifact.

---

### P.3 — Security/tenant model decided

**Method**: Confirm [`docs/production-readiness-v2/04-security-tenant-model-adr.md`](../../production-readiness-v2/04-security-tenant-model-adr.md) or equivalent operator decision exists.

**Pass criteria**: Tenant model (single-tenant vs multi-tenant) and security posture are documented and signed.

**Evidence to record**: Copy of the security/tenant model decision block.

---

### P.4 — SLO metrics available

**Method**: Confirm Prometheus is scraping ferrumd and SLO dashboard exists.

**Command**:
```bash
curl -sf http://<PROM_ADDR>:9090/api/v1/targets | jq '.data.activeTargets[] | select(.labels.job == "ferrumgate") | {health, lastScrape}'
```

**Pass criteria**: Target health is `UP` and last scrape is recent (< 2 min).

**Evidence to record**: Target health JSON.

---

### P.5 — Backup/restore evidence exists

**Method**: Confirm a dated restore drill artifact exists for the primary PostgreSQL instance.

**Pass criteria**: `YYYY-MM-DD-pg-restore-drill-evidence.md` or equivalent exists and shows `PASS`.

**Evidence to record**: Reference to the restore drill artifact.

---

### P.6 — Manual failover runbook drafted

**Method**: Confirm [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md) exists and was reviewed.

**Pass criteria**: Runbook is present in repo and contains operator signoff block (planning-only signoff acceptable).

**Evidence to record**: Reference to the runbook and its signoff block.

---

### P.7 — Read replica design drafted

**Method**: Confirm [`docs/production-readiness-v2/read-replica-design.md`](../../production-readiness-v2/read-replica-design.md) exists and was reviewed.

**Pass criteria**: Design doc is present and contains routing rules, lag semantics, and observability design.

**Evidence to record**: Reference to the design doc.

---

## 5. HA-2 — Manual failover drill procedure

This section provides a step-by-step execution guide for the manual failover drill. It is derived from [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md) but adds **measurement rigor** and **evidence capture** at every step.

### 5.1 Pre-drill baseline (record before any failure injection)

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.1.1 | `date -u +%Y-%m-%dT%H:%M:%SZ` — baseline timestamp | Timestamp |
| 5.1.2 | `pg_isready -h <PRIMARY_HOST> -p <PG_PORT>` | Output |
| 5.1.3 | `psql -h <PRIMARY_HOST> -c "SELECT pg_is_in_recovery();"` | Must return `f` |
| 5.1.4 | `psql -h <STANDBY_HOST> -c "SELECT pg_is_in_recovery();"` | Must return `t` |
| 5.1.5 | `psql -h <STANDBY_HOST> -c "SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) AS lag_seconds;"` | Lag seconds |
| 5.1.6 | `curl -sf http://<FERRUMD_BIND>/v1/readyz/deep \| jq .` | Full JSON |
| 5.1.7 | `curl -sf http://<FERRUMD_BIND>/v1/metrics \| grep ferrumgate_store_health_up` | Metric line |

---

### 5.2 Primary failure injection

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.2.1 | `date -u +%Y-%m-%dT%H:%M:%SZ` — injection timestamp | Timestamp |
| 5.2.2 | `ssh <PRIMARY_HOST> sudo -u postgres pg_ctl stop -D /var/lib/postgresql/data -m fast` | Command and host response |
| 5.2.3 | `pg_isready -h <PRIMARY_HOST> -p <PG_PORT>` | Must fail (non-zero) |

> **Stop condition**: If the primary does **not** stop (e.g., `pg_isready` still returns accepting connections), **abort the drill** to avoid split-brain. Do not proceed to promotion.

---

### 5.3 Detection phase (measure time to alert / readyz failure)

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.3.1 | `date -u +%Y-%m-%dT%H:%M:%SZ` — detection start | Timestamp |
| 5.3.2 | Wait for monitoring alert (AlertManager / PagerDuty / email) | Screenshot or log line with alert timestamp |
| 5.3.3 | `curl -sf http://<FERRUMD_BIND>/v1/readyz/deep \| jq .` | HTTP status and JSON after failure |
| 5.3.4 | `date -u +%Y-%m-%dT%H:%M:%SZ` — detection end | Timestamp |

**Measurement**:
```bash
# Calculate detection duration in seconds
echo "Detection duration = $(($(date -d 'DETECTION_END' +%s) - $(date -d 'INJECTION_TIMESTAMP' +%s))) seconds"
```

**Pass criteria**:
- Detection time ≤ operator-defined threshold (e.g., 60 s).
- `/v1/readyz/deep` returns non-200 or `store: unhealthy`.

---

### 5.4 Standby promotion

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.4.1 | `date -u +%Y-%m-%dT%H:%M:%SZ` — promotion start | Timestamp |
| 5.4.2 | `ssh <STANDBY_HOST> sudo -u postgres pg_ctl promote -D /var/lib/postgresql/data` | Command and stdout |
| 5.4.3 | `sleep 5` | N/A |
| 5.4.4 | `psql -h <STANDBY_HOST> -c "SELECT pg_is_in_recovery();"` | Must return `f` |
| 5.4.5 | `psql -h <STANDBY_HOST> -c "CREATE TABLE _failover_probe (id int); DROP TABLE _failover_probe;"` | Must succeed |
| 5.4.6 | `date -u +%Y-%m-%dT%H:%M:%SZ` — promotion end | Timestamp |

**Measurement**:
```bash
echo "Promotion duration = $(($(date -d 'PROMOTION_END' +%s) - $(date -d 'PROMOTION_START' +%s))) seconds"
```

**Pass criteria**:
- `pg_is_in_recovery()` returns `f`.
- Write probe succeeds.
- Promotion duration ≤ operator-defined threshold (e.g., 120 s).

> **Rollback criterion**: If `pg_is_in_recovery()` still returns `t` after 60 s, **abort**. Do not restart ferrumd. Investigate WAL replay state (`pg_stat_wal_receiver`) before retrying.

---

### 5.5 ferrumd reconnect / reroute

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.5.1 | Update `FERRUMD_STORE_DSN` in `/etc/ferrumgate/ferrumd.env` to point to `<STANDBY_HOST>` | Config diff (redacted password) |
| 5.5.2 | `date -u +%Y-%m-%dT%H:%M:%SZ` — restart start | Timestamp |
| 5.5.3 | `sudo systemctl restart ferrumd` | Command |
| 5.5.4 | `sleep 5 && systemctl is-active ferrumd` | Must return `active` |
| 5.5.5 | `curl -sf http://<FERRUMD_BIND>/v1/healthz \| jq .` | Must return HTTP 200 |
| 5.5.6 | `curl -sf http://<FERRUMD_BIND>/v1/readyz/deep \| jq .` | Must return HTTP 200, `store: healthy` |
| 5.5.7 | `curl -sf http://<FERRUMD_BIND>/v1/metrics \| grep ferrumgate_store_health_up` | Must show `1` |
| 5.5.8 | `date -u +%Y-%m-%dT%H:%M:%SZ` — restart end | Timestamp |

**Measurement**:
```bash
echo "Reconnect duration = $(($(date -d 'RESTART_END' +%s) - $(date -d 'RESTART_START' +%s))) seconds"
```

**Pass criteria**:
- ferrumd restarts and stays active.
- `/v1/readyz/deep` returns `200` with `store: healthy` within 60 s of restart.
- `ferrumgate_store_health_up` is `1`.

---

### 5.6 Smoke test (mutating endpoint)

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.6.1 | `date -u +%Y-%m-%dT%H:%M:%SZ` — smoke test start | Timestamp |
| 5.6.2 | `curl -X POST -H "Authorization: Bearer __REDACTED__" -H "Content-Type: application/json" -d '{"intent":"failover-smoke","adapter":"fs","operation":"write","path":"/tmp/ha-smoke.txt","content":"ok"}' http://<FERRUMD_BIND>/v1/intents` | HTTP status and response body |
| 5.6.3 | `date -u +%Y-%m-%dT%H:%M:%SZ` — smoke test end | Timestamp |

**Pass criteria**:
- HTTP status `200` or `202` (not `503`).
- Response contains a valid intent ID or success indicator.

---

### 5.7 Split-brain check

| Step | Command | Evidence to record |
|------|---------|-------------------|
| 5.7.1 | `psql -h <NEW_PRIMARY_HOST> -c "SELECT pg_is_in_recovery();"` | Must return `f` |
| 5.7.2 | `pg_isready -h <OLD_PRIMARY_HOST> -p <PG_PORT>` | Must still fail (non-zero) |
| 5.7.3 | `psql -h <OLD_PRIMARY_HOST> -c "SELECT pg_is_in_recovery();" 2>&1` | Must fail (connection refused) |

**Pass criteria**:
- Exactly one node accepts writes (`pg_is_in_recovery() = f`).
- Old primary remains unreachable or, if reachable, is in recovery mode (`t`).

> **Rollback criterion**: If the old primary is still accepting writes (`f`) while the new primary is also `f`, **you have split-brain**. Stop ferrumd immediately. Fence the old primary (firewall block, `pg_ctl stop`, revoke replication credentials) before resuming.

---

## 6. RPO / RTO measurement template

RPO and RTO must be measured **during the drill**, not estimated afterward. Use the timestamps collected in §5.

### 6.1 Data collection table

| Metric | Symbol | Calculation | Value (seconds) |
|--------|--------|-------------|-----------------|
| Failure injection time | `T_inject` | From §5.2.1 | |
| Detection end time | `T_detect` | From §5.3.4 | |
| Promotion start time | `T_promo_start` | From §5.4.1 | |
| Promotion end time | `T_promo_end` | From §5.4.6 | |
| ferrumd restart start | `T_restart_start` | From §5.5.2 | |
| ferrumd restart end | `T_restart_end` | From §5.5.8 | |
| Smoke test pass time | `T_smoke` | From §5.6.3 | |
| Last WAL replayed on standby *before* promotion | `T_last_wal` | Query standby before injection: `SELECT pg_last_xact_replay_timestamp();` | |

### 6.2 Formulas

```text
Detection time      = T_detect - T_inject
Promotion time      = T_promo_end - T_promo_start
Failover duration   = T_smoke - T_inject
RTO                 = T_smoke - T_inject
RPO                 = T_inject - T_last_wal  (if async replication)
                    = 0                      (if synchronous replication)
```

### 6.3 Pass/fail criteria

| Metric | Target | Measured | Pass/Fail |
|--------|--------|----------|-----------|
| Detection time | ≤ 60 s | | |
| Promotion time | ≤ 120 s | | |
| Failover duration | ≤ 10 min | | |
| RTO | ≤ 10 min | | |
| RPO | ≤ 15 s (sync) or ≤ 5 min (async) | | |

> **Operator decision**: Adjust targets based on infrastructure. Record any deviation and operator acceptance in the evidence artifact.

---

## 7. HA-3 — Read replica validation checklist

Execute these steps **only if** read replicas are deployed (Step 2 per HA ADR).

### 7.1 Replication baseline

| Check | Command | Pass criteria |
|-------|---------|---------------|
| Streaming state | `psql -h <PRIMARY_HOST> -c "SELECT client_addr, state, sent_lsn, write_lsn, flush_lsn, replay_lsn FROM pg_stat_replication;"` | `state = streaming` |
| Replica lag | `psql -h <REPLICA_HOST> -c "SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) AS lag_seconds;"` | ≤ operator threshold (e.g., 5 s under normal load) |
| Replica recovery mode | `psql -h <REPLICA_HOST> -c "SELECT pg_is_in_recovery();"` | Must return `t` |

### 7.2 Read routing validation (if Strategy B dual DSN is implemented)

| Check | Command | Pass criteria |
|-------|---------|---------------|
| Read DSN configured | `grep FERRUMD_STORE_READ_DSN /etc/ferrumgate/ferrumd.env` | Present and points to replica |
| Replica lag metric | `curl -sf http://<FERRUMD_BIND>/v1/metrics \| grep ferrumgate_store_replica_lag_seconds` | Present and numeric |
| Read-only endpoint uses replica | Review ferrumd logs or proxy logs for replica DSN | Confirmed by log line or `EXPLAIN` on replica |

### 7.3 Stale read handling

| Check | Command | Pass criteria |
|-------|---------|---------------|
| Lag threshold alert | `curl -sf http://<PROM_ADDR>:9090/api/v1/rules \| jq '.data.groups[] \| select(.rules[].alert == "FerrumGateReplicaLagHigh")'` | Rule exists |
| Readiness shows replica state | `curl -sf http://<FERRUMD_BIND>/v1/readyz/deep \| jq '.replica'` | Present (value depends on config) |

### 7.4 Misconfiguration and failure tests

| Check | Command | Pass criteria |
|-------|---------|---------------|
| Misconfiguration — read DSN points to primary | Temporarily set `FERRUMD_STORE_READ_DSN` to primary DSN; restart ferrumd; verify `/v1/healthz` | HTTP 200 (graceful degradation to no-read-replica mode) |
| Replica disconnect | Stop replica process; `curl` a read-only endpoint | No panic; endpoint returns data from primary or graceful error |

> **Non-claim**: Read replica validation steps are procedural templates. No read replica has been deployed or tested.

---

## 8. Rollback criteria and revert procedure

### 8.1 Abort criteria (do not proceed)

Abort the failover drill **before** restarting ferrumd if any of the following occur:

| # | Abort trigger | Why |
|---|---------------|-----|
| A.1 | Standby promotion fails (`pg_is_in_recovery()` remains `t` after 60 s) | Standby is not ready to serve writes. |
| A.2 | Schema version mismatch (`SELECT version FROM _schema_version` does not match ferrumd expectation) | Data corruption or incomplete migration risk. |
| A.3 | Old primary still accepts writes (`pg_is_in_recovery() = f` and reachable) | Split-brain risk. Fence old primary first. |
| A.4 | ferrumd cannot connect to new primary after 3 restart attempts | Network or credential issue. Debug before claiming recovery. |
| A.5 | Smoke test returns 503 or panics after restart | New primary may be unstable. Do not sign off. |

### 8.2 Revert procedure (if promotion must be undone)

> **Warning**: Reverting a failover is high-risk and causes downtime. Only perform during a maintenance window.

1. **Stop ferrumd** on all instances: `sudo systemctl stop ferrumd`.
2. **Do not attempt to "demote" the new primary** while it has active writes. This causes data divergence.
3. **Rebuild the old primary as a standby** (use `pg_rewind` if timelines allow, or restore from base backup).
4. **Promote the old primary** using the same promotion steps in §5.4.
5. **Update `FERRUMD_STORE_DSN`** to point back at the old primary and restart ferrumd.
6. **Verify** with `readyz/deep` and smoke test.

> **Non-claim**: No revert drill has been executed. This procedure is documented only.

---

## 9. Operator signoff boundaries

| Responsibility | Engineering | Operator |
|----------------|-------------|----------|
| Provision primary + standby PostgreSQL instances | Advise, document | Execute |
| Configure streaming replication | Provide replication docs | Execute, validate |
| Execute manual failover drill | Provide runbook, scripts | Execute, record evidence |
| Measure RPO/RTO | Provide template, formulas | Capture timestamps, calculate |
| Validate read replica routing | Provide design doc | Execute routing tests |
| Deploy automated failover (future Step 3) | Provide ADR guidance | Execute infrastructure (Patroni/repmgr) |
| Sign off on HA evidence pack | Review evidence, approve template | Final signoff on real evidence |

> **Rule**: Engineering may review and comment on operator evidence, but **only the operator may check the final signoff box** in [`TEMPLATE-ha-multinode-evidence-pack.md`](./TEMPLATE-ha-multinode-evidence-pack.md).

---

## 10. Non-claims

- **NOT a production-ready claim by itself**: HA evidence is a prerequisite for HA-capable production, not sufficient alone.
- **NOT validated for all topologies**: This runbook assumes a specific HA strategy (managed PG / Patroni / manual). Other topologies may require additional checks.
- **NOT self-executing**: This runbook records commands only. Real failover drills and evidence creation are required.
- **NOT retroactive**: Evidence applies only to the specific cluster configuration, versions, and date listed.
- **Does not close Block A**: HA evidence is independent of domain/DNS closure.
- **Does not replace PG production signoff**: HA evidence pack is separate from `TEMPLATE-pg-production-deployment-signoff.md`.
- **Manual failover ≠ true HA**: HA-2 pass does not constitute an HA claim. True HA requires HA-4 automated failover.
- **No live drill has been performed**: Every command in this runbook is a template. No output has been captured from a real failover or read replica deployment.
- **Read replica code does not exist**: ferrumd uses a single DSN. Read replica support is deferred to a follow-up ADR.

---

## 11. Related docs

- [`docs/production-readiness-v2/09-ha-roadmap.md`](../../production-readiness-v2/09-ha-roadmap.md) — HA roadmap and phased plan.
- [`docs/production-readiness-v2/ha-adr.md`](../../production-readiness-v2/ha-adr.md) — HA architecture decisions.
- [`docs/production-readiness-v2/manual-failover-runbook.md`](../../production-readiness-v2/manual-failover-runbook.md) — Manual failover procedure.
- [`docs/production-readiness-v2/read-replica-design.md`](../../production-readiness-v2/read-replica-design.md) — Read replica behavior design.
- [`docs/implementation-path/artifacts/TEMPLATE-ha-multinode-evidence-pack.md`](./TEMPLATE-ha-multinode-evidence-pack.md) — Evidence pack template to fill after executing this runbook.
- [`docs/implementation-path/artifacts/TEMPLATE-pg-production-deployment-signoff.md`](./TEMPLATE-pg-production-deployment-signoff.md) — PostgreSQL production signoff template.
- [`docs/guides/operator.md`](../../guides/operator.md) — General operator guide.

---

*End of HA / Multi-Node Evidence Runbook — planning artifact only (2026-05-25).*
