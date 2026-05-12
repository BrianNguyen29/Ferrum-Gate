# FerrumGate operator configuration examples

These files are examples only. Review and adapt paths, users, hostnames, TLS certificate paths, and retention policy before installing them on a non-production or production host.

They do not complete G2/operator signoff and do not authorize a production pilot.

Files:

- `nonprod-ferrumgate.toml` — Non-production configuration template for Path 2 Option 2 / target environment spec. It can also be adapted for local bearer-auth practice, but the local simulation guide primarily uses `configs/ferrumgate.dev.toml`. Use as base for adapting target environment spec per [`63-path-2-target-environment-spec.md`](../../docs/implementation-path/63-path-2-target-environment-spec.md).
- `ferrumd.service` — systemd service example for running `ferrumd` with an external environment file for secrets.
- `ferrumd.env.example` — Template environment file for bearer token. Copy to `/etc/ferrumgate/ferrumd.env` and replace the placeholder with a generated token (`openssl rand -hex 32`). **Do not store real tokens in version control.**
- `ferrumgate-backup.cron` — cron-based SQLite backup schedule example.
- `ferrumgate-backup.service` / `ferrumgate-backup.timer` — systemd timer-based backup schedule example.
- `postgres-backup.cron` — cron-based PostgreSQL `pg_dump` backup schedule example (15-minute interval for RPO=15min).
- `postgres-backup.service` / `postgres-backup.timer` — systemd timer-based PostgreSQL backup schedule example.
- `nginx-ferrumgate.conf` — TLS-terminating reverse proxy example.

## Handoff Flow

Path 2 preparation follows a two-phase handoff:

1. **Phase A (Repo-Side)**: FerrumGate team prepares artifacts in this directory and the docs below. This phase is complete when [`66-path-2-operator-handoff.md`](../../docs/implementation-path/66-path-2-operator-handoff.md) §Phase A is marked complete.

2. **Phase B (Target Execution)**: Operator takes possession of artifacts, deploys to target, runs drills, and signs G2 gates. Blocked until operator has target host access and has generated bearer token.

Key handoff documents:
- [`66-path-2-operator-handoff.md`](../../docs/implementation-path/66-path-2-operator-handoff.md) — Phase A/B separation and blocked gates
- [`65-path-2-target-questionnaire.md`](../../docs/implementation-path/65-path-2-target-questionnaire.md) — Operator input questionnaire (all PROVIDE fields)
- [`63-path-2-target-environment-spec.md`](../../docs/implementation-path/63-path-2-target-environment-spec.md) — Target field spec with PROVIDE/OPERATOR-GENERATED/DERIVED markers

See [`61-path-2-execution-plan.md`](../../docs/implementation-path/61-path-2-execution-plan.md) Step 4 and [`64-local-staging-simulation-guide.md`](../../docs/implementation-path/64-local-staging-simulation-guide.md) for usage context.
