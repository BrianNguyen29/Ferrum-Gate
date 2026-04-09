# 44 — v1 Review/Readiness Template

**Template for:** FerrumGate v1 single-node review and readiness cycles  
**Scope:** v1 single-node, SQLite-backed, T1/T2/T3 support contract boundary  
**Last updated:** 2026-04-09  
**Usage:** Fill in evidence/results before each review cycle. Mark inherited evidence with source doc; mark unverified items as `NEEDS_REFRESH`.

---

## How to Use This Template

1. **Snapshot section** — run workspace commands or inherit from existing doc evidence
2. **Per-section checklists** — check each item; fill in Result and Evidence columns
3. **Status key:**
   - `✅ PASS` — verified today with current command output
   - `⚠️ INHERITED` — verified in a prior doc but not re-run today (cite source doc + date)
   - `❌ FAIL` — check failed; requires investigation
   - `⬜ NEEDS_REFRESH` — item not yet verified in current cycle
   - `N/A` — not applicable to v1 scope

---

## SECTION 0 — Current Readiness Snapshot

> Populate from commands run today. Inherited entries cite the source doc and note "(inherited — not re-verified today)".

### 0.1 Workspace Quality (Commands run: 2026-04-09)

| Check | Command | Result | Evidence |
|-------|---------|--------|----------|
| Cargo check | `cargo check --workspace` | ✅ PASS | `Finished dev profile [unoptimized + debuginfo] target(s) in 24.53s` (2026-04-09) |
| Cargo fmt | `cargo fmt --all -- --check` | ✅ PASS | No output = pass (2026-04-09) |
| Cargo clippy | `cargo clippy --workspace -- -D warnings` | ✅ PASS | `Finished dev profile [unoptimized + debuginfo] target(s) in 9.14s` (2026-04-09) |
| Cargo test | `cargo test --workspace` | ✅ PASS | All suites passed; see test output (2026-04-09) |
| Contract consistency | `python3 scripts/check_contract_consistency.py` | ✅ PASS | `VALIDATION PASSED` (2026-04-09) |
| Perf baseline build | `cargo build -p ferrum-perf-baseline` | ✅ PASS | `Finished dev profile [unoptimized + debuginfo] target(s) in 3.36s` (2026-04-09) |
| ferrumctl tests | `cargo test -p ferrumctl` | ✅ PASS | 3 lib + 20 integration tests passed; see outputs (2026-04-09) |
| ferrumctl --help | `cargo run -p ferrumctl -- server compile-intent --help` | ✅ PASS | Help output shown; dev profile built in 13.22s (2026-04-09) |
| ferrumctl --help (2) | `cargo run -p ferrumctl -- server commit-execution --help` | ✅ PASS | Help output shown; dev profile built in 14.89s (2026-04-09) |
| ferrum-sync --lib | `cargo test -p ferrum-sync --lib` | ✅ PASS | 232 passed; 0 failed (2026-04-09) |
| ferrum-store sync_preflight | `cargo test -p ferrum-store --lib sync_preflight` | ✅ PASS | 25 passed; 0 failed (2026-04-09) |
| ferrum-store sync_service | `cargo test -p ferrum-store --lib sync_service` | ✅ PASS | 31 passed; 0 failed (2026-04-09) |
| ferrum-perf-baseline test | `cargo test -p ferrum-perf-baseline` | ✅ PASS | 1 test passed in 23.31s (2026-04-09) |
| ferrum-gateway --lib | `cargo test -p ferrum-gateway --lib` | ✅ PASS | 48 passed; 0 failed (2026-04-09) |
| Perf baseline run | dev: `cargo run -p ferrum-perf-baseline -- --concurrency 2 --iterations 2`; release: `cargo run --release -p ferrum-perf-baseline -- --concurrency 5 --iterations 5` | ⚠️ INHERITED | Harness re-run succeeded on 2026-04-09. Dev: 4 ops/scenario, 19.4s total, 0 errors. Release: 25 ops/scenario, 75s total, 32 errors total across scenarios (68% success overall). Treat as refreshed baseline evidence aligned with `42-p2-performance-baseline-evidence.md`, not as an error-free perf pass or SLO claim. |
| ferrumctl check | `cargo check -p ferrumctl` | ✅ PASS | `Finished dev profile [unoptimized + debuginfo] target(s) in 1.39s` (2026-04-09) |

