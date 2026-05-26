# HA-2 — Manual Failover Runbook

> **Status**: PLANNING ARTIFACT — runbook drafted; local simulation drill performed 2026-05-26; no operator-environment live drill; no HA implementation.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-26
> **Parent**: [`docs/production-readiness-v2/09-ha-roadmap.md`](./09-ha-roadmap.md)
> **Scope**: [`00-scope-and-nonclaims.md`](./00-scope-and-nonclaims.md)
> **ADR**: [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md)

> **Delegated signoff (planning-only)**
> - **Signed by**: BrianNguyen (session authorization)
> - **Date**: 2026-05-21
> - **Scope**: Manual failover runbook reviewed and accepted as planning artifact.
> - **Nature**: Planning/decision document signoff only. This does not constitute evidence of live failover drill, standby promotion, RPO/RTO measurement, or production readiness. Does not substitute for missing evidence.
> - **Authority**: User explicitly authorized delegated signoff for planning and decision documents.

---

## 1. Purpose

This runbook describes the manual steps an operator should follow to fail over FerrumGate from a failed PostgreSQL primary to a promoted standby. It is a **planning artifact** intended to guide future operator action once a replicated PostgreSQL topology exists. It does not describe current capability: FerrumGate v1 runs single-node with no replication.

---

## 2. Prerequisites (assumed for when this runbook is used)

| Prerequisite | Why |
|--------------|-----|
| PostgreSQL streaming replication configured between primary and at least one standby | Without replication, there is no standby to promote. |
| Standby is reachable from the operator bastion / admin host | Operator must be able to run `pg_ctl` or equivalent. |
| ferrumd operator has credentials and network path to both old primary and standby | Required for DSN update and health checks. |
| Backup taken within the configured RPO window | Limits data loss if replication is asynchronous. |
| `ferrumctl` or shell access to the ferrumd host | Required to restart or reconfigure the gateway. |

---

## 3. Primary down detection

### 3.1 Automated signals (monitoring)

1. **`ferrumgate_store_health_up` == 0** for more than the configured threshold (e.g., 30 s).
2. **`/v1/readyz/deep` returns 503** consistently.
3. **PostgreSQL primary health probe fails** from the monitoring host (e.g., `pg_isready -h <primary>` returns non-zero).

### 3.2 Manual confirmation

Before promoting a standby, confirm the primary is truly down and not merely slow:

```bash
# From the ferrumd host or a bastion with PG connectivity
pg_isready -h <primary_host> -p <primary_port> -U <monitor_user>
# Expected: non-zero / connection refused / timeout

# Check if the primary process is running (if you have OS access)
ssh <primary_host> sudo systemctl status postgresql
# Expected: inactive (failed) or host unreachable
```

> **Do not promote the standby if the primary is still accepting writes.** Promoting while the primary is live causes split-brain.

---

## 4. Standby promotion checklist (manual)

### 4.1 Stop replication on the standby (prevent further WAL replay)

```bash
# On the standby host
sudo -u postgres psql -c "SELECT pg_wal_replay_pause();"
```

> If the standby is already in recovery and the primary is confirmed dead, skip pause and proceed to promote.

### 4.2 Promote standby to primary

```bash
# Option A: pg_ctl promote (self-hosted)
sudo -u postgres pg_ctl promote -D /var/lib/postgresql/data

# Option B: managed provider console (RDS, Cloud SQL, etc.)
# Use provider UI/API to initiate failover; skip OS-level steps.
```

### 4.3 Verify promotion

```bash
# On the newly promoted primary
sudo -u postgres psql -c "SELECT pg_is_in_recovery();"
# Expected: f (false)

# Verify write capability
sudo -u postgres psql -c "CREATE TABLE _failover_probe (id int); DROP TABLE _failover_probe;"
```

### 4.4 Schema version check

```bash
# Verify schema version matches ferrumd expectation
sudo -u postgres psql -c "SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1;"
# Compare with the ferrumd binary's embedded migration version.
```

> If versions mismatch, do not restart ferrumd until migrations are reconciled. See [`02-postgres-production-plan.md`](./02-postgres-production-plan.md) §PG-4.

### 4.5 Update connection credentials (if they changed)

If the promoted standby uses a different host, port, or credentials, prepare the new DSN now. In managed HA, the endpoint may be unchanged.

---

## 5. ferrumd reconnect / reroute procedure

ferrumd initializes its `PgPool` once at startup using `FERRUMD_STORE_DSN`. It does not dynamically reload the DSN. Therefore, after a primary change, the operator must update the DSN and restart ferrumd.

### 5.1 Update DSN

Edit the ferrumd config or environment:

```bash
# Example: systemd env file
sudoedit /etc/ferrumgate/ferrumd.env
# Update FERRUMD_STORE_DSN to point to the new primary
FERRUMD_STORE_DSN="postgres://user:pass@<new_primary_host>:5432/ferrumgate"
```

### 5.2 Restart ferrumd

```bash
sudo systemctl restart ferrumgate
```

### 5.3 Verify reconnect

