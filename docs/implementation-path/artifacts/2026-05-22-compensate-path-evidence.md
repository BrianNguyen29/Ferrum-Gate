# 2026-05-22 — Compensate Path Evidence

> **Status**: Evidence compilation. No production-ready claim. No full G2 closure claim.
> **Scope**: Consolidated compensate/rollback path evidence for G2.8, compiled from existing implementation, test, and drill artifacts.
> **Repository**: `/home/uong_guyen/work/ferrum-gate/Ferrum-Gate-verify`
> **Environment**: Local evidence only; no live production infra commands; no secrets.
> **Constraint**: This artifact is documentation-only. It does NOT upgrade any gate status to COMPLETE. G2.8 remains signed for conditional single-node SQLite pilot scope only.

---

## 1. Scope and Non-Claims

### What This Artifact Covers
- Gateway compensate handler implementation and state guards
- R3 auto-commit suppression at verify time
- Cancel/rollback provenance emission
- Integration test matrix for compensate flows
- Adapter compensation behavior matrix (fs, git, http, sqlite, maildraft)
- D1–D6 local drill evidence (2026-05-18)
- MCP client compensate wiring
- G2.8 conditional signoff context

### What This Artifact Does NOT Claim
| Non-Claim | Reason |
|---|---|
| Production-ready | FerrumGate v1 is RC-ready/conditional only |
| Full G2 closure | G2.1–G2.8 signed for conditional pilot scope only (09/05/2026) |
| Block A closed | Block A remains WAIVED/CONDITIONAL; real owned domain still required |
| PostgreSQL production | PG evidence is local Docker fallback only |
| HA/multi-node | Not implemented; planning artifacts only |
| Uniform compensation guarantee | Adapter compensation is non-uniform per `56-adapter-compensation-evidence-matrix.md` |
| Operator drill substitution | Local evidence does not replace operator-executed target-host drills |

---

## 2. Compensate Handler and State Machine

### 2.1 Handler Location
`crates/ferrum-gateway/src/server.rs`, function `compensate_execution` (lines ~3806–4011).

### 2.2 State Guard (WS-Compensate)
Lines ~3886–3909 enforce that compensation is allowed **only** when:
- `contract.state == ExecutedAwaitingVerify` AND
- `execution.state == Running` OR `AwaitingVerification`

Any other state combination returns HTTP 409 Conflict with:
```
compensate not allowed in current state: contract={:?}, execution={:?}
```

### 2.3 Handler Flow
1. Parse and validate `execution_id`
2. Look up execution record; 404 if missing
3. Extract `rollback_contract_id`; 404 if absent
4. Look up rollback contract; 404 if missing
5. **State guard** (see 2.2)
6. Enrich HTTP placeholder compensation plans (`enrich_http_compensation_if_needed`)
7. Call `state.runtime.rollback.compensate(&contract).await`
8. Update contract state → `Compensated`
9. Update execution state → `Compensated`
10. Emit `SideEffectCompensated` provenance event
11. Return `CompensateExecutionResponse { compensated: true, ... }`

### 2.4 R3 Auto-Commit Suppression
`crates/ferrum-gateway/src/server.rs`, lines ~3660–3675.

R3 (irreversible-high-consequence) executions set `auto_commit=false` at prepare time. The verify handler respects this:
- If `verified=true` and `auto_commit=true` → execution becomes `Committed`
- If `verified=true` and `auto_commit=false` → execution stays `Running`/`AwaitingVerification`
- `SideEffectCommitted` provenance event is **suppressed** when `auto_commit=false`

This preserves the rollback/compensate window for R3-class executions. Explicit commit is required to make them irreversible.

### 2.5 Cancel → Rollback Provenance
`crates/ferrum-gateway/src/server.rs`, function `cancel_execution` (lines ~4017–4169), lines ~4113–4133 emit `SideEffectRolledBack`.

Cancel is allowed for non-terminal states (`Proposed`, `Authorized`, `Prepared`, `Running`, `AwaitingApproval`, `AwaitingVerification`). Terminal states (`Verified`, `Committed`, `Compensated`, `RolledBack`, `Failed`, `Expired`, `Denied`, `Quarantined`) return 409.

Even when no rollback contract exists, cancel emits a `SideEffectRolledBack` provenance event to preserve lineage completeness.

---

## 3. Provenance Chain

### 3.1 Terminal Events
| Event | Emitted By | Condition |
|---|---|---|
| `SideEffectCompensated` | `compensate_execution` | Compensation succeeds and states updated |
| `SideEffectRolledBack` | `cancel_execution` | Cancel succeeds (regardless of contract presence) |
| `SideEffectCommitted` | `verify_execution` | `verified=true` AND `auto_commit=true` |

### 3.2 Lineage Chain Minimum
Per `crates/ferrum-proto/src/provenance.rs` and integration tests:
- `PolicyEvaluated` → `CapabilityMinted` → `ActionProposalSubmitted` → `SideEffectPrepared` → `ToolCallPrepared` → `ToolCallExecuted` → `SideEffectVerified` → Terminal (`SideEffectCommitted` | `SideEffectCompensated` | `SideEffectRolledBack`)