**Refresh summary (2026-04-09):** `ferrumctl` tests refreshed and passed (3 lib + 20 integration); signoff-linked sync/store tests refreshed and passed; `ferrum-gateway --lib` additionally verified. Perf baseline runtime evidence was refreshed and preserved as baseline-only evidence aligned with `42-p2-performance-baseline-evidence.md`, not promoted to an error-free PASS. `NEEDS_REFRESH` on `ferrumctl tests` resolved to ✅ PASS.

### 0.2 Production Evaluation Gates (inherited from prior cycles)

| Gate | Status | Source | Last Verified |
|------|--------|--------|---------------|
| G-E1: P2 adapter hardening | ✅ DONE 2026-04-08 | `30-production-roadmap.md` | 2026-04-08 |
| G-E2: P2 performance baseline | ✅ DONE 2026-04-08 | `42-p2-performance-baseline-evidence.md` | 2026-04-08 |
| G-E3: ferrumctl advanced flows | ✅ DONE 2026-04-08 | `30-production-roadmap.md` | 2026-04-08 |
| G-E4: P5 Sync-1 preflight ratified | ✅ DONE 2026-04-08 | `30-production-roadmap.md` | 2026-04-08 |
| G-E5: Production evaluation sign-off | ✅ DONE 2026-04-08 | `43-production-readiness-signoff.md` | 2026-04-08 |

### 0.3 Docs Linkage Check

| Check | Expected Link | Status | Notes |
|-------|--------------|--------|-------|
| `00-project-canon.md` links to support contract | `./19-v1-single-node-support-contract.md` | ✅ VERIFIED | Section 4; canon states T1/T2/T3 boundaries |
| `19-v1-single-node-support-contract.md` links back to canon | `docs/00-project-canon.md` | ✅ VERIFIED | Section 0 references canon |
| `16-release-checklist.md` references support contract | `./19-v1-single-node-support-contract.md` | ✅ VERIFIED | Header of `16-release-checklist.md` |
| `30-production-roadmap.md` cross-links to signoff | `43-production-readiness-signoff.md` | ✅ VERIFIED | Table in Section 2 |
| `23-production-readiness-assessment.md` cross-links to RC evidence | `./25-v1-single-node-rc-evidence.md` | ✅ VERIFIED | Line 12 |
| This doc lives in `implementation-path/` | `docs/implementation-path/44-v1-review-readiness-template.md` | ✅ VERIFIED | — |

---

## SECTION 1 — Docs Alignment

### 1.1 Support Contract Integrity

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| Support contract (`19-v1-single-node-support-contract.md`) exists | — | — | ⬜ |
| Support contract Section 0 matches `00-project-canon.md` T1/T2/T3 boundaries | — | — | ⬜ |
| Supported routes (Section 1.2) match actual router implementation | — | — | ⬜ |
| CLI surface (Section 1.3) matches `ferrumctl` actual commands | — | — | ⬜ |
| T2 partial adapters listed match `30-production-roadmap.md` P2 items | — | — | ⬜ |
| T3 out-of-scope items match `30-production-roadmap.md` and `11-remaining-tasks.md` | — | — | ⬜ |
| SLA surface (Section 7) is consistent with known limitations (Section 3) | — | — | ⬜ |
| EOL policy (Section 9) is present and unambiguous | — | — | ⬜ |

### 1.2 Cross-Doc Consistency

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| `00-project-canon.md` hard rules (Section 5) consistent with PDP/engine behavior | — | — | ⬜ |
| `16-release-checklist.md` checklist items reflect current repo state | — | — | ⬜ |
| `23-production-readiness-assessment.md` verdicts consistent with `25-v1-single-node-rc-evidence.md` | — | — | ⬜ |
| `30-production-roadmap.md` gate statuses current (G-E1 through G-E5) | — | — | ⬜ |
| `41-production-execution-plan.md` phase tracking consistent with `30-production-roadmap.md` | — | — | ⬜ |
| No doc claims multi-node/HA support (all v1 docs scoped to single-node) | — | — | ⬜ |

---

## SECTION 2 — Technical Verification

