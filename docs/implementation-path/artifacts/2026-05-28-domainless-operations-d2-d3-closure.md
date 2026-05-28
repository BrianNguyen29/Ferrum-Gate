# Domainless Operations D.2/D.3 Closure — 2026-05-28

> **Status**: Closure artifact. Records completion of the next bounded non-real-domain batch after D.2/D.3.
> **Base commit before this batch**: `db25135a9ba075b1f69668d167de595f12949626` (`db25135`)
> **Final commit**: assigned after commit/push
> **Scope**: Domainless operations only. No real domain, no Tier 2, no production-ready claim.

---

## What this batch completed

1. **`ferrumctl admin config`** (read-only CLI command)
   - Added to `bins/ferrumctl/src/main.rs`.
   - Displays effective `server_url`, bearer token presence (`<set:redacted>` / `<unset>`), and relevant env var presence.
   - No server call; no mutation.
   - Clap parse test added.

2. **Deployment docs gaps closed**
   - `docs/guides/hosted-deployment.md` now includes:
     - **Automated backup scheduling** subsection referencing existing `configs/examples/postgres-backup.timer`, `ferrumgate-backup.timer`, `postgres-backup.cron`, and `ferrumgate-backup.cron`.
     - **Managed PostgreSQL guide** subsection referencing `configs/examples/postgres-target-env.template` and PGPASSFILE guidance; no secrets.

3. **Evidence checklist housekeeping**
   - `docs/production-readiness-v2/06-admin-operator-ux-plan.md`: marked `ferrumctl admin config` complete; web dashboard remains deferred.
   - `docs/production-readiness-v2/08-hosted-deployment-plan.md`: marked backup cron/timer docs and managed PostgreSQL guide complete.
   - `docs/production-readiness-v2/10-evidence-checklist.md`: added rows 6.7 (UX-7), 8.8 (DEP-8), and 8.9 (DEP-9) as complete.

4. **Closure and snapshot artifacts**
   - This file (`2026-05-28-domainless-operations-d2-d3-closure.md`).
   - [`2026-05-28-domainless-end-state-snapshot.md`](./2026-05-28-domainless-end-state-snapshot.md).

---

## Non-claims preserved

| Non-claim | Status |
|-----------|--------|
| **production-ready** | **NO** |
| **Tier 2** | **NOT COMPLETE** |
| **full G2** | **NOT COMPLETE** |
| **Block A** | **WAIVED/CONDITIONAL** — real domain still required |
| **multi-host production HA** | **NOT COMPLETE** |
| **HA-4 unattended automated failover** | **NOT COMPLETE** |
| **sustained SLO window** | **NOT COMPLETE** |
| **D.2 web dashboard** | **DEFERRED** — TUI MVP only |
| **D.3 cloud provider modules** | **DEFERRED** — local Terraform artifact generator only |

---

*End of closure artifact. All non-claims remain unchanged.*
