# 31 — Release Paths: Todo/Checklist Plan

Single-node v1 scope. Three mutually exclusive post-P6 decision paths with
detailed checklists, owners, gates, evidence references, risks, and
rollback/abort criteria.

**No full production-ready claim is made anywhere in this document.** RC status
is `RC-ready` per `23-production-readiness-assessment.md`. Full production
posture requires operator signoff (Path 2) and Phase 3/operational gates
(Path 3) to be satisfied explicitly.

---

## Path 1 — RC Release (Tag + Release Notes)

> **Status**: Complete. v0.1.0-rc.1 published as GitHub prerelease at target commit `5fce844d2850be45268db37544f17dd4dba988a9`.

Cut a v1 RC tag for the single-node SQLite release candidate. No production
deployment implied. No PostgreSQL/multi-node scope.

### Owner
Release engineer / documentation owner.

### Gate (go/no-go before cutting tag)

> **Footnote**: G1 observed PASS as of 2026-04-28 (see [`53-rc-tag-checklist.md`](./53-rc-tag-checklist.md) §Latest RC Prep Verification Observed). All G1 gates re-verified and PASS immediately before tagging.

| # | Gate criterion | Evidence | Status |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | Fresh P6 validation (2026-04-28) | ☑ PASS |
| G1.2 | `cargo fmt --all --check` passes | Fresh P6 validation | ☑ PASS |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | Fresh P6 validation | ☑ PASS |
| G1.4 | `cargo test --workspace` passes (~797 tests) | Fresh feature-completeness validation | ☑ PASS |
| G1.5 | `scripts/generate_rc_evidence.py` passes all five checks | `docs/artifacts/2026-03-30/05-contract-consistency.txt` or fresh run | ☑ PASS |
| G1.6 | `scripts/validate_repo_layout.sh` passes | "Repository layout looks OK" | ☑ PASS |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | "VALIDATION PASSED" | ☑ PASS |

### Evidence references (preserve existing P6 links)
- `25-EV-v1-single-node-rc-evidence.md` — canonical RC evidence record
- `23-production-readiness-assessment.md` — RC-ready declaration with all dimensions verified
- `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` — 12 VERIFIED / 0 PARTIAL / 0 INFERRED
- `27-production-evaluation-plan.md` Dimension 5 — Release Confidence checklist

### Accepted risks (must appear in release notes)
| Risk | Reference |
|---|---|
| SQLite single-node write throughput ceiling (~300 writes/s sustained) | `27-production-evaluation-plan.md` §1.2 |
| No PostgreSQL/multi-node/HA in scope | ADR-50; `30-production-roadmap.md` §3 |
| Phase 2 transaction batching reverted — Phase 1 write queue is production target | `30-production-roadmap.md` §2 |
| `ferrumctl backup` bounded offline workflow with opt-in retention pruning (`--retention-days N`); no automated scheduling, no encryption | `27-production-evaluation-plan.md` §3.5 |
| Compensate may be noop-backed depending on adapter implementation | `27-production-evaluation-plan.md` §3.6 |
| Health endpoints are shallow; functional probe required for readiness | `27-production-evaluation-plan.md` §4.2 |

### Todo checklist (Path 1 — Historical Completed)
- [x] Re-run all G1 gates immediately before tagging
- [x] Verify/update CHANGELOG: document all P0/P1/P2 resolutions (scope-mismatch deny, poisoned-context fixtures, Phase F docs pack, clippy clean, RC script)
- [x] Verify/update RELEASE notes: explicitly state single-node SQLite scope, Phase 3 deferred, conditional production posture
- [x] Include accepted-risks table from above in release notes
- [x] Include signoff language: "This is an RC tag for v1 single-node SQLite. Production deployment requires evaluation against `27-production-evaluation-plan.md` and explicit operator signoff."
- [x] Do NOT claim production-ready in release notes
- [x] Do NOT bump Cargo.toml version; `Cargo.toml` remains `0.1.0`
- [x] Publish CHANGELOG.md and RELEASE.md as release-facing documentation

### Rollback / abort criteria
| Trigger | Action |
|---|---|
| Any G1 gate fails on final verification | Abort RC tag; resolve gate failure first |
| Integration test regression detected | Abort RC tag; regression is P0 blocker |
| New scope-mismatch or governance regression | Abort RC tag; revert/fix before proceeding |

---

## Path 2 — Conditional Production Pilot

Deploy v1 single-node SQLite to a limited production target with explicit
operator signoff. Preserves conditional single-node SQLite posture.
PostgreSQL/multi-node deferred.

