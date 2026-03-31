# FerrumGate Unified Project

FerrumGate la mot **intent-scoped reversible execution plane** cho MCP agents.

Repository nay la **khung du an thong nhat**, gom day du:
- code scaffold Rust (crates + binaries)
- docs nghiep vu va kien truc
- machine-readable contracts
- OpenAPI
- JSON Schemas
- prompts cho AI agents
- examples
- configs
- scripts
- test skeleton

Muc tieu la de **AI agents hoac engineers khac co the bam vao va hoan thien tiep**, thay vi phai ghep cac bo roi rac.

## Bat dau tu dau

1. `docs/00-repo-map.md`
2. `docs/01-business-overview.md`
3. `docs/02-runtime-flow.md`
4. `docs/06-constraints-and-invariants.md`
5. `docs/12-agent-handoff.md`
6. `contracts/ferrumgate-agent-contract.v1.yaml`
7. `prompts/agent_system.md`

## Trang thai hien tai

FerrumGate dang o **single-node v1 RC candidate**.
Phan core cua workspace da compile, gateway orchestration da co, SQLite persistence da hoat dong, integration tests da pass.

### RC gates as of 2026-03-29

- **P0/P1/P2 closed** for single-node v1 RC.
- **Supported in v1 contract** = single-node SQLite governance core only. See [19-v1-single-node-support-contract.md](./docs/19-v1-single-node-support-contract.md).
- **Partial** = adapter crates and limited `ferrumctl` inspect surface.
- **Post-v1** = real adapters, multi-node/HA/read-replica, U1-U4.

### Nhung phan da co

- `ferrum-proto`, `ferrum-pdp`, `ferrum-cap`, `ferrum-rollback`, `ferrum-store`, `ferrum-firewall`, `ferrum-graph`, `ferrum-ledger`
- `ferrum-gateway` voi full orchestration: evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate (commit available but rarely needed in single-node; compensate/rollback are primary recovery paths)
- SQLite-backed persistence cho intents, proposals, capabilities, executions, rollback contracts, provenance, approvals
- Trust labeler, taint scorer, contradiction checks
- CLI (`ferrumctl`) voi: health, inspect-capability, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance, inspect-lineage-query, watch-execution, watch-approvals, resolve-approval, revoke-capability, cancel-execution, pause-execution, resume-execution, prepare-execution, execute-execution, compensate-execution, rollback-execution
- Integration tests cho: capability single-use, R3 no-auto-commit, rollback/compensate distinct ops, taint-based quarantine, compensate end-to-end, pending-approvals pagination/filter, lineage endpoint
- CI pipeline voi cargo check, repo layout validation, contract consistency

### Nhung phan nam ngoai v1 scope (post-v1)

- real adapter implementations (fs, sqlite, maildraft, git, http)
- multi-node / HA / read-replica
- Outcome-aware Governance (U1)
- Reversible Execution Planner (U2)
- Cross-runtime Provenance Fabric (U3)
- Runtime Integrations MCP/local/NemoClaw (U4)
- ledger hash chain

## Diem vao cho agent khac

- `docs/implementation-path/README.md`
- `docs/implementation-path/00-start-here.md`
- `docs/implementation-path/01-current-state.md`
- `docs/implementation-path/11-remaining-tasks.md`
