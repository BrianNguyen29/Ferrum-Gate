# 05 — Adapter roadmap

## Execution Pack — Q2 adapter sequencing

This document (`05`) tracks adapter strategy and priority for the execution pack.

### Q2 adapter sequencing
| Step | Adapter | Action | Dependency |
|---|---|---|---|
| 2.3 | `ferrum-adapter-fs` | backup / hash verify / restore path | Store adapter artifacts (G4 in `03`) |
| 2.4 | `ferrum-adapter-git` | before_ref / after_ref / revert path | Store adapter artifacts (G4 in `03`) |
| 2.5 | `ferrum-adapter-sqlite` | transaction wrapper / verify predicate / rollback | Store adapter artifacts (G4 in `03`) |
| 2.6 | All three | Gateway orchestration integration | Adapter implementations stable (G5 in `03`) |

### Q2 adapter gates
| Gate | Criteria |
|---|---|
| G4 (in `03`) | Store must persist adapter artifacts before real adapter work begins |
| G5 (in `03`) | All three adapters must have real implementations before gateway integration |

### Evidence expectations per adapter
For each adapter, record in `docs/artifacts/<date>/`:
- Test output for prepare/execute/verify/compensate path
- Or code reference (file:line) pointing to the implementing code
- If noop/mock: explicitly note which methods are noop/mock

---

## Adapter strategy

Adapter work phải đi từ nơi recovery semantics rõ và giá trị thương mại cao:

1. Filesystem
2. Git
3. SQLite / PostgreSQL
4. Controlled HTTP mutation
5. Maildraft

Không triển khai tất cả cùng lúc.

> **V1 boundary**: All adapters listed in this document are explicitly **out of v1 scope**
> per the v1 support contract (`19-v1-single-node-support-contract.md`). The adapter
> crates (ferrum-adapter-fs, ferrum-adapter-git, ferrum-adapter-sqlite, ferrum-adapter-http,
> ferrum-adapter-maildraft) may exist in the repo as skeleton or partial implementations
> but are not covered by the v1 support contract. The mere existence of adapter code
> in the repo does not expand v1 scope. Compensate in v1 may be noop-backed.

---

## Adapter 1 — Filesystem

### Use cases
- bounded file patch
- config file mutation
- write in approved workspace path
- deny write outside scope

### Required contract
- prepare
- execute
- verify
- compensate/rollback

### Checklist
- [x] capture pre-mutate backup
- [x] hash snapshot before/after — `verify_checks` (FileHashMatches) stored in contract and exercised post-retrieval
- [x] write/rename/delete operations with explicit target model — partial; fs adapter execute path exercised via integration tests
- [x] restore path — `compensate()` invokes FsAdapter rollback; HTTP compensate endpoint confirmed in `test_compensate_endpoint_restores_file_via_fs_adapter` (integration_gateway_flow.rs:3689–4035)
- [ ] path allowlist/scope binding
- [ ] verify using hash and optional diff rules — via compensate path (verify_checks run during compensation)
- [ ] lineage artifacts for backup + verify
- [ ] tests for success, verify fail, restore fail, path deny

> **Q2 partial status (2026-04-11):** fs-first FileWrite demonstrates prepare → persist → execute → verify → compensate/restore
> at the HTTP level (server.rs:155–162) per `11-gateway-execute-verify-surface-design-note.md`.
> Gateway-level HTTP compensate endpoint is live. Remaining checklist items (path allowlist, scope binding, lineage artifacts) are not yet implemented. Git and sqlite adapters are not yet implemented.

### Release target
- Q2 beta

### Notes
This is the first adapter because it demonstrates real side-effect recovery with minimal external dependency.

> **V1 boundary**: `ferrum-adapter-fs` is out of v1 scope. Do not claim fs adapter is
> v1-supported regardless of any implementation code present in the repo.

---

## Adapter 2 — Git