> **G2 Gate Ownership Note**: All G2 gates (G2.1–G2.8) are **operator-owned** and **operator signoff still required** before any production pilot begins. Structured pre-fill templates for each gate are provided in the **G2 Evidence Packet Appendix** (`54-operator-signoff-packet.md` Appendix). These templates are **repo-side tooling validation only** and do not substitute for explicit operator acknowledgment. Do not mark G2 items complete on behalf of the operator.

### Owner
Operator / site reliability / deployment authority.

### Gate (go/no-go before production pilot)
| # | Gate criterion | Evidence | Owner |
|---|---|---|---|
| G2.1 | Write workload modeled against SQLite single-node capacity (≤300 writes/s sustained) | Operator signoff per `27-production-evaluation-plan.md` §Operator Signoff Packet §1 + Template 1 | Operator |
| G2.2 | Bearer auth configured; TLS/reverse proxy confirmed | Operator signoff per `27-production-evaluation-plan.md` §Operator Signoff Packet §2 | Operator |
| G2.3 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` | Operator |
| G2.4 | Restore drill completed; `PRAGMA integrity_check` passes on restored DB | Operator evidence per `27-production-evaluation-plan.md` §Operator Signoff Packet §3 + Template 3 | Operator |
| G2.5 | RPO/RTO formally accepted for target workload | Operator signoff per `27-production-evaluation-plan.md` §Operator Signoff Packet §3 + Template 3 | Operator |
| G2.6 | All production evaluation dimensions SATISFIED or CONDITIONAL | `27-production-evaluation-plan.md` Evaluation Decision Framework completed + Template 2 | Operator |
| G2.7 | Accepted risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed + Template 5 | Operator |
| G2.8 | Compensate noop risk formally accepted | Operator acknowledges compensate may be noop-backed for target adapters + Template 4 | Operator |

**No production pilot begins until all G2 items are satisfied with documented operator signoff.** Do not mark G2 items complete on behalf of the operator. All G2 gates remain **operator-owned/pending** — see `54-operator-signoff-packet.md` Appendix for structured pre-fill templates (repo-side tooling validation only).

### Evidence references
- `27-production-evaluation-plan.md` — canonical production evaluation framework; Operator Signoff Packet in §309–385
- `27-production-evaluation-plan.md` §Engineer-Side Pre-Fill Table — advisory repo-side pre-fill (operator signoff still required)
- `54-operator-signoff-packet.md` Appendix — G2 Evidence Packet Templates (Templates 1–5: workload model, evaluation framework pre-fill, restore drill report, compensate behavior matrix, accepted-risk verification checklist); repo-side tooling validation only
- `19-v1-single-node-support-contract.md` — accepted risks §4, support constraints §3
- `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` — Weak Spots 1–4 resolved
- `23-production-readiness-assessment.md` — RC-ready declaration
- `25-EV-v1-single-node-rc-evidence.md` — evidence record
- `61-path-2-execution-plan.md` — ordered Path 2 execution plan: D1–D6 runner, restore drill, backup scheduler, TLS/reverse proxy, Phase 3 decision gate

### Workload-fit review checklist
- [ ] Confirm expected sustained write rate ≤300 writes/s
- [ ] Confirm single-node topology (no HA/replica/multi-node required)
- [ ] Confirm bounded execution history is acceptable for target use case
- [ ] Confirm target workflow is in the supported flows list (`25-EV-v1-single-node-rc-evidence.md` Evidence 9)
- [ ] If any of the above do not fit: defer to Path 3 (Phase 3 PostgreSQL)

### Backup/restore/runbook checklist
- [ ] `ferrumctl backup create` scheduled externally (cron, CI job, or manual)
- [ ] Backup retention policy defined (opt-in CLI pruning `--retention-days N` available; full policy management including scheduling remains operator-owned)
- [ ] `ferrumctl backup verify` run after each backup to confirm `PRAGMA integrity_check` passes
- [ ] `ferrumctl backup restore` drill performed in non-production environment
- [ ] RPO understood: any writes after last backup timestamp are lost on restore
- [ ] RTO understood: restore time + restart + verification; no automated recovery in FerrumGate
- [ ] Compensate behavior verified for target adapters: confirm whether compensate is noop-backed or performs real undo

### Rollback / abort criteria
| Trigger | Action |
|---|---|
| Write throughput exceeds Phase 1 capacity | Abort pilot; migrate to Path 3 PostgreSQL |
| Backup restore drill fails | Abort pilot deployment; fix backup procedure |
| RPO/RTO does not meet target workload SLA | Abort pilot; evaluate whether Phase 3 PostgreSQL needed |
| Any G2 signoff item declined by operator | Abort pilot; resolve or formally accept risk |
| Compensate noop risk is unacceptable for target adapters | Abort pilot; adapter implementation required before production use of R1/R2/R3 |

### PostgreSQL / multi-node status in Path 2
PostgreSQL and multi-node are **not in scope** for Path 2. Per `27-production-evaluation-plan.md` §Operator Signoff Packet §4: "Operator acknowledges PostgreSQL/multi-node is deferred and not part of the current production pilot scope." If PostgreSQL is needed, proceed to Path 3.

### Pilot Runbook (Path 2 — Conditional Production Pilot)

#### Pilot Start Conditions
All of the following must be confirmed before the first production pilot deployment:

| # | Condition | Verification |
|---|---|---|
| 1 | All G2 gates satisfied with documented operator signoff | `54-operator-signoff-packet.md` completed |
| 2 | Write workload modeled against SQLite capacity (≤300 writes/s sustained) | Operator evidence |
| 3 | Bearer auth configured; TLS/reverse proxy confirmed | Operator configuration review |
| 4 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` |
| 5 | Restore drill completed with `PRAGMA integrity_check` passing | Operator evidence |
| 6 | RPO/RTO formally accepted for target workload | Operator signoff |
| 7 | All production evaluation dimensions SATISFIED or CONDITIONAL | `27-production-evaluation-plan.md` Evaluation Decision Framework |
| 8 | Accepted risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed |
| 9 | Compensate noop risk formally accepted | Operator acknowledgment |

