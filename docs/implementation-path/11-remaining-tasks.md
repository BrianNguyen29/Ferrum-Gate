# 11 — Remaining tasks

Prioritized checklist of incomplete work, grounded in existing docs.
Do not invent scope; all items cite source docs.

## P0 — Firewall validation (Phase C residual)

- [ ] poisoned context test suite (>= 80% on curated fixtures)
  - Src: `91-phase-success-criteria-and-kpis.md` F.3 "Poisoned-context test suite pass rate: >= 80% target", `91-phase-success-criteria-and-kpis.md` 7.5 evidence "poisoned-context tests"
  - Note: Phase F residual — Phase C firewall logic exists (trust labels, taint, sanitize, contradiction checks confirmed via `test_high_taint_triggers_quarantine`) but curated regression fixtures still needed

## P1 — Phase F evidence pack

- [ ] final docs pack for Phase F
  - Src: `91-phase-success-criteria-and-kpis.md` 7.5 evidence "final docs pack"
  - Note: Phase F partial; integration tests and provenance evidence stronger, but final docs pack not yet done

- [ ] supported flows list (Phase F evidence)
  - Src: `91-phase-success-criteria-and-kpis.md` 7.5 evidence "supported flows list"

- [ ] open gaps list (Phase F evidence)
  - Src: `91-phase-success-criteria-and-kpis.md` 7.5 evidence "open gaps list"

## P2 — Future work (not MVP scope)

- [ ] ledger hash chain
  - Src: `08-next-issue-backlog.md` P2

- [ ] ferrumctl more useful (beyond health/inspect)
  - Src: `08-next-issue-backlog.md` P2

- [ ] git adapter
  - Src: `08-next-issue-backlog.md` P2, `04-crate-by-crate-tasks.md` "adapters: ... -> git/http"

- [ ] http adapter
  - Src: `08-next-issue-backlog.md` P2, `04-crate-by-crate-tasks.md` "adapters: ... -> git/http"

## Documented drift / cleanup tasks

- scope mismatch deny is already complete in current docs/code; keep it out of remaining work (`16-release-checklist.md` line 16, `tests/integration_gateway_flow.rs:6983`)
- all other Phase A/B/E items treated as complete per `91-phase-success-criteria-and-kpis.md` lines 13-15
