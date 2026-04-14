# 01 — Current state

Last updated: 2026-04-12
Single-node v2 RATIFIED (2026-04-12). Scope: v2 single-node production per `44-v2-production-execution-plan.md`.

**Note on fs-first hash slice (2026-04-09)**: The fs-first `before_hash`/`after_hash` wiring for the fs adapter and gateway rollback path was completed as a **T2/partial beta-slice hardening step**. This is a narrow internal wiring improvement; it does not change the T1/T2/T3 support contract boundaries declared in `43-production-readiness-signoff.md`. See `docs/artifacts/2026-04-09/closure-note.txt` for evidence.

**Release support contract**:
- Supported = SQLite-backed single-node governance core.
- Partial = bounded local adapter implementations plus early upgrade-track slices that are not yet production-verified.
- Deferred/post-v1 = broader adapter hardening, multi-node/HA/read-replica, remaining U1 expressiveness/operator tooling work, and deeper U2-U4 upgrade-track work.

## What exists

### Core crates
- `ferrum-proto` — domain shapes, proto definitions
- `ferrum-pdp` — Policy Decision Point (StaticPdpEngine; trust labels, taint scoring, contradiction checks, advisory outcome-aware assessment with explicit forbidden-outcome deny)
- `ferrum-cap` — capability mint, mark_used, single-use enforcement
- `ferrum-rollback` — rollback/compensate operations, R3/R2/R0/R1 contract classes, auto_commit semantics
- `ferrum-store` — SQLite persistence (intents, proposals, capabilities, executions, rollback contracts, provenance events, approvals)
- `ferrum-gateway` — full orchestration: evaluate -> mint -> authorize -> prepare -> execute -> verify -> compensate (internal: commit/rollback as orchestration semantics); negative paths: deny, quarantine, RequireApproval, draft-only gated at evaluate (before prepare); U1-S2 verify-time outcome assessment annotation (annotate-only, does not change verify decision semantics); U1-S5a soft gate preview DONE at prepare-time (emit warn signals does not block state-machine or change auto-commit); U1-S5b hard gate DONE at prepare-time (block only when would_block=true, state-machine halts to Denied, auditability via ErrorRaised event and u1_s5b_hard_gate metadata, auto-commit unchanged per R3 contract semantics); U1-S6 selector-aware effective match DONE (selector-bearing clauses require effect_type AND selectors to match for effective match, enabling HIGH-confidence selector mismatch to trigger would_block=true at prepare-time); U1-S7a list-based selector matching DONE (additive `adapter_family_in`, `target_family_in`, `request_class_in`, `mutation_family_in` fields enable OR semantics: match scalar OR any list member); U1-S8a operator compile-time ergonomics DONE (compile endpoint accepts optional `allowed_outcomes`/`forbidden_outcomes` using existing OutcomeClause/OutcomeSelectors schema with fail-closed validation); U1-S9a deterministic policy bundle fingerprinting DONE (PolicyBundleId::derive() uses UUID v5 to create deterministic identity from canonical outcome contract; IntentEnvelope.policy_bundle_fingerprint stores the derived fingerprint; capability mint propagates fingerprint into CapabilityLease.policy_bundle_id; provenance events carry derived bundle identity)
- `ferrum-firewall` — trust labeler, taint scorer, sanitize, contradiction checks, and U1-aware read-only contradiction gating
- `ferrum-graph` — provenance graph
- `ferrum-ledger` — ledger (skeleton)
- `ferrum-sync` — sync probe (skeleton, infrastructure only)
- `ferrum-testkit` — test helpers

