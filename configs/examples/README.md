# FerrumGate operator configuration examples

These files are examples only. Review and adapt paths, users, hostnames, TLS certificate paths, and retention policy before installing them on a non-production or production host.

They do not complete G2/operator signoff and do not authorize a production pilot.

Files:

- `nonprod-ferrumgate.toml` — Non-production configuration template for Path 2 Option 2 / target environment spec. It can also be adapted for local bearer-auth practice, but the local simulation guide primarily uses `configs/ferrumgate.dev.toml`.
- `ferrumd.service` — systemd service example for running `ferrumd` with an external environment file for secrets.
- `ferrumd.env.example` — Template environment file for bearer token. Copy to `/etc/ferrumgate/ferrumd.env` and replace the placeholder with a generated token (`openssl rand -hex 32`). **Do not store real tokens in version control.**
- `ferrumgate-backup.cron` — cron-based SQLite backup schedule example.
- `ferrumgate-backup.service` / `ferrumgate-backup.timer` — systemd timer-based backup schedule example.
- `postgres-backup.cron` — cron-based PostgreSQL `pg_dump` backup schedule example (15-minute interval for RPO=15min).
- `postgres-backup.service` / `postgres-backup.timer` — systemd timer-based PostgreSQL backup schedule example.
- `nginx-ferrumgate.conf` — TLS-terminating reverse proxy example.

## Handoff Flow

Path 2 preparation follows a two-phase handoff:

1. **Phase A (Repo-Side)**: FerrumGate team prepares artifacts in this directory.

2. **Phase B (Target Execution)**: Operator takes possession of artifacts, deploys to target, runs drills, and signs G2 gates. Blocked until operator has target host access and has generated bearer token.

See [`docs/guides/operator.md`](../../docs/guides/operator.md) for operator procedures.