---

## 4. Integration Test Matrix

### 4.1 Gateway-Level Compensate Tests
`crates/ferrum-integration-tests/src/integration_gateway_flow.rs`

| Test | Line | What It Validates |
|---|---|---|
| `compensate_execution_flow` | ~829 | Full lifecycle through compensate using `NoopRollbackAdapter` |
| `test_inspect_after_compensate_execution_flow` | ~1886 | Inspect → execute → compensate → verify file restored |
| `test_compensate_endpoint_restores_file_via_fs_adapter` | ~7658 | HTTP-level POST /v1/executions/{id}/compensate restores file |
| `test_compensate_before_verify_returns_409_conflict` | ~11008 | State guard: compensate on Prepared contract returns 409 |
| `test_compensate_on_prepared_contract_returns_409_conflict` | (see above) | Duplicate state guard validation |

### 4.2 Lineage Chain Tests
`crates/ferrum-integration-tests/src/integration_lineage_chain.rs`

| Test | What It Validates |
|---|---|
| `test_lineage_chain_minimum_provenance_events` | `SideEffectCompensated` present in compensated flow lineage |
| Partial-flow adversarial lineage test (~397–639) | Adversarial check: partial flow does NOT contain `SideEffectCompensated` or `SideEffectRolledBack` |
| Additional lineage compensate variants (~1011–2820) | Minimum chain with `SideEffectCompensated` terminal event across multiple flow variants |

### 4.3 Test Result Summary (Local)
All above integration tests pass locally as of 2026-05-17 workspace test run (`cargo test --workspace` PASSED).

---

## 5. Adapter Compensation Matrix

Reference: `docs/implementation-path/56-adapter-compensation-evidence-matrix.md`

### 5.1 Summary by Adapter
| Adapter | Classification | Evidence Level |
|---|---|---|
| fs | `alias_to_rollback` + `real_undo` for bounded local actions | High |
| git | `alias_to_rollback`; `real_undo` local; `fail_closed` remote push | Medium-High |
| http | `replay_compensation` for strict `http.replay_v1`; `fail_closed` otherwise | Medium |
| sqlite | `alias_to_rollback` + SQL compensation for bounded mutations | Medium |
| maildraft | `alias_to_rollback` + `real_undo` in in-memory draft store | Medium |

### 5.2 Key Adapter Test Evidence

**Filesystem (`crates/ferrum-adapter-fs/src/lib.rs`)**
- `test_compensate_aliases_rollback` (~3263)
- `test_compensate_restores_deleted_file` (~3357)
- `test_file_move_compensate_aliases_rollback` (~6449)
- `test_file_copy_compensate_aliases_rollback` (~6895)
- `test_file_append_compensate_aliases_rollback` (~7463)
- `test_file_chmod_compensate_aliases_rollback` (~7840)
- `test_dir_create_compensate_aliases_rollback` (~8133)
- `test_dir_delete_compensate_aliases_rollback` (~8338)
- `test_compensate_inherits_fail_closed_behavior` (~8943)

**Git (`crates/ferrum-adapter-git/src/lib.rs`)**
- `test_git_push_rollback_resets_remote_ref` (~5894)
- `test_git_push_rollback_returns_recovered_false_on_remote_deletion_failure` (~5963)

**HTTP (`crates/ferrum-adapter-http/src/lib.rs`)**
- `test_compensate_with_valid_http_replay_v1_succeeds` (~3683)
- `test_compensate_fails_on_status_mismatch` (~4124)
- `test_compensate_fails_on_wrong_operation` (~4173)
- `test_http_put_replay_compensate_succeeds` (~4928)
- `test_http_patch_replay_compensate_succeeds` (~4981)

**SQLite (`crates/ferrum-adapter-sqlite/src/lib.rs`)**
- `test_compensate_calls_rollback` (~917)

**Maildraft (`crates/ferrum-adapter-maildraft/src/lib.rs`)**
- `test_maildraft_compensate_aliases_rollback` (~1368)

---

## 6. D1–D6 Drill Evidence

Reference: `docs/implementation-path/artifacts/2026-05-18-local-confidence-polish-evidence.md` §D1–D6 API Live Lifecycle Local

### 6.1 Drill Results (2026-05-18)
All 6 drills passed the complete 9-step API lifecycle via `python3 scripts/run_d1_d6_drills.py --api-live`:

| Drill | Adapter | Result |
|---|---|---|
| D1 | fs | PASS — compile→proposal→mint→authorize→prepare→execute→compensate→capture |
| D2 | git | PASS — full lifecycle; temp git repos created locally |
| D3 | git remote fail-closed | PASS — full lifecycle |
| D4 | http | PASS — local echo server used |
| D5 | sqlite | PASS — temp sqlite DB created locally |
| D6 | maildraft | PASS — full lifecycle |

