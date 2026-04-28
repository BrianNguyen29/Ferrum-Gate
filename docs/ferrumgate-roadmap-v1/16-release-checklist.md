# 16 — Release checklist

> **⚠️ Historical / Superseded by P6/P7**: This checklist was current as of 2026-03-30. It has been superseded by P6/P7 validation. The Phase status summary in `docs/implementation-path/01-current-state.md` and the remaining tasks in `docs/implementation-path/11-remaining-tasks.md` reflect the current state.
>
> **Note**: Phase 2 (transaction batching) was partially implemented but deferred/regressed. Phase 1 write-queue remains production target. See `docs/PERFORMANCE_OPTIMIZATION_PLAN.md`.

Single-node v1 scope. Last updated: 2026-03-30.

> **Canonical support contract**: [19-v1-single-node-support-contract.md](./19-v1-single-node-support-contract.md)

## Production posture notes

> **Production architecture**: Phase 1 SQLite write queue. Phase 2 (transaction batching + direct UPDATE) was partially implemented but **deferred/regressed** due to performance regression in benchmarking. See `docs/PRODUCTION_NOTES.md` Phase 2 entry.
>
> **Production readiness**: RC-ready (single-node SQLite). Production deployment should be evaluated against `docs/implementation-path/27-production-evaluation-plan.md`. Conditional constraints apply due to documented invariant/control gaps (see `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` Weak Spots 1-4 and `19-v1-single-node-support-contract.md` Accepted Risks §4).
>
> **Production branch**: Phase 1 only. Phase 2 deferred. Phase 3 (PostgreSQL) is the path to full production scale.

## Contract integrity
- [x] contracts cap nhat (ran check_contract_consistency.py: VALIDATION PASSED) — evidence: `docs/artifacts/2026-03-30/05-contract-consistency.txt`
- [x] schemas cap nhat — evidence: `docs/artifacts/2026-03-30/05-contract-consistency.txt`
- [x] openapi cap nhat (synced to actual routes/auth) — evidence: `docs/artifacts/2026-03-30/05-contract-consistency.txt`
- [x] docs cap nhat (14, 15, 17, 01 updated; see also implementation-path docs)

## Workspace quality
- [x] cargo check pass (`cargo check --workspace`) — evidence: `docs/artifacts/2026-03-30/01-cargo-check.txt` — PASS
- [x] fmt pass — evidence: `docs/artifacts/2026-03-30/02-cargo-fmt.txt` — PASS
- [x] clippy pass — evidence: `docs/artifacts/2026-03-30/03-cargo-clippy.txt` — PASS
- [x] cargo test pass — evidence: `docs/artifacts/2026-03-30/04-cargo-test.txt` — PASS (tests pass)

## Behavior quality
- [x] scope mismatch deny test — VERIFIED: empty scope + non-R0 mutation = Deny (`scope.mismatch.empty.scope`), empty scope + R0 = Allow (`test_scope_mismatch_deny_on_empty_scope_with_mutation`, `test_r0_allowed_with_empty_scope`)
- [x] single-use capability test — VERIFIED (service-level): capability marked Used returns AlreadyUsed error on reuse (`test_single_use_capability_cannot_be_reused`); note: gateway authorize path does not call `mark_used` — caller-side tracking required (Weak Spot 3, 19-v1-single-node-support-contract.md §4.2)
- [x] R3 no auto-commit test — VERIFIED (service-level): R3 contracts have auto_commit=false, R0 have auto_commit=true (`test_r3_contracts_have_auto_commit_false`); note: gateway prepare hardcodes R0 at `server.rs:422-427` — R3 semantics enforced at service layer; caller responsible for correct rollback_class at intent creation (Weak Spot 1, 19-v1-single-node-support-contract.md §4.1)
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
