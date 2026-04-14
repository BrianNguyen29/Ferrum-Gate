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

1. `docs/README.md` — doc index and reading order
2. `docs/00-project-canon.md` — project definition and intent
3. `docs/02-project-overview.md` — business overview
4. `docs/04-runtime-flow.md` — runtime flow
5. `docs/06-constraints-and-invariants.md` — constraints and invariants
6. `docs/implementation-path/07-agent-handoff-prompt.md` — agent handoff
7. `contracts/ferrumgate-agent-contract.v1.yaml`
8. `prompts/agent_system.md`

## Current status

FerrumGate is at **v2 RATIFIED** (2026-04-12) for single-node production. v1 RC gates passed 2026-04-02; v2 ratification completed 2026-04-12 per `44-v2-production-execution-plan.md`.

### H1 delivery (post-v2 ratification)

Ten H1 sub-slices shipped since v2 ratification:
- **H1.1a** — policy bundle persistence API + `PolicyBundleRepo` storage + `ferrumctl` surface
- **H1.1b** — policy bundle metadata update/delete (`PUT/DELETE /v1/policy-bundles/{id}`) + created_at preservation
- **H1.1c** — policy bundle lineage via `supersedes_bundle_id` + successor listing
- **H1.1d** — policy bundle authoring CLI for registration payloads (`ferrumctl author request generate|validate|bump`) — distinct from H1.2b rules-format authoring
- **H1.2b** — policy bundle authoring CLI for rules-format YAML (`ferrumctl author intent|bundle generate|validate`)
- **H1.3a** — persistent named-remote configuration (`GitRemoteStore`)
- **H1.3b** — git-native credential delegation (HTTPS username/password, SSH private key); no in-process secret storage
- **H1.4b** — `ferrumctl store backup`/`restore` for SQLite automation
- **H1.4c** — streaming/chunked query patterns for larger-than-memory datasets
- **H1.5a** — retry/backoff with idempotency key management for HTTP mutations

Remaining H1.3c, H1.4a, H1.4d–H1.4e, H1.5b–H1.5c are planned. Full detail in `docs/implementation-path/50-post-v2-roadmap.md`.

### Support contract (v2)

- **Supported** = SQLite-backed single-node governance core.
- **Partial** = bounded local adapter implementations (fs, sqlite, maildraft, git, http) plus early H1 slices.
- **Deferred/post-v1** = broader adapter hardening, multi-node/HA, U1 expressiveness backlog, U2/U3/U4 upgrade-track work.

### RC gates as of 2026-04-02

- **P0/P1/P2 closed** for single-node v1 RC.
- **Supported in v1 contract** = single-node SQLite governance core only. See [19-v1-single-node-support-contract.md](./docs/19-v1-single-node-support-contract.md).
- **Partial** = bounded local adapter implementations (fs, sqlite, maildraft, git, http) plus limited `ferrumctl` inspect surface; broader production hardening is post-v1 backlog.
- **Post-v1** = broader adapter hardening, multi-node/HA/read-replica, remaining U1 authoring/expressiveness work, and U2-U4.

### Nhung phan da co

- `ferrum-proto`, `ferrum-pdp`, `ferrum-cap`, `ferrum-rollback`, `ferrum-store`, `ferrum-firewall`, `ferrum-graph`, `ferrum-ledger`
- `ferrum-gateway` voi full orchestration: evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate (commit available but rarely needed in single-node; compensate/rollback are primary recovery paths)
- SQLite-backed persistence cho intents, proposals, capabilities, executions, rollback contracts, provenance, approvals
- Trust labeler, taint scorer, contradiction checks
- CLI (`ferrumctl`) voi: health, inspect-capability, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance, inspect-lineage-query, watch-execution, watch-approvals, resolve-approval, revoke-capability, cancel-execution, pause-execution, resume-execution, prepare-execution, execute-execution, compensate-execution, rollback-execution
- Integration tests cho: capability single-use, R3 no-auto-commit, rollback/compensate distinct ops, taint-based quarantine, compensate end-to-end, pending-approvals pagination/filter, lineage endpoint
- CI pipeline voi cargo check, repo layout validation, contract consistency

### Nhung phan nam ngoai v1 scope (post-v1)

- broader adapter hardening and production verification for fs/sqlite/maildraft/git/http
- multi-node / HA / read-replica
- richer Outcome-aware Governance expressiveness and operator/migration tooling
- Reversible Execution Planner (U2)
- Cross-runtime Provenance Fabric (U3)
- Runtime Integrations MCP/local/NemoClaw (U4)
- ledger hash chain

## Diem vao cho agent khac

- `docs/implementation-path/README.md`
- `docs/implementation-path/00-start-here.md`
- `docs/implementation-path/01-current-state.md`
- `docs/implementation-path/11-remaining-tasks.md`
