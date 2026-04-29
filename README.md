# FerrumGate Unified Project

> **⚠️ Status (2026-04-29/RC tag):** This README reflects P6/P7 plus M1–M3/S1 feature-completeness validation (~797 current workspace tests, all passing; RC tag evidence dated 2026-04-28). FerrumGate v1 is **RC-ready for single-node SQLite-backed deployment only**. Do not claim production-ready. PostgreSQL/multi-node/HA are not implemented. Default package version: `0.1.0`. Repository: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials). See [19-v1-single-node-support-contract.md](./docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md) for the authoritative v1 boundary.

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

1. `docs/implementation-path/00-start-here.md`
2. `docs/implementation-path/01-current-state.md`
3. `docs/implementation-path/06-guardrails-and-invariants.md`
4. `docs/PRODUCTION_NOTES.md`
5. `contracts/ferrumgate-agent-contract.v1.yaml`
6. `prompts/agent_system.md`

## Trang thai hien tai (P6/P7 — 2026-04-28)

FerrumGate dang o **Phase 3 bottleneck analysis documented, single-node v1 RC-ready**.

### Verification summary (G1 observed PASS — 2026-04-28)

- `cargo check --workspace`: ✅ PASS
- `cargo fmt --all -- --check`: ✅ PASS
- `cargo clippy --workspace --all-targets -- -D warnings`: ✅ PASS
- `cargo test --workspace`: ✅ PASS — **~797 tests passing**
- `scripts/generate_rc_evidence.py`: ✅ PASS — **Overall: ALL PASS**
- Layout/contract validation: ✅ PASS

Full G1 evidence: [`53-rc-tag-checklist.md`](./docs/implementation-path/53-rc-tag-checklist.md) §Latest RC Prep Verification Observed.

### RC gates — P0/P1/P2 closed; Phase 3 analysis documented

- **v1 supported** = single-node SQLite governance core only. See [19-v1-single-node-support-contract.md](./docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md).
- **Phase 1** = Write queue production-ready; **Phase 2** = Deferred/regressed. See [01-current-state.md](./docs/implementation-path/01-current-state.md) Phase status.
- **Phase 3 D5** = Bottleneck analysis report complete. See [51-d5-bottleneck-analysis-report.md](./docs/implementation-path/51-d5-bottleneck-analysis-report.md).
- **Phase 3 D6** = Priority list for extension complete. See [52-d6-priority-expansion-list.md](./docs/implementation-path/52-d6-priority-expansion-list.md).
- **Partial** = adapter crates (verified local slices exist per-adapter), limited `ferrumctl` inspect surface.
- **Post-v1** = real adapters beyond local slices, multi-node/HA/read-replica, PostgreSQL, U1-U4 upgrade tracks.

### Nhung phan da co (key implemented feature domains)

- **Governance core**: `ferrum-proto`, `ferrum-pdp`, `ferrum-cap`, `ferrum-rollback`, `ferrum-store`, `ferrum-firewall`, `ferrum-graph`, `ferrum-ledger`, `ferrum-sync`
- **Gateway orchestration**: full evaluate → mint → authorize → prepare → execute → verify → compensate flow (internal commit/rollback semantics; compensate is the v1 recovery endpoint)
- **SQLite-backed persistence**: intents, proposals, capabilities, executions, rollback contracts, provenance, approvals; write-queue eliminates lock thrash
- **Security enforcement**: trust labeler, taint scorer, contradiction checks, taint-based quarantine
- **CLI (`ferrumctl`)**: health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance, policy bundle CRUD, backup/restore
- **Verified adapter slices** (bounded local scope, post-v1 for broader surface):
  - `ferrum-adapter-fs`: 146 tests — FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod + PlannableFsAdapter
  - `ferrum-adapter-git`: 86 tests — GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete + rollback fail-closed
  - `ferrum-adapter-http`: 103 tests — HttpMutation + http.replay_v1 (POST/PUT/PATCH) + pooling/retry
  - `ferrum-adapter-sqlite`: 16 tests — transaction rollback + G-E1 verify fail-closed
  - `ferrum-adapter-maildraft`: 13 tests — create/update/delete lifecycle
- **U1–U4 upgrade tracks** (implemented, post-v1 scope per support contract): Outcome-aware Governance, Reversible Execution Planner, Cross-runtime Provenance Fabric, MCP/local/NemoClaw integrations
- **Integration tests**: 82 tests — contracts(2) + fs-roundtrip(7) + gateway-flow(65) + lineage-chain(8)
- **CI pipeline**: cargo check, repo layout validation, contract consistency

### Nhung phan nam ngoai v1 scope (post-v1 / production-deferred)

- real adapter implementations beyond verified local slices (permissions/symlink/cross-fs for fs; remote push/pull/submodule for git; broader replay/idempotency for http)
- multi-node / HA / read-replica
- PostgreSQL (Phase 3 path — recommended for production scale)
- Outcome-aware Governance (U1), Reversible Execution Planner (U2), Cross-runtime Provenance Fabric (U3), Runtime Integrations MCP/local/NemoClaw (U4) — all implemented but explicitly out-of-v1-contract
- ledger hash chain (beyond current bounded SHA-256 chain)
- deep health check, full backup scheduling/retention/encryption

## Diem vao cho agent khac

- `docs/implementation-path/README.md`
- `docs/implementation-path/00-start-here.md`
- `docs/implementation-path/01-current-state.md` (current state + phase status + test coverage matrix)
- `docs/implementation-path/32-feature-completeness-audit.md` (route/API reconciliation)
- `docs/implementation-path/33-feature-completion-backlog.md` (incomplete/partial feature backlog)
- `docs/implementation-path/45-current-feature-audit.md` (Phase 1/2/3 analysis)
- `docs/implementation-path/11-remaining-tasks.md` (remaining tasks by priority)
- `docs/PRODUCTION_NOTES.md` (production posture notes)

### Canonical references

- v1 support contract: `docs/ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md`
- Invariant control evidence: `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`
- Feature completeness: `docs/implementation-path/32-feature-completeness-audit.md`
- Feature completion backlog: `docs/implementation-path/33-feature-completion-backlog.md`
- Production evaluation: `docs/implementation-path/27-production-evaluation-plan.md`
- Adapter compensation evidence: `docs/implementation-path/56-adapter-compensation-evidence-matrix.md`
- Workload compensation drill plan: `docs/implementation-path/57-workload-compensation-drill-plan.md`