#### Daily Pilot Checks
| Check | Frequency | Threshold | Action if Exceeded |
|---|---|---|---|
| `GET /v1/readyz/deep` returns HTTP 200 | Daily or per deployment cycle | HTTP 503 indicates store unreachable | Investigate store connectivity; restore from backup if corruption suspected |
| Backup verify (`ferrumctl backup verify`) passes | After each backup | `PRAGMA integrity_check` failure | Do not use backup; take new backup after resolving write issues |
| Error rate on S4/S5/S6/S7 scenarios | Per monitoring interval | >0% error rate | Page on-call; evaluate against abort criteria |
| Write queue depth / lag | Per monitoring interval | Sustained backlog >100 items | Evaluate write throughput fit; consider Path 3 if sustained >300 writes/s |
| Disk space for SQLite store | Daily | <10% free on store volume | Alert; risk of DB lock or crash |

#### Monitoring Thresholds
| Metric | Warning | Critical | Go/No-Go |
|---|---|---|---|
| Sustained write rate | >200 writes/s | >250 writes/s | >300 writes/s triggers Path 3 evaluation |
| p50 write latency | >50ms | >100ms | >200ms triggers Path 3 evaluation |
| Error rate (any scenario) | >0.1% | >0% | >0% on S4/S5/S6/S7 = abort pilot |
| Backup verify | N/A | `PRAGMA integrity_check` fail | Do not deploy; fix before proceeding |

#### Pilot Abort Triggers
| Trigger | Action |
|---|---|
| Write throughput exceeds Phase 1 capacity (>300 writes/s sustained) | Abort pilot; migrate to Path 3 PostgreSQL |
| `PRAGMA integrity_check` fails on any backup or store | Abort pilot; restore from last known-good backup |
| Error rate >0% on S4/S5/S6/S7 stress scenarios | Abort pilot; investigate regression |
| RPO/RTO no longer meets target workload SLA | Abort pilot; evaluate Path 3 |
| Any G2 signoff item declined by operator | Abort pilot; resolve or formally accept risk |
| Compensate noop risk is unacceptable for target adapters | Abort pilot; adapter implementation required before R1/R2/R3 use |
| SQLite store corruption or data integrity failure | Abort pilot; restore from backup and investigate |

#### Pilot Completion Criteria
| # | Criterion | Evidence Required |
|---|---|---|
| 1 | Pilot workload successfully processed for the agreed evaluation period | Operator logs / monitoring data |
| 2 | All governance behaviors (scope-mismatch deny, single-use capability, rollback/compensate) verified for pilot workflow | Integration test evidence or manual verification log |
| 3 | Backup/restore drill completed successfully in pilot environment | Operator evidence with `PRAGMA integrity_check` passing |
| 4 | No abort triggers encountered during pilot period | Operator incident log |
| 5 | Operator formally accepts pilot outcome and recommends proceeding to Path 3 OR declares pilot sufficient for bounded single-node production | Signed completion statement per `54-operator-signoff-packet.md` |

