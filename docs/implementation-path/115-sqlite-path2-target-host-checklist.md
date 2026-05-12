# 115 — SQLite Path 2 Target-Host Checklist

> **Status**: Operator checklist. Partial evidence gathered 2026-05-12 (SSH unblocked, Phase3E script passed, safe restore drill, authenticated compile-only probe). B1 D1–D6 not executed. G3.6 full acceptance not achieved. No production-ready claim.  
> **Purpose**: Target-host execution checklist for closing SQLite Path 2 operator blockers B1–B5 and B8 from `66-path-2-operator-handoff.md`.  
> **Scope**: Single-node SQLite target host only. No PostgreSQL/multi-node.  
> **Constraint**: This checklist does NOT authorize production deployment. All target-host work remains operator-owned. Do not record token values.

---

## 1. Purpose

This checklist guides the operator through closing the **SQLite Path 2 target-host blockers** after selecting **Option A — Continue SQLite** in `113-operator-path-selection-packet.md`.

Blockers covered:

| Blocker ID | Description | Doc Ref |
|---|---|---|
| B1 | Target-host D1–D6 evidence | `62-path-2-operator-runbook.md` §Phase 3 |
| B2 | SQLite restore drill with `PRAGMA integrity_check` | `61-path-2-execution-plan.md` §Step 3 |
| B3 | Backup automation / external scheduler | `61-path-2-execution-plan.md` §Step 4 |
| B4 | TLS/reverse proxy configuration | `61-path-2-execution-plan.md` §Step 4 |
| B5 | Bearer token generation | `66-path-2-operator-handoff.md` §B.0 |
| B8 | G3.6 real workload / post-deploy monitoring | `116-g36-monitoring-execution-plan.md` |

**Operator-owned**: All execution, credential management, configuration adaptation, and evidence recording are operator responsibilities.

---

## 2. Explicit Non-Claims

- **No production-ready claim**: Completing this checklist does NOT make FerrumGate production-ready.
- **No G2 complete**: G2.1–G2.8 remain pending until operator signs `59-pilot-readiness-evidence-packet.md` and `54-operator-signoff-packet.md`.
- **No PostgreSQL**: PostgreSQL/multi-node/HA is not in scope for this checklist.
- **No secret recording**: Do not record bearer token values, passwords, or private key paths in this checklist or evidence.
- **No fabricated evidence**: Check boxes only after executing the step on the target host.

---

## 3. Prerequisites

Before starting, confirm:

| # | Prerequisite | Evidence | Status |
|---|---|---|---|
| P1 | Operator has selected Option A (SQLite) in doc 113 | `113-operator-path-selection-packet.md` signed for Option A | ☐ |
| P2 | Operator has access to target host | SSH / sudo confirmed | ☐ |
| P3 | `65-path-2-target-questionnaire.md` completed | All PROVIDE fields filled | ☐ |
| P4 | DNS configured | A record resolves to target host | ☐ |
| P5 | `ferrumd` binary deployed to target host | `which ferrumd` succeeds on target | ☐ |
| P6 | Backup directory exists and is writable | `test -w /var/backups/ferrumgate` | ☐ |

---

## 4. Blocker B5 — Bearer Token Generation

> **Do NOT record the token value in this checklist or any evidence document.**

| # | Step | Command / Check | Status |
|---|---|---|---|
| B5-1 | Generate bearer token on target host or secure workstation | `openssl rand -hex 32` | ☐ |
| B5-2 | Create env file from template | `cp configs/examples/ferrumd.env.example /etc/ferrumgate/ferrumd.env` | ☐ |
| B5-3 | Insert generated token into env file (value NOT recorded here) | Edit `/etc/ferrumgate/ferrumd.env` | ☐ |
| B5-4 | Set secure permissions on env file | `chmod 600 /etc/ferrumgate/ferrumd.env` | ☐ |
| B5-5 | Verify `ferrumd` starts with `auth_mode=bearer` | `ferrumd --config /etc/ferrumgate/ferrumgate.toml` starts without auth error | ☐ |

