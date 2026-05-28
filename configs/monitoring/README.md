# FerrumGate Monitoring Configuration Templates

This directory contains monitoring configuration templates for FerrumGate deployment. These are **templates only** — no actual secrets, email literals, or production configurations are included.

**LOCAL_ONLY: These configs support local-only monitoring deployment with localhost receivers. No real alert contact required for local mode.**

## Files

| File | Purpose | Status |
|------|---------|--------|
| `prometheus-scrape-config.yaml` | Prometheus scrape job for ferrumgate /v1/metrics | Template |
| `alertmanager-config.yaml` | AlertManager routing and receiver config | Template (local-only default) |
| `ferrumgate-alerts.yaml` | Prometheus alert rules for ferrumgate (includes PG-specific rules) | Template |
| `ferrumgate-grafana-dashboard.json` | Grafana dashboard JSON for FerrumGate overview panels | Template |

## Usage

### Local-Only Mode (Default for non-production)

For local testing/monitoring without real alert contacts:

```bash
# Use phase3g_configure_monitoring.sh with --local-only flag
bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --local-only [...]
```

Local-only mode特点:
- Uses localhost receivers only
- No real alert contact required
- Non-production claim boundary clearly stated

### External Mode (Requires real alert contact)

For production alerting with external Prometheus/AlertManager:

```bash
# Use phase3g_configure_monitoring.sh with --alert-contact flag
bash scripts/gcp/phase3g_configure_monitoring.sh --confirm --alert-contact CONTACT [...]
```

### Prometheus Scrape Config

Copy `prometheus-scrape-config.yaml` to your Prometheus server and include it in your `prometheus.yml`:

```yaml
# In prometheus.yml
scrape_configs:
  - job_name: 'ferrumgate'
    file_sd_configs:
      - files:
          - /etc/prometheus/ferrumgate-scrape.yaml
```

Or use `prometheus --config.file` to load directly.

### AlertManager Config

1. Copy `alertmanager-config.yaml` to your AlertManager server
2. Replace PLACEHOLDER values with actual receiver configurations
3. Validate: `amtool check-config /path/to/alertmanager.yml`
4. Reload: `curl -X POST http://localhost:9093/-/reload`

### Alert Rules

1. Copy `ferrumgate-alerts.yaml` to your Prometheus rules directory
2. Update `prometheus.yml` to include the rules file:
   ```yaml
   rule_files:
     - /etc/prometheus/rules/ferrumgate-alerts.yaml
   ```
3. Reload Prometheus

### Grafana Dashboard

1. Copy `ferrumgate-grafana-dashboard.json` to your Grafana provisioning directory or import via the UI:
   - **UI import**: `Dashboards → Import → Upload JSON file → select ferrumgate-grafana-dashboard.json`
   - **Provisioning**: Place the file in your Grafana `dashboards` provisioning path (e.g., `/etc/grafana/provisioning/dashboards/`)
2. Ensure your Prometheus data source is named `prometheus` (or update the `datasource` fields in the JSON if you use a different name).
3. The dashboard includes panels for HTTP request rate, error rate, P95 latency, PG pool metrics, acquire timeouts, health status, active connections, and pool saturation.

**Important**: The dashboard PromQL uses FerrumGate application metric names (`ferrumgate_http_requests_total`, `ferrumgate_request_duration_seconds_bucket`, `ferrumgate_store_health_up`, and PostgreSQL pool metrics). Review and adjust expressions if your Prometheus relabeling or exporter setup changes metric names.

#### PostgreSQL Alert Rules

The `ferrumgate-alerts.yaml` file includes a `ferrumgate_postgres` rule group with alerts for:

