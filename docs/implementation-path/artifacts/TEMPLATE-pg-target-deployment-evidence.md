# TEMPLATE — PostgreSQL Target/Staging Deployment Evidence

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output.
> See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) Phase PG-1 for the execution runbook.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `staging / target-host-name / local-docker-compose` |
| **Operator** | `name` |
| **ferrumd version / commit** | `git describe --always` |
| **DSN (sanitized — no password)** | `postgres://user@host:port/dbname?sslmode=require` |
| **Evidence owner** | Engineering |

---

## PG-1.1 — PostgreSQL Target/Staging Provisioned

**Placeholder**: Record how the PostgreSQL instance was provisioned.

- **Method**: (e.g., Docker Compose, managed PG, systemd unit)
- **Host / port**: `host:port`
- **Database name**: `ferrumgate_staging`
- **Connection test**:
  ```bash
  psql "${FERRUMD_STORE_DSN}" -c "SELECT 1 AS connection_test;"
  ```
- **Expected output**: `connection_test = 1`
- **Actual output**: *(paste here)*

---

## PG-1.2 — ferrumd Start Result

**Placeholder**: Record ferrumd startup with PostgreSQL DSN.

- **Command**:
  ```bash
  FERRUMD_STORE_DSN=postgres://... ferrumd --config configs/ferrumgate.staging.toml
  ```
- **Start time**: `YYYY-MM-DD HH:MM:SS UTC`
- **Process status after 60s**: (e.g., `running`, `exited`, `panicked`)
- **Relevant log excerpt**:
  ```
  (paste startup logs here)
  ```
- **Errors observed**: `none / <list>`

---

## PG-1.3 — `/v1/readyz/deep` Output

**Placeholder**: Record the deep readiness probe response.

- **Command**:
  ```bash
  curl -sf http://<bind_addr>/v1/readyz/deep
  ```
- **HTTP status code**: `200 / 503 / other`
- **Response body**:
  ```json
  {
    "status": "healthy / degraded",
    "components": {
      "store": "healthy / unhealthy",
      "write_queue": "healthy / unhealthy"
    }
  }
  ```
- **Store component**: `healthy` required for PG-1.3 pass.

---

## PG-1.4 — `ferrum-migrate` Summary

**Placeholder**: Record the SQLite → PostgreSQL migration execution.

- **Source SQLite file**: `/path/to/snapshot.db`
- **Target DSN (sanitized)**: `postgres://user@host:port/dbname`
- **Command**:
  ```bash
  ferrum-migrate --from sqlite:/path/to/snapshot.db --to "postgres://..."
  ```
- **Exit code**: `0 / non-zero`
- **Migration start**: `YYYY-MM-DD HH:MM:SS UTC`
- **Migration end**: `YYYY-MM-DD HH:MM:SS UTC`
- **Duration**: `N seconds`
- **Tables migrated**: `list`
- **Warnings / errors**: `none / <list>`
- **Log excerpt**:
  ```
  (paste migration output here)
  ```

---

## PG-1.5 — Row Count Validation

**Placeholder**: Compare row counts per table between SQLite source and PostgreSQL target.

| Table | SQLite COUNT(*) | PostgreSQL COUNT(*) | Diff | Pass/Fail |
|-------|-----------------|---------------------|------|-----------|
| `intents` | `N` | `N` | `0` | ☐ |
| `proposals` | `N` | `N` | `0` | ☐ |
| `capabilities` | `N` | `N` | `0` | ☐ |
| `executions` | `N` | `N` | `0` | ☐ |
| `rollback_contracts` | `N` | `N` | `0` | ☐ |
| *(add others as needed)* | | | | |

**Overall row-count result**: `PASS / FAIL`

---

## PG-1.6 — Content Hash Validation

**Placeholder**: Record deterministic content hash comparison.

- **Hash method**: (e.g., `SHA-256` of `pg_dump --data-only` output, or per-table ordered-column hash)
- **SQLite source hash**: `sha256:...`
- **PostgreSQL target hash**: `sha256:...`
- **Match**: `YES / NO`
- **Command used**:
  ```bash
  (paste exact hash-generation command)
  ```

**Overall hash-validation result**: `PASS / FAIL`

---

## Known Gaps at Time of Evidence

**Placeholder**: List any gaps observed during PG-1 execution that do not block the baseline but should be tracked.

- [ ] *(example)* Connection pool metrics not yet visible.
- [ ] *(example)* `statement_timeout` not configured.
- [ ] *(example)* TLS/SSL mode not enforced on DSN.
- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: PG-1 is a target/staging baseline. Production readiness requires PG-2 through PG-5 and operator signoff.
- **NOT HA**: No failover, replication, or Patroni is configured.
- **NOT Block A closed**: Block A (real owned domain + DNS) remains WAIVED/CONDITIONAL.
- **NOT full G2**: G2 operator signoff requires real domain and final evidence pack review.
- **NOT schema migration discipline complete**: PG-1 uses the current one-shot migration runner; idempotent incremental migrations are PG-4.

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | | |
| Operator (optional for PG-1) | | | |

---

## Related Docs

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md)
- [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md)
- [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md)
