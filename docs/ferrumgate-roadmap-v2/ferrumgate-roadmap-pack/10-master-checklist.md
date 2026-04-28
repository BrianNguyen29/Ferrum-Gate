# 10 — Master checklist

## Execution Pack — Q1–Q2 gate checklist

This document (`10`) is the consolidated gate checklist for Q1–Q2 delivery.

### Gate summary
| Gate | Location | Pass criterion |
|---|---|---|
| Q1 Exit Gate | `01-quarterly-plan.md` — Q1 exit gate | Weak Spots 1–4 closed or risk-accepted in `19-v1-single-node-support-contract.md` |
| v1.1 Release Gate | `02-release-plan.md` — Release 1 gate | Evidence recorded in `docs/artifacts/<date>/` |
| Q2 Entry Gate | `01-quarterly-plan.md` — Q2 entry precondition | v1.1 release gate evidence exists |
| v1.2 Release Gate | `02-release-plan.md` — Release 2 gate | Evidence recorded in `docs/artifacts/<date>/` |

### Gate chain
```
[Q1 Exit Gate] → [v1.1 Release Gate] → [Q2 Entry Gate] → [v1.2 Release Gate]
```

Do not mark Q2 items as in-progress until Q1 exit gate evidence exists.

---

## Q1 — v1.1 Kernel Hardening

### Gate
> Q1 exit gate must be passed before Q2 work is committed. Record evidence in
> `docs/artifacts/<date>/`. Evidence link: `02-release-plan.md` Gate evidence (v1.1).

- [x] G1 — Proto shape lock: field names locked, `cargo check --workspace` passes, `cargo test --package ferrum-proto` passes (4 tests) — evidence in `docs/artifacts/2026-04-09/`
- [x] G2 — Store integrity and explicit state transitions: transition rules implemented, 46 tests pass (24 unit + 22 repo-layer), InvalidState returned for invalid transitions; raw update() paths remain defense-in-depth gap (see `docs/artifacts/2026-04-09/02-q1-p2-g2-store-integrity-evidence.md`)
- [x] Gate A (Q1-P3/PDP slice + Q1-P4a mark_used) — PDP branch coverage: 7 branches deterministic (scope, taint, R3, draft-only, forbidden outcome, advisory mismatch, default allow); no "maybe" branches; draft-only approval_mode propagation fix confirmed (server.rs:275; see `docs/artifacts/2026-04-09/03-q1-p3-pdp-audit-evidence.md`); Q1-P4a mark_used at authorize confirmed (server.rs:464; see `docs/artifacts/2026-04-09/05-q1-p4-combined-closure-note.md`); Gate A not fully closed — remaining integration coverage (steps 1.6–1.8) still open
- [x] Q1-P4 (P4a + P4b) — mark_used enforced at authorize (server.rs:464) + rollback_class propagated at prepare (server.rs:540); combined closure in `docs/artifacts/2026-04-09/05-q1-p4-combined-closure-note.md`
- [x] Q1-P5 minimum chain (Gate C conservative) — authorize+prepare+terminal events emitted on existing surface; no literal execute endpoint; conservative slice pass documented in `docs/artifacts/2026-04-09/06-q1-p5-minimum-chain-evidence.md`
- [x] Q1-P6 adversarial first slices (Gate C) — WS1/WS2/WS3/WS4 each have a passing adversarial regression test; first-pass suite scope; documented in `docs/artifacts/2026-04-09/07-q1-p6-adversarial-first-slices-evidence.md`
- [x] Q1-P7 invariant matrix pass (Q1 exit gate / v1.1) — cargo test --workspace passed; cargo test -p ferrum-gateway passed; route parity 19/19 confirmed; conservative gate pass for Q1/v1.1 scope only; documented in `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`
- [x] Enforce single-use capability end-to-end (P4a mark_used at authorize, server.rs:464)
- [x] Revalidate draft-only on prepare or equivalent safe checkpoint (server.rs:275 fix in Q1-P3)
- [x] Reconcile evaluate endpoint docs/spec/runtime (route parity 19/19 confirmed; Q1-P7 evidence)
- [x] Update release checklist — release-plan checklist items updated to reflect Q1-P7/v1.1 gate evidence; no support-contract amendment required (defect closure within existing v1 scope); evidence in `08-q1-p7-invariant-matrix-pass-evidence.md`

---

## Q2 — v1.2 Governed Engineering Changes Beta

### Gate
> Q2 entry gate requires v1.1 release gate evidence. Do not begin adapter work
> until v1.1 exit gate is documented.

### Status note (2026-04-11)
> fs-first FileWrite foundation slice confirmed: prepare → persist → compensate/restore contract
> at store+adapter layer + HTTP compensate endpoint live. Gateway-level execute and verify HTTP
> surfaces now exist with state guards (server.rs:155–162). See
> `11-gateway-execute-verify-surface-design-note.md` for details. Q2 items below
> reflect the full Q2 scope; partial progress on fs adapter is recorded for accuracy but does not
> change the gate requirements.

- [ ] Implement fs adapter real path — **partial:** backup/hash/restore at store+adapter layer confirmed; gateway-level HTTP execute/verify implemented with state guards
- [ ] Implement git adapter real path
- [ ] Implement sqlite adapter real path
- [ ] Add engineering policy templates
- [ ] Add examples for file/git/db workflows
- [ ] Demonstrate verify + compensate/rollback on real workload — **partial:** compensate/restore path demonstrated for fs; verify exercised via compensate path (verify_checks); gateway-level execute/verify HTTP surfaces implemented with state guards

## Q3 — Self-hosted Commercial Beta
- [ ] Add Postgres support
- [ ] Ship operator UI beta
- [ ] Add observability package
- [ ] Ship private deployment bundle
- [ ] Update runbooks and troubleshooting
- [ ] Prepare design-partner pilot checklist

## Q4 — MCP Governance Beta + Enterprise Evidence Alpha
- [ ] Add MCP/runtime wrapper mode
- [ ] Add tool-scoped capability mapping
- [ ] Add trust/taint propagation for tool outputs
- [ ] Add tamper-evident ledger alpha
- [ ] Add evidence export bundle
- [ ] Add signed approval/evidence alpha

## Global
- [ ] Never overclaim support scope
- [ ] Keep docs/contracts/openapi/schemas in sync
- [ ] Keep invariants and tests as first-class release blockers
- [ ] Track deferred items separately from committed roadmap

---

## V1 boundary reminder

The checklist above mixes Q1 hardening work within the locked v1 boundary and
Q2–Q4 post-v1 roadmap work. Items in this checklist (Q2–Q4
deliverables, adapter work, postgres support, operator UI, MCP integration,
enterprise evidence) are **not** v1 scope unless explicitly gated by a formal
amendment to `19-v1-single-node-support-contract.md`.

For any item in this checklist that overlaps with a known v1 limitation
(e.g., HA, multi-node, operator UI, real adapters), treat it as a post-v1
deliverable, not as a v1 scope expansion. The v1 support contract is the
only authoritative reference for what v1 covers.
