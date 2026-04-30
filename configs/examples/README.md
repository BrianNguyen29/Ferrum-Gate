# FerrumGate operator configuration examples

These files are examples only. Review and adapt paths, users, hostnames, TLS certificate paths, and retention policy before installing them on a non-production or production host.

They do not complete G2/operator signoff and do not authorize a production pilot.

Files:

- `nonprod-ferrumgate.toml` — Non-production configuration template for Path 2 Option 2 / target environment spec. It can also be adapted for local bearer-auth practice, but the local simulation guide primarily uses `configs/ferrumgate.dev.toml`. Use as base for adapting target environment spec per [`63-path-2-target-environment-spec.md`](../../docs/implementation-path/63-path-2-target-environment-spec.md).
- `ferrumd.service` — systemd service example for running `ferrumd` with an external environment file for secrets.
- `ferrumgate-backup.cron` — cron-based SQLite backup schedule example.
- `ferrumgate-backup.service` / `ferrumgate-backup.timer` — systemd timer-based backup schedule example.
- `nginx-ferrumgate.conf` — TLS-terminating reverse proxy example.

See [`61-path-2-execution-plan.md`](../../docs/implementation-path/61-path-2-execution-plan.md) Step 4 and [`64-local-staging-simulation-guide.md`](../../docs/implementation-path/64-local-staging-simulation-guide.md) for usage context.