| Alert | Condition | Notes |
|-------|-----------|-------|
| `FerrumGatePostgresMetricsAbsent` | `absent(ferrumgate_store_pg_pool_max) == 1` | **TEMPLATE** — enable only when ferrumd uses PostgreSQL. Uses absence of PG pool metrics as a proxy for PG down / disconnected. |
| `FerrumGatePostgresPoolSaturation` | `pool_idle == 0` and `pool_size >= pool_max` | Fires when all PG connections are in use. |
| `FerrumGatePostgresSlowAcquire` | `rate(ferrumgate_store_pg_acquire_timeouts_total[5m]) > 0` | Fires on any acquire timeout. Tune threshold for your workload. |
| `FerrumGatePostgresBackupStale` | `time() - backup_last_success_timestamp > 7200` | **TEMPLATE** — 2-hour threshold. Adjust to your backup cadence. Relies on generic backup metric. |
| `FerrumGatePostgresReplicationLag` | `pg_stat_replication_pg_wal_lsn_diff > 1 GB` | **PLACEHOLDER / DEFERRED** — requires `postgres_exporter` or equivalent. Do not enable until HA/replication is deployed. |

**Important**: The PG alert rules are templates. `FerrumGatePostgresMetricsAbsent` is a heuristic (absence of application-level metrics), not a definitive "database is down" signal. For production, supplement with `postgres_exporter` or cloud PG monitoring. The replication-lag alert is a placeholder with a fictional metric name and will not fire without external tooling.

**Validation status**: Docker `promtool check rules` passed (`SUCCESS: 21 rules found`) on 2026-05-21. Live Prometheus evaluation of these specific rules was not performed in the local environment because the running Prometheus instance loads a different rule file (`intent_api_alerts.yml`). Operator must validate firing behavior in their environment before deploying.

## Placeholder Values

The following placeholders must be replaced before use:

| Placeholder | Description |
|-------------|-------------|
| `${ALERT_CONTACT}` | Alert contact email (or use other receiver type) |
| `34-158-51-8.nip.io` | Default nip.io domain (replace with real domain) |
| `localhost:9093` | Default AlertManager URL (local-only mode default) |
| `REPLACE_WITH_*` | Various webhook URLs, keys, passwords |

## Alert Deployment Validation Runbook

This section documents how an operator deploys the FerrumGate alert templates to a live Prometheus instance and validates them. It is a **runbook** — no live deployment has been performed in the engineering environment.

### Prerequisites

| Check | How | Gate |
|-------|-----|------|
| Prometheus server running | `curl http://<prometheus>:9090/-/healthy` returns `Prometheus Server is Healthy.` | Block if not 200 |
| `promtool` installed | `promtool --version` succeeds | Block if missing |
| AlertManager running (if routing alerts) | `curl http://<alertmanager>:9093/-/healthy` returns `OK` | Warn if missing |
| `ferrumgate-alerts.yaml` reviewed | Operator has read this README and the alert descriptions above | Required |

### Step 1 — Syntax validation with promtool

```bash
# Validate rule syntax before deployment
promtool check rules configs/monitoring/ferrumgate-alerts.yaml
```

- **Pass**: `SUCCESS: 21 rules found` (or equivalent; rule count may change).
- **Fail**: Fix syntax errors before proceeding.
- **Evidence**: Capture `promtool` output.

### Step 2 — Deploy to Prometheus rules directory

```bash
# Copy rules to Prometheus rules path (path is operator-specific)
sudo cp configs/monitoring/ferrumgate-alerts.yaml /etc/prometheus/rules/

# Ensure prometheus.yml includes the rules file
# rule_files:
#   - /etc/prometheus/rules/ferrumgate-alerts.yaml

# Reload Prometheus (method depends on deployment)
curl -X POST http://localhost:9090/-/reload
# or
sudo systemctl reload prometheus
```

- **Pass**: Prometheus reloads without error; `/api/v1/rules` shows `ferrumgate` group.
- **Fail**: Check Prometheus logs for rule-loading errors.
- **Evidence**: Screenshot or curl output of `/api/v1/rules` showing loaded groups.

### Step 3 — Verify rule evaluation state

