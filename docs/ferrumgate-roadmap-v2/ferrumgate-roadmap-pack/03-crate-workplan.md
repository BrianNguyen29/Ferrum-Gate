# 03 — Crate workplan

> **⚠️ Historical / Planning-draft**: This document is a planning-era work breakdown. Many unchecked Q1/Q2 boxes are stale — work may have since been completed, changed scope, or been deferred. Do not treat unchecked boxes as authoritative pending work.
>
> **For current state**: See `docs/implementation-path/01-current-state.md` and `docs/implementation-path/11-remaining-tasks.md`.
>
> **Canonical Phase completion** (per `01-current-state.md`): Phase A/B/C/E/F all DONE; Phase D is PARTIAL (verified local adapter slices exist).

## Execution Pack — Q1–Q2 crate sequencing

> **Status note (2026-04-09):** Q1 exit gate is **SATISFIED** per `08-q1-p7-invariant-matrix-pass-evidence.md`.
> Q2 entry gate is satisfied. The Q1 boxes below are planning-era unchecked items; they
> reflect pre-Q1-P7 planning scope, not authoritative pending-work status. Do not treat
> unchecked Q1 boxes as implying Q1 is still open.
>
> **Superseded by P6/P7 (2026-04-28)**: This document predates P6/P7 validation. Some adapter descriptions (e.g., "skeleton/no real implementation") are now stale — verified local slices exist with test coverage.

This document (`03`) is the crate-level work breakdown for the execution pack.

### Cross-crate dependency gates (Q1)
| Gate | From | To | Blocked if |
|---|---|---|---|
| G1 | `ferrum-proto` shape lock | All other crates | Proto field names still unstable |
| G2 | `ferrum-store` state transitions | `ferrum-cap`, `ferrum-rollback` | Store trait changes not settled |
| G3 | `ferrum-pdp` hard rules settled | `ferrum-cap` mark_used closure | PDP decision branches still changing |

### Cross-crate dependency gates (Q2)
| Gate | From | To | Blocked if |
|---|---|---|---|
| G4 | `ferrum-store` adapter artifacts | `ferrum-adapter-fs/git/sqlite` integration | Store does not persist adapter artifacts |
| G5 | All three adapters real-implemented | `ferrum-gateway` orchestration integration | Adapter contract not stable |
| G6 | `ferrum-gateway` orchestration | `ferrum-pdp` policy pack work | Gateway API for adapters not finalized |

### Evidence expectations per crate
For each crate, record evidence that "done when" criteria are met:
- Test output showing the behavior
- Or a code reference (file:line) pointing to the implementing code
- Or a note in `docs/artifacts/<date>/` if the criterion is risk-accepted

## Global crate rules

- `ferrum-proto` ở tầng thấp nhất
- adapters không phụ thuộc gateway
- gateway là orchestration layer trên cùng
- store không phụ thuộc gateway
- mọi thay đổi object shape phải cập nhật code + docs + contracts + schemas + openapi

### V1 boundary rule

Adapter crate shapes (e.g., `ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-sqlite`,
`ferrum-adapter-http`, `ferrum-adapter-maildraft`) may exist in the repo as skeleton or
partial implementations. Their existence does **not** mean they are supported in v1.
The v1 support contract (`19-v1-single-node-support-contract.md`) is the authoritative
scope for adapters: all real adapter implementations are explicitly out of v1 scope.

---

## `ferrum-proto`

### Q1
- [ ] sync shapes với JSON schemas và contracts
- [ ] thêm validation helpers cho domain objects
- [ ] chốt naming ổn định cho intent/proposal/capability/rollback/provenance/approval
- [ ] thêm digest/hash stability helpers nếu cần

> **Q1 gate**: This crate must complete shape lock before G1 is satisfied.
> All downstream crate work (ferrum-store, ferrum-pdp, ferrum-cap) depends on stable proto shapes.

### Q2
- [ ] mở rộng types cho fs/git/db adapter payloads nếu cần
- [ ] thêm richer action type enums cho engineering workflows

### Q3
- [ ] versioning strategy cho production payload evolution

### Q4
- [ ] tool governance / MCP mapping payloads
- [ ] evidence export payload shapes

### Done when
- shapes ổn định
- test fixture không bị gãy do rename field liên tục
- schema diff có kiểm soát

---

## `ferrum-store`

### Q1
- [ ] audit lại repo traits
- [ ] explicit state transitions cho executions/capabilities/approvals
- [ ] ensure provenance append-only semantics
- [ ] close gaps cho single-use capability persistence path

> **Q1 gate**: G2 — Store state transitions must be settled before ferrum-cap mark_used
> and ferrum-rollback state machine work can proceed.

