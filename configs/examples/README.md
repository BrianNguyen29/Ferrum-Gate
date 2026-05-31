# FerrumGate operator configuration examples

These files are examples only. Review and adapt paths, users, hostnames, TLS certificate paths, and retention policy before installing them on any host.

Files:

- `nonprod-ferrumgate.toml` — Target-environment configuration template for local simulation. It can also be adapted for local bearer-auth practice, but the local simulation guide primarily uses `configs/ferrumgate.dev.toml`.
- `ferrumd.service` — systemd service example for running `ferrumd` with an external environment file for secrets.
- `ferrumd.env.example` — Template environment file for bearer token. Copy to `/etc/ferrumgate/ferrumd.env` and replace the placeholder with a generated token (`openssl rand -hex 32`). **Do not store real tokens in version control.**
- `ferrumgate-backup.cron` — cron-based SQLite backup schedule example.
- `ferrumgate-backup.service` / `ferrumgate-backup.timer` — systemd timer-based backup schedule example.
- `postgres-backup.cron` — cron-based PostgreSQL `pg_dump` backup schedule example (15-minute interval for RPO=15min).
- `postgres-backup.service` / `postgres-backup.timer` — systemd timer-based PostgreSQL backup schedule example.
- `nginx-ferrumgate.conf` — TLS-terminating reverse proxy example.

## Usage Steps

1. Review the examples in this directory.

2. Copy and adapt artifacts to your target environment, then configure and deploy.

See [`docs/guides/operator.md`](../../docs/guides/operator.md) for operator procedures.
