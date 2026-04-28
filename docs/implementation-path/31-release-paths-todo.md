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

Cut a v1 RC tag for the single-node SQLite release candidate. No production
deployment implied. No PostgreSQL/multi-node scope.

### Owner
Release engineer / documentation owner.

### Gate (go/no-go before cutting tag)

> **Footnote**: G1 observed PASS as of 2026-04-28 (see [`53-rc-tag-checklist.md`](./53-rc-tag-checklist.md) §Latest RC Prep Verification Observed). All G1 gates must be re-verified immediately before tagging.

| # | Gate criterion | Evidence | Status |
|---|---|---|---|
| G1.1 | `cargo check --workspace` passes | Fresh P6 validation (2026-04-28) | Must re-verify |
| G1.2 | `cargo fmt --all --check` passes | Fresh P6 validation | Must re-verify |
| G1.3 | `cargo clippy --workspace --all-targets -- -D warnings` passes | Fresh P6 validation | Must re-verify |
| G1.4 | `cargo test --workspace` passes (~761 tests) | Fresh feature-completeness validation | Must re-verify |
| G1.5 | `scripts/generate_rc_evidence.py` passes all five checks | `docs/artifacts/2026-03-30/05-contract-consistency.txt` or fresh run | Must re-verify |
| G1.6 | `scripts/validate_repo_layout.sh` passes | "Repository layout looks OK" | Must re-verify |
| G1.7 | `python3 scripts/check_contract_consistency.py` passes | "VALIDATION PASSED" | Must re-verify |

### Evidence references (preserve existing P6 links)
- `25-v1-single-node-rc-evidence.md` — canonical RC evidence record
- `23-production-readiness-assessment.md` — RC-ready declaration with all dimensions verified
- `26-v1-single-node-invariant-control-test-evidence-matrix.md` — 12 VERIFIED / 0 PARTIAL / 0 INFERRED
- `27-production-evaluation-plan.md` Dimension 5 — Release Confidence checklist

### Accepted risks (must appear in release notes)
| Risk | Reference |
|---|---|
| SQLite single-node write throughput ceiling (~300 writes/s sustained) | `27-production-evaluation-plan.md` §1.2 |
| No PostgreSQL/multi-node/HA in scope | ADR-50; `30-production-roadmap.md` §3 |
| Phase 2 transaction batching reverted — Phase 1 write queue is production target | `30-production-roadmap.md` §2 |
| `ferrumctl backup` bounded offline workflow only; no automated scheduling, no retention policy | `27-production-evaluation-plan.md` §3.5 |
| Compensate may be noop-backed depending on adapter implementation | `27-production-evaluation-plan.md` §3.6 |
| Health endpoints are shallow; functional probe required for readiness | `27-production-evaluation-plan.md` §4.2 |

### Todo checklist (Path 1)
- [ ] Re-run all G1 gates immediately before tagging
- [ ] Verify/update CHANGELOG: document all P0/P1/P2 resolutions (scope-mismatch deny, poisoned-context fixtures, Phase F docs pack, clippy clean, RC script)
- [ ] Verify/update RELEASE notes: explicitly state single-node SQLite scope, Phase 3 deferred, conditional production posture
- [ ] Include accepted-risks table from above in release notes
- [ ] Include signoff language: "This is an RC tag for v1 single-node SQLite. Production deployment requires evaluation against `27-production-evaluation-plan.md` and explicit operator signoff."
- [ ] Do NOT claim production-ready in release notes
- [ ] Do NOT bump Cargo.toml version (unless explicitly requested later)
- [ ] Do NOT create CHANGELOG.md or RELEASE.md files unless needed for external references

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

### Owner
Operator / site reliability / deployment authority.

