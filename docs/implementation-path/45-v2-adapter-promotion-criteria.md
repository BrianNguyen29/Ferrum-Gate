# 45 — v2 Adapter T2→T1 Promotion Criteria

**Status:** DRAFT / PROPOSED — not yet ratified. Criteria are forward-looking;
per-adapter promotion is gated on Phase 3 and Phase 5 completion per
`44-v2-production-execution-plan.md`.

---

## Purpose

This document defines the concrete, measurable gates that each adapter must
satisfy to be promoted from T2 partial-contract to T1 production-supported.
Promotion is sequential:

- **Phase 3** gates fs and sqlite adapters → T1
- **Phase 5** gates git and http adapters → T1

maildraft remains T2 partial in v2; real provider send is post-v2 backlog.

v2 adapters are confirmed at **T2 partial-contract level** (hardened to partial
contract, not full production-verified external integrations). T1 promotion is
earned when all criteria in this document are verified and the corresponding
phase is marked DONE in `44-v2-production-execution-plan.md`.

---

## Gate Notation

| Symbol | Meaning |
|--------|---------|
| ✅ DONE | Criterion verified by existing test evidence or implementation |
| 🟡 IN-PROGRESS | Partially addressed; remaining work tracked |
| ⬜ NOT STARTED | Not yet started |
| N/A | Not applicable to this adapter |

---

## fs Adapter — Promotion Gates