### Q2
- [x] persist adapter-specific artifacts cần thiết cho fs/git/db verify and restore
  - **Partial status (2026-04-11):** fs-first FileWrite artifact persistence confirmed (prepare → persist → compensate/restore contract); tests in `integration_fs_roundtrip.rs` and `rollback.rs` pass; gateway-level execute/verify HTTP surfaces exist for fs-first FileWrite slice per `11-gateway-execute-verify-surface-design-note.md` (server.rs:155–162); git and sqlite adapters are not yet implemented
- [ ] query tối ưu cho execution detail và lineage detail

> **Q2 gate**: G4 — Store must support adapter artifact persistence before adapter
> crates can integrate with real storage.

### Q3
- [ ] Postgres implementation
- [ ] migration strategy sqlite -> postgres (nếu cần)
- [ ] backup/restore docs và tool helpers

### Q4
- [ ] storage support cho tool governance events / richer provenance queries
- [ ] ledger persistence enhancements

### Done when
- IDs ổn định
- lineage không rewrite
- query theo execution_id / intent_id / capability_id usable và đủ nhanh

---

## `ferrum-pdp`

### Q1
- [ ] rà toàn bộ hard rules cho scope/taint/R3/draft-only
- [ ] expose explainable decision structure ổn định
- [ ] tests cho deny/quarantine/approval/draft-only branches

> **Q1 gate**: G3 — PDP hard rules must be stable before ferrum-cap mark_used
> closure can be verified against stable decision branches.

### Q2
- [ ] policy packs cho engineering workflows
- [ ] protected path / branch / destructive SQL / admin-like mutation rules

### Q3
- [ ] policy bundle loading / environment-specific bundle selection
- [ ] UI-facing explainability payload

### Q4
- [ ] tool governance policies cho MCP/open runtime
- [ ] policy packs cho enterprise evidence scenarios

### Done when
- decision path deterministic
- operator đọc được “why” của decision
- policy changes có test coverage rõ

---

## `ferrum-cap`

### Q1
- [ ] TTL enforcement end-to-end
- [ ] single-use enforcement end-to-end
- [ ] resource scope subset validation
- [ ] approval digest binding validation
- [ ] revoke path usable via API

### Q2
- [ ] path/ref/table scoped bindings cho fs/git/db
- [ ] arg constraint validation chi tiết hơn cho engineering workflows

### Q3
- [ ] operator UI support cho inspect/revoke

### Q4
- [ ] tool-scoped capability mapping cho MCP/open runtime

### Done when
- capability không thể reuse
- scope vượt intent bị chặn chắc
- approval-bound action không thể replay với digest lệch

---

## `ferrum-firewall`

### Q1
- [ ] trust labeling ổn định
- [ ] taint scoring calibration
- [ ] contradiction checks có regression tests
- [ ] sanitize output and DLP path usable

### Q2
- [ ] repo/file/db oriented taint heuristics
- [ ] suspicious diff / output lineage checks nếu cần

### Q3
- [ ] UI/ops payload cho trust/taint explanation

### Q4
- [ ] tool output trust propagation qua runtime/MCP
- [ ] quarantine investigation metadata

### Done when
- taint decision không “mờ”
- quarantine có lý do rõ
- output sanitize không làm lộ control data

---

## `ferrum-rollback`

### Q1
- [ ] close lifecycle/state machine gaps
- [ ] fix rollback_class propagation at prepare
- [ ] verify/compensate/rollback transitions nhất quán
- [ ] tests cho R0/R1/R2/R3

### Q2
- [ ] adapter registry thật cho fs/git/sqlite
- [ ] verify orchestration usable
- [ ] compensation plan shape cho db and git actions

### Q3
- [ ] richer status/explanation cho operator plane

### Q4
- [ ] tool/runtime-level recovery contracts
- [ ] evidence hooks for rollback/compensate traces

### Done when
- recovery semantics không còn chỉ là placeholder
- operator thấy rõ execution đang ở state nào và vì sao

---

## `ferrum-adapter-fs`

