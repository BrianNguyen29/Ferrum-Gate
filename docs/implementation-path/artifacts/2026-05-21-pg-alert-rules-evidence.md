# PostgreSQL Alert Rules Evidence — 2026-05-21

> **Artifact type**: Config/docs evidence  
> **Owner**: Engineering  
> **Scope**: Phase PG-2.4 — PostgreSQL-specific Prometheus alert rules (template)  
> **Non-claims preserved**: production-ready = NO; PostgreSQL production deployment = NO; HA/multi-node = NO.

---

## What was done

Added a `ferrumgate_postgres` alert group to `configs/monitoring/ferrumgate-alerts.yaml` with five rules covering the four required alert types plus one deferred placeholder.

| # | Alert | Metric / Expression | Severity | Status |
|---|-------|---------------------|----------|--------|
| 1 | `FerrumGatePostgresMetricsAbsent` | `absent(ferrumgate_store_pg_pool_max) == 1` | critical | **Template** — heuristic proxy for PG down / disconnected. |
| 2 | `FerrumGatePostgresPoolSaturation` | `pool_idle == 0 and pool_size >= pool_max and max > 0` | warning | **Template** — fires when all connections are in use. |
| 3 | `FerrumGatePostgresSlowAcquire` | `rate(acquire_timeouts_total[5m]) > 0` | warning | **Template** — fires on any acquire timeout. |
| 4 | `FerrumGatePostgresBackupStale` | `time() - backup_last_success_timestamp > 7200` | warning | **Template** — 2-hour threshold; relies on generic backup metric. |
| 5 | `FerrumGatePostgresReplicationLag` | `pg_stat_replication_pg_wal_lsn_diff / 1024^3 > 1` | warning | **Placeholder / deferred** — requires `postgres_exporter`; metric name is fictional. |

## Metric provenance

The following metrics are emitted by `ferrumd` when the PostgreSQL backend is active (see `crates/ferrum-store/src/store_facade.rs` and related metrics code):

- `ferrumgate_store_pg_pool_size`
- `ferrumgate_store_pg_pool_idle`
- `ferrumgate_store_pg_pool_max`
- `ferrumgate_store_pg_acquire_timeouts_total`
- `ferrumgate_backup_last_success_timestamp` (generic, emitted by all backends)

## File changes

- `configs/monitoring/ferrumgate-alerts.yaml` — added `ferrumgate_postgres` rule group (lines 252–331).
- `configs/monitoring/README.md` — added PG alert table and template notes.
- `docs/production-readiness-v2/02-postgres-production-plan.md` — marked gap closed, added PG-2.4 subsection.
- `docs/production-readiness-v2/10-evidence-checklist.md` — added item 1.17.

## Conservative claims

- **NOT production-deployed**: These rules exist in a template YAML file. They have **not** been loaded into a live Prometheus instance against a production PostgreSQL backend.
- **NOT definitive PG-down detection**: `FerrumGatePostgresMetricsAbsent` is a heuristic based on metric absence. A real production setup should supplement with `postgres_exporter` or cloud-provider PG monitoring.
- **NOT validated end-to-end**: Docker `promtool check rules` passed (`SUCCESS: 21 rules found`). Live Prometheus evaluation of these specific PG template rules **was not performed** because the running local Prometheus instance does not load `ferrumgate-alerts.yaml` (it loads `intent_api_alerts.yml` from a different path). Live evaluation remains **operator/env-dependent**. Operator must copy the rules file into their Prometheus rules directory and validate firing behavior before deploying.
- **Replication lag is a placeholder**: The metric name `pg_stat_replication_pg_wal_lsn_diff` does not exist in the current codebase and requires external tooling. This rule will not fire and must not be enabled until HA/replication is deployed (PG-5).
- **Thresholds are placeholders**: Acquire timeout threshold (`> 0`) and backup stale threshold (`7200s`) are conservative starting points. Operator must tune based on real workload and backup cadence.

## Verification performed

| Check | Result | Notes |
|-------|--------|-------|
| YAML syntax eye-review | Pass | Multi-line `expr` blocks match existing file style. |
| Metric name cross-check | Pass | Names match documented PG pool metrics from PG-2.2 / PG-2.3a. |
| `python3 scripts/check_contract_consistency.py` | Pass | No contract changes; script passes. |
| `git diff --check` | Pass | No trailing whitespace or conflict markers introduced. |
| `promtool check rules` (local binary) | **Skipped** | `promtool` not installed in this environment. |
| `promtool check rules` (Docker `prom/prometheus:v2.55.1`) | **PASS** | `SUCCESS: 21 rules found`. Syntax validation passed via Docker. |
| Live Prometheus config API (`/-/ready` + `/api/v1/status/config`) | **Ready but template not loaded** | Local Prometheus (`127.0.0.1:9090`) reports `Prometheus Server is Ready.` Config shows `rule_files: /etc/prometheus/rules/*.yml`. Live evaluation of `ferrumgate-alerts.yaml` rules **not performed** in this environment because the file is not present in the running Prometheus container's rules path. Operator must copy `ferrumgate-alerts.yaml` into their Prometheus rules directory and validate firing behavior before enabling. |

## Related docs

- `docs/production-readiness-v2/02-postgres-production-plan.md` §PG-2.4
- `docs/production-readiness-v2/10-evidence-checklist.md` item 1.17
- `configs/monitoring/ferrumgate-alerts.yaml`
- `configs/monitoring/README.md`
