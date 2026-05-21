# TEMPLATE — Prometheus Alert Deployment Validation Evidence

> **⚠️ THIS IS A TEMPLATE — NOT ACTUAL EVIDENCE**
>
> Do not rename this file to a date-stamped evidence file until all sections are filled with real execution output.
> See [`configs/monitoring/README.md`](../../configs/monitoring/README.md) §Alert Deployment Validation Runbook and [`docs/guides/operator.md`](../../guides/operator.md) §Alert deployment validation for the runbook.

---

## Metadata

| Field | Template Placeholder |
|-------|---------------------|
| **Timestamp** | `YYYY-MM-DD HH:MM:SS UTC` |
| **Environment** | `staging / production / local-docker-compose` |
| **Operator** | `name` |
| **Prometheus version** | `prometheus --version` |
| **AlertManager version** | `alertmanager --version` (if used) |
| **ferrumd backend** | `SQLite / PostgreSQL` |
| **Evidence owner** | Operator |

---

## T-ALR-1 — promtool Syntax Validation

**Check**: Alert rule file passes syntax validation before deployment.

- **Rule file path**: `configs/monitoring/ferrumgate-alerts.yaml`
- **Validation command**:
  ```bash
  promtool check rules configs/monitoring/ferrumgate-alerts.yaml
  ```
- **Expected output**: `SUCCESS: N rules found`
- **Actual output**:
  ```
  (paste promtool output)
  ```
- **Exit code**: `0 / non-zero`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-ALR-2 — Rule Deployment

**Check**: Rules are copied to Prometheus rules directory and Prometheus reloads successfully.

- **Source file**: `configs/monitoring/ferrumgate-alerts.yaml`
- **Destination path**: `/etc/prometheus/rules/ferrumgate-alerts.yaml`
- **Deployment method**: `manual copy / config management / CI pipeline`
- **Prometheus reload command**:
  ```bash
  curl -X POST http://localhost:9090/-/reload
  # or
  sudo systemctl reload prometheus
  ```
- **Reload result**: `success / failure`
- **Prometheus log after reload** (relevant lines):
  ```
  (paste log excerpt)
  ```

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-ALR-3 — Rule Group Presence Verification

**Check**: The `ferrumgate` and `ferrumgate_postgres` rule groups are loaded.

- **Verification command**:
  ```bash
  curl -s "http://<prometheus>:9090/api/v1/rules" | jq '.data.groups[] | select(.file | contains("ferrumgate-alerts")) | {name, file, interval}'
  ```
- **Groups found**:
  | Group name | Interval | Status |
  |------------|----------|--------|
  | `ferrumgate` | `15s` | `loaded / missing` |
  | `ferrumgate_postgres` | `15s` | `loaded / missing` |
- **Total rules in file**: `N` (expected: 21)

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-ALR-4 — Rule State Check

**Check**: Individual rules show expected evaluation state (not unexpectedly firing).

- **Verification command**:
  ```bash
  curl -s "http://<prometheus>:9090/api/v1/rules" | jq '.data.groups[] | select(.name == "ferrumgate") | .rules[] | {name, state, health}'
  ```
- **Rules summary**:
  | Rule Name | Expected State | Actual State | Health |
  |-----------|---------------|--------------|--------|
  | `FerrumGateStoreDown` | `inactive` | | |
  | `FerrumGateWriteQueueDeep` | `inactive` | | |
  | `FerrumGateHighErrorRate` | `inactive` | | |
  | `FerrumGatePostgresMetricsAbsent` | `inactive` (if PG backend) / `inactive` (if SQLite but absent metric) | | |
  | `FerrumGatePostgresPoolSaturation` | `inactive` | | |
  | `FerrumGatePostgresSlowAcquire` | `inactive` | | |
  | `FerrumGatePostgresBackupStale` | `inactive` | | |
  | *(add others as needed)* | | | |
- **Unexpectedly firing rules**: `none / <list>`

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## T-ALR-5 — PG-Specific Alert Behavior (If PG Backend Active)

**Check**: PG pool metrics are present and `MetricsAbsent` alert is not falsely active.

- **Metric presence query**:
  ```bash
  curl -s "http://<prometheus>:9090/api/v1/query?query=ferrumgate_store_pg_pool_max"
  ```
- **Metric present**: `yes / no`
- **Metric value**: `N`
- **MetricsAbsent alert state query**:
  ```bash
  curl -s "http://<prometheus>:9090/api/v1/query?query=ALERTS{alertname=%22FerrumGatePostgresMetricsAbsent%22}"
  ```
- **MetricsAbsent active**: `yes / no` (must be `no` when PG backend is active)
- **PoolSaturation alert state**: `inactive / firing`
- **SlowAcquire alert state**: `inactive / firing`

**Pass/Fail**: ☐ PASS / ☐ FAIL / ☐ NOT APPLICABLE (SQLite backend)

---

## T-ALR-6 — Alert Simulation (Optional — Non-Production Only)

**Check**: In a test environment, trigger an alert and confirm AlertManager receives it.

- **Simulation method**: `stopped ferrumd / injected test metric / other`
- **Alert triggered**: `FerrumGatePostgresMetricsAbsent` (or other safe-to-trigger alert)
- **AlertManager query**:
  ```bash
  curl -s "http://<alertmanager>:9093/api/v2/alerts?filter=alertname=\"FerrumGatePostgresMetricsAbsent\""
  ```
- **Alert received by AlertManager**: `yes / no`
- **Labels correct**: `yes / no`
- **Annotations correct**: `yes / no`
- **Routing correct** (if configured): `yes / no`

**Pass/Fail**: ☐ PASS / ☐ FAIL / ☐ NOT PERFORMED

---

## T-ALR-7 — Documentation and Rollback Preparedness

**Check**: Operator has documented how to disable or roll back the alert rules if needed.

- **Disable method documented**: `remove rule file and reload / rename rule file / comment out group`
- **Rollback tested**: `yes / no`
- **Rollback command**:
  ```bash
  (paste command)
  ```

**Pass/Fail**: ☐ PASS / ☐ FAIL

---

## Known Gaps at Time of Evidence

- [ ] *(add as discovered)*

---

## Non-Claims

- **NOT production-ready**: Alert deployment validation is one component of monitoring readiness. Production readiness requires PG-1 through PG-5 and operator signoff.
- **NOT a security audit**: This evidence validates rule loading and basic firing behavior. It does not assess AlertManager authentication, TLS, or network segmentation.
- **NOT exhaustive metric validation**: This evidence checks a subset of rules. Operator should review all rules in `ferrumgate-alerts.yaml` for relevance to their environment.
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

- [`configs/monitoring/README.md`](../../configs/monitoring/README.md) §Alert Deployment Validation Runbook
- [`configs/monitoring/ferrumgate-alerts.yaml`](../../configs/monitoring/ferrumgate-alerts.yaml)
- [`docs/guides/operator.md`](../../guides/operator.md) §Alert deployment validation
- [`docs/implementation-path/artifacts/2026-05-21-pg-alert-rules-evidence.md`](./2026-05-21-pg-alert-rules-evidence.md)
- [`docs/implementation-path/artifacts/2026-05-21-phase-b-pg-production-foundation-prep.md`](./2026-05-21-phase-b-pg-production-foundation-prep.md)