### Gate (go/no-go before production pilot)
| # | Gate criterion | Evidence | Owner |
|---|---|---|---|
| G2.1 | Write workload modeled against SQLite single-node capacity (≤300 writes/s sustained) | Operator signoff per `27-production-evaluation-plan.md` §Operator Signoff Packet §1 | Operator |
| G2.2 | Bearer auth configured; TLS/reverse proxy confirmed | Operator signoff per `27-production-evaluation-plan.md` §Operator Signoff Packet §2 | Operator |
| G2.3 | Backup schedule implemented external to FerrumGate | Operator evidence of scheduled `ferrumctl backup create` | Operator |
| G2.4 | Restore drill completed; `PRAGMA integrity_check` passes on restored DB | Operator evidence per `27-production-evaluation-plan.md` §Operator Signoff Packet §3 | Operator |
| G2.5 | RPO/RTO formally accepted for target workload | Operator signoff per `27-production-evaluation-plan.md` §Operator Signoff Packet §3 | Operator |
| G2.6 | All production evaluation dimensions SATISFIED or CONDITIONAL | `27-production-evaluation-plan.md` Evaluation Decision Framework completed | Operator |
| G2.7 | Accepted risks documented (Weak Spots 1–4) | `19-v1-single-node-support-contract.md` §4 reviewed | Operator |
| G2.8 | Compensate noop risk formally accepted | Operator acknowledges compensate may be noop-backed for target adapters | Operator |

**No production pilot begins until all G2 items are satisfied with documented operator signoff.** Do not mark G2 items complete on behalf of the operator.

### Evidence references
- `27-production-evaluation-plan.md` — canonical production evaluation framework; Operator Signoff Packet in §309–385
- `19-v1-single-node-support-contract.md` — accepted risks §4, support constraints §3
- `26-v1-single-node-invariant-control-test-evidence-matrix.md` — Weak Spots 1–4 resolved
- `23-production-readiness-assessment.md` — RC-ready declaration
- `25-v1-single-node-rc-evidence.md` — evidence record

### Workload-fit review checklist
- [ ] Confirm expected sustained write rate ≤300 writes/s
- [ ] Confirm single-node topology (no HA/replica/multi-node required)
- [ ] Confirm bounded execution history is acceptable for target use case
- [ ] Confirm target workflow is in the supported flows list (`25-v1-single-node-rc-evidence.md` Evidence 9)
- [ ] If any of the above do not fit: defer to Path 3 (Phase 3 PostgreSQL)

### Backup/restore/runbook checklist
- [ ] `ferrumctl backup create` scheduled externally (cron, CI job, or manual)
- [ ] Backup retention policy defined and enforced external to FerrumGate
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

---

## Path 3 — Phase 3 PostgreSQL / Full Production Posture

Begin Phase 3 PostgreSQL implementation per ADR-50. Full production scale.
StoreFacade implementation with PostgresStore. Migrations and data-integrity
validation. Go/no-go gates before production claim.

### Owner
Engineering lead / architect.

### Gate (go/no-go before beginning Phase P1)
| # | Gate criterion | Evidence | Owner |
|---|---|---|---|
| G3.1 | v1 RC tag cut and Path 1 complete | RC tag exists; release notes published | Release engineer |
| G3.2 | Production pilot (Path 2) has confirmed single-node SQLite posture is acceptable for target workload | Operator signoff per `27-production-evaluation-plan.md` | Operator |
| G3.3 | Engineering capacity confirmed for ~2000–3000 LOC + migrations + container tests | ADR-50 effort estimate | Engineering lead |
| G3.4 | ADR-50 Phase P1 reviewed and approved to proceed | `50-p4-postgres-store-facade-adr.md` §3 | Engineering lead |

**Do not begin Phase P1 until G3.1–G3.4 are satisfied.**

### Phase P1 checklist (PostgreSQL migrations + testcontainer strategy)
Per `50-p4-postgres-store-facade-adr.md` §3 Phase P1:
- [ ] Enable `sqlx::postgres` feature flag in `Cargo.toml`
- [ ] Create `PostgresStore` skeleton with placeholder repo implementations
- [ ] Define migration strategy (SQLite → PostgreSQL compatibility layer)
- [ ] Add container test infrastructure (Docker Compose for postgres)
- [ ] All P1 deliverables code-reviewed and passing CI

