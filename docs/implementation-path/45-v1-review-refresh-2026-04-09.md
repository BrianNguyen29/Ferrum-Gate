# 45 — v1 Review Refresh (2026-04-09)

Single-node v1 scope. This document records the 2026-04-09 readiness-review
refresh pass using `44-v1-review-readiness-template.md`.

> **Review template**: [44-v1-review-readiness-template.md](./44-v1-review-readiness-template.md)
>
> **Prior sign-off**: [43-production-readiness-signoff.md](./43-production-readiness-signoff.md)
>
> **Support contract**: [19-v1-single-node-support-contract.md](../19-v1-single-node-support-contract.md)

---

## Purpose

This refresh does **not** issue a new broader-production sign-off. It records a
post-sign-off review cycle that:

1. refreshes current workspace-quality evidence,
2. rechecks key sign-off-linked commands,
3. reconciles narrow docs drift discovered during the review, and
4. captures what remains inherited from the 2026-04-02 to 2026-04-08 evidence set.

The scoped T1/T2/T3 declaration from `43-production-readiness-signoff.md`
remains unchanged.

---

## Refreshed on 2026-04-09

### Workspace and sign-off-linked checks

The following were rerun successfully on 2026-04-09:

- `cargo check --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace`
- `python3 scripts/check_contract_consistency.py`
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
- `cargo test -p ferrum-gateway --lib`

Full row-by-row evidence is recorded in
`44-v1-review-readiness-template.md` Section 0.

### Docs alignment corrections made in this refresh

The review found and corrected narrow docs drift:

- `14-api-and-contracts-map.md` now includes the v1 execution routes for
  `commit` and `rollback`, and explicitly separates additional implemented
  non-contract routes from the v1 support surface.
- `41-production-execution-plan.md` G-E1 status now matches the already-ratified
  DONE state from the roadmap/sign-off doc set.
- `42-p2-performance-baseline-evidence.md` now reflects the already-ratified
  G-E2 DONE status instead of implying ratification was still pending.

---

## Current Review Outcome

Per `44-v1-review-readiness-template.md`:

- **Docs Alignment** — ✅ PASS
- **Technical Verification** — ⚠️ INHERITED
- **Performance** — ⚠️ INHERITED
- **Security** — ✅ PASS (narrowed from ⚠️ INHERITED)
- **Stability** — ⚠️ INHERITED

Interpretation:

- No new blocker was found at the v1 support-boundary level.
- The 2026-04-08 broader-production declaration remains intact **within the
  documented v1 single-node scope**.
- Security now has fresh targeted evidence in two places:
  - 8 request-level auth tests verify 401 responses on missing/invalid/malformed
    bearer tokens and pass-through on health endpoints.
  - an internal-error sanitization test verifies 500 responses do not echo raw
    file paths, SQL fragments, or internal error strings back to the user plane.
  Remaining input-validation rows still rely on prior-cycle evidence.

---

## Remaining Next-Cycle Refresh Targets

The next formal review cycle should prioritize:

1. normalized performance regression comparison against the established G-E2 baseline,
2. optional rerun of P3.G1–P3.G4 live drills if the deployment environment or
   operating procedures changed since 2026-04-03.
3. targeted refresh for inherited input-validation rows (SQLite identifier safety,
   lineage invalid UUID handling, maildraft malformed verify_checks).

---

## Conclusion

**The 2026-04-09 review refresh found no new v1 support-boundary blocker and no
reason to retract the scoped broader-production declaration from 2026-04-08.**

This document is an evidence-refresh companion to `44-v1-review-readiness-template.md`,
not a replacement for `43-production-readiness-signoff.md`.

---

### Post-merge addendum — PR #165 (2026-04-09)

PR #165 merged a narrow fs-first `before_hash`/`after_hash` evidence slice after
this refresh was conducted. The slice added two integration tests
(`test_new_file_before_hash_none_after_prepare_after_hash_some_after_execute`,
`test_existing_file_before_hash_some_after_prepare_before_hash_ne_after_hash_after_execute`)
confirming the fs adapter rollback-path wiring.

**This PR did not alter the T1/T2/T3 declaration** from `43-production-readiness-signoff.md`
and did not introduce a new blocker or new roadmap track. It is recorded here
as evidence that the fs-first beta slice wiring is closed; it supersedes no
prior sign-off and creates no new production claim.