#### Decision Log
| Date | Decision | Owner | Rationale |
|---|---|---|---|
| (fill in) | Pilot started | Operator | Reason for pilot scope and target workload |
| (fill in) | Abort / Continue / Complete | Operator | Evidence-based assessment against abort/completion criteria |
| (fill in) | Proceed to Path 3 or claim single-node production | Operator + Engineering lead | Based on pilot outcome and production requirements |

**No production-ready claim is made during the pilot period.** FerrumGate v1 remains RC-ready/conditional. Full production posture requires Path 3 completion (Phase P1–P4) or explicit documented acceptance of single-node SQLite constraints for the target workload.

---

## Path 3 — Phase 3 PostgreSQL / P5 Production Readiness

P1–P4.4 PostgreSQL StoreFacade implementation and local Docker/runtime validation
are complete per ADR-50. P5 production readiness is deferred and gated.
P5a (design/ADR review) is the next authorized phase; P5b–P5e implementation
requires G2 pilot data, refreshed G3 gates, and operator D1–D3 signoff.
Full production scale, HA, multi-node, and production-grade PostgreSQL remain
explicitly deferred. No production-ready claim is made.

### Owner
Engineering lead / architect.

### Gate (go/no-go before beginning P5a)
| # | Gate criterion | Evidence | Owner | Status |
|---|---|---|---|---|
| G3.1 | v1 RC tag cut and Path 1 complete | RC tag `v0.1.0-rc.1` at commit `5fce844d`; GitHub prerelease published | Release engineer | ☑ DONE |
| G3.2 | Conditional single-node SQLite pilot signed (doc59/doc54) | Operator signoff per `27-production-evaluation-plan.md` | Operator | ☑ DONE (conditional pilot only; not full production) |
| G3.3 | P1–P4.4 local Docker/runtime implementation complete | ADR-50 §6 summary table; `cargo test --workspace` with `--features postgres` passes | Engineering lead | ☑ DONE (local Docker/runtime; not production deployment) |
| G3.4 | ADR-50 P5a design review approved | `50-p4-postgres-store-facade-adr.md` §3.5 P5a + `104-g3-4-p5a-adr-approval-packet.md` | Engineering lead | ☑ DONE (P5a design only; P5b–P5e still gated) |
| G3.5 | Operator D1–D3 signoff obtained for P5b–P5e | `105-g3-5-operator-d1-d3-signoff-packet.md` | Operator | ☐ Pending (ready for operator review) |
| G3.6 | G2 pilot data available for P5b pool-tuning input | Path 2 pilot metrics/logs (sustained write rate, connection patterns) | Operator | ☐ Pending |

> **Phase naming note**: ADR-50 uses "Phase P1–P4" for PostgreSQL implementation stages and "Phase P5" for production readiness. P5 is split into P5a (design/ADR) and P5b–P5e (implementation). This document's "Phase 3" maps to ADR-50 Phase P1 start through P5e completion. P5a is the only currently authorized phase.

### Phase P1 checklist (PostgreSQL migrations + testcontainer strategy)
Per `50-p4-postgres-store-facade-adr.md` §3 Phase P1:
- [x] Enable `sqlx::postgres` feature flag in `Cargo.toml`
- [x] Create `PostgresStore` skeleton with real repo implementations
- [x] Define migration strategy (embedded SQL migrations for PostgreSQL compatibility)
- [x] Add container test infrastructure (live postgres integration tests)
- [x] All P1 deliverables code-reviewed and passing CI

### Phase P2–P4 checklist (StoreFacade implementation + migrations)
- [x] Implement all nine PostgresStore repos (Intent, Proposal, Capability, Execution, Rollback, Approval, Provenance, Ledger, PolicyBundle)
- [ ] Adapt write queue architecture for PostgreSQL concurrency model — **deferred**; no v1 PostgreSQL write queue (SQLite write queue remains the v1 path)
- [x] Implement embedded migration runner for postgres
- [x] Data integrity validation: P4.4 MVP migration CLI with count+ID validation complete — **content-hash/production equivalence deferred to P5c/P5e**
- [x] Integration tests with live postgres pass
- [x] Benchmark validation: ≥1000 writes/s sustained throughput confirmed (local Docker only; not a production benchmark)

### Phase P5 checklist (Production Readiness — Design Only)

