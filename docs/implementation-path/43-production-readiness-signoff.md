# 43 — Production Readiness Sign-off

**Last updated:** 2026-04-08  
**Gate:** G-E5 — Production evaluation sign-off  
**Status:** ✅ DONE

---

## Decision

FerrumGate v1 single-node is **broader production-ready** as of 2026-04-08.

This declaration is intentionally scoped:

- **T1 surface is production-supported** for single-node SQLite-backed deployments.
- **T2 surface is hardened to the PARTIAL contract level**, not promoted to T1.
- **T3 remains out of scope** and deferred post-v1.

This sign-off does **not** claim multi-node / HA support, broad external adapter
production verification, external undo guarantees, or throughput / latency SLOs.

---

## Scope of the Claim

### T1 — Production-supported

The following surface is treated as production-supported for the supported
single-node deployment model:

- governance core loop (`evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate`)
- defined REST route surface
- `ferrumctl` operator surface, including advanced authoring/control flows added in G-E3
- provenance, lineage, approvals, and ledger verification paths

### T2 — Hardened to PARTIAL contract level

The adapter-backed surfaces remain **T2 PARTIAL** per the support contract, but
they are now hardened to that partial boundary:

- fs, sqlite, git, http, and maildraft bounded local implementations
- fail-closed verify semantics
- gateway verify-false → `Failed` → commit-rejected coverage
- recovery/rollback/compensate drill coverage for the in-scope adapters

This means T2 is safer and evidence-backed within its bounded contract. It does
**not** mean T2 has been promoted to T1 or that all external side effects are
production-verified.

### T3 — Still deferred / out of scope

The following remain outside this declaration:

- multi-node / HA / read-replica (`P5.7`)
- policy bundle lifecycle tooling (`P4.2`)
- real provider send integration for EmailSend (`P2.6` post-v1 boundary)
- U1.1–U1.2 richer outcome-governance backlog
- U2 / U3 / U4 upgrade tracks

---

## Gate Evidence Summary

| Gate | Status | Evidence |
|------|--------|----------|
| G-E1 | ✅ DONE 2026-04-08 | `30-production-roadmap.md`; adapter hardening/test PR sequence through `#154` |
| G-E2 | ✅ DONE 2026-04-08 | `42-p2-performance-baseline-evidence.md` |
| G-E3 | ✅ DONE 2026-04-08 | `bins/ferrumctl/src/main.rs`; PRs `#157`, `#158` |
| G-E4 | ✅ DONE 2026-04-08 | `30-production-roadmap.md`; sync/preflight ratification PR `#159` |

Foundational pre-sign-off evidence:

- RC evidence: `25-v1-single-node-rc-evidence.md`
- support contract: `19-v1-single-node-support-contract.md`
- production roadmap gate table: `30-production-roadmap.md`
- production execution plan: `41-production-execution-plan.md`

---

## Verification Inputs Used For Sign-off

- `cargo build -p ferrum-perf-baseline`
- `cargo test -p ferrum-perf-baseline`
- `cargo run -p ferrum-perf-baseline -- --concurrency 2 --iterations 2`
- `cargo run --release -p ferrum-perf-baseline -- --concurrency 5 --iterations 5`
- `cargo check -p ferrumctl`
- `cargo test -p ferrumctl`
- `cargo run -p ferrumctl -- server compile-intent --help`
- `cargo run -p ferrumctl -- server commit-execution --help`
- `cargo test -p ferrum-sync --lib`
- `cargo test -p ferrum-store --lib sync_preflight`
- `cargo test -p ferrum-store --lib sync_service`

---

## Attestation

Sign-off basis:

1. G-E1 through G-E4 are complete in repo truth.
2. Cross-doc status has been synchronized to the canonical roadmap and execution plan.
3. The declaration preserves the support-contract boundary:
   - T1 supported
   - T2 partial but hardened within scope
   - T3 deferred / out of scope

Result: **G-E5 is ratified and broader production-ready is declared with the
scoped T1/T2/T3 interpretation above.**
