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

- [x] ledger hash chain (initial integration slice DONE)
  - Src: `08-next-issue-backlog.md` P1
  - Plan: `12-ledger-hash-chain-execution-plan.md` (Commits 1-4 complete; evidence at `18-phase-f-evidence-pack.md` line 159)
  - Future: live hash verification, ledger read-model, cross-node sync remain open per `12-ledger-hash-chain-execution-plan.md` line 130
  - **Recommended next slice: runtime integration boundary** (see `08-next-issue-backlog.md` P3)

- [x] provenance query/read-model enhancement (core surface DONE)
  - Src: `08-next-issue-backlog.md` P2 lines 19-22
  - `POST /v1/provenance/query` expanded with filters on `intent_id`, `proposal_id`, `execution_id`, `capability_id`, event kind, terminal state, time range; `ferrum-graph` read-model helpers implemented (`terminal_events`, `walk_backwards_from`, `walk_forwards_from`); integration tests at `tests/integration_provenance_query.rs`
  - Future P2: advanced replay/fabric tooling, cross-node ledger sync
  - **Recommended next slice: runtime integration boundary** (see `08-next-issue-backlog.md` P3)

- [x] operator/runtime hardening (DONE - Commit 2 complete)
  - Src: `08-next-issue-backlog.md` P2 lines 23-27
  - Confirm troubleshooting doc has clear startup-failure diagnostic entry; mark backlog done
  - Plan: `docs/implementation-path/13-operator-runtime-hardening-execution-plan.md`
  - Evidence: `docs/17-troubleshooting.md` startup-failure section exists per `13-operator-runtime-hardening-execution-plan.md` Commit 1

- [ ] ferrumctl more useful (beyond health/inspect)
  - Src: `08-next-issue-backlog.md` P2

- [x] git adapter
  - Evidence added end-to-end per `91-phase-success-criteria-and-kpis.md` line 18

- [x] http adapter (full-parity)
  - Evidence added end-to-end per `91-phase-success-criteria-and-kpis.md` line 18

## Documented drift / cleanup tasks

- scope mismatch deny is already complete in current docs/code; keep it out of remaining work (`16-release-checklist.md` line 16, `tests/integration_gateway_flow.rs:6983`)
- all other Phase A/B/E items treated as complete per `91-phase-success-criteria-and-kpis.md` lines 13-15