**Evidence**: Config file review showing `auth_mode = "bearer"` (token value redacted). Partial evidence: token present on target host (`TOKEN_PRESENT`); see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §5, §8.

---

## 5. Blocker B4 — TLS / Reverse Proxy Configuration

| # | Step | Command / Check | Status |
|---|---|---|---|
| B4-1 | Choose reverse proxy (nginx or Caddy) | Operator decision recorded | ☐ |
| B4-2 | Copy example config and adapt | `cp configs/examples/nginx-ferrumgate.conf /etc/nginx/sites-available/ferrumgate` | ☐ |
| B4-3 | Update `server_name` with real domain | Edit config file | ☐ |
| B4-4 | Update TLS certificate and key paths | Edit config file | ☐ |
| B4-5 | Test configuration syntax | `nginx -t` (nginx) or `caddy validate` (Caddy) | ☐ |
| B4-6 | Enable and start reverse proxy | `systemctl enable --now nginx` | ☐ |
| B4-7 | Verify HTTP → HTTPS redirect | `curl -I http://<domain>/v1/healthz` returns 301/308 | ☐ |
| B4-8 | Verify HTTPS probe passes | `curl -I https://<domain>/v1/readyz/deep` returns HTTP 200 | ☐ |
| B4-9 | Verify bearer auth through proxy | `curl -H "Authorization: Bearer $TOKEN" https://<domain>/v1/metrics` returns HTTP 200 | ☐ |

**Evidence**: TLS config excerpt (cert paths redacted), `curl` output showing HTTP 200. Partial evidence: HTTPS probes pass, `caddy.service` active; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §6, §8.

---

## 6. Blocker B3 — Backup Automation / External Scheduler

| # | Step | Command / Check | Status |
|---|---|---|---|
| B3-1 | Choose scheduler (cron or systemd timer) | Operator decision recorded | ☐ |
| B3-2 | Copy example service/timer or cron file | `cp configs/examples/ferrumgate-backup.* /etc/systemd/system/` | ☐ |
| B3-3 | Adapt paths in backup service | Edit `ExecStart` paths for target environment | ☐ |
| B3-4 | Set retention policy | Document retention (e.g., 7 daily snapshots) | ☐ |
| B3-5 | Enable and start timer | `systemctl daemon-reload && systemctl enable --now ferrumgate-backup.timer` | ☐ |
| B3-6 | Verify first backup created after timer fires | `ls -lh /var/backups/ferrumgate/` | ☐ |
| B3-7 | Verify backup integrity | `ferrumctl backup verify --db-path /var/backups/ferrumgate/ferrumgate_*.db` | ☐ |
| B3-8 | Verify retention pruning (after > retention period) | Oldest backups removed per policy | ☐ |

**Evidence**: Systemd status output, backup listing, `verify` OK output. Partial evidence: `ferrumgate-backup.timer` enabled; latest backup `ferrumgate_20260508_154446.db` present; retention pruning not yet verified; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §6.

---

## 7. Blocker B2 — SQLite Restore Drill

| # | Step | Command / Check | Status |
|---|---|---|---|
| B2-1 | Create fresh backup | `ferrumctl backup create --db-path /var/lib/ferrumgate/ferrumgate.db --output-dir /var/backups/ferrumgate` | ☐ |
| B2-2 | Verify backup integrity | `ferrumctl backup verify --db-path /var/backups/ferrumgate/ferrumgate_*.db` | ☐ |
| B2-3 | Stop ferrumd | `systemctl stop ferrumd` or `kill $(pgrep -f ferrumd)` | ☐ |
| B2-4 | Create pre-restore copy (optional but recommended) | `cp /var/lib/ferrumgate/ferrumgate.db /var/lib/ferrumgate/ferrumgate.db.pre_restore` | ☐ |
| B2-5 | Restore to temporary directory (safe drill) | `TMPDIR=$(mktemp -d); ferrumctl backup restore --backup-path /var/backups/ferrumgate/ferrumgate_*.db --target-dir "$TMPDIR"` | ☐ |
| B2-6 | Verify restored database | `ferrumctl backup verify --db-path "$TMPDIR"/ferrumgate.db` | ☐ |
| B2-7 | Run `PRAGMA integrity_check` on restored DB | `sqlite3 "$TMPDIR"/ferrumgate.db "PRAGMA integrity_check;"` → `ok` | ☐ |
| B2-8 | Remove temp directory | `rm -rf "$TMPDIR"` | ☐ |
| B2-9 | Restart ferrumd | `systemctl start ferrumd` | ☐ |
| B2-10 | Verify readiness after restart | `curl -H "Authorization: Bearer $TOKEN" https://<domain>/v1/readyz/deep` → HTTP 200 | ☐ |

