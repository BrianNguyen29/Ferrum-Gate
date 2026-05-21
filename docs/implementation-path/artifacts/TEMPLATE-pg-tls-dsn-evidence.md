# TEMPLATE — PostgreSQL TLS/SSL DSN Validation Evidence

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output.
> See [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) Phase PG-2.5 and [`docs/guides/operator.md`](../../guides/operator.md) §PostgreSQL TLS/SSL DSN configuration for the runbook.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `staging / production / local-docker-compose` |
| **Operator** | `name` |
| **ferrumd version / commit** | `git describe --always` |
| **PostgreSQL version** | `pg_config --version` |
| **TLS mode tested** | `require / verify-ca / verify-full` |
| **Evidence owner** | Operator |

---

## T-TLS-1 — Certificate Files Present and Permissions Correct

**Check**: CA certificate, client certificate, and client key files exist with correct permissions.

- **CA certificate path**: `/etc/ferrumgate/certs/pg-ca.crt`
- **Client certificate path** (if using client cert auth): `/etc/ferrumgate/certs/pg-client.crt`
- **Client key path** (if using client cert auth): `/etc/ferrumgate/certs/pg-client.key`
- **Permission check commands**:
  ```bash
  ls -la /etc/ferrumgate/certs/pg-ca.crt
  ls -la /etc/ferrumgate/certs/pg-client.crt
  ls -la /etc/ferrumgate/certs/pg-client.key
  ```
- **Expected permissions**:
  - CA cert: `644`, owner `root:ferrumgate` or `root:root`
  - Client cert: `644`, owner `ferrumgate:ferrumgate`
  - Client key: `600`, owner `ferrumgate:ferrumgate`
- **Actual permissions**: *(paste `ls -la` output)*

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-TLS-2 — DSN Connectivity Test

**Check**: ferrumd can connect to PostgreSQL using the TLS-enabled DSN.

- **DSN (sanitized — no password or client key contents)**:
  ```text
  postgres://user@host:5432/dbname?sslmode=verify-ca&sslrootcert=/etc/ferrumgate/certs/pg-ca.crt
  ```
- **Connection test command**:
  ```bash
  psql "${FERRUMD_STORE_DSN}" -c "SELECT 1 AS tls_connection_test;"
  ```
- **Expected output**: `tls_connection_test = 1`
- **Actual output**: *(paste here)*
- **Errors observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-TLS-3 — ferrumd Startup with TLS DSN

**Check**: ferrumd starts successfully using the TLS-enabled DSN.

- **Startup command**:
  ```bash
  FERRUMD_STORE_DSN="postgres://..." ferrumd --config /etc/ferrumgate/ferrumd.toml
  ```
- **Start time**: `YYYY-MM-DD HH:MM:SS UTC`
- **Process status after 60s**: `running / exited / panicked`
- **Relevant log excerpt**:
  ```
  (paste startup logs here — redact DSN password if present)
  ```
- **Errors observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-TLS-4 — `/v1/readyz/deep` with TLS Backend

**Check**: Deep readiness reports healthy store when connected via TLS.

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

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-TLS-5 — Certificate Rotation Drill (Optional but Recommended)

**Check**: Operator can rotate certificates without data loss.

- **Rotation steps executed**:
  1. Replace certificate files: `YYYY-MM-DD HH:MM:SS UTC`
  2. Restart ferrumd: `YYYY-MM-DD HH:MM:SS UTC`
  3. Verify `/v1/readyz/deep` returns 200: `YYYY-MM-DD HH:MM:SS UTC`
- **Downtime duration**: `N seconds`
- **Errors observed**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL / ☐ NOT PERFORMED

---

## Known Gaps at Time of Evidence

- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: TLS validation is one component of PostgreSQL hardening. Production readiness requires PG-1 through PG-5 and operator signoff.
- **NOT a security audit**: This evidence validates TLS connectivity and configuration only. It does not assess cipher suites, certificate authority trust, or penetration resistance.
- **NOT HA**: No failover, replication, or Patroni is configured.
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

- [`docs/production-readiness-v2/02-postgres-production-plan.md`](../../production-readiness-v2/02-postgres-production-plan.md) §PG-2.5
- [`docs/guides/operator.md`](../../guides/operator.md) §PostgreSQL TLS/SSL DSN configuration
- [`docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md`](./2026-05-21-phase-b-pg-production-foundation-prep.md)