```bash
# List all rules and their evaluation state
curl -s "http://<prometheus>:9090/api/v1/rules" | jq '.data.groups[] | select(.name == "ferrumgate")'

# Check a specific alert state
curl -s "http://<prometheus>:9090/api/v1/rules" | jq '.data.groups[] | select(.name == "ferrumgate_postgres") | .rules[] | {name, state}'
```

- **Pass**: All rules show `state: inactive` (normal) or documented expected state.
- **Fail**: Rules in `firing` unexpectedly; investigate metric availability or thresholds.
- **Evidence**: JSON output or screenshot of rule states.

### Step 4 — Validate PG-specific alert behavior (if PG backend is active)

If ferrumd is running with PostgreSQL and Prometheus is scraping `/v1/metrics`:

```bash
# Confirm PG pool metrics are present
curl -s "http://<prometheus>:9090/api/v1/query?query=ferrumgate_store_pg_pool_max"

# Confirm the absence-based alert is not falsely firing
curl -s "http://<prometheus>:9090/api/v1/query?query=ALERTS{alertname=\"FerrumGatePostgresMetricsAbsent\"}"
```

- **Pass**: `ferrumgate_store_pg_pool_max` returns a value; `MetricsAbsent` alert is not active.
- **Fail**: Metrics missing → check scrape config and ferrumd store backend.
- **Evidence**: Query result JSON.

### Step 5 — Simulate an alert condition (optional, non-production only)

In a non-production environment, temporarily trigger an alert to verify AlertManager routing:

```bash
# Example: artificially stop ferrumd to cause metrics absence
# (Only in test/staging environments)
# Then verify AlertManager receives the alert:
curl -s "http://<alertmanager>:9093/api/v2/alerts?filter=alertname=\"FerrumGatePostgresMetricsAbsent\""
```

- **Pass**: Alert appears in AlertManager with correct labels and annotations.
- **Fail**: Alert missing → check Prometheus → AlertManager connectivity and routing config.
- **Evidence**: AlertManager UI screenshot or API response.

### Step 6 — Evidence artifact

After validation, create:

**Path**: `docs/implementation-path/artifacts/YYYY-MM-DD-pg-alert-deployment-evidence.md`

Template sections:
1. **Environment** — Prometheus version, AlertManager version, ferrumd backend.
2. **promtool validation** — output and result.
3. **Deployment method** — copy path, reload method.
4. **Rule evaluation state** — screenshot or API output.
5. **PG-specific alert check** — metric presence, alert state.
6. **Simulation results** (if performed) — trigger method, AlertManager receipt.
7. **Operator signoff** — blank until signed.

> **Non-claim**: This runbook documents the intended validation procedure. No live Prometheus evaluation of these specific rules was performed in the engineering environment because the running Prometheus instance loads a different rule file (`intent_api_alerts.yml`). Operator must execute this runbook in their environment and create the evidence artifact.

## Non-Claims

- **NOT production-ready configuration**
- **NOT production alerting**
- **NOT deployment-ready without operator review**
- **No secrets stored** in these templates
- **No email literals** — all contacts are placeholders
- **nip.io domain** is temporary and must be replaced with real domain for production
- **LOCAL_ONLY mode**: Uses localhost receivers only, no real alerting

## Security Notes

- AlertManager config contains template receivers only — must be configured with real credentials for production
- Prometheus TLS config uses `insecure_skip_verify: true` for nip.io (temporary)
- For production: use proper CA certificates and TLS verification
- Service account keys for GCS backup should be stored securely (not in version control)
- LOCAL_ONLY mode intentionally uses localhost receivers to avoid secret/contact requirements

## References

- [101-phase3g-ops-hardening-plan.md](../../docs/implementation-path/101-phase3g-ops-hardening-plan.md)
- [Phase 3F Conditional Pilot Authorization](../../docs/implementation-path/100-phase3f-conditional-sqlite-pilot-authorization.md)
- [FerrumGate Monitoring Metrics](../../docs/implementation-path/...) (see metrics endpoint /v1/metrics)
