# 11 — Remaining tasks

Prioritized checklist of incomplete work, grounded in existing docs.
Do not invent scope; all items cite source docs.

## P0 — Firewall validation (Phase C residual)

- [x] poisoned context test suite (>= 80% on curated fixtures)
  - Src: `91-phase-success-criteria-and-kpis.md` F.3 "Poisoned-context test suite pass rate: >= 80% target", `91-phase-success-criteria-and-kpis.md` 7.5 evidence "poisoned-context tests"
  - Done: 5/5 pass (curated poisoned-context regression suite) per `91-phase-success-criteria-and-kpis.md` line 28. P1 backlog: expanding fixture breadth.
  - Note: Phase C firewall logic exists (trust labels, taint, sanitize, contradiction checks confirmed via tests). Fixture breadth expansion is P1.

## P1 — Phase F evidence pack

- [x] final docs pack for Phase F
  - Src: `91-phase-success-criteria-and-kpis.md` 7.5 evidence "final docs pack"
  - Done: `docs/18-phase-f-evidence-pack.md` exists and covers all sections

- [x] supported flows list (Phase F evidence)
  - Src: `91-phase-success-criteria-and-kpis.md` 7.5 evidence "supported flows list"
  - Done: Section 2 of `docs/18-phase-f-evidence-pack.md` has full table

- [x] open gaps list (Phase F evidence)
  - Src: `91-phase-success-criteria-and-kpis.md` 7.5 evidence "open gaps list"
  - Done: Section 3 of `docs/18-phase-f-evidence-pack.md` has P0/P1/P2 gaps

## P2 — Future work (not MVP scope)

- [ ] ledger hash chain
  - Src: `08-next-issue-backlog.md` P2

- [ ] ferrumctl more useful (beyond health/inspect)
  - Src: `08-next-issue-backlog.md` P2

- [x] git adapter
  - Evidence added end-to-end per `91-phase-success-criteria-and-kpis.md` line 18

- [x] http adapter (full-parity)
  - Evidence added end-to-end per `91-phase-success-criteria-and-kpis.md` line 18

## Documented drift / cleanup tasks

- scope mismatch deny is already complete in current docs/code; keep it out of remaining work (`16-release-checklist.md` line 16, `tests/integration_gateway_flow.rs:6983`)
- all other Phase A/B/E items treated as complete per `91-phase-success-criteria-and-kpis.md` lines 13-15
