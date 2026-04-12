# 16 — Release checklist

v2 RATIFIED. Single-node v2 scope. Last updated: 2026-04-12.

> **Canonical support contract**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)
>
> **RC status**: FerrumGate v1 single-node is RC-ready as of 2026-04-02 — see
> [25-v1-single-node-rc-evidence.md](./implementation-path/25-v1-single-node-rc-evidence.md) for full evidence.
>
> **Broader production-ready status**: achieved 2026-04-08 via G-E1 through G-E5 —
> see [43-production-readiness-signoff.md](./implementation-path/43-production-readiness-signoff.md)
> and [30-production-roadmap.md](./implementation-path/30-production-roadmap.md).
>
> **Out-of-tree SQLite candidate**: NOT merged — see
> [40-out-of-tree-sqlite-performance-candidate.md](./implementation-path/40-out-of-tree-sqlite-performance-candidate.md).

## Contract integrity
- [x] contracts cap nhat (ran check_contract_consistency.py: VALIDATION PASSED) — evidence: `docs/artifacts/2026-03-30/05-contract-consistency.txt`
- [x] schemas cap nhat — evidence: `docs/artifacts/2026-03-30/05-contract-consistency.txt`
- [x] openapi cap nhat (synced to actual routes/auth) — evidence: `docs/artifacts/2026-03-30/05-contract-consistency.txt`
- [x] docs cap nhat (14, 15, 17, 01 updated; see also implementation-path docs)
- [x] support contract review: scope-affecting changes require update to [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md) before release

## Workspace quality
- [x] cargo check pass (`cargo check --workspace`) — evidence: `cargo check --workspace` PASS (2026-04-02) — PASS
- [x] fmt pass (`cargo fmt --all -- --check`) — evidence: `cargo fmt --all -- --check` PASS (2026-04-02) — PASS
- [x] clippy pass (`cargo clippy --workspace -- -D warnings`) — evidence: `cargo clippy --workspace -- -D warnings` PASS (2026-04-02) — PASS
- [x] cargo test pass (`cargo test --workspace`) — evidence: `cargo test --workspace` PASS (2026-04-02) — PASS

## Behavior quality
- [x] scope mismatch deny test — VERIFIED: empty scope + non-R0 mutation = Deny (`scope.mismatch.empty.scope`), empty scope + R0 = Allow (`test_scope_mismatch_deny_on_empty_scope_with_mutation`, `test_r0_allowed_with_empty_scope`)
- [x] single-use capability test — VERIFIED: capability marked Used returns AlreadyUsed error on reuse (`test_single_use_capability_cannot_be_reused`)
- [x] R3 no auto-commit test — VERIFIED: R3 contracts have auto_commit=false, R0 have auto_commit=true (`test_r3_contracts_have_auto_commit_false`)
- [x] rollback/compensate test — VERIFIED: rollback and compensate are distinct adapter operations (`test_rollback_and_compensate_are_distinct_operations`)
- [x] poisoned context test — VERIFIED: high taint score (>=70) triggers Quarantine decision for non-R0 (`test_high_taint_triggers_quarantine`)
- [x] compensate end-to-end flow test — VERIFIED: full evaluate -> mint -> authorize -> prepare -> compensate flow with state transitions (`compensate_execution_flow`)
- [x] pending approvals pagination test — VERIFIED: limit/offset pagination returns correct subsets (`test_pending_approvals_pagination`)
- [x] pending approvals filter test — VERIFIED: filter by proposal_id returns correct subset (`test_pending_approvals_filtered_by_proposal_id`)
- [x] lineage endpoint shape tests — VERIFIED: empty lineage for unknown execution returns 200, invalid UUID returns 400, content-type correct, max_hops clamping works (`test_lineage_endpoint_*`)

## Operator readiness
- [x] config docs dung (15-deployment-and-operations.md updated)
- [x] CLI huu ich toi thieu (server health/inspect-execution/inspect-approvals/inspect-approval/inspect-lineage/inspect-provenance)
- [x] lineage usable (GET /v1/provenance/lineage/{execution_id} implemented)
- [x] approval flow documented (GET /v1/approvals, GET /v1/approvals/{approval_id} implemented)
- [x] startup SOP requires a functional readiness probe after healthz/readyz; shallow checks alone are not sufficient
- [x] backup/restore drill procedure and evidence template documented in 18-single-node-operations-runbook.md Section 6.4; operator backup cadence and RPO guidance documented in Sections 5.3 and 5.4

## Broader production-ready sign-off
- [x] G-E1 adapter hardening complete — evidence: `docs/implementation-path/30-production-roadmap.md`
- [x] G-E2 performance baseline captured — evidence: `docs/implementation-path/42-p2-performance-baseline-evidence.md`
- [x] G-E3 ferrumctl advanced operator coverage complete — evidence: `bins/ferrumctl/src/main.rs`, roadmap G-E3 row
- [x] G-E4 sync preflight / decision ratified — evidence: roadmap P5.4/P5.5 + G-E4 row
- [x] G-E5 production evaluation sign-off completed — evidence: `docs/implementation-path/43-production-readiness-signoff.md`
