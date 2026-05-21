# Local Alert Rule Syntax and Readiness Validation Evidence — 2026-05-21

> **Status**: LOCAL EVIDENCE — syntax validation and Prometheus readiness only. Live rule deployment NOT performed.
> **Purpose**: Validate FerrumGate alert rule syntax with `promtool` and confirm local Prometheus readiness endpoint responds.
> **Scope**: Local Docker `promtool` and local Prometheus instance. Live deployment of `ferrumgate-alerts.yaml` to Prometheus was NOT performed.
> **Constraint**: `production-ready = NO`. Block A remains WAIVED/CONDITIONAL. Full G2 remains NOT COMPLETE.

---

## Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Syntax validation only; live rule deployment not performed |
| **Live Prometheus alert deployment** | **NO** | `ferrumgate-alerts.yaml` was not loaded into the running Prometheus instance |
| **Full G2** | **NOT COMPLETE** | Conditional pilot only |
| **Block A** | **WAIVED/CONDITIONAL** | No real domain |

---

## Metadata

| Field | Value |
|-------|-------|
| **Timestamp** | 2026-05-21 |
| **Environment** | Local Docker (promtool) + local Prometheus |
| **Prometheus version** | `v2.55.1` (Docker image `prom/prometheus:v2.55.1`) |
| **Rule file** | `configs/monitoring/ferrumgate-alerts.yaml` |
| **Evidence owner** | Engineering |

---

## T-ALR-1 — promtool Syntax Validation

**Command**:
```bash
docker run --rm --entrypoint promtool \
  -v /home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify/configs/monitoring:/rules \
  prom/prometheus:v2.55.1 check rules /rules/ferrumgate-alerts.yaml
```
**Result**:
```
Checking /rules/ferrumgate-alerts.yaml
  SUCCESS: 21 rules found
```
**Exit code**: `0`

**Pass/Fail**: ✅ PASS

---

## T-ALR-2 — Prometheus Readiness Check

**Command**:
```bash
curl -sf http://127.0.0.1:9090/-/ready
```
**Result**: `Prometheus Server is Ready.`
**HTTP status code**: `200`

**Pass/Fail**: ✅ PASS

---

## T-ALR-3 — Live Rule Deployment Status (NOT PERFORMED)

**Observation**: The running Prometheus instance loads a different rule file:
```bash
curl -sf http://127.0.0.1:9090/api/v1/rules
```
**Result**: Success, but loaded file is `/etc/prometheus/rules/intent_api_alerts.yml`, not `ferrumgate-alerts.yaml`.

**Implication**: `ferrumgate-alerts.yaml` was **not deployed** to the live Prometheus instance. Live rule evaluation, PG-specific alert behavior verification, and AlertManager routing tests were **not performed**.

**Status**: ☐ NOT PERFORMED — requires operator to copy `ferrumgate-alerts.yaml` to Prometheus rules directory and reload.

---

## T-ALR-4 — What Was Validated vs. What Remains

| Check | Status | Notes |
|-------|--------|-------|
| promtool syntax validation | ✅ DONE | 21 rules pass syntax check |
| Prometheus server readiness | ✅ DONE | Server responds 200 on `/-/ready` |
| Rule file deployment | ☐ NOT DONE | File not copied to Prometheus rules dir |
| Rule group presence in `/api/v1/rules` | ☐ NOT DONE | Cannot verify until deployed |
| Individual rule evaluation state | ☐ NOT DONE | Cannot verify until deployed |
| PG-specific alert behavior | ☐ NOT DONE | Requires PG backend + deployed rules |
| Alert simulation | ☐ NOT DONE | Requires AlertManager + deployed rules |
| AlertManager routing | ☐ NOT DONE | Requires AlertManager config + deployed rules |

---

## Limitations and Non-Production Caveats

| Limitation | Why it matters |
|------------|---------------|
| **Syntax ≠ behavior** | `promtool check rules` validates YAML and PromQL syntax. It does not prove alerts fire correctly under real metrics. |
| **No live deployment** | The running Prometheus instance does not load `ferrumgate-alerts.yaml`. Rule evaluation against live metrics was not tested. |
| **No PG metrics available** | The local Prometheus instance may not be scraping a ferrumd process with PostgreSQL backend. `ferrumgate_store_pg_pool_max` metric may be absent. |
| **No AlertManager testing** | AlertManager routing, receiver configuration, and notification delivery were not tested. |
| **Docker promtool only** | The syntax check ran inside a Docker container. Production `promtool` may differ in version or environment. |

---

## Signoff

| Role | Name | Date | Signature / Ack |
|------|------|------|-----------------|
| Engineering | | 2026-05-21 | Local syntax validation |
| Operator | | | *(blank — operator signoff requires live deployment and validation)* |

---

## Related Docs

- [`docs/implementation-path/artifacts/TEMPLATE-pg-alert-deployment-evidence.md`](./TEMPLATE-pg-alert-deployment-evidence.md) — Full template for operator live deployment evidence
- [`configs/monitoring/README.md`](../../configs/monitoring/README.md) §Alert Deployment Validation Runbook
- [`configs/monitoring/ferrumgate-alerts.yaml`](../../configs/monitoring/ferrumgate-alerts.yaml)
- [`docs/implementation-path/artifacts/2026-05-21-pg-alert-rules-evidence.md`](./2026-05-21-pg-alert-rules-evidence.md)