### 2.1 Core Build and Quality Gates

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| `cargo check --workspace` passes | — | — | ⬜ |
| `cargo fmt --all -- --check` passes | — | — | ⬜ |
| `cargo clippy --workspace -- -D warnings` passes | — | — | ⬜ |
| `cargo test --workspace` passes | — | — | ⬜ |
| `python3 scripts/check_contract_consistency.py` passes | — | — | ⬜ |
| OpenAPI spec (`openapi.yaml` or equivalent) exists and matches supported routes | — | — | ⬜ |

### 2.2 Gateway Flow

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| evaluate → mint → authorize → prepare → execute → verify → compensate flow is wired | — | — | ⬜ |
| scope-mismatch deny implemented (empty scope + non-R0 = Deny) | — | — | ⬜ |
| single-use capability (cap marked Used → AlreadyUsed on reuse) | — | — | ⬜ |
| R3 no auto-commit (R3 contracts have auto_commit=false) | — | — | ⬜ |
| compensate and rollback are distinct adapter operations | — | — | ⬜ |
| high taint (>=70) triggers Quarantine for non-R0 | — | — | ⬜ |
| R3 requires approval (RequireApproval decision) | — | — | ⬜ |

### 2.3 Adapter Surfaces (T2 — Partial)

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| fs adapter: fail-closed verify on I/O errors | — | — | ⬜ |
| fs adapter: compensate deletes new file (when no snapshot) | — | — | ⬜ |
| sqlite adapter: identifier safety (SQL injection prevention) | — | — | ⬜ |
| sqlite adapter: fail-closed verify on corruption | — | — | ⬜ |
| git adapter: fail-closed verify on I/O errors | — | — | ⬜ |
| git adapter: GitPull rollback fail-closed when branch changed | — | — | ⬜ |
| http adapter: fail-closed on transport failure and timeout | — | — | ⬜ |
| http adapter: verify_checks mismatch → verified=false → commit rejected | — | — | ⬜ |
| maildraft adapter: fail-closed on storage/db error | — | — | ⬜ |

### 2.4 API Surface (T1)

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| GET /v1/healthz — shallow health check | — | — | ⬜ |
| GET /v1/readyz — shallow readiness check | — | — | ⬜ |
| POST /v1/intents/compile | — | — | ⬜ |
| POST /v1/proposals/{id}/evaluate | — | — | ⬜ |
| POST /v1/capabilities/mint | — | — | ⬜ |
| GET /v1/capabilities/{id} | — | — | ⬜ |
| POST /v1/executions/authorize | — | — | ⬜ |
| POST /v1/executions/{id}/prepare | — | — | ⬜ |
| POST /v1/executions/{id}/execute | — | — | ⬜ |
| POST /v1/executions/{id}/verify | — | — | ⬜ |
| POST /v1/executions/{id}/commit | — | — | ⬜ |
| POST /v1/executions/{id}/compensate | — | — | ⬜ |
| POST /v1/executions/{id}/rollback | — | — | ⬜ |
| GET /v1/executions/{id} | — | — | ⬜ |
| GET /v1/approvals (pagination, filter) | — | — | ⬜ |
| GET /v1/approvals/{id} | — | — | ⬜ |
| POST /v1/approvals/{id}/resolve | — | — | ⬜ |
| GET /v1/provenance/lineage/{id} | — | — | ⬜ |
| POST /v1/provenance/lineage | — | — | ⬜ |
| POST /v1/provenance/query | — | — | ⬜ |

---

## SECTION 3 — Performance

### 3.1 Benchmark Baseline

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| `benches/` harness exists and builds | — | — | ⬜ |
| S4 intent-compile baseline captured | — | — | ⬜ |
| S5 execution-pipeline baseline captured | — | — | ⬜ |
| S6 capability-cycle baseline captured | — | — | ⬜ |
| S7 sqlite-contention baseline captured | — | — | ⬜ |
| Baseline results documented in `42-p2-performance-baseline-evidence.md` | — | — | ⬜ |
| No benchmark regression vs prior baseline (if re-running) | — | — | ⬜ |

---

## SECTION 4 — Security

### 4.1 Auth and Access

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| Bearer-token auth enforced on mutating endpoints | — | — | ⬜ |
| No raw internal control data leaked to user plane | — | — | ⬜ |
| No bypass gateway for mutation (hard rule in canon) | — | — | ⬜ |

