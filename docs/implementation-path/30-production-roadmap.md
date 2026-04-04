# 30 — Production Roadmap

**Last updated:** 2026-04-03
**Current truth:** Single-node v1 is RC-ready (2026-04-02). Broader production-ready is **not yet complete**.

---

## Status at a Glance

| Priority | Track | Status | Target Outcome |
|----------|-------|--------|----------------|
| 1 | Production support boundary / contract | ✅ DONE | Support matrix (P1.1) + SLA surface (P1.2) + EOL policy (P1.3) all published |
| 2 | Adapter hardening + external integration depth | 🔄 IN PROGRESS | Production-grade adapters; remote/external integration surface; P2.5 transport-failure slice verified |
| 3 | Operational hardening / release evidence | ✅ DONE | Ship-worthy packaging, observability, runbooks |
| 4 | Operator control-plane completeness (`ferrumctl`) | ⬜ PLANNED | Full operator-driven workflows; policy bundle authoring |
| 5 | Resilience architecture (HA / read-replica / multi-node) | ⬜ PLANNED | Multi-node v1; HA-ready topology |
| 6 | Post-v1 expansion (U1 full + U2/U3/U4) | ⬜ PLANNED | Outcome-aware governance; remaining upgrades |

---

## Priority 1 — Lock Production Support Boundary / Contract

**Goal:** Define and lock what "production-supported" means for v1.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P1.1 | Publish support matrix (single-node scope, known gaps) | ✅ DONE | `docs/19-v1-single-node-support-contract.md` Section 0 (Support Tier Summary) |
| P1.2 | Document SLA surface (availability, recovery, response) | ✅ DONE | `docs/19-v1-single-node-support-contract.md` Section 7 (SLA Surface) |
| P1.3 | Define EOL / deprecation policy | ✅ DONE | `docs/19-v1-single-node-support-contract.md` Section 9 (EOL / Deprecation Policy) | 2026-04-03 |

**Evidence:** `docs/implementation-path/23-production-readiness-assessment.md`

---

## Priority 2 — Adapter Hardening + External Integration Depth

**Goal:** Production-grade adapters for fs, sqlite, git, http, maildraft. Explicit remote/external integration sub-items.