**Evidence**: Restore drill log showing `integrity_check: ok`, `readyz/deep` HTTP 200. Partial evidence: safe temp-copy drill passed (`INTEGRITY=ok`, `SIZE_BYTES=4239360`, `TEMP_CLEANED=yes`); **caveat** `table_count=0` observed on both backup and current DB — operator review required; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §7.

### B2 Stop Conditions

| Trigger | Action |
|---|---|
| `ferrumctl backup verify` fails pre-restore | Do not restore. Take new backup; investigate. |
| `PRAGMA integrity_check` returns not `ok` | Do not proceed. Investigate corruption. |
| `readyz/deep` fails after restart | Investigate; restore `.pre_restore` copy if needed. |

---

## 8. Blocker B1 — Target-Host D1–D6 Evidence

| # | Step | Command / Check | Status |
|---|---|---|---|
| B1-1 | Start ferrumd on target host | `systemctl start ferrumd` | ☐ |
| B1-2 | Verify readiness | `curl -H "Authorization: Bearer $TOKEN" https://<domain>/v1/readyz/deep` → HTTP 200 | ☐ |
| B1-3 | Run D1 drills (FS: FileWrite, FileDelete) | Per `62-path-2-operator-runbook.md` §Phase 3 | ☐ |
| B1-4 | Run D2 drills (Git: GitCommit, GitPush) | Per `62-path-2-operator-runbook.md` §Phase 3 | ☐ |
| B1-5 | Run D3 drills (Git remote push/rollback) | Per `62-path-2-operator-runbook.md` §Phase 3 | ☐ |
| B1-6 | Run D4 drills (HTTP POST, non-idempotent) | Per `62-path-2-operator-runbook.md` §Phase 3 | ☐ |
| B1-7 | Run D5 drills (SQLite mutation rollback) | Per `62-path-2-operator-runbook.md` §Phase 3 | ☐ |
| B1-8 | Run D6 drills (MailDraftCreate) | Per `62-path-2-operator-runbook.md` §Phase 3 | ☐ |
| B1-9 | Capture drill output | `scripts/run_d1_d6_drills.py` or manual logs | ☐ |
| B1-10 | Fill `58-workload-compensation-drill-evidence-template.md` | Operator annotations per drill | ☐ |
| B1-11 | Verify no `fail_closed_verified: false` on critical adapters | GitPush and HTTP non-idempotent must be fail-closed | ☐ |

**Evidence**: Completed `58-workload-compensation-drill-evidence-template.md`. **Not yet executed** — B1 remains open; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §11.

### B1 Stop Conditions

| Trigger | Action |
|---|---|
| Any drill `recovered: false` with unacceptable risk | Operator evaluates; may abort or accept with compensating control. |
| `fail_closed_verified: false` on GitPush or HTTP | Abort pilot; adapter implementation required. |
| `ferrumd` unavailable during drills | Fix deployment before continuing. |

---

## 9. Blocker B8 — G3.6 Real Workload / Post-Deploy Monitoring

> **This blocker is shared with Option B.** Execute per `116-g36-monitoring-execution-plan.md`.