### 4.2 Input Validation

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| SQLite adapter rejects unsafe table names (SQL injection) | — | — | ⬜ |
| Invalid UUIDs rejected with 400 on lineage endpoints | — | — | ⬜ |
| Malformed verify_checks fail closed on maildraft adapter | — | — | ⬜ |

---

## SECTION 5 — Stability

### 5.1 Operational Runbooks

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| Deployment config docs (`15-deployment-and-operations.md`) exist and are current | — | — | ⬜ |
| Operations runbook (`18-single-node-operations-runbook.md`) exists | — | — | ⬜ |
| Backup/restore drill procedure documented (runbook Section 6.4) | — | — | ⬜ |
| RPO/RTO ownership documented (operator responsibilities clear) | — | — | ⬜ |
| Functional readiness probe guidance documented (not just healthz/readyz) | — | — | ⬜ |

### 5.2 Live Smoke / Drill Evidence (if available)

| Check | Result | Evidence | Status |
|-------|--------|----------|--------|
| P3.G1 functional readiness walkthrough evidence | — | — | ⬜ |
| P3.G2 smoke stability evidence | — | — | ⬜ |
| P3.G3 backup/restore drill evidence | — | — | ⬜ |
| P3.G4 observability verification evidence | — | — | ⬜ |

---

## SECTION 6 — Final Verdict

### 6.1 Readiness Summary

| Dimension | Overall Status | Key Evidence | Outstanding Items |
|-----------|---------------|--------------|-------------------|
| Docs Alignment | ⬜ | — | — |
| Technical Verification | ⬜ | — | — |
| Performance | ⬜ | — | — |
| Security | ⬜ | — | — |
| Stability | ⬜ | — | — |

### 6.2 Support Tier Alignment

| Tier | Surface | Status | Notes |
|------|---------|--------|-------|
| T1 — Supported | Governance core + SQLite + REST/CLI surface | ⬜ | — |
| T2 — Partial | fs/sqlite/git/http/maildraft adapters (bounded local) | ⬜ | — |
| T3 — Out of Scope | multi-node/HA, U2-U4, policy bundle lifecycle tooling | ⬜ | — |

### 6.3 Declaration

- [ ] **v1 single-node is REVIEW-READY for the current cycle.**
- [ ] **No new P0/P1 blockers identified.** All blockers inherited from prior cycles are documented in `11-remaining-tasks.md`.
- [ ] **Support contract boundary upheld.** No claims beyond T1/T2/T3 scope.

### 6.4 Review Metadata

| Field | Value |
|-------|-------|
| Review date | — |
| Reviewer(s) | — |
| Prior review date | 2026-04-08 (G-E5 sign-off) |
| Prior review doc | `43-production-readiness-signoff.md` |
| New blockers found | — |
| Items requiring next-cycle refresh | — |

---

## Quick Reference — Prior Cycle Evidence Sources

| Evidence | File | Key Lines |
|----------|------|-----------|
| RC gates passed | `25-v1-single-node-rc-evidence.md` | Evidence 1, 2, 8 |
| Support contract | `19-v1-single-node-support-contract.md` | Section 0, 1, 7 |
| Production readiness assessment | `23-production-readiness-assessment.md` | Verdict, Dimension 1–5 |
| Production roadmap | `30-production-roadmap.md` | G-E1 through G-E5 |
| Performance baseline | `42-p2-performance-baseline-evidence.md` | Baseline results table |
| Production sign-off | `43-production-readiness-signoff.md` | Decision, Gate Evidence Summary |
| Operational drill evidence | `31-p3-g3-backup-restore-drill-evidence.md` | Full drill |
| Smoke stability evidence | `35-p3-g2-executed-evidence.md` | 100% pass rate |
| Observability evidence | `32-p3-g4-observability-verification-evidence.md` | All probes 200 |
| Functional walkthrough | `34-p3-g1-executed-evidence.md` | End-to-end operator walkthrough |

---

**Inherited from prior cycles (not re-verified today):**
- All G-E1 through G-E5 gate evidence (2026-04-08)
- All P3 drill evidence (2026-04-03)
- All RC evidence from `25-v1-single-node-rc-evidence.md` (2026-04-02)