### 6.2 Boundary / Non-Claim
- This is **local/test-drill evidence only** (2026-05-18)
- Does NOT complete any G2 gate
- Does NOT replace operator-executed target-host drills
- D4 did not exercise a live external HTTP service
- D3 confirms fail-closed at adapter level; operator must still evaluate target remote branch protection

---

## 7. MCP Client Compensate Wiring

`crates/ferrum-integrations-mcp/src/http_client.rs`
- `compensate_execution()` (line ~708): POST `/v1/executions/{execution_id}/compensate`
- Parses `CompensateExecutionResponse` with `compensated` flag and updated `rollback_contract`

`crates/ferrum-integrations-mcp/src/rest_mapper.rs`
- `ferrum_gate_compensate` tool mapped to `call_compensate` (line ~179)
- `call_compensate` dispatches to `client.compensate_execution()` (line ~743)

MCP lifecycle smoke (2026-05-18): 15/15 checks passed, including `ferrum_gate_compensate` registry presence.

---

## 8. G2.8 Conditional Signoff

### 8.1 Original Signoff
- **Operator**: BrianNguyen
- **Date**: 09/05/2026
- **Scope**: Conditional single-node SQLite pilot only
- **G2.8 phrase**: "Operator accepts compensate noop risk with manual verification procedure for conditional single-node pilot scope."

### 8.2 Evidence Supporting the Signoff
| Evidence | Date | Reference |
|---|---|---|
| Adapter compensation matrix completed | 2026-04-29 | `56-adapter-compensation-evidence-matrix.md` |
| Workload compensation drill plan created | 2026-04-29 | `57-workload-compensation-drill-plan.md` |
| Drill evidence template prefill (local) | 2026-04-29 | `58-workload-compensation-drill-evidence-template.md` |
| D1–D6 local API-live drills passed | 2026-05-18 | `2026-05-18-local-confidence-polish-evidence.md` |
| Integration tests (gateway + lineage) | Ongoing | `integration_gateway_flow.rs`, `integration_lineage_chain.rs` |
| MCP client compensate wiring | Ongoing | `ferrum-integrations-mcp/src/http_client.rs` |

### 8.3 What G2.8 Does NOT Cover
- Full target-host operator drill execution
- Live external HTTP replay compensation validation
- Production remote git push rollback validation
- PostgreSQL adapter compensation (not yet implemented)

---

## 9. Gaps and Deferred Controls

| Gap | Status | Impact |
|---|---|---|
| Uniform compensation across all adapters | Not provided | Operators must evaluate per adapter/action |
| HTTP true undo | Not provided | Replay compensation only; server-side idempotency required |
| Git remote rollback guarantee | Not provided | Remote protections/permissions can block rollback |
| Durable/encrypted fs snapshots | Not provided | Local temp artifacts only |
| Generic database time travel | Not provided | SQLite relies on specific compensation plans |
| PostgreSQL adapter compensation | Not implemented | PG adapter exists for store only; no PG-side-effect adapter |
| Target-host D1–D6 operator drill signoff | Pending | Local evidence only; operator must rerun on target env |

---

## 10. Current Status

| Item | Status |
|---|---|
| Compensate handler implemented with state guards | **YES** — `server.rs:3806-4011` |
| R3 auto-commit suppression | **YES** — `server.rs:3660-3675` |
| Cancel → rollback provenance | **YES** — `server.rs:4113-4133` |
| Integration tests pass locally | **YES** — `cargo test --workspace` PASSED (2026-05-17) |
| Adapter compensation matrix documented | **YES** — `56-adapter-compensation-evidence-matrix.md` |
| D1–D6 local drills passed | **YES** — 2026-05-18 |
| MCP client compensate wired | **YES** — `http_client.rs:708` |
| G2.8 operator signoff | **CONDITIONAL** — 09/05/2026; single-node SQLite pilot only |
| Production-ready claim | **NO** |
| Full G2 closure | **NOT COMPLETE** |
| Block A | **WAIVED/CONDITIONAL** |

---

## Cross-References

| Document | Purpose |
|---|---|
| `docs/implementation-path/56-adapter-compensation-evidence-matrix.md` | Per-adapter compensation classification |
| `docs/implementation-path/57-workload-compensation-drill-plan.md` | Operator drill procedures |
| `docs/implementation-path/58-workload-compensation-drill-evidence-template.md` | Drill evidence template |
| `docs/implementation-path/59-pilot-readiness-evidence-packet.md` | G2.1–G2.8 evidence packet |
| `docs/implementation-path/artifacts/2026-05-18-local-confidence-polish-evidence.md` | D1–D6 local drill results |
| `docs/implementation-path/artifacts/TEMPLATE-full-g2-resignoff.md` | Full G2 re-signoff template |
| `docs/production-readiness-v2/10-evidence-checklist.md` | Phase evidence checklist |

---

*Artifact generated: 2026-05-22. Consolidated from existing implementation/test/drill evidence. No production-ready claim.*
