# 01 — Current state

Last updated: 2026-03-29
Single-node v1 scope unless noted.

**Release support contract**:
- Supported = SQLite-backed single-node governance core.
- Partial = adapter crates and extended runtime integrations (skeleton only, not production-verified).
- Deferred/post-v1 = real adapter implementations, multi-node/HA/read-replica, U1-U4 upgrade tracks.

## What exists

### Core crates
- `ferrum-proto` — domain shapes, proto definitions
- `ferrum-pdp` — Policy Decision Point (StaticPdpEngine; trust labels, taint scoring, contradiction checks)
- `ferrum-cap` — capability mint, mark_used, single-use enforcement
- `ferrum-rollback` — rollback/compensate operations, R3/R2/R0/R1 contract classes, auto_commit semantics
- `ferrum-store` — SQLite persistence (intents, proposals, capabilities, executions, rollback contracts, provenance events, approvals)
- `ferrum-gateway` — full orchestration: evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate (internal: commit/rollback as orchestration semantics); negative paths: deny, quarantine, RequireApproval, draft-only gated at evaluate (before prepare)
- `ferrum-firewall` — trust labeler, taint scorer, sanitize, contradiction checks
- `ferrum-graph` — provenance graph
- `ferrum-ledger` — ledger (skeleton)
- `ferrum-sync` — sync probe (skeleton, infrastructure only)
- `ferrum-testkit` — test helpers

### Adapters
- `ferrum-adapter-fs` — filesystem adapter (local file write/delete with hash-based verify semantics; durability/hardening still limited)
- `ferrum-adapter-sqlite` — SQLite adapter (single-row and atomic multi-row rollback/compensate path for bounded local table/row mutations)
- `ferrum-adapter-maildraft` — maildraft adapter (SQLite-backed draft persistence; verify semantics implemented; send semantics explicitly out of scope)
- `ferrum-adapter-git` — git adapter (local HEAD capture/reset and branch-create rollback path; remote workflows explicitly out of scope)
- `ferrum-adapter-http` — HTTP adapter (skeleton, no real implementation)

### Binaries
- `ferrumd` — server binary
- `ferrumctl` — operator CLI (health; inspect-capability/execution/approvals/approval/lineage/provenance; watch-execution/watch-approvals; resolve-approval; revoke-capability; cancel/pause/resume/prepare/execute/compensate/rollback execution)

### Integrations
- `ferrum-integration-tests` — integration tests covering: capability single-use, R3 no-auto-commit, rollback/compensate distinct ops, taint-based quarantine, compensate end-to-end flow, pending-approvals pagination/filter, lineage endpoint shape/validation

### Infrastructure
- `.github/workflows/ci.yml` — cargo check, repo layout validation, contract consistency check
- `scripts/check_contract_consistency.py` — validates contracts against schemas
- `scripts/validate_repo_layout.sh` — validates directory structure

## What is missing

### P0 — v1 RC blockers
- (none) — scope-mismatch deny implemented in `crates/ferrum-pdp/src/engine.rs` lines 31-46

### P1 — v1 RC evidence gaps
- (none) — poisoned-context regression fixtures implemented (6 fixture tests)
- (none) — Phase F docs pack finalized in `docs/implementation-path/`
- (none) — supported flows list documented in `25-v1-single-node-rc-evidence.md`
- (none) — open gaps list documented in `11-remaining-tasks.md`

### P2 — v1 polish
- (none) — all verified: 128 tests pass, clippy passes, `scripts/generate_rc_evidence.py` exists and passes

## Phase status summary

- **Phase A** (compile/shape stability): DONE
- **Phase B** (SQLite storage boundary): DONE
- **Phase C** (firewall MVP): DONE — logic exists, curated regression fixtures implemented (6 tests)
- **Phase D** (adapters): PARTIAL — skeleton adapters exist for fs/sqlite/maildraft/git/http; real implementations are post-v1
- **Phase E** (gateway orchestration): DONE for SQLite-backed single-node flow
- **Phase F** (hardening/evidence): DONE — integration tests strong, poisoned-context fixtures curated, supported flows and gaps documented, evidence script present

## Next step

All P0/P1/P2 items closed. v1 RC is unblocked for single-node SQLite-backed deployment. Remaining work is post-v1 backlog (multi-node/HA, real adapters, U1-U4 upgrade tracks).