> **P5 scope**: P5a is GO for design/ADR only. P5b–P5e implementation is gated on G3.4–G3.6 and operator D1–D3 signoff. No P5 implementation begins until G3.4 is satisfied.

#### P5a — Design / ADR Review (Authorized)
- [ ] P5a ADR/design doc approved with D1–D6 decisions documented
- [ ] Risk register updated with P5-specific risks (pool exhaustion, failover gaps, backup inconsistency)
- [ ] Verification gates defined for P5b–P5e
- [ ] Non-claims language reviewed and preserved (no production-ready, no HA, no multi-node)
- [ ] Operator decision framework D1–D6 drafted and ready for signoff

#### P5b — Connection Pool Tuning (Implementation Gated)
- [ ] Pool size model validated against pilot workload data (G2 metrics)
- [ ] `max_connections`, `min_idle`, `acquire_timeout` tuned for target throughput
- [ ] Connection-leak detection and circuit-breaker behavior defined
- [ ] **Blocked until**: G3.6 pilot data available

#### P5c — Backup / Restore for PostgreSQL (Implementation Gated)
- [ ] `pg_dump`/`pg_restore` or logical-replication backup strategy documented
- [ ] Backup automation design (external scheduler, not in-tree)
- [ ] Restore drill procedure for PostgreSQL defined
- [ ] RPO/RTO targets for PostgreSQL documented and operator-accepted
- [ ] **Blocked until**: G3.5 operator D1–D3 signoff

#### P5d — HA / Clustering Design (Implementation Gated)
- [ ] HA topology reviewed (read replica, failover, partitioning)
- [ ] Multi-node deployment validated in staging (not production)
- [ ] StoreFacade concurrency model adapted for multi-node (if required)
- [ ] **Blocked until**: G3.5 operator D1–D3 signoff; explicitly out of v1 scope

#### P5e — Migration Grade-Up (Implementation Gated)
- [ ] SQLite → PostgreSQL migration upgraded from MVP to production-grade
- [ ] Idempotent/resumable migration with checkpointing
- [ ] Content-hash validation for lineage equivalence
- [ ] Large-dataset streaming and chunking
- [ ] **Blocked until**: G3.5 operator D1–D3 signoff; P5c backup/restore design complete

> **P5 completion does not claim production-ready.** Even after P5a–P5e complete, a later P6 assessment is required before any full production-ready claim. PostgreSQL production deployment remains operator-owned and gated.

### Evidence references
- `50-p4-postgres-store-facade-adr.md` — phased implementation plan; this is the canonical reference
- `30-production-roadmap.md` §3 — Phase 3 PostgreSQL migration path
- `27-production-evaluation-plan.md` — production evaluation framework to re-run after PostgreSQL is operational
- `23-production-readiness-assessment.md` — to be refreshed after Phase 3 complete

### Rollback / abort criteria
| Trigger | Action |
|---|---|
| Phase P1 infrastructure fails container test setup | Abort Phase P1; resolve test infrastructure |
| PostgresStore repo implementation has fundamental design conflict with StoreFacade trait | Abort Phase P3; redesign StoreFacade abstraction first |
| Benchmark validation fails to reach ≥1000 writes/s | Abort Phase P3; evaluate alternative approaches (connection pooling tuning, batch inserts, or different architecture) |
| Data integrity validation fails (SQLite → PostgreSQL migration produces divergent state) | Abort Phase P3; fix migration before proceeding |
| Engineering capacity exhausted before Phase P3 complete | Evaluate Path 2 continuation; do not claim PostgreSQL support until all repos implemented and tested |

### What Phase 3 / P5 is NOT
- Phase 3 is **NOT** an extension of the v1 RC tag
- Phase 3 is **NOT** a minor feature addition (~2000–3000 LOC + migrations + container tests)
- Phase 3 is **NOT** covered by the current v1 single-node support contract
- P5a is **NOT** P5 implementation; design/ADR only
- P5 completion is **NOT** a production-ready claim; P6 assessment required afterward
- PostgreSQL production deployment is **NOT** authorized by P1–P4.4 completion; only local Docker/runtime is implemented
- Starting P5a does not imply v1 is production-ready; v1 RC tag remains a candidate requiring operator signoff