```bash
# Liveness
curl -s http://127.0.0.1:8080/v1/healthz | jq .

# Readiness (includes store health)
curl -s http://127.0.0.1:8080/v1/readyz/deep | jq .

# Metrics
curl -s http://127.0.0.1:8080/v1/metrics | grep ferrumgate_store_health_up
# Expected: 1
```

### 5.4 Smoke test a mutating endpoint

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"intent":"failover-smoke","adapter":"fs","operation":"write","path":"/tmp/ha-smoke.txt","content":"ok"}' \
  http://127.0.0.1:8080/v1/intents
# Expected: 200 or 202 (not 503)
```

---

## 6. RPO / RTO expectations

| Metric | Expectation (manual failover) | Source |
|--------|-------------------------------|--------|
| **RTO** | Minutes (operator detection + promotion + ferrumd restart). Target: < 10 min for experienced operator. | HA ADR §4.3 Step 2 |
| **RPO** | Bounded by replication lag. If async replication: seconds to minutes of unflushed WAL may be lost. If sync replication: near-zero RPO, at the cost of write latency. | HA ADR §5.3 |
| **Current reality** | N/A — no replication configured; single-node SQLite is the only supported runtime today. | HA ADR §1.1 |
| **Local simulation (2026-05-26)** | Latest observed RTO 3 s, RPO 0 rows lost. Local Docker primary/standby with `pg_promote()`. | [`2026-05-26-ha-local-failover-simulation-evidence.md`](../../implementation-path/artifacts/2026-05-26-ha-local-failover-simulation-evidence.md) |

> Local simulation values are **not representative** of production RTO/RPO. They measure only container stop + `pg_promote()` latency on a single host with no network partitions, no operator decision time, and no ferrumd restart.

---

## 7. Split-brain prevention checks

Manual failover relies on the operator, not automation, to prevent split-brain.

| Check | Action |
|-------|--------|
| Confirm primary is dead | `pg_isready`, `systemctl status`, or cloud console shows stopped/failed. |
| Verify only one writable primary | After promotion, run `SELECT pg_is_in_recovery();` on the new node (must be `f`). |
| Fence old primary if it recovers | If the old primary restarts later, it must not accept writes. Options: `pg_ctl stop`, firewall block, or revoke replication credentials. |
| Update DSN on all ferrumd instances | Ensure every ferrumd process points to the same new primary. |

> **No automated split-brain prevention exists today.** Automated fencing and consensus-based leader election are deferred to Step 3 (Patroni/repmgr). See HA ADR §6.

---

## 8. Rollback / revert procedure

If the promoted standby is unstable or the old primary recovers and you must revert:

1. **Do not** attempt a "demote" while the new primary is actively serving writes. This will cause data divergence.
2. **Plan a maintenance window** for rollback.
3. **Reconfigure the old primary as a standby** (rebuild from base backup or use `pg_rewind` if timelines allow).
4. **Promote the old primary again** using the same promotion steps above.
5. **Update `FERRUMD_STORE_DSN`** and restart ferrumd.

> Rollback is high-risk and should only be performed when the operator can accept downtime. Prefer forward recovery (fix the new primary) over backward rollback.

---

## 9. Post-failover actions

1. **Verify backup schedule** is still targeting the new primary (or update backup jobs).
2. **Update monitoring targets** (Prometheus `pg_exporter`, alert rules) to scrape the new primary.
3. **Recreate read replicas** if they were attached to the old primary.
4. **Record the incident** in the audit log and operator logbook.
5. **Review replication lag** to confirm catch-up is occurring.

---

## 10. Non-claims

- **NOT a live operator-environment drill**: A local Docker simulation was executed 2026-05-26. No operator-environment or target-host failover drill has been performed.
- **NOT HA implementation**: FerrumGate remains single-node. Local simulation is rehearsal-only; no production replication, standby, or automated failover exists.
- **NOT production-ready**: This document does not make FerrumGate production-ready.
- **NOT a guarantee of RPO/RTO**: Actual bounds depend on operator speed, replication configuration, and infrastructure. Values are targets, not SLA commitments.
- **NOT automated failover**: This runbook covers manual steps only. Automated failover is deferred to HA ADR Step 3.
- **NOT closing BLK-A-DOM**: Real owned domain remains an external operator blocker. This runbook does not address DNS, TLS, or domain concerns.
- **NOT a substitute for operator training**: The operator should rehearse these steps in a non-production environment before relying on them.

---

## Related docs

- [`docs/production-readiness-v2/ha-adr.md`](./ha-adr.md) — HA architecture decisions, phased strategy, RPO/RTO rationale.
- [`docs/production-readiness-v2/09-ha-roadmap.md`](./09-ha-roadmap.md) — HA roadmap and task tracking.
- [`docs/guides/operator.md`](../../guides/operator.md) — General operator procedures, health checks, and monitoring.
- [`docs/production-readiness-v2/02-postgres-production-plan.md`](./02-postgres-production-plan.md) — PostgreSQL hardening and schema migration handling.

---

*End of HA-2 Manual Failover Runbook — planning artifact only (2026-05-21).*