### Phase P2–P4 checklist (StoreFacade implementation + migrations)
- [ ] Implement all nine PostgresStore repos (Intent, Proposal, Capability, Execution, Rollback, Approval, Provenance, Ledger, PolicyBundle)
- [ ] Adapt write queue architecture for PostgreSQL concurrency model
- [ ] Implement embedded migration runner for postgres
- [ ] Data integrity validation: SQLite backup restore to PostgreSQL produces identical lineage and state
- [ ] Integration tests with live postgres pass
- [ ] Benchmark validation: ≥1000 writes/s sustained throughput confirmed

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

### What Phase 3 is NOT
- Phase 3 is **NOT** an extension of the v1 RC tag
- Phase 3 is **NOT** a minor feature addition (~2000–3000 LOC + migrations + container tests)
- Phase 3 is **NOT** covered by the current v1 single-node support contract
- Starting Phase 3 does not imply v1 is production-ready; v1 RC tag remains a candidate requiring operator signoff

### Post-Phase 3 go/no-go (before claiming full production-ready)
| # | Gate criterion | Evidence | Owner |
|---|---|---|---|
| G3.P3.1 | All PostgreSQL repos implemented and integration-tested | `cargo test --workspace` passes with postgres feature | Engineering |
| G3.P3.2 | Production evaluation framework re-run and all dimensions SATISFIED or CONDITIONAL | Fresh run of `27-production-evaluation-plan.md` Evaluation Decision Framework | Operator |
| G3.P3.3 | Backup/restore validated for PostgreSQL | Operator drill with `pg_dump`/`pg_restore` or equivalent | Operator |
| G3.P3.4 | RPO/RTO confirmed for target workload with PostgreSQL | Operator signoff | Operator |
| G3.P3.5 | Multi-node / HA topology reviewed and capacity planned if required | Site reliability / architecture review | SRE / Architect |

**Full production-ready claim only after G3.P3.1–G3.P3.5 are satisfied.**

---

## Cross-Link Index

| From | To | Purpose |
|---|---|---|
| `27-production-evaluation-plan.md` §Decision Tree | This doc | Path 1/2/3 decision framework |
| `23-production-readiness-assessment.md` §Verdict | This doc | Release paths for RC-ready declaration |
| `11-remaining-tasks.md` §P3 backlog | This doc | Phase 3 PostgreSQL as post-v1 path |
| `30-production-roadmap.md` §Phase 3 | This doc | Phase 3 go/no-go gates |
| `50-p4-postgres-store-facade-adr.md` §3 Phase P1 | This doc | Phase 3 entry criteria |
| `25-v1-single-node-rc-evidence.md` | This doc | Path 1 evidence preservation |
| `32-feature-completeness-audit.md` | This doc | Route/API reconciliation for v1 boundary audit |
| `CHANGELOG.md` | `RELEASE.md` | RC candidate changelog and release notes |
| `RELEASE.md` | This doc | Pre-tag checklist for Path 1 RC release |
| `RELEASE.md` | `25-v1-single-node-rc-evidence.md` | RC evidence cross-reference |
| `RELEASE.md` | `26-v1-single-node-invariant-control-test-evidence-matrix.md` | Invariant matrix cross-reference |
| `RELEASE.md` | `27-production-evaluation-plan.md` | Production evaluation framework cross-reference |

---

## Summary

| Path | Goal | Key constraint |
|---|---|---|
| **1 — RC Release** | Cut v1 RC tag + release notes | No production claim; single-node SQLite only |
| **2 — Production Pilot** | Limited production deployment with operator signoff | All G2 gates must be operator-signed before pilot begins; PostgreSQL deferred |
| **3 — Phase 3 PostgreSQL** | Full production scale via PostgresStore | G3 gates required before Phase P1 starts; ~2000–3000 LOC effort |

**No path claims full production-ready status.** Path 1 is RC tag only. Path 2 is conditional pilot requiring operator signoff. Path 3 requires Phase P1–P4 completion and re-running the production evaluation framework.

---

*Document generated: 2026-04-28. Grounded in P6 evidence base and existing implementation-path docs.*