### Q2 priority 1
- [x] backup before mutate — captured via snapshot in prepare phase; `snapshot_path` persisted in rollback contract
- [x] hash verify — `verify_checks` (FileHashMatches) stored in contract and exercised post-retrieval; integration tests confirm round-trip
- [x] restore path — `compensate()` invokes FsAdapter rollback; `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035) confirms HTTP-level compensate restores file
- [ ] path-scoped target model
- [ ] meaningful error model
- [ ] integration tests

> **Q2 partial status (2026-04-11):** backup/hash/restore at the adapter+store layer is confirmed via integration tests (`integration_fs_roundtrip.rs`, `integration_gateway_flow.rs`). Gateway-level HTTP execute and verify endpoints exist for fs-first FileWrite slice per `11-gateway-execute-verify-surface-design-note.md` (server.rs:155–162). Full adapter checklist items (path-scoped target model, error model) remain open. Git and sqlite adapters are not yet implemented.

### Q3
- [ ] production hardening
- [ ] artifact retention strategy

### Done when
- file mutation có thể prepare/execute/verify/restore thật

> **V1 boundary**: `ferrum-adapter-fs` is listed as explicitly unsupported in the v1
> support contract. All work on this adapter is post-v1 scope.

---

## `ferrum-adapter-git`

### Q2 priority 1
- [ ] before_ref / after_ref capture
- [ ] revert/reset path
- [ ] protected branch behavior
- [ ] verify diff/ref movement
- [ ] integration tests

### Q3
- [ ] production hardening for remote/local repo assumptions

### Done when
- git mutation có recovery path rõ và lineage đủ

> **V1 boundary**: `ferrum-adapter-git` is listed as explicitly unsupported in the v1
> support contract. All work on this adapter is post-v1 scope.

---

## `ferrum-adapter-sqlite` (và về sau Postgres)

### Q2 priority 1
- [ ] transaction wrapper
- [ ] savepoint strategy nếu phù hợp
- [ ] verify predicate / row count
- [ ] rollback transaction
- [ ] integration tests

### Q3
- [ ] postgres adapter or generalized db adapter path
- [ ] migration/compat notes

### Done when
- DB mutation có verify và rollback thật trong supported scope

> **V1 boundary**: `ferrum-adapter-sqlite` is listed as explicitly unsupported in the
> v1 support contract. "Postgres support" is not in the v1 support contract and is
> explicitly Q3 post-v1 scope. Compensate may be noop-backed in v1.

---

## `ferrum-adapter-http`

### Q2 optional / Q3-Q4 target
- [ ] endpoint allowlist
- [ ] method/path binding
- [ ] idempotency-aware semantics
- [ ] destructive mutation -> R3 by default
- [ ] verify hooks where feasible

### Done when
- controlled internal HTTP mutation có governance, không claim generic undo

---

## `ferrum-adapter-maildraft`

### Q2 or Q3 optional
- [ ] create/delete draft
- [ ] no-send hard rule
- [ ] draft-only semantics

### Done when
- external communication path có demo draft-only kiểm soát được

---

## `ferrum-graph`

### Q1
- [ ] lineage query helpers ổn định

### Q2
- [ ] execution detail graph helpers

### Q3
- [ ] UI graph data APIs

### Q4
- [ ] multi-hop investigation helpers cho runtime/MCP

### Done when
- lineage query usable cho operator, không chỉ cho tests

---

## `ferrum-ledger`

### Q3
- [ ] append-only audit trail usable in product plane

### Q4
- [ ] optional hash chain / tamper-evident alpha
- [ ] evidence export hooks

### Done when
- audit trail có thể export và kiểm tra tính toàn vẹn ở mức alpha

---

## `ferrum-gateway`

### Q1
- [ ] wire invariant-safe happy path
- [ ] sanitize outputs consistently
- [ ] emit complete provenance chain

### Q2
- [ ] integrate real fs/git/db adapters
- [ ] engineering workflows end-to-end

> **Q2 gate**: G5 — Gateway integration depends on all three adapters having
> real implementations. Adapter contracts must be stable before wiring.

### Q3
- [ ] productized auth/ops integration

### Q4
- [ ] MCP/open runtime wrapper mode
- [ ] tool governance interception

### Done when
- gateway là nơi duy nhất hợp lệ cho mutation flow

---

## `ferrumctl`

### Q1
- [ ] debug/inspect commands audit lại
- [ ] validate helpful diagnostics

### Q2
- [ ] engineering workflow inspect helpers

### Q3
- [ ] support self-hosted ops diagnostics
- [ ] evidence export helper nếu phù hợp

### Q4
- [ ] runtime/MCP inspect helpers

### Done when
- CLI đủ mạnh cho operator/engineer debug trước khi UI hoàn thiện

---

## `ferrum-testkit / tests`

### Q1
- [ ] invariant closure suite
- [ ] adversarial/bypass attempts
- [ ] full lineage chain test

### Q2
- [ ] real adapter integration fixtures
- [ ] rollback/restore tests for fs/db/git

### Q3
- [ ] deployment-level smoke tests
- [ ] postgres tests
- [ ] UI/API integration smoke tests

### Q4
- [ ] runtime/MCP governance tests
- [ ] evidence/tamper tests

### Done when
- mutation tests luôn assert recovery path
- gateway tests luôn assert decision + provenance
- lineage tests luôn assert minimum chain
