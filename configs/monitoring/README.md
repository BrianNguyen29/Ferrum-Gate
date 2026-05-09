# FerrumGate Monitoring Configuration Templates

This directory contains monitoring configuration templates for FerrumGate deployment. These are **templates only** — no actual secrets, email literals, or production configurations are included.

## Files

| File | Purpose | Status |
|------|---------|--------|
| `prometheus-scrape-config.yaml` | Prometheus scrape job for ferrumgate /v1/metrics | Template |
| `alertmanager-config.yaml` | AlertManager routing and receiver config | Template |
| `ferrumgate-alerts.yaml` | Prometheus alert rules for ferrumgate | Template |

## Usage

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

## Placeholder Values

The following placeholders must be replaced before use:

| Placeholder | Description |
|-------------|-------------|
| `${ALERT_CONTACT}` | Alert contact email (or use other receiver type) |
| `34-158-51-8.nip.io` | Default nip.io domain (replace with real domain) |
| `localhost:9093` | Default AlertManager URL (replace with actual endpoint) |
| `REPLACE_WITH_*` | Various webhook URLs, keys, passwords |

## Non-Claims

- **NOT production-ready configuration**
- **NOT deployment-ready without operator review**
- **No secrets stored** in these templates
- **No email literals** — all contacts are placeholders
- **nip.io domain** is temporary and must be replaced with real domain for production

## Security Notes

- AlertManager config contains template receivers only — must be configured with real credentials
- Prometheus TLS config uses `insecure_skip_verify: true` for nip.io (temporary)
- For production: use proper CA certificates and TLS verification
- Service account keys for GCS backup should be stored securely (not in version control)

## References

- [101-phase3g-ops-hardening-plan.md](../../docs/implementation-path/101-phase3g-ops-hardening-plan.md)
- [Phase 3F Conditional Pilot Authorization](../../docs/implementation-path/100-phase3f-conditional-sqlite-pilot-authorization.md)
- [FerrumGate Monitoring Metrics](../../docs/implementation-path/...) (see metrics endpoint /v1/metrics)