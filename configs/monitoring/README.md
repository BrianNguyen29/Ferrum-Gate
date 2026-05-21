# FerrumGate Monitoring Configuration Templates

This directory contains monitoring configuration templates for FerrumGate deployment. These are **templates only** — no actual secrets, email literals, or production configurations are included.

**LOCAL_ONLY: These configs support local-only monitoring deployment with localhost receivers. No real alert contact required for local mode.**

## Files

| File | Purpose | Status |
|------|---------|--------|
| `prometheus-scrape-config.yaml` | Prometheus scrape job for ferrumgate /v1/metrics | Template |
| `alertmanager-config.yaml` | AlertManager routing and receiver config | Template (local-only default) |
| `ferrumgate-alerts.yaml` | Prometheus alert rules for ferrumgate (includes PG-specific rules) | Template |

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

#### PostgreSQL Alert Rules

The `ferrumgate-alerts.yaml` file includes a `ferrumgate_postgres` rule group with alerts for:

| Alert | Condition | Notes |
|-------|-----------|-------|
| `FerrumGatePostgresMetricsAbsent` | `absent(ferrumgate_store_pg_pool_max) == 1` | **TEMPLATE** — enable only when ferrumd uses PostgreSQL. Uses absence of PG pool metrics as a proxy for PG down / disconnected. |
| `FerrumGatePostgresPoolSaturation` | `pool_idle == 0` and `pool_size >= pool_max` | Fires when all PG connections are in use. |
| `FerrumGatePostgresSlowAcquire` | `rate(acquire_timeouts_total[5m]) > 0` | Fires on any acquire timeout. Tune threshold for your workload. |
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