# 01 — Current state

Last updated: 2026-05-18 — P5a–P5e complete; Path 1 RC tag `v0.1.0-rc.2` cut; Path 2 safe probes executed (shallow/deep/metrics PASS, no G2 completion); Path 3 local workload plan generated + MCP lifecycle smoke passed (15/15); Blocks A/B/C status updated; full workspace gate passed (ALL LOCAL CHECKS PASSED); cargo-deny + cargo-audit installed and `make audit` passing; bearer token rotation executed on VM; secondary alert contact delivery confirmed; SendGrid API key rotation verified on VM with primary+secondary delivery and old-key revocation; MCP D1 local coverage hardened (c661a15, 239 tests); bridge-to-live toolkit and operator unblock packet created
Single-node v1 scope unless noted.

**Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate` (upstream/original — private, accessible with authorized GitHub credentials) | **Default package version**: `0.1.0` | **Status**: P5a–P5e engineering complete within authorized scope; P6 CONDITIONAL GO for operator signoff/pilot; Block C CLOSED; Block B CLOSED; Block A WAIVED/CONDITIONAL; production-ready remains NO; HA/multi-node/PostgreSQL production deployment remain NO; single-node SQLite-backed deployment is the only supported runtime

**Paths execution evidence**: [`artifacts/2026-05-17-all-paths-execution-evidence.md`](./artifacts/2026-05-17-all-paths-execution-evidence.md) — concise evidence for Path 1 (RC tag), Path 2 (safe probes), Path 3 (local plan/smoke), and remaining blockers.

**May 18 local drill evidence**: [`artifacts/2026-05-18-local-extended-drill-evidence.md`](./artifacts/2026-05-18-local-extended-drill-evidence.md) — local extended operational drills (G2.1-local, B3 retention, D1–D6, API lifecycle plan, G3.6 workload plan, pre-target gate, WAL sanity); [`artifacts/2026-05-18-wal-crash-recovery-evidence.md`](./artifacts/2026-05-18-wal-crash-recovery-evidence.md) — structured SQLite WAL crash-recovery drill and script-hygiene fixes; [`artifacts/2026-05-18-local-confidence-polish-evidence.md`](./artifacts/2026-05-18-local-confidence-polish-evidence.md) — D1–D6 API live local 6/6 pass, G3.6 bounded local execute pass, MCP lifecycle smoke 15/15 pass, make wal-drill pass, pre-target gate with WAL integrated.

**Release support contract**:
- Supported = SQLite-backed single-node governance core.
- Partial = adapter crates and extended runtime integrations (uneven implementation slices exist, not production-verified).
- Deferred/post-v1 = broader adapter completion, multi-node/HA/read-replica, PostgreSQL production deployment (local Docker/runtime support is implemented).

## What exists

### Core crates
- `ferrum-proto` — domain shapes, proto definitions
- `ferrum-pdp` — Policy Decision Point (StaticPdpEngine; trust labels, taint scoring, contradiction checks)
- `ferrum-cap` — capability mint, mark_used, single-use enforcement
- `ferrum-rollback` — rollback/compensate operations, R3/R2/R0/R1 contract classes, auto_commit semantics
- `ferrum-store` — SQLite persistence (intents, proposals, capabilities, executions, rollback contracts, provenance events, approvals); exposes `StoreFacade` trait (9 repo accessors + health_check) for store-agnostic access
- `ferrum-gateway` — full orchestration: evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate (internal: commit/rollback as orchestration semantics); negative paths: deny, quarantine, RequireApproval, draft-only gated at evaluate (before prepare); store-agnostic via `Arc<dyn StoreFacade>`
- `ferrum-firewall` — TaintScoringFirewall with taint scoring, trust labeling, contradiction detection, sanitize, quarantine (21 tests)
- `ferrum-graph` — HashMap adjacency indexing, BFS ancestor/descendant traversal with cycle protection (10 tests)
- `ferrum-ledger` — SHA-256 hash chain ledger with chain integrity verification (13 tests)
- `ferrum-sync` — sync probe, ExternalEventSource trait, RuntimeBridge trait, McpBridge bridge infrastructure (65 tests)
- `ferrum-testkit` — test helpers

### Adapters
- `ferrum-adapter-fs` — filesystem adapter (partial: prepare/verify/validation/execute/rollback for FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod; FileWrite/FileDelete: snapshot-recovery for existing files via deterministic snapshot paths, new-file FileWrite with cleanup-on-rollback; FileMove (rename with snapshot-based rollback, dest/source dual-check verify, hash verification); FileCopy (copy with dest snapshotting, hash-matching verify, idempotent cleanup/restore rollback); DirCreate (validate parent exists + dir absent, mkdir, verify dir exists, rollback removes created dir); DirDelete (reject non-empty dirs, rm empty dir, verify dir gone, rollback recreates dir); **FileAppend** (prepare captures original hash + length, execute appends data, verify confirms file growth, rollback truncates to original length with hash verification); **FileChmod** (prepare captures current permissions, execute changes mode bits, verify confirms new permissions, rollback restores original mode with verification); explicit checks fail-closed on target-state invariants, target-path mismatch, malformed config, unsupported checks with phase-aware validation, plus phase-consistent normalization for fs/internal prepare/verify/execute/rollback failures; 146 tests passing)
- `ferrum-adapter-sqlite` — SQLite adapter (transaction-based rollback implementation; not production-verified)
- `ferrum-adapter-maildraft` — maildraft adapter (16 tests: create/update/delete lifecycle + rollback idempotency)
- `ferrum-adapter-git` — git adapter (local rollback/recovery implementation: prepare captures HEAD ref, rollback resets hard with dirty-worktree guard, verify checks ref matches, execute captures after_ref from payload, GitBranchCreate support with branch creation/deletion, base_ref validation/resolution, prepare-time rejection of existing branches and detached-HEAD-without-explicit-base, branch-name validation during prepare using git-native `git check-ref-format --branch` (fail-closed), verify fail-closed when the created branch is currently checked out, and detached-HEAD / safe-delete fail-closed guards; P2.3 slice adds implicit HEAD base_ref_sha persistence and enriched verify audit metadata; **GitTagCreate/GitTagDelete added**: GitTagCreate (prepare validates tag name + rejects existing tag, execute creates lightweight tag at HEAD, verify confirms tag exists, rollback deletes tag idempotently); GitTagDelete (prepare validates tag exists + captures tag_sha, execute deletes tag, verify confirms tag gone, rollback recreates tag at captured SHA with hash verification); **GitBranchDelete added**: safe branch deletion with recreate rollback (prepare captures branch SHA + current HEAD, execute deletes branch, verify confirms deletion, rollback recreates branch at captured SHA); 86 tests passing)
- `ferrum-adapter-http` — HTTP adapter (partial: bounded HttpMutation prepare/execute/verify plus bounded replay-based recovery for POST/PUT/PATCH. Prepare validates target/method/url shape and optional prepare checks; execute sends real HTTP requests, captures request/response metadata + digests, emits digest-only `rollback_groundwork_v1` and `http_recovery_readiness_v1`, and sends `Idempotency-Key` when a valid `http.replay_v1` contract is present; verify supports method-aware `HttpStatusExpected` plus `expected_statuses` arrays with phase-aware normalization; rollback/compensate succeed only for the strict one-step `http.replay_v1` POST/PUT/PATCH case with exact URL/digest binding, header-safe idempotency key, and required strict `expected_statuses` (non-empty, `100..=599`), and fail closed otherwise; **connection pooling and retry/backoff with rollback semantics**; 103 tests passing)

### Binaries
- `ferrumd` — server binary
- `ferrumctl` — CLI (health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance)
  - **Policy bundle authoring (H1.1d)**: `server create-policy-bundle`, `server get-policy-bundle` (with `--export`), `server list-policy-bundles`, `server update-policy-bundle`, `server delete-policy-bundle`, `server set-policy-bundle-active`; local `author bundle bump` for version bumping
- `ferrum-migrate` — database migration binary (SQLite → PostgreSQL migration; runs schema migrations against configured store DSN; feature-gated `--features postgres`)

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
- (none) — supported flows list documented in `25-EV-v1-single-node-rc-evidence.md`
- (none) — open gaps list documented in `11-remaining-tasks.md`

### P2 — v1 polish
Current verified slices are green: `ferrum-adapter-fs` (146 tests, FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod), `ferrum-adapter-git` (86 tests, GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete), `ferrum-adapter-http` (103 tests, POST/PUT/PATCH replay), clippy passes on those packages, and `scripts/generate_rc_evidence.py` exists and passes

## Phase status summary

- **Phase A** (compile/shape stability): DONE
- **Phase B** (SQLite storage boundary): DONE
- **Phase C** (firewall MVP): DONE — logic exists, curated regression fixtures implemented (6 tests)
- **Phase D** (adapters): PARTIAL — fs has verified local slices (146 tests: FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod), git has verified local slices (86 tests), http has a bounded prepare/verify slice with PUT/PATCH replay (103 tests), sqlite has a transaction-based rollback implementation (16 tests), maildraft has full lifecycle implementation (16 tests: create/update/delete), and broader adapter completion is post-v1
- **Phase E** (gateway orchestration): DONE for SQLite-backed single-node flow
- **Phase F** (hardening/evidence): DONE — integration tests strong, poisoned-context fixtures curated, supported flows and gaps documented, evidence script present

## Test coverage matrix (2026-04-28)

Full workspace check/clippy/test pass locally with 0 failures. Prefer command-level verification over stale aggregate test counts.

**Governance fixes completed**:
- Active policy bundle rules evaluated in gateway before PDP fallback.
- `ferrum-firewall` taint scoring wired into gateway PDP and policy bundle matching.
- Lineage emits PolicyEvaluated, CapabilityMinted, ToolCallPrepared, ToolCallExecuted, SideEffectVerified, SideEffectCommitted in happy path.
- `SqliteStore::verify_ledger_chain()` delegates to real chain checks over `ledger_entries`.
- Gateway capability authorize/revoke falls back to persisted capability state after in-memory loss; revoke persistence/provenance is synchronous.
- **Note**: U1 (Outcome-aware Governance) complete — `evaluate_outcome` endpoint implemented, 2 integration tests added (`test_outcome_evaluation_aligned_flow`, `test_outcome_evaluation_forbidden_flow`).
- **Note**: U2 (Reversible Execution Planner) complete — PlannableAdapter trait, PlannableNoopAdapter, PlannableFsAdapter implemented, 4 new tests added.
- **Note**: U3 (Cross-runtime Provenance Fabric) complete — ExternalEventSource trait, FakeExternalEventSource, POST /v1/provenance/ingest endpoint, source_runtime_id on ProvenanceEvent. 13 new tests.
- **Note**: U4 (Runtime Integrations) complete — RuntimeBridge trait, McpBridge, GET /v1/bridges + GET /v1/bridges/{id}/tools endpoints. 13 new tests.

| Crate | Tests | Status |
|---|---|---|
| ferrum-adapter-fs | 146 | FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod + cross-filesystem + PlannableFsAdapter |
| ferrum-adapter-git | 86 | GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete + GitPush/GitPull |
| ferrum-adapter-http | 103 | HttpMutation + http.replay_v1 (POST/PUT/PATCH) + pooling/retry |
| ferrum-adapter-sqlite | 16 | SqlRowCountRange checks + transaction rollback + G-E1 verify fail-closed |
| ferrum-adapter-maildraft | 16 | create/update/delete lifecycle + rollback idempotency |
| ferrum-cap | 9 | Capability TTL boundaries + mark_used idempotency/concurrent/revoked/expired |
| ferrum-firewall | 21 | TaintScoringFirewall, taint scoring, contradiction detection, sanitizer |
| ferrum-graph | 10 | HashMap adjacency indexing, BFS ancestor/descendant traversal |
| ferrum-ledger | 13 | SHA-256 hash chain with chain integrity verification |
| ferrum-gateway | 50 | Server endpoints + evaluate-outcome + provenance ingest + bridge endpoints + readiness + deep readiness failure-mode (S2 improved) |
| ferrum-pdp | 19 | Outcome-aware governance |
| ferrum-proto | 18 | Intent validation + canonical action digest + schemas |
| ferrum-store | 82 | SQLite persistence + StoreFacade + readiness health check |
| ferrum-sync | 65 | ExternalEventSource + RuntimeBridge + McpBridge + preflight/decision/diff-classifier |
| ferrumctl | 48 | list-intents/cancel-execution/pause-execution + policy bundle CRUD + author bundle bump + backup/restore |
| ferrumd | 6 | Daemon config + unsupported DSN guardrails |
| Integration tests | 87 | contracts(2) + fs-roundtrip(7) + gateway-flow(65) + lineage-chain(13) |
| ferrum-rollback | 11 | ExecutionPlan + PlannableAdapter + auto-planning in RollbackService |
| ferrum-integrations-mcp | 239 | MCP D1 local coverage (231 lib + 8 binary) |

## Next step

All P0–P2 items closed. U1–U4 upgrade tracks complete. P5a–P5e engineering complete within authorized scope. Full workspace gate rerun passed (ALL LOCAL CHECKS PASSED 2026-05-17). P6 oracle verdict: CONDITIONAL GO. Production-ready remains NO; HA/multi-node/PostgreSQL production deployment remain NO. Remaining explicit operator blocker: Block A WAIVED/CONDITIONAL (real domain still required for production-ready or full G2 closure).

Remaining operator blockers before pilot (as of 2026-05-17):
- **Block A — Real owned domain**: WAIVED/CONDITIONAL — operator selected Path A (acknowledge conditional pilot closure, no real domain yet) on 2026-05-18; DuckDNS accepted for single-node SQLite pilot only; real owned domain still required for production-ready or full G2 closure. See `artifacts/2026-05-18-path-a-conditional-pilot-closure-acknowledgment.md`
- **Block B — Off-VM alerting**: CLOSED — operator confirmed inbox receipt for primary and secondary contacts (TEST_IDs `fg-inbox-check-20260516-052910` and `fg-secondary-check-20260516-153221`, G-B1/G-B2); bearer token rotation executed on VM; SendGrid API key rotation verified on VM, synthetic alert `FerrumGateSendGridDirPermFix` delivered to primary+secondary inboxes, old key revoked/deleted (G-B3 verified; see [`artifacts/2026-05-17-sendgrid-rotation-evidence.md`](./artifacts/2026-05-17-sendgrid-rotation-evidence.md)); escalation matrix formally acknowledged on 2026-05-17 (see [`artifacts/2026-05-17-escalation-matrix-acknowledgment.md`](./artifacts/2026-05-17-escalation-matrix-acknowledgment.md))
- **Block C — Keyless backup**: CLOSED — C1 keyless backup verified, residual key removed, offsite sync confirmed; `n2-standard-2` remains temporary operational type

Engineering/operator items completed (May 16):
- `cargo-deny v0.19.6` and `cargo-audit v0.22.1` installed; `make audit` passes with both tools (cargo-deny advisory DB fetched, advisories ok; cargo-audit loaded 1090 advisories, scanned 384 dependencies, 0 actionable issues). `RUSTSEC-2023-0071` (`rsa` via `sqlx-mysql`) ignored as uncompiled optional dependency blocked by `default-features = false` on `sqlx`.
- Bearer token rotation executed on VM securely: token generated on VM and never printed; old token backed up; env updated; ferrumgate.service active; LOCAL_READYZ=200; LOCAL_DEEP=200; new token HTTP=200; old token HTTP=401; ROTATION_RESULT=PASS; PUBLIC_READYZ=200. SSH firewall temporarily opened to `14.239.184.129/32` for live work and restored to `118.69.4.63/32` after.
- Secondary alert contact delivery confirmed: active AlertManager config (`/etc/prometheus/alertmanager.yml`) validated with `ACTIVE_CONFIG_CHECK=PASS`, `ALERTMANAGER_SERVICE=active`, `ACTIVE_SECONDARY_PRESENT=YES`, `ACTIVE_EMAIL_TO_COUNT=4`; synthetic alert posted successfully (`ALERT_POST_HTTP=200`, `ALERT_VISIBLE=YES`, TEST_ID `fg-secondary-check-20260516-153221`, START_AT_UTC `2026-05-16T15:32:21Z`); operator confirmed secondary inbox receipt.
- SendGrid API key rotation VM verification: active secret path `/etc/ferrumgate/secrets/sendgrid-api-key` verified with `MODE=640 OWNER=root:prometheus`, directory permissions corrected to `750 root:prometheus`, AlertManager active, synthetic alert delivered to primary and secondary inboxes, and old key revoked/deleted. See [`artifacts/2026-05-17-sendgrid-rotation-evidence.md`](./artifacts/2026-05-17-sendgrid-rotation-evidence.md).

Historical completed items (May 13–16):
- ✅ Target-host D1–D6 evidence — passed 6/6 on 2026-05-13
- ✅ SQLite restore drill — passed 2026-05-15 (0.463s restore)
- ✅ Backup automation — verified with retention pruning (run id `20260515T1606Z-b3-retention`)
- ✅ TLS/reverse proxy configuration — closed via delegated authority 2026-05-15
- ✅ Bearer token generation — closed via delegated authority 2026-05-15
- P5c.V1 backup drill and P5c.V2 restore drill — SQLite path selected; PostgreSQL deferred
- G3.6 real workload/post-deploy monitoring — fully accepted for P5b engineering review only

Next decision routing:
1. **Path 1 (RC tag)**: ✅ Executed — `v0.1.0-rc.2` cut at `e229f76` with fresh G1 gates PASS; GitHub prerelease published. See [`artifacts/2026-05-17-all-paths-execution-evidence.md`](./artifacts/2026-05-17-all-paths-execution-evidence.md) §Path 1.
2. **Path 2 (Operator signoff/pilot)**: Safe probes executed (shallow/deep/metrics PASS against duckdns); L2 auth now PASS after root-cause remediation (missing `store_dsn` → in-memory SQLite, fixed on VM); L4 bounded workload PASS (clean rerun after script artifact bug); L5 backup verification PASS (`ferrumctl backup verify` OK, timer active, offsite script present); **G2/operator signoff NOT complete**; Block A WAIVED/CONDITIONAL for single-node SQLite pilot only with operator acknowledgment recorded on 2026-05-17. SendGrid rotation and escalation matrix acknowledgment are now verified. See [`54-operator-signoff-packet.md`](./54-operator-signoff-packet.md).
3. **Path 3 (PostgreSQL/Phase3)**: P3 repository implementations and P4.1–P4.4 MVP/local Docker/runtime complete. Local workload plan generated (3360 requests, no live traffic); MCP lifecycle smoke passed (15/15). Production/HA/multi-node remains deferred. See [`31-release-paths-todo.md`](./31-release-paths-todo.md) §Path 3.

Completion tracker and remaining work:
- [122-completion-roadmap-and-hardening-tracker.md](./122-completion-roadmap-and-hardening-tracker.md) — 10-item tracker for docs updates, Block B hardening, ferrum-cap tests, cargo-audit gate, and Block A domain path
- [artifacts/2026-05-17-operator-unblock-packet.md](./artifacts/2026-05-17-operator-unblock-packet.md) — Consolidated operator-action checklist for Block A domain, Block B SendGrid rotation, and escalation matrix acknowledgment
- [artifacts/2026-05-17-bridge-to-live-runbook.md](./artifacts/2026-05-17-bridge-to-live-runbook.md) — Safe-by-default validation toolkit with L1–L5 gates for live target-host transition
- [artifacts/2026-05-17-bridge-l0-preflight-evidence.md](./artifacts/2026-05-17-bridge-l0-preflight-evidence.md) — Bridge L0 pre-flight evidence packet: local gate results, plan-mode output, blocker summary, and operator handoff statement
- [artifacts/2026-05-17-bridge-l1-l3-live-evidence.md](./artifacts/2026-05-17-bridge-l1-l3-live-evidence.md) — Bridge L1–L3 live evidence: L1/L3 PASS, L2 PASS after root-cause remediation (missing `store_dsn` → in-memory SQLite, fixed on VM); historical initial state was partial/blocked due to SSH/firewall constraints
- [artifacts/2026-05-17-bridge-l4-l5-live-evidence.md](./artifacts/2026-05-17-bridge-l4-l5-live-evidence.md) — Bridge L4–L5 live evidence: L4 bounded workload PASS (script artifact bug on first run, clean rerun PASS), L5 backup verification PASS (`ferrumctl backup verify` OK, timer active, latest backup present, offsite script present), runbook drift corrected (`--store-path` → `--db-path`, `ferrumctl` → `/opt/ferrumgate/ferrumctl`)
- [artifacts/2026-05-17-sendgrid-rotation-evidence.md](./artifacts/2026-05-17-sendgrid-rotation-evidence.md) — SendGrid API key rotation evidence: active secret path/permission fix, primary+secondary delivery, old-key revocation, and SSH firewall restoration
- [artifacts/2026-05-17-escalation-matrix-acknowledgment.md](./artifacts/2026-05-17-escalation-matrix-acknowledgment.md) — Formal escalation matrix acknowledgment for FerrumGate v1 conditional single-node SQLite pilot; closes Block B
- [artifacts/2026-05-17-block-a-duckdns-conditional-pilot-waiver.md](./artifacts/2026-05-17-block-a-duckdns-conditional-pilot-waiver.md) — Block A DuckDNS conditional pilot waiver; operator acknowledgment recorded 2026-05-17; Block A WAIVED/CONDITIONAL for single-node SQLite pilot only
- [33-feature-completion-backlog.md](./33-feature-completion-backlog.md) — Must/Should/Production-only backlog for incomplete/partial features
- [45-current-feature-audit.md](./45-current-feature-audit.md) — Phase 3 D5 bottleneck analysis complete; D6 priority list complete. Full report: [51-d5-bottleneck-analysis-report.md](./51-d5-bottleneck-analysis-report.md); Priority list: [52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md)
- [32-feature-completeness-audit.md](./32-feature-completeness-audit.md) — Route/API reconciliation
- [56-adapter-compensation-evidence-matrix.md](./56-adapter-compensation-evidence-matrix.md) — Adapter compensation behavior evidence (post-RC docs-only)
- [57-workload-compensation-drill-plan.md](./57-workload-compensation-drill-plan.md) — Operator drill plan for compensation verification (post-RC docs-only)
- [58-workload-compensation-drill-evidence-template.md](./58-workload-compensation-drill-evidence-template.md) — Operator-fillable drill evidence template (post-RC docs-only)
- [54-operator-signoff-packet.md](./54-operator-signoff-packet.md) — Operator signoff packet: G2.1–G2.8 signed 09/05/2026 for conditional single-node SQLite pilot; 2026-05-17 DuckDNS conditional pilot evidence addendum added (L1–L5 live PASS summary, runbook drift correction, Block A/B/C status)
- [59-pilot-readiness-evidence-packet.md](./59-pilot-readiness-evidence-packet.md) — G2.1–G2.8 evidence packet for Path 2 pilot; 2026-05-17 addendum added with bridge L1–L5 live evidence summary and non-claims
- [60-bounded-hardening-examples.md](./60-bounded-hardening-examples.md) — Bounded hardening drill examples (post-RC docs-only)
