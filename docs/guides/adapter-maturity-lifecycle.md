# Adapter Maturity Lifecycle

> **Parent**: [`guides/README.md`](./README.md)  
> **Related**: [`adapter-reference.md`](./adapter-reference.md), [`custom-adapter.md`](./custom-adapter.md), [`ROADMAP.md`](../ROADMAP.md)

---

## Overview

FerrumGate adapters move through a P0–P5 maturity lifecycle. The level is **not**
a claim of general production readiness — it only describes how much evidence
exists for the adapter's prepare, execute, verify, rollback, and error paths.
Promotion requires evidence, not just implementation.

| Level | Meaning | Gate to promote |
|-------|---------|-----------------|
| **P0 — Not implemented** | No `AdapterPort` implementation exists, or it is a stub that returns `NotImplemented` for all phases. | Write the prepare/execute/verify/rollback contract and a failing test for the missing phase. |
| **P1 — Design / spike** | A design ADR or branch exists; code may compile but at least one phase is unimplemented or mocked. | Implement all four phases behind a feature gate or `live: false` default; add unit tests that exercise happy paths and at least one error path. |
| **P2 — Experimental** | All four phases are implemented, but at least one of the following is true: no live SDK integration, no target-host smoke, no operator runbook, or no load evidence. | Add shape-only unit tests for every error branch; add gated live integration tests (`#[ignore]` or CI job requiring a service); run a target-host smoke or MinIO equivalent; document known limitations and risks. |
| **P3 — Beta** | The adapter works end-to-end in live or near-live environments, and has a documented operator runbook, but has not been stable for two release cycles or lacks multi-environment evidence. | Two consecutive release cycles without breaking changes to the adapter contract; positive evidence from target-host or CI live-service runs; no open P2 gaps in runbook or risk model. |
| **P4 — Stable** | The adapter has been P3 for at least two release cycles, has an operator runbook, and is exercised by CI smoke or integration tests. | Not normally promoted further within this repo. |
| **P5 — Mature / governed** | Reserved for adapters with formal governance: external compliance evidence, long-term support policy, and documented migration/compatibility guarantees. | No FerrumGate adapter currently claims P5. Promotion requires an updated ADR and explicit maintainer sign-off. |

---

## Required evidence per promotion

### P0 → P1
- `AdapterPort` skeleton exists.
- `Cargo.toml` feature gate is defined if live SDK dependencies are involved.
- ADR or issue captures the adapter's risk class and rollback model.
- A failing or skipped test proves the unimplemented phase is intentional.

### P1 → P2
- All four phases implemented and compile.
- Shape-only or mocked-live unit tests for every phase (prepare, execute, verify, rollback/compensate).
- At least one explicit error path tested (e.g., invalid input, unsupported action, missing pre-state).
- `live: false` default unless live SDK integration is implemented and gated.
- Entry added to `docs/ROADMAP.md` as P2 Experimental.
- Cross-reference from `adapter-reference.md`.

### P2 → P3
- Gated live integration tests exist (`#[ignore]`, feature-gated, or CI job requiring a service such as MinIO).
- Target-host or CI live-service smoke evidence exists and is recorded in a runbook or evidence file.
- Operator runbook covers failure modes, rollback limits, and credentials/endpoint setup.
- Two release cycles without breaking changes to the adapter contract or action binding.
- `ROADMAP.md` updated to P3 Beta.

### P3 → P4
- Continuous CI smoke or integration coverage that runs on every PR or nightly.
- Operator runbook is complete and validated.
- No known P2/P3 gaps for at least two release cycles.
- `ROADMAP.md` updated to P4 Stable; `adapter-reference.md` and runbook updated if needed.

### P4 → P5
- Not promoted automatically. Requires an updated ADR, compatibility policy, and explicit maintainer sign-off. No FerrumGate adapter claims P5 today.

---

## Current adapter classification

| Adapter | Level | Notes / evidence |
|---------|-------|------------------|
| `fs` | P4 Stable | Sandbox + snapshot rollback; CI coverage. |
| `git` | P4 Stable | Repository-root allowlist + rollback; CI coverage. |
| `http` | P4 Stable | rustls client, SSRF guard, bounded timeout, no redirects; CI coverage. |
| `sqlite` | P4 Stable | File-backed mutation with database-root allowlist; CI coverage. |
| `maildraft` | P4 Stable | Drafts only; does not send email; CI coverage. |
| `s3` | P2 Experimental | Live put/delete/get/copy implemented; versioning-based rollback; MinIO integration tests gated; shape-only unit tests run via `make adapter-smoke`. Remains P2 because production S3 readiness requires operator-managed credentials, endpoint validation, and target-host evidence. |
| `gcs` | P2 Experimental | Shape-only put/delete/get; generation-based rollback modeled; live SDK path is a declared seam; shape-only unit tests run via `make adapter-smoke`. Remains P2 until live SDK integration and target-host smoke exist. |
| `azure` | P0 Not implemented | Deferred until GCS semantics are stable. |

> **Why S3 is conservatively P2 despite live paths**: S3 has a live client seam and
> MinIO integration tests, but the live path requires operator-managed credentials
> and endpoints. The default `s3_config` uses `live: false` unless the operator
> explicitly enables it. Therefore the label reflects the *default, operator-safe*
> maturity level, not the maximum possible capability when fully configured.

---

## Promotion process

When an adapter is ready to move up:

1. Open a separate PR scoped to that adapter.
2. Add or promote evidence:
   - Unit tests for every new error path.
   - Live integration tests or CI smoke evidence.
   - Target-host smoke evidence where required.
   - Operator runbook entry in `docs/operations/runbook.md` or adapter-specific docs.
3. Update `docs/ROADMAP.md` and `docs/guides/adapter-reference.md`.
4. If the promotion changes the public action binding, rollback contract, or risk model, update the relevant ADR.
5. Run `python3 scripts/validate_adapter_maturity.py` to confirm the docs/code labels still align.

---

## Advisory drift check

`scripts/validate_adapter_maturity.py` runs a lightweight, non-blocking check that
warns if a P4 adapter's source directory contains `not implemented` placeholders or
if the ROADMAP adapter table is missing an expected row. It is advisory; it does
not change labels or block CI.