| # | Step | Status |
|---|---|---|
| B8-1 | Confirm load generator script available | ☐ |
| B8-2 | Execute baseline → low → target → spike → cooldown workload sequence | ☐ |
| B8-3 | Collect metrics snapshots at each phase | ☐ |
| B8-4 | Verify sustained write rate, queue depth, `readyz/deep` success rate | ☐ |
| B8-5 | Update `106-g3-6-pilot-metrics-evidence-packet.md` with real workload data | ☐ |
| B8-6 | Operator re-signs G3.6 (full, not conditional) | ☐ |

**Evidence**: Updated G3.6 evidence packet with real workload metrics. Partial evidence: authenticated bounded compile-only probe executed (173 total, 133×200, 40×429, p50 ~205.12ms); full phase sequence and adapter mix not performed; see [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md) §9, §10.

---

## 10. Acceptance Criteria

| Blocker | Criterion | Evidence |
|---|---|---|
| B1 | D1–D6 drills executed and template filled | `58-workload-compensation-drill-evidence-template.md` |
| B2 | Restore drill log shows `PRAGMA integrity_check: ok` | Restore drill log |
| B3 | Backup timer operational; `verify` passes | Systemd status + backup listing |
| B4 | HTTPS probe passes; `readyz/deep` returns 200 through proxy | `curl` output |
| B5 | `ferrumd` starts with `auth_mode=bearer`; token not recorded in docs | Config review (value redacted) |
| B8 | G3.6 real workload monitoring complete; operator signed | Updated `106-g3-6-pilot-metrics-evidence-packet.md` |

---

## 11. Operator Signoff

> **P6 CONDITIONAL GO**: Closing these blockers supports a conditional go/no-go assessment. It does NOT constitute full production-ready signoff. Full production posture requires `59-pilot-readiness-evidence-packet.md` G2.1–G2.8 and `54-operator-signoff-packet.md`.

| Blocker | Closed | Initials |
|---|---|---|
| B1 — D1–D6 target-host evidence | ☐ | |
| B2 — SQLite restore drill | ☐ | |
| B3 — Backup automation | ☐ | |
| B4 — TLS/reverse proxy | ☐ | |
| B5 — Bearer token | ☐ | |
| B8 — G3.6 real workload monitoring | ☐ | |

| Acknowledgment | Initials |
|---|---|
| I understand that closing these blockers does NOT make FerrumGate production-ready | |
| I understand that G2.1–G2.8 and doc 54 signoff are still required | |
| I understand that no secret values were recorded in this checklist | |

**Operator Name**: ____________________  
**Date**: ____________________  
**Signature**: ____________________

---

## 12. Cross-References

| This Checklist | Links To | Purpose |
|---|---|---|
| `115-sqlite-path2-target-host-checklist.md` | `113-operator-path-selection-packet.md` | Option A prerequisite |
| `115-sqlite-path2-target-host-checklist.md` | `66-path-2-operator-handoff.md` §B.0 | Blocker definitions B1–B5, B8 |
| `115-sqlite-path2-target-host-checklist.md` | `61-path-2-execution-plan.md` | Ordered execution steps |
| `115-sqlite-path2-target-host-checklist.md` | `62-path-2-operator-runbook.md` | D1–D6 drill commands |
| `115-sqlite-path2-target-host-checklist.md` | `58-workload-compensation-drill-evidence-template.md` | Drill evidence template |
| `115-sqlite-path2-target-host-checklist.md` | `116-g36-monitoring-execution-plan.md` | B8 execution plan |
| `115-sqlite-path2-target-host-checklist.md` | `59-pilot-readiness-evidence-packet.md` | G2.1–G2.8 evidence packet |
| `115-sqlite-path2-target-host-checklist.md` | `54-operator-signoff-packet.md` | Final operator signoff |

---

## 13. Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-12 | Initial checklist | Engineering |
| 2026-05-12 | Partial evidence gathered: SSH unblocked, Phase3E script passed, safe restore drill (`table_count=0` caveat), authenticated compile-only probe. B1 still not executed. G3.6 full acceptance not claimed. See [`artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md`](../artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md). | Engineering |

---

*Document updated: 2026-05-12. SQLite Path 2 Target-Host Checklist — operator-executable. No production-ready claim. No token values. P6 CONDITIONAL GO.*