**Target phase:** Phase 3 (`44-v2-production-execution-plan.md`)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| FS-1 | Adapter compiles without errors; `cargo check --workspace` clean | ✅ DONE | `cargo check --workspace` |
| FS-2 | Fail-closed verify on I/O errors (read/write failure → verify returns false) | ✅ DONE | `30-production-roadmap.md` P2.1 slice 1 |
| FS-3 | Compensate deletes newly created file when no pre-execute snapshot exists | ✅ DONE | `30-production-roadmap.md` P2.1 slice 2 |
| FS-4 | Fail-closed compensate/rollback on I/O error during recovery | ✅ DONE | `30-production-roadmap.md` P2.1 slice 3 |
| FS-5 | Gateway-level verify: hash mismatch → execution state set to Failed → commit rejected | ✅ DONE | `30-production-roadmap.md` P2.1 slice 4 |
| FS-6 | Gateway-level rollback drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.1 slice 5 |
| FS-7 | Gateway-level compensate drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.1 slice 6 |
| FS-8 | `before_hash`/`after_hash` wiring confirmed (PR #165 closed) | ✅ DONE | `artifacts/2026-04-09/closure-note.txt` |

**Promotion gate:** All FS-1 through FS-8 = ✅ DONE → fs promoted to T1.

---

## sqlite Adapter — Promotion Gates

**Target phase:** Phase 3 (`44-v2-production-execution-plan.md`)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| SQ-1 | Adapter compiles without errors; `cargo check --workspace` clean | ✅ DONE | `cargo check --workspace` |
| SQ-2 | Identifier safety: rollback on duplicate/malformed identifier | ✅ DONE | `30-production-roadmap.md` P2.2 slice 1 |
| SQ-3 | File-backed lifecycle: file exists → open → mutate → close → file persists | ✅ DONE | `30-production-roadmap.md` P2.2 slice 2 |
| SQ-4 | Error-path tests: failed open, failed write, failed close → graceful error propagation | ✅ DONE | `30-production-roadmap.md` P2.2 slice 2 |
| SQ-5 | Fail-closed verify on DB-open error | ✅ DONE | `30-production-roadmap.md` P2.2 slice 3 |
| SQ-6 | Fail-closed compensate/rollback on DB error during recovery | ✅ DONE | `30-production-roadmap.md` P2.2 slice 4 |
| SQ-7 | Fail-closed verify on DB-corruption mid-operation | ✅ DONE | `30-production-roadmap.md` P2.2 slice 5 |
| SQ-8 | Gateway-level verify false → execution state set to Failed → commit rejected | ✅ DONE | `30-production-roadmap.md` P2.2 slice 6 |
| SQ-9 | Gateway-level rollback drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.2 slice 7 |
| SQ-10 | Gateway-level compensate drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.2 slice 8 |

**Promotion gate:** All SQ-1 through SQ-10 = ✅ DONE → sqlite promoted to T1.

---

## git Adapter — Promotion Gates

**Target phase:** Phase 5 (`44-v2-production-execution-plan.md`)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| GT-1 | Adapter compiles without errors; `cargo check` clean | ✅ DONE | `cargo check --workspace` |
| GT-2 | Fail-closed verify on I/O errors; noop edge-case tests | ✅ DONE | `30-production-roadmap.md` P2.3 slice 2 |
| GT-3 | GitBranchCreate prepare fails closed on detached HEAD | ✅ DONE | `30-production-roadmap.md` P2.3 slice 3 |
| GT-4 | GitPush rollback no-op when no `pre_push_ref` exists | ✅ DONE | `30-production-roadmap.md` P2.3 slice 4 |
| GT-5 | GitFetch rollback restores existing local ref | ✅ DONE | `30-production-roadmap.md` P2.3 slice 5 |
| GT-6 | GitPull compensate/rollback fail-closed when branch changed since prepare/execute | ✅ DONE | `30-production-roadmap.md` P2.3 slice 6 |
| GT-7 | Gateway-level verify false → execution state set to Failed → commit rejected | ✅ DONE | `30-production-roadmap.md` P2.3 slice 7 |
| GT-8 | GitPush/GitFetch rollback fail-closed when recovery force-push/force-update fails | ✅ DONE | `30-production-roadmap.md` P2.3 slice 8 |
| GT-9 | Gateway-level rollback drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.3 slice 9 |
| GT-10 | Gateway-level compensate drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.3 slice 10 |
| GT-11 | GitPush local workflow (bounded local implementation) | ✅ DONE | `30-production-roadmap.md` P2.4 slice 1 |
| GT-12 | GitFetch local workflow (bounded local implementation) | ✅ DONE | `30-production-roadmap.md` P2.4 slice 2 |
| GT-13 | GitPull fast-forward-only workflow (bounded local implementation) | ✅ DONE | `30-production-roadmap.md` P2.4 slice 3 |

**Promotion gate:** All GT-1 through GT-13 = ✅ DONE → git promoted to T1.

---

## http Adapter — Promotion Gates

**Target phase:** Phase 5 (`44-v2-production-execution-plan.md`)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| HT-1 | Adapter compiles without errors; `cargo check --workspace` clean | ✅ DONE | `cargo check --workspace` |
| HT-2 | Fail-closed on transport errors (connection-refused, timeout) | ✅ DONE | `30-production-roadmap.md` P2.5 slices 1–2 |
| HT-3 | Explicit check mismatch/matches: verify returns false on mismatch | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5–6 and 9–10 |
| HT-4 | Gateway-level verify false → execution state set to Failed → commit rejected | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5, 7, 8, 9 |
| HT-5 | Bounded HTTP execute/verify with body-aware digest | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5–10 |
| HT-6 | Header-shape binding and canonical query string support | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5–10 |
| HT-7 | Auth support (credentials, token) | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5–10 |
| HT-8 | Conservative rollback no-op for R3 mutation boundary | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5–10 |
| HT-9 | Gateway-level rollback drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.5 slice 10 |
| HT-10 | Gateway-level compensate drill after verify returns false | ✅ DONE | `30-production-roadmap.md` P2.5 slices 5–10 |

**Promotion gate:** All HT-1 through HT-10 = ✅ DONE → http promoted to T1.

---

## maildraft Adapter — NOT Promoted in v2

**Status:** T2 partial in v2; real provider send integration is post-v2 backlog.

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| MD-1 | Adapter compiles without errors; scaffold in place | ✅ DONE | `30-production-roadmap.md` P2.6 scaffold |
| MD-2 | SQLite-backed draft persistence and verify semantics | ✅ DONE | `30-production-roadmap.md` P2.7 slice 1 |
| MD-3 | Compensate no-op (no real provider send in v2) | ✅ DONE | `30-production-roadmap.md` P2.7 slice 2 |
| MD-4 | Gateway-level verify drill | ✅ DONE | `30-production-roadmap.md` P2.7 slice 3 |
| MD-5 | Gateway-level compensate drill | ✅ DONE | `30-production-roadmap.md` P2.7 slice 4 |

maildraft is confirmed at T2 partial-contract level. T1 promotion is
**not targeted in v2**; it requires real provider send integration, which is
post-v2 backlog per `44-v2-production-execution-plan.md` Phase 5 note and
`20-v2-single-node-production-support-contract.md` Section 2.2.

---

## Summary — T1 Promotion Status

| Adapter | Target Phase | T2 Partial ✅ | T1 Promotion Gate | v2 T1 Status |
|---------|-------------|---------------|-------------------|--------------|
| fs | Phase 3 | ✅ | FS-1–FS-8 all ✅ | **Promoted to T1 in v2** |
| sqlite | Phase 3 | ✅ | SQ-1–SQ-10 all ✅ | **Promoted to T1 in v2** |
| git | Phase 5 | ✅ | GT-1–GT-13 all ✅ | **Promoted to T1 in v2** |
| http | Phase 5 | ✅ | HT-1–HT-10 all ✅ | **Promoted to T1 in v2** |
| maildraft | — | ✅ | MD-1–MD-5 all ✅ | **T2 partial only (v2)** |

---

## Source Docs

- `docs/implementation-path/44-v2-production-execution-plan.md` — phase sequencing
- `docs/implementation-path/30-production-roadmap.md` — P2.1/P2.2/P2.3/P2.4/P2.5/P2.7 slices
- `docs/20-v2-single-node-production-support-contract.md` — v2 support contract (DRAFT)
- `docs/implementation-path/11-remaining-tasks.md` — remaining task evidence
- `docs/artifacts/2026-04-09/closure-note.txt` — PR #165 closure evidence