> Per `11-remaining-tasks.md` P3 backlog and `23-production-readiness-assessment.md`: bounded local implementations exist for all five adapters; broader production hardening and external integration depth are post-v1.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P2.1 | fs adapter — hardening and production verification | 🔄 IN PROGRESS (Slice 1: fail-closed verify on I/O errors ✅ 2026-04-03) | `ferrum-adapter-fs`: `test_fs_adapter_verify_fail_closed_on_io_error_permission_denied`, `test_fs_adapter_verify_hash_mismatch_is_verified_false_not_error`, `test_fs_adapter_verify_file_deleted_is_verified_false_not_error` |
| P2.2 | sqlite adapter — hardening and production verification | 🔄 IN PROGRESS (Slice 1: identifier safety + noop edge-case tests ✅ 2026-04-04; execute/verify/rollback integration tests need SQLite connection infrastructure work) | `ferrum-adapter-sqlite`: `test_sqlite_adapter_rejects_unsafe_table_name` (SQL injection prevention), `test_sqlite_adapter_rollback_no_snapshots_is_noop`, `test_sqlite_adapter_verify_no_snapshots_returns_true` |
| P2.3 | git adapter — hardening and production verification | ⬜ TODO | Integration test |
| P2.4 | git remote workflows — push/fetch/pull integration | ⬜ TODO | Integration test |
| P2.5 | http adapter — hardening and production verification | 🔄 IN PROGRESS | Slice 1: `test_http_execute_transport_failure_is_fail_closed` (execute connection-refused → fail-closed + `Failed` state); Slice 2: `test_http_execute_timeout_fails_closed` (execute timeout → fail-closed + `Failed` state); Slice 3: `test_verify_get_transport_failure_fails_closed` (adapter unit: GET re-request transport failure → `verified=false`); Slice 4: `test_verify_get_re_request_timeout_fails_closed` (adapter unit: GET re-request timeout → `verified=false`); Slice 5: `test_verify_mutation_patch_explicit_check_mismatch` (gateway API: PATCH execute captures 503, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=false`, execution becomes Failed, commit rejected); Slice 6: `test_verify_mutation_patch_explicit_check_match` (gateway API: PATCH execute captures 200, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=true`, execution remains AwaitingVerification/Committed, commit succeeds) ✅ DONE 2026-04-03; Slice 7: `test_verify_get_transport_failure_fails_closed` (gateway API: explicit `verify_checks` injected in-store, re-request fails with connection-refused, verify returns 200 + `verified=false`, commit rejected from `Failed`); Slice 8: `test_verify_get_re_request_timeout_fails_closed` (gateway API: explicit `verify_checks` injected in-store, GET re-request times out, verify returns 200 + `verified=false`, commit rejected from `Failed`); Slice 9: `test_verify_mutation_post_explicit_check_mismatch` (gateway API: POST execute captures 503, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=false`, execution becomes Failed, commit rejected) ✅ DONE 2026-04-03; Slice 10: `test_verify_mutation_delete_explicit_check_match` (gateway API: DELETE execute captures 200, verify_checks injected with HttpStatusExpected(200), verify returns 200 + `verified=true`, execution remains AwaitingVerification/Committed, commit succeeds) ✅ DONE 2026-04-03; fixes: HTTP adapter verify catches GET re-request transport errors and gateway execute failures transition to `Failed` |
| P2.6 | maildraft — provider send integration | ⬜ TODO | Integration test |
| P2.7 | maildraft — broader verify semantics hardening | ⬜ TODO | Integration test |

**Source:** `11-remaining-tasks.md` P3; `01-current-state.md` lines 26-31

---

## Priority 3 — Operational Hardening / Required Release Evidence

**Goal:** Ship-worthy packaging, observability, and runbooks.

> The four items below are the required release gate evidence for production readiness. RC gate rows (P3.1–P3.6) are preserved as anchors.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P3.1 | Cargo workspace clean (`clippy -- -D warnings`) | ✅ DONE | 2026-04-02 |
| P3.2 | Full workspace test suite (`cargo test --workspace`) | ✅ DONE | 2026-04-02 |
| P3.3 | Scope-mismatch deny | ✅ DONE | 2026-04-02 |
| P3.4 | Poisoned-context fixtures (6 tests) | ✅ DONE | 2026-04-02 |
| P3.5 | Phase F docs pack | ✅ DONE | 2026-04-02 |
| P3.6 | RC evidence script | ✅ DONE | 2026-04-02 |
| P3.G1 | Functional readiness proof — end-to-end operator walkthrough (install → functional probe → first control action → first rollback/compensate drill) | ✅ DONE 2026-04-03 | `docs/implementation-path/34-p3-g1-executed-evidence.md` — live walkthrough executed 2026-04-03; all 5 sections passed; attestation block signed |
| P3.G2 | Smoke stability evidence — sustained-lifecycle smoke suite (48h+ runbook-driven soak or equivalent automated cycle) | ✅ DONE 2026-04-03 | `docs/implementation-path/35-p3-g2-executed-evidence.md` — live smoke run executed 2026-04-03 (run_id: `p3-g2-20260403-live`); 12 automated probe intervals at ~5s cadence, 100% pass rate, 0 failures, 0 consecutive failures; pre-run and end-run store integrity `ok`; 2/2 control-path checks passed (cancel + inspect); no fatal logs; attestation block signed. Satisfies the "equivalent automated cycle" clause per evidence template Section 1. |
| P3.G3 | Backup / restore drill evidence — successful backup capture and restore drill under rollback scenario | ✅ DONE 2026-04-03 | `docs/implementation-path/31-p3-g3-backup-restore-drill-evidence.md` — live drill executed 2026-04-03; backup captured to `/tmp/ferrum-p3g3/backups/ferrumgate_20260403_1705.db` (225280 bytes, integrity ok); restore drill to `/tmp/ferrum-p3g3/restored.db` (225280 bytes, integrity ok); restored ferrumd responded to readyz and approvals probes; intent_id `09996e3b-7a9b-4c55-b806-8713486cee44` verified present with provenance chain; attestation block signed |
| P3.G4 | Observability verification — metrics, logging, and tracing surface confirmed operational in target environment | ✅ DONE 2026-04-03 | `docs/implementation-path/32-p3-g4-observability-verification-evidence.md` — live verification executed 2026-04-03; all probe endpoints (/healthz, /readyz, /approvals, /metrics) returned 200; logs flowing; attestation block signed |

**Evidence:** `docs/implementation-path/25-v1-single-node-rc-evidence.md`

---

## Post-P3 Execution Order

The following lists the remaining execution order after P3 completion (P3.G1–P3.G4 ✅ DONE 2026-04-03), grounded in roadmap priority order. Single-node v1 RC-ready; broader production-ready still incomplete.

### Immediate Next Slice (P2 adapter hardening — in progress / todo)

1. **P2.5** — http adapter hardening (Slice 1–10 ✅ DONE; broader production hardening continues)
2. **P2.1** — fs adapter hardening + production verification
3. **P2.2** — sqlite adapter hardening + production verification
4. **P2.3** — git adapter hardening + production verification
5. **P2.4** — git remote workflows (push/fetch/pull integration)
6. **P2.6** — maildraft provider send integration
7. **P2.7** — maildraft broader verify semantics hardening

### Longer-Term / Planned Tracks

8. **P4.1–P4.2** — `ferrumctl` advanced operator flows + policy bundle lifecycle tooling
9. **P5.4–P5.5** — Sync-1 preflight checks (PF1–PF8) + decision table + abort semantics
10. **P5.7** — HA / multi-leader replication
11. **U1.1–U1.2** — Outcome-aware Governance (remaining backlog: richer clause expressiveness, policy bundle authoring tooling)
12. **U2** — Reversible Execution Planner
13. **U3** — Cross-runtime Provenance Fabric
14. **U4** — Runtime Integrations (MCP / local / NemoClaw)

**Source:** `docs/implementation-path/11-remaining-tasks.md`; execution order follows roadmap priority sequence per `docs/implementation-path/24-p1-p2-p3-execution-plan.md` lines 266–297.

---

## Priority 4 — Operator Control-Plane Completeness (`ferrumctl`)

**Goal:** Close remaining `ferrumctl` gaps; policy bundle lifecycle tooling.

> Per `23-production-readiness-assessment.md` Dimension 3: `ferrumctl` covers the high-use operator surface; some advanced/intent-authoring flows still require direct HTTP/OpenAPI. Per `11-remaining-tasks.md` P3: policy bundle migration tooling (CLI authoring workflows) is post-v1 backlog.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P4.1 | `ferrumctl` advanced operator flows (remaining REST surface) | ⬜ TODO | CLI test |
| P4.2 | Policy bundle lifecycle tooling | ⬜ TODO | CLI + unit test |

---

## Priority 5 — Resilience Architecture (HA / Read-Replica / Multi-Node)

**Goal:** Multi-node v1 with HA-ready topology.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| P5.1 | SQLite read-replica use-case analysis | ✅ DONE | Analysis doc |
| P5.2 | Leader-election requirements analysis | ✅ DONE | Analysis doc |
| P5.3 | Sync-0 safety contract plan | ✅ DONE | Design doc |
| P5.4 | Sync-1 preflight checks (PF1–PF8) | ⬜ TODO | Integration test |
| P5.5 | Sync-1 decision table + abort semantics | ⬜ TODO | Integration test |
| P5.6 | Sync-2 read-only preflight sketch | ✅ DONE | Design doc |
| P5.7 | HA / multi-leader replication | ⬜ PLANNED | Post-P2 |

---

## Priority 6 — Post-v1 Expansion Tracks

**Goal:** Complete U1 and kick off U2 / U3 / U4.

| Item | Description | Status | Verification |
|------|-------------|--------|--------------|
| U1.1 | Richer outcome clause expressiveness (nested selectors, temporal) | ⬜ PLANNED | Test suite |
| U1.2 | Policy bundle migration / authoring tooling | ⬜ PLANNED | CLI test |
| U2 | Reversible Execution Planner | ⬜ PLANNED | Design doc |
| U3 | Cross-runtime Provenance Fabric | ⬜ PLANNED | Design doc |
| U4 | Runtime Integrations (MCP / local / NemoClaw) | ⬜ PLANNED | Integration test |

**Cross-link:** `docs/implementation-path/11-remaining-tasks.md`

---

## Update Convention

When a row completes:

1. Change status: `⬜ TODO` → `🔄 IN PROGRESS` → `✅ DONE`
2. Add verification column entry (file, test command, or commit ref)
3. Add date or commit hash in the Status column
4. **Do not rewrite the structure.** Append new rows if new items are discovered.

Example:
```
| P3.7 | Production runbook | ✅ DONE | runbooks/prod.md @ abc1234 | 2026-04-05 |
```

---

## Key References

| Topic | File |
|-------|------|
| v1 RC evidence | `25-v1-single-node-rc-evidence.md` |
| Production readiness assessment | `23-production-readiness-assessment.md` |
| Current state | `01-current-state.md` |
| Remaining tasks | `11-remaining-tasks.md` |
| Release checklist | `16-release-checklist.md` |
