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

## H1 delivery (post-v2 ratification)
- [x] H1.1a — policy bundle persistence API + `PolicyBundleRepo` storage + `ferrumctl` surface
- [x] H1.1b — policy bundle metadata update/delete (`PUT/DELETE /v1/policy-bundles/{id}`) + created_at preservation
- [x] H1.1c — policy bundle lineage via `supersedes_bundle_id` + successor listing
- [x] H1.1d — policy bundle authoring CLI for registration payloads (`ferrumctl author request generate|validate|bump`) — distinct from H1.2b rules-format authoring
- [x] H1.2b — policy bundle authoring CLI for rules-format YAML (`ferrumctl author intent|bundle generate|validate`)
- [x] H1.3a — persistent named-remote configuration (`GitRemoteStore`)
- [x] H1.4b — `ferrumctl store backup`/`restore` for SQLite automation
- [x] H1.4c — streaming/chunked query patterns for larger-than-memory datasets
- [x] H1.5a — retry/backoff with idempotency key management for HTTP mutations
- [ ] Remaining H1.2a, H1.3b–H1.3c, H1.4a, H1.4d–H1.4e, H1.5b–H1.5c — ⬜ PLANNED; full detail in `50-post-v2-roadmap.md`

## Broader production-ready sign-off
- [x] G-E1 adapter hardening complete — evidence: `docs/implementation-path/30-production-roadmap.md`
- [x] G-E2 performance baseline captured — evidence: `docs/implementation-path/42-p2-performance-baseline-evidence.md`
- [x] G-E3 ferrumctl advanced operator coverage complete — evidence: `bins/ferrumctl/src/main.rs`, roadmap G-E3 row
- [x] G-E4 sync preflight / decision ratified — evidence: roadmap P5.4/P5.5 + G-E4 row
- [x] G-E5 production evaluation sign-off completed — evidence: `docs/implementation-path/43-production-readiness-signoff.md`