### Adapters
- `ferrum-adapter-fs` — filesystem adapter (local file write/delete with hash-based verify semantics; durability/hardening still limited)
- `ferrum-adapter-sqlite` — SQLite adapter (single-row and atomic multi-row rollback/compensate path for bounded local table/row mutations)
- `ferrum-adapter-maildraft` — maildraft adapter (SQLite-backed draft persistence; verify semantics implemented; send semantics explicitly out of scope)
- `ferrum-adapter-git` — git adapter (local HEAD capture/reset and branch-create rollback path; remote workflows explicitly out of scope)
- `ferrum-adapter-http` — HTTP adapter (bounded HTTP execute/verify with body-aware digest, header-shape binding, canonical query strings, auth support, and conservative rollback no-op; mutation recovery is R3 boundary; verify semantics clarified via issue #97 — mutations use execute-time metadata only, fail-closed on non-2xx without explicit check; gateway integration coverage added)

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

### P0 — v1 RC blockers (as of 2026-04-02)
- (none) — all P0 blockers resolved as of 2026-04-02 gate run; issue #97 HTTP adapter semantics and gateway integration coverage merged 2026-04-03

### P1 — v1 RC evidence gaps
- (none) — poisoned-context regression fixtures implemented (6 fixture tests)
- (none) — Phase F docs pack finalized in `docs/implementation-path/`
- (none) — supported flows list documented in `25-v1-single-node-rc-evidence.md`
- (none) — open gaps list documented in `11-remaining-tasks.md`

### P2 — v1 polish
- scope-mismatch deny: DONE in `crates/ferrum-pdp/src/engine.rs` lines 31-46
- `scripts/generate_rc_evidence.py` exists and PASS — verdict is ALL GATES PASSED (2026-04-02)

## Phase status summary

- **Phase A** (compile/shape stability): ✅ DONE
- **Phase B** (SQLite storage boundary): ✅ DONE
- **Phase C** (firewall MVP): ✅ DONE — logic exists, curated regression fixtures implemented (6 tests)
- **Phase D** (adapters): ✅ DONE — bounded local implementations exist for fs/sqlite/maildraft/git/http; broader production hardening is post-v1
- **Phase E** (gateway orchestration): ✅ DONE for SQLite-backed single-node flow
- **Phase F** (hardening/evidence): ✅ DONE — integration tests strong, poisoned-context fixtures curated, supported flows and gaps documented, evidence script present; P3.G1-G4 live evidence all executed and attested (2026-04-03); all gates cleared as of 2026-04-02
- **Phase 6** (v2 ratification): ✅ DONE — v2 RATIFIED 2026-04-12 (`46-v2-readiness-signoff.md`)

## H1 Shipped Summary (as of 2026-04-13)

The following H1 sub-slices completed post-v2-ratification (2026-04-12):

| Sub-slice | What | Status |
|-----------|------|--------|
| **H1.1a** | Policy bundle persistence API + `PolicyBundleRepo` storage + `ferrumctl` surface (`register-policy-bundle`, `inspect-policy-bundle`, `list-policy-bundles`) | ✅ DONE |
| **H1.1b** | Policy bundle metadata update/delete (`PUT /v1/policy-bundles/{id}`, `DELETE /v1/policy-bundles/{id}`) + created_at preservation on re-registration | ✅ DONE |
| **H1.1c** | Policy bundle lineage via optional supersedes relationship — `supersedes_bundle_id`, `GET /v1/policy-bundles/{id}/successors`, `list-policy-bundle-successors` CLI, delete-referenced-blocked | ✅ DONE |
| **H1.1d** | Policy bundle authoring CLI for registration payloads — `ferrumctl author request generate\|validate\|bump` + `ferrumctl server register-policy-bundle --request-file` | ✅ DONE |
| **H1.2b** | Policy bundle authoring CLI for rules-format YAML — `ferrumctl author intent generate\|validate`, `ferrumctl author bundle generate\|validate` | ✅ DONE |
| **H1.3a** | Persistent named-remote configuration — `GitRemoteStore` with add/get/list/update/remove; remotes persist in git config | ✅ DONE |
| **H1.3b** | Authenticated remote support — git-native credential delegation (HTTPS username/password, SSH private key); no in-process secret storage | ✅ DONE |
| **H1.4b** | `ferrumctl store backup` / `restore` for local SQLite backup/restore automation | ✅ DONE |
| **H1.4c** | Streaming/chunked query patterns for larger-than-memory dataset handling (single-node) | ✅ DONE |
| **H1.5a** | Retry/backoff with idempotency key management for mutation methods | ✅ DONE |

Remaining H1 sub-slices (H1.3c, H1.4a, H1.4d–H1.4e, H1.5b–H1.5c) are ⬜ PLANNED.

Full per-slice detail: **`50-post-v2-roadmap.md`** (Horizons H1/H2/H3).

## Post-v2 execution queue

v2 is RATIFIED. The immediate post-v2 execution queue is:

1. **H1.1** — Policy bundle lifecycle tooling (operator-facing, unblocks advanced authoring)
2. **H1.2** — U1 remaining expressiveness backlog + authoring CLI
3. **H1.3** — git adapter deeper remote integration hardening (authenticated remote, multi-remote support)
4. **H1.4** — fs/sqlite broader production-verified integration (WAL-mode tuning, backup/restore automation, larger-than-memory handling)
5. **H1.5** — http adapter broader external integration (mTLS, OAuth2, retry/idempotency)

Full detail: **`50-post-v2-roadmap.md`** (Horizons H1/H2/H3).