### Use cases
- commit staged changes in allowed repo
- revert a governed change
- reset controlled ref in bounded workflows
- protect branch and path scopes

### Required contract
- prepare snapshot of refs
- execute mutation
- verify ref movement and diff
- compensate via revert/reset path

### Checklist
- [ ] capture before_ref
- [ ] record after_ref
- [ ] support revert path
- [ ] support reset path where policy allows
- [ ] protected branch rules
- [ ] repo/path scoped capability constraints
- [ ] tests for ref mismatch, verify failure, protected branch deny

### Release target
- Q2 beta

### Notes
Git adapter is part of the wedge because engineering teams immediately understand the risk and value.

> **V1 boundary**: `ferrum-adapter-git` is out of v1 scope. Do not claim git adapter is
> v1-supported regardless of any implementation code present in the repo.

---

## Adapter 3 — SQLite / PostgreSQL

### Use cases
- bounded UPDATE/INSERT/DELETE in internal workflows
- migration or admin mutation under approval
- rollback to transaction/savepoint on verify failure

### Required contract
- prepare transaction boundary
- execute statement(s)
- verify predicate/row count
- rollback/compensate

### Checklist
- [ ] transaction wrapper
- [ ] savepoint or equivalent strategy
- [ ] predicate/row-count verification
- [ ] rollback transaction
- [ ] mutation class classification (safe/high-risk/destructive)
- [ ] schema/table scoped capability constraints
- [ ] tests for row mismatch, verify failure, partial failure

### Release target
- SQLite Q2 beta
- PostgreSQL Q3 beta or later

### Notes
Start with sqlite because project already uses it. Generalize only after semantics are proven.

> **V1 boundary**: `ferrum-adapter-sqlite` is out of v1 scope. "Postgres support" is not
> in the v1 support contract. Compensate may be noop-backed in v1 for any database adapter.
> Do not claim db adapter is v1-supported regardless of any implementation code present in
> the repo.

---

## Adapter 4 — HTTP

### Use cases
- internal API mutation under allowlist
- admin-like mutation with approval
- non-idempotent remote operation with explicit risk elevation

### Required contract
- prepare preconditions
- execute request
- verify response and optional follow-up state
- compensate only when real compensating action exists

### Checklist
- [ ] endpoint allowlist
- [ ] method/path/body constraint model
- [ ] destructive mutation -> R3 default
- [ ] idempotency-aware semantics
- [ ] optional verify hook plugin
- [ ] tests for allowlist deny, risk escalation, missing verify path

### Release target
- Q3/Q4 depending on bandwidth

### Notes
Do not overclaim undo for generic HTTP. Only expose compensate where a real compensating call exists.

> **V1 boundary**: `ferrum-adapter-http` is out of v1 scope. Do not claim HTTP adapter
> is v1-supported regardless of any implementation code present in the repo.

---

## Adapter 5 — Maildraft

### Use cases
- create draft only
- delete draft on compensate
- no-send hard rule in v1/v1.x

### Required contract
- prepare
- execute draft create
- verify draft exists
- compensate draft delete

### Checklist
- [ ] create draft path
- [ ] verify draft creation
- [ ] delete draft path
- [ ] ensure no-send hard rule cannot be bypassed
- [ ] tests for draft-only and compensate

### Release target
- Q2/Q3 optional

### Notes
Maildraft is useful for policy demonstration but is not the primary wedge.

> **V1 boundary**: `ferrum-adapter-maildraft` is out of v1 scope. The no-send hard rule
> and draft-only semantics are policy intent, not v1 implementation guarantees. Do not
> claim maildraft is v1-supported regardless of any implementation code present in the repo.

---

## Adapter common requirements

All adapters must satisfy:
- [ ] explicit target model
- [ ] capability binding enforcement
- [ ] prepare/execute/verify/compensate path
- [ ] structured error model
- [ ] provenance emission
- [ ] integration tests
- [ ] no gateway dependency inside adapter crate
