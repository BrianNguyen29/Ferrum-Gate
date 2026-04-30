# FerrumGate operator configuration examples

These files are examples only. Review and adapt paths, users, hostnames, TLS certificate paths, and retention policy before installing them on a non-production or production host.

They do not complete G2/operator signoff and do not authorize a production pilot.

Files:

- `ferrumgate-backup.cron` — cron-based SQLite backup schedule example.
- `ferrumgate-backup.service` / `ferrumgate-backup.timer` — systemd timer-based backup schedule example.
- `nginx-ferrumgate.conf` — TLS-terminating reverse proxy example.

See `docs/implementation-path/61-path-2-execution-plan.md` Step 4.
