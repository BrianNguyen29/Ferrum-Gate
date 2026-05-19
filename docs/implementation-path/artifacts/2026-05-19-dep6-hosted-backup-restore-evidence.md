# DEP-6 Hosted Backup/Restore Evidence — 2026-05-19

## Status

- **Scope**: DEP-6 hosted-mode SQLite backup/restore drill on `ferrumgate-nonprod`.
- **Verdict**: ✅ PASS for hosted single-node SQLite backup/restore drill using temp-copy restore.
- **Production-ready**: NO.
- **Full G2**: NOT COMPLETE.
- **PostgreSQL production deployment**: NO.
- **HA/multi-node**: NO.

This artifact records a hosted backup/restore drill on the target VM after DEP-4 passed. The restore used an isolated temporary copy and did **not** overwrite the live database.

## Safety constraints followed

- Live DB was not overwritten.
- Restore target was a temporary path under `/tmp`.
- Temp ferrumd restore smoke bound to loopback `127.0.0.1:19081` only.
- Bearer tokens and offsite bucket names were not printed.
- Live service remained active after the drill.

## Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-19 |
| Project | `fairy-b13f4` |
| Zone | `asia-southeast1-a` |
| VM | `ferrumgate-nonprod` |
| Mode | SQLite single-node hosted VM |
| Live DB | `/var/lib/ferrumgate/ferrumgate.db` |
| Backup directory | `/var/lib/ferrumgate/backups` |
| Successful drill backup | `/var/lib/ferrumgate/backups/dep6_20260519T131036Z.db` |
| Temp restore DB | `/tmp/ferrumgate-dep6-20260519T131036Z/restored.db` |

## Backup creation

Command pattern:

```bash
sudo -u ferrumgate sqlite3 /var/lib/ferrumgate/ferrumgate.db ".backup '/var/lib/ferrumgate/backups/dep6_20260519T131036Z.db'"
```

Observed result:

```text
-rw------- ferrumgate:ferrumgate 339968 dep6_20260519T131036Z.db
backup_integrity=ok
backup_table_count=14
```

## Temp-copy restore and integrity

The backup was copied to an isolated temporary restore path. It was not copied over the live DB.

Observed result:

```text
restore_integrity=ok
restore_table_count=14
```

Hash comparison confirmed the temp restore copy matched the backup exactly:

```text
486e3ddf088fb8b360da654a9378cad2d992f7c8be79efc8660497a975f3dad1  backup
486e3ddf088fb8b360da654a9378cad2d992f7c8be79efc8660497a975f3dad1  temp restore copy
```

## Restore smoke test

A temporary ferrumd process was started with a temporary config pointing at the restored copy:

```toml
[server]
bind_addr = "127.0.0.1:19081"
store_dsn = "sqlite://[REDACTED_TEMP_DB]"
auth_mode = "disabled"
```

Observed result:

```text
restore_ready_http=200
restore_rto_seconds=1
restore_process_stopped=yes
```

Readiness body showed store and write queue healthy.

Sanitized temp ferrumd log:

```text
starting ferrumd with config: auth_mode=disabled, bind_addr=127.0.0.1:19081, store_dsn=sqlite://[REDACTED_TEMP_DB]
ferrumd listening on 127.0.0.1:19081
shutdown signal received, draining connections...
write queue writer task shutting down
```

## Timer and offsite sync evidence

Timers observed:

```text
ferrumgate-backup.timer: active
ferrumgate-offsite-backup.timer: active
```

Backup timer evidence:

- `ferrumgate-backup.timer` is active and triggers `ferrumgate-backup.service`.
- Journal showed repeated successful hourly backup runs.

Offsite sync evidence:

- `ferrumgate-offsite-backup.timer` is active.
- Manual `systemctl start ferrumgate-offsite-backup.service` returned exit code `0`.
- `systemctl status ferrumgate-offsite-backup.service` showed `status=0/SUCCESS`.
- Journal showed `Finished ferrumgate-offsite-backup.service - FerrumGate offsite backup sync to GCS`.
- Bucket name was redacted from script output.

## Live service post-drill check

After the temp restore drill and offsite sync:

```text
systemctl is-active ferrumgate: active
live_healthz_http=200
{"status":"ok"}
```

## Anomaly recorded

An initial temp-copy attempt failed to copy a root/ferrumgate-owned backup into `/tmp` due permissions. The failed attempt did not touch the live DB. The corrected drill used `sudo cp`, verified hash equality, and passed integrity/readiness checks.

## Non-claims

- **NOT production-ready**: This validates the hosted backup/restore procedure only.
- **NOT PostgreSQL production**: Drill used the active SQLite target.
- **NOT HA/multi-node**: Single VM only.
- **NOT full G2**: Real-domain/full-G2 requirements remain separate.
- **RPO/RTO not guaranteed**: Observed temp restore readiness took 1 second for this drill only.

## Gate result

DEP-6 hosted backup/restore validation is complete for the current single-node SQLite target:

- [x] Hosted target VM used.
- [x] Backup created from live SQLite DB.
- [x] Backup integrity check passed.
- [x] Temp restore copy integrity check passed.
- [x] Backup and restore-copy hashes matched.
- [x] Temp ferrumd restore smoke returned readyz HTTP 200.
- [x] Live DB was not overwritten.
- [x] Live service remained active after the drill.
- [x] Backup timer active.
- [x] Offsite sync service completed with status 0.
- [x] No production-ready or HA claim introduced.