### Post-P5 go/no-go (before claiming full production-ready)
| # | Gate criterion | Evidence | Owner |
|---|---|---|---|
| G3.P5.1 | P5a ADR/design review approved with D1–D6 decisions | `50-p4-postgres-store-facade-adr.md` §3.5 P5a signoff | Engineering lead |
| G3.P5.2 | P5b–P5e implementation complete and integration-tested | `cargo test --workspace` passes with postgres feature | Engineering |
| G3.P5.3 | Production evaluation framework re-run and all dimensions SATISFIED or CONDITIONAL | Fresh run of `27-production-evaluation-plan.md` Evaluation Decision Framework | Operator |
| G3.P5.4 | Backup/restore validated for PostgreSQL | Operator drill with `pg_dump`/`pg_restore` or equivalent | Operator |
| G3.P5.5 | RPO/RTO confirmed for target workload with PostgreSQL | Operator signoff | Operator |
| G3.P5.6 | Multi-node / HA topology reviewed and capacity planned if required | Site reliability / architecture review | SRE / Architect |
| G3.P5.7 | P6 assessment completed after P5e | Separate P6 production-readiness assessment (not part of P5) | Engineering + Operator |

**Full production-ready claim only after G3.P5.1–G3.P5.7 are satisfied. P5 completion alone does not claim production-ready.**

---

## Cross-Link Index

| From | To | Purpose |
|---|---|---|
| `27-production-evaluation-plan.md` §Decision Tree | This doc | Path 1/2/3 decision framework |
| `23-production-readiness-assessment.md` §Verdict | This doc | Release paths for RC-ready declaration |
| `11-remaining-tasks.md` §P3 backlog | This doc | Phase 3 PostgreSQL as post-v1 path |
| `30-production-roadmap.md` §Phase 3 | This doc | Phase 3 go/no-go gates |
| `50-p4-postgres-store-facade-adr.md` §3 Phase P1 | This doc | Phase 3 entry criteria |
| `25-EV-v1-single-node-rc-evidence.md` | This doc | Path 1 evidence preservation |
| `32-feature-completeness-audit.md` | This doc | Route/API reconciliation for v1 boundary audit |
| `CHANGELOG.md` | `RELEASE.md` | RC candidate changelog and release notes |
| `RELEASE.md` | This doc | Pre-tag checklist for Path 1 RC release |
| `RELEASE.md` | `25-EV-v1-single-node-rc-evidence.md` | RC evidence cross-reference |
| `RELEASE.md` | `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` | Invariant matrix cross-reference |
| `RELEASE.md` | `27-production-evaluation-plan.md` | Production evaluation framework cross-reference |
| `RELEASE.md` | `56-adapter-compensation-evidence-matrix.md` | Adapter compensation evidence cross-reference |
| `RELEASE.md` | `57-workload-compensation-drill-plan.md` | Workload compensation drill plan cross-reference |
| `RELEASE.md` | `58-workload-compensation-drill-evidence-template.md` | Drill evidence template cross-reference |
| `RELEASE.md` | `59-pilot-readiness-evidence-packet.md` | G2.1–G2.8 evidence packet cross-reference |
| `RELEASE.md` | `60-bounded-hardening-examples.md` | Bounded hardening examples cross-reference |
| `100-phase3f-conditional-sqlite-pilot-authorization.md` | This doc | Conditional pilot authorization with G2 signed status |
| `50-p4-postgres-store-facade-adr.md` §3.5 P5a | This doc | P5a design/ADR review entry criteria |
| `104-g3-4-p5a-adr-approval-packet.md` | This doc | G3.4 approval packet (ready for approval) |
| `105-g3-5-operator-d1-d3-signoff-packet.md` | This doc | G3.5 operator D1–D3 signoff packet (ready for operator review) |

---

## Summary

| Path | Goal | Key constraint |
|---|---|---|
| **1 — RC Release** | Cut v1 RC tag + release notes | No production claim; single-node SQLite only |
| **2 — Production Pilot** | Limited production deployment with operator signoff | All G2 gates must be operator-signed before pilot begins; PostgreSQL deferred |
| **3 — Phase 3 PostgreSQL / P5** | P1–P4.4 local Docker/runtime complete; P5a design authorized; P5b–P5e implementation gated | G3 gates required before P5a starts; P5 completion ≠ production-ready; P6 assessment required |

**No path claims full production-ready status.** Path 1 is RC tag only. Path 2 is conditional pilot requiring operator signoff. Path 3 P1–P4.4 is local Docker/runtime only; P5a is design-only; P5b–P5e implementation is gated; P6 assessment required before any full production-ready claim.

---

*Document updated: 2026-05-11. P1–P4.4 completion reflected; P5a entry criteria added; G3 gates refreshed. Grounded in P6 evidence base and existing implementation-path docs.*
