# 13 — Operator / Runtime Hardening Execution Plan

Commit-by-commit plan for operator/runtime hardening.
Grounded in existing repo reality: `ferrumd` has `--print-effective-config`
and `--check-startup-guard` flags, `startup_guard_diagnostic` exists at
`bins/ferrumd/src/main.rs:365`, `ferrumctl` has `Ready` command with bearer
auth at `bins/ferrumctl/src/main.rs:726`, and an ops TLS ingress runbook
exists at `docs/runbooks/ops-tls-ingress-runbook.md`.

ASCII only.

---

## Current State

| Component | Status |
|-----------|--------|
| `--print-effective-config` flag | DONE (`bins/ferrumd/src/main.rs:59`) |
| `--check-startup-guard` flag | DONE (`bins/ferrumd/src/main.rs:64`) |
| `startup_guard_diagnostic` function | DONE (`bins/ferrumd/src/main.rs:365`) |
| `validate_startup_guard` function | DONE (`bins/ferrumd/src/main.rs:394`) |
| `run_startup_guard_check` (CLI) | DONE (`bins/ferrumd/src/main.rs:408`) |
| `ferrumctl Ready` with bearer auth | DONE (`bins/ferrumctl/src/main.rs:726`) |
| TLS termination (external) | DONE (external proxy required; documented at `docs/15-deployment-and-operations.md:15,55`) |
| Ops TLS ingress runbook | DONE (`docs/runbooks/ops-tls-ingress-runbook.md`) |
| Startup guard preflight steps | DONE (troubleshooting doc covers; see `docs/17-troubleshooting.md:42-44`) |
| Quickstart bearer-auth section | DONE (`docs/01-quickstart.md:84-90`) |

**Partial infrastructure exists.** The only remaining concrete work is
confirming the troubleshooting doc has a clear startup-failure diagnostic
entry and marking the backlog item done.

---

## Commit 1: Confirm troubleshooting startup-failure coverage

**Target:** `docs/17-troubleshooting.md`

**Scope:**
- Verify the troubleshooting doc has a clear, dedicated entry for
  "ferrumd refuses to start" covering both failure modes of
  `startup_guard_diagnostic`:
  1. non-loopback bind with auth disabled (unless `allow_insecure_nonlocal = true`)
  2. bearer auth mode with missing token
- The existing content at lines 42-56 covers this but may benefit from a
  dedicated header to make it findable under "startup failures" search.
- Cross-reference the existing ops TLS ingress runbook from the startup section.

**Validation:**
- An operator searching "startup" in the troubleshooting doc finds a
  concrete step-by-step for each failure mode.

**Out of scope:**
- Adding new runtime features

---

## Commit 2: Mark done in backlog docs

**Status:** DONE (this execution)

**Scope:**
- Update `docs/implementation-path/08-next-issue-backlog.md`: move
  operator/runtime hardening from RECOMMENDED NEXT SLICE to P2 DONE
- Update `docs/implementation-path/11-remaining-tasks.md`: mark operator/runtime
  hardening `[x]` done with citation to this doc

**Validation:**
- Both docs are consistent and refer to each other correctly

---

## Slice Status: COMPLETE

Both commits are done. This plan is a historical execution record.
Next slice: **ferrumctl more useful** (see Recommended Next Slice below).

---

## Out of Scope (Future Backlog)

- In-process TLS listener (intentionally out of scope per `18-phase-f-evidence-pack.md:155`)
- Cross-node ledger sync
- Runtime integration boundary with external event bridges

---

## Recommended Next Slice

**ferrumctl more useful** (beyond health/inspect), grounded in `08-next-issue-backlog.md` P2.
Operator/runtime hardening is complete; ferrumctl is the next open P2 item
and does not require runtime integration boundary work to proceed.

Source: `docs/implementation-path/11-remaining-tasks.md` line 47.

---

## Key Files

| File | Role |
|------|------|
| `bins/ferrumd/src/main.rs:59` | `--print-effective-config` flag |
| `bins/ferrumd/src/main.rs:64` | `--check-startup-guard` flag |
| `bins/ferrumd/src/main.rs:365` | `startup_guard_diagnostic` function |
| `bins/ferrumd/src/main.rs:394` | `validate_startup_guard` function |
| `bins/ferrumctl/src/main.rs:726` | `Ready` command with bearer auth |
| `docs/runbooks/ops-tls-ingress-runbook.md` | Ops TLS ingress runbook |
| `docs/17-troubleshooting.md` | Troubleshooting runbook |
| `docs/01-quickstart.md` | Quickstart guide |
| `docs/implementation-path/11-remaining-tasks.md:41` | Remaining work checklist |
