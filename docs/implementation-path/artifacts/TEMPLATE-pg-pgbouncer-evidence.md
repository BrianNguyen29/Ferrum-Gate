# TEMPLATE — PgBouncer / Connection Pooling Validation Evidence

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output.
> See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) Phase PG-2.6 and [`docs/guides/operator.md`](../../guides/operator.md) §PgBouncer / connection pooling for the runbook.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `staging / production / local-docker-compose` |
| **Operator** | `name` |
| **ferrumd version / commit** | `git describe --always` |
| **PgBouncer version** | `pgbouncer --version` |
| **Pool mode** | `transaction / session / statement` |
| **Evidence owner** | Operator |

---

## T-PGB-1 — PgBouncer Configuration Review

**Check**: PgBouncer config is correct for FerrumGate.

- **Config file path**: `/etc/pgbouncer/pgbouncer.ini`
- **Key settings**:
  | Setting | Value | Recommended | Pass/Fail |
  |---------|-------|-------------|-----------|
  | `pool_mode` | `transaction` | `transaction` | ☐ |
  | `max_client_conn` | `N` | `≥ sum(ferrumd pg_max_connections) × 1.2` | ☐ |
  | `default_pool_size` | `N` | `≤ PG max_connections / pgbouncer_instances - overhead` | ☐ |
  | `reserve_pool_size` | `N` | `5` | ☐ |
  | `reserve_pool_timeout` | `N` | `3` | ☐ |
  | `server_idle_timeout` | `N` | `600` | ☐ |
  | `listen_port` | `6432` | `6432` | ☐ |
- **Config file hash** (for change tracking): `sha256:...`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-PGB-2 — PgBouncer Health Check

**Check**: PgBouncer is accepting connections.

- **Command**:
  ```bash
  psql -h <pgbouncer_host> -p 6432 -U pgbouncer -d pgbouncer -c "SHOW pools;"
  ```
- **Expected output**: Table with pool stats (`cl_active`, `cl_waiting`, `sv_active`, `sv_idle`, etc.)
- **Actual output**: *(paste here — redact sensitive fields if any)*
- **Errors observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-PGB-3 — ferrumd Connectivity Through PgBouncer

**Check**: ferrumd connects and operates correctly through PgBouncer.

- **ferrumd DSN (sanitized)**:
  ```text
  postgres://user@pgbouncer-host:6432/ferrumgate?sslmode=require
  ```
- **Connection test**:
  ```bash
  psql "${FERRUMD_STORE_DSN}" -c "SELECT 1 AS pgbouncer_connection_test;"
  ```
- **Expected output**: `pgbouncer_connection_test = 1`
- **Actual output**: *(paste here)*

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-PGB-4 — ferrumd Startup with PgBouncer DSN

**Check**: ferrumd starts and reports healthy when DSN points to PgBouncer.

- **Startup command**:
  ```bash
  FERRUMD_STORE_DSN="postgres://user@pgbouncer-host:6432/ferrumgate?sslmode=require" ferrumd --config /etc/ferrumgate/ferrumd.toml
  ```
- **Start time**: `YYYY-MM-DD HH:MM:SS UTC`
- **Process status after 60s**: `running / exited / panicked`
- **Relevant log excerpt**:
  ```
  (paste startup logs here)
  ```
- **Errors observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-PGB-5 — `/v1/readyz/deep` with PgBouncer Backend

**Check**: Deep readiness reports healthy store when backend is PgBouncer.

- **Command**:
  ```bash
  curl -sf http://<bind_addr>/v1/readyz/deep
  ```
- **HTTP status code**: `200 / 503 / other`
- **Response body**:
  ```json
  (paste JSON here)
  ```
- **Store component**: `healthy` required for pass.
- **Pool metrics present** (`ferrumgate_store_pg_pool_size`, `pool_idle`, `pool_max`): `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-PGB-6 — Session Feature Compatibility Check (If `transaction` Pool Mode)

**Check**: ferrumd's `SET` commands (e.g., `statement_timeout`) work correctly through PgBouncer in `transaction` mode.

- **Test query**:
  ```bash
  psql "${FERRUMD_STORE_DSN}" -c "SHOW statement_timeout;"
  ```
- **Expected output**: `statement_timeout = 5000ms` (or configured value)
- **Actual output**: *(paste here)*
- **Notes**: In `transaction` mode, `SET` may not persist across transactions. Verify ferrumd applies `statement_timeout` per-connection via `after_connect`.

**Pass/Fail**: ☐ PASS / ☐ FAIL / ☐ NOT APPLICABLE (session mode)

---

## T-PGB-7 — Load / Connection Count Observation (Optional)

**Check**: Under representative load, PgBouncer prevents PG connection exhaustion.

- **Load description**: (e.g., `slo-validation-runbook.md` canonical workload, stress test, or representative agent traffic)
- **Peak ferrumd pool size observed**: `N`
- **Peak PG backend count observed** (via `pg_stat_activity`): `N`
- **PgBouncer `SHOW pools` during load**: *(paste or summarize)*
- **Connection exhaustion observed**: `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL / ☐ NOT PERFORMED

---

## Known Gaps at Time of Evidence

- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: PgBouncer validation is one component of PostgreSQL hardening. Production readiness requires PG-1 through PG-5 and operator signoff.
- **NOT performance benchmark**: This evidence validates connectivity and basic behavior under load. It does not establish maximum throughput or latency guarantees.
- **NOT HA**: PgBouncer itself is a single point of failure unless made HA. No failover or replication is configured.
- **NOT Block A closed**: Block A (real owned domain + DNS) remains WAIVED/CONDITIONAL.
- **NOT full G2**: G2 operator signoff requires real domain and final evidence pack review.

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Operator | | | |
| Engineering (witness) | | | |

---

## Related Docs

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-2.6
- [`docs/guides/operator.md`](../../guides/operator.md) §PgBouncer / connection pooling
- [`docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md`](./2026-05-21-phase-b-pg-production-foundation-prep.md)
