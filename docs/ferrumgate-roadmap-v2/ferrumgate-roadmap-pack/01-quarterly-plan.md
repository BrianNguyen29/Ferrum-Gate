# 01 — Quarterly plan

## Execution Pack — Q1–Q2

This document is part of a controlled execution pack (`01`–`05`, `10`). The pack
is structured for operator/engineer use: explicit sequencing, dependency gates,
evidence expectations, and implementation boundaries.

### Doc map
- `01` — This document: quarterly goals, exit gates, execution order
- `02` — Release taxonomy, scope per release, release-gate evidence expectations
- `03` — Crate-level work breakdown and sequencing
- `04` — API surface evolution and route-level dependencies
- `05` — Adapter strategy, priority, and implementation order
- `10` — Consolidated master checklist (Q1–Q4 gate checklist)

### Cross-quarter dependency rule
> Q2 **must not start** until Q1 exit gate is passed. Q1 exit gate evidence
> is the entry precondition for Q2 work. Document the gate-pass signal in `02`.

---

## Q1 — Kernel hardening and invariant closure

### Execution Pack — Q1

#### Q1 Execution sequence
| Step | Action | Dependency |
|---|---|---|
| 1.1 | Proto shape lock — finalize naming for intent/proposal/capability/rollback/provenance/approval | None |
| 1.2 | Store integrity — explicit state transitions for executions/capabilities/approvals | 1.1 |
| 1.3 | PDP hard-rules audit — scope/taint/R3/draft-only enforcement | 1.1 |
| 1.4 | Cap mark_used path closure — single-use enforcement at authorize path | 1.3 |
| 1.5 | Rollback state machine fix — rollback_class propagation at prepare | 1.3 |
| 1.6 | Gateway lineage completeness test — end-to-end minimum-chain assertion | 1.2, 1.4, 1.5 |
| 1.7 | Testkit adversarial suite — bypass attempts against weak spots 1–4 | 1.6 |
| 1.8 | Invariant matrix pass — full test suite with evidence summary | 1.7 |

#### Q1 Dependency gates
- Gate A (step 1.3 → 1.4): PDP rules must be stable before cap mark_used is wired
- Gate B (step 1.5 → 1.6): Rollback state machine must be fixed before lineage test runs
- Gate C (step 1.4 + 1.5 → 1.6): Both cap enforcement and rollback fix must pass before end-to-end lineage test

#### Q1 Evidence expectations
| Gate | Evidence |
|---|---|
| G1 | Proto shapes locked; `cargo check --workspace` passes; `cargo test --package ferrum-proto` passes (4 tests) |
| G2 | Store transition rules implemented (24 unit tests pass); InvalidState returned for invalid transitions (22 integration tests pass); documented in `docs/artifacts/2026-04-09/02-q1-p2-g2-store-integrity-evidence.md` |
| Gate A | PDP audit notes show scope/taint/R3/draft-only rules are deterministic; no "maybe" branches; Q1-P3 slice satisfied (03-q1-p3-pdp-audit-evidence.md); Q1-P4a mark_used at authorize evidenced (05-q1-p4-combined-closure-note.md); full closure requires remaining integration coverage (steps 1.6–1.8) |
| Gate B | prepare-step rollback_class test passes; R3 `auto_commit=false` respected at prepare → **PASS** (Q1-P4b, evidence: `docs/artifacts/2026-04-09/04-q1-p4b-prepare-rollback-class-evidence.md`; combined closure: `docs/artifacts/2026-04-09/05-q1-p4-combined-closure-note.md`) |
| Gate C (Q1-P5 conservative) | Minimum chain (authorize + prepare + terminal-present) demonstrated over existing gateway execution surface via `integration_lineage_chain.rs`; conservative slice pass — no literal execute endpoint claimed (Q1-P5, evidence: `docs/artifacts/2026-04-09/06-q1-p5-minimum-chain-evidence.md`) |
| Gate C (Q1-P6 adversarial first slices) | WS1/WS2/WS3/WS4 each have a passing adversarial regression test; first-pass suite scope only — Q1-P7 (exit gate) was the remaining step (Q1-P6, evidence: `docs/artifacts/2026-04-09/07-q1-p6-adversarial-first-slices-evidence.md`) |
| Exit gate (Q1-P7) | cargo test --workspace passed; cargo test -p ferrum-gateway passed; route parity 19/19 confirmed; Q1-P6 chain (WS1-WS4 adversarial slices) passed; conservative gate pass for Q1/v1.1 scope only; no v1 scope expansion (Q1-P7, evidence: `docs/artifacts/2026-04-09/08-q1-p7-invariant-matrix-pass-evidence.md`) |

#### Q1 Definition of done
- Weak Spots 1–4 from `19-v1-single-node-support-contract.md` are closed or have documented accepted-risk status
- Proto shapes are stable (no field renames mid-quarter)
- Route table is reconciled — evaluate endpoint naming consistent across docs/spec/runtime
- OpenAPI spec matches runtime router
- Evidence summary is recorded (can be a test output summary or a short note in the artifact log)

### Mục tiêu
Đóng các weak spot của v1, đồng bộ docs/spec/routes, biến governance core thành nền tin cậy cho product wedge.

### Kết quả bắt buộc
- không còn bypass rõ ở prepare-step rollback class
- single-use capability được enforce end-to-end tại authorize/execute path
- draft-only được revalidate đúng chỗ
- lineage minimum chain có integration assertion thật
- docs/contracts/openapi/schemas/code đồng bộ

### Deliverables
- v1.1-kernel-hardening release
- invariant matrix pass
- route map canonical
- updated release checklist

### Work packages
- proto shape lock
- store integrity and explicit state transitions
- pdp hard rules audit
- cap mark_used path closure
- rollback state machine fix
- gateway lineage completeness test
- testkit adversarial cases

### Exit gate
Không được sang Q2 nếu:
- còn accepted risk có thể phá R3 no auto-commit
- capability single-use còn phụ thuộc client discipline
- lineage chain chưa kiểm chứng end-to-end

> **V1 boundary note**: The Q1 exit gate tests the v1 kernel. The v1 support contract
> (`19-v1-single-node-support-contract.md`) is the authoritative boundary for what
> "v1 kernel" means. Accepted risks listed in the v1 support contract are the
> accepted baseline; closing them improves v1 but does not expand the support contract.

---

## Q2 — Governed engineering changes beta

### Execution Pack — Q2

#### Q2 Entry precondition
> **Q2 must not begin until v1.1 exit gate is passed.** Evidence of gate pass
> is required before Q2 work is treated as committed. Record gate evidence in
> `docs/artifacts/<date>/` and link from `02-release-plan.md` v1.1 gate section.

#### Q2 Execution sequence
| Step | Action | Dependency |
|---|---|---|
| 2.1 | Extend ferrum-proto types for fs/git/db adapter payloads | v1.1 done |
| 2.2 | Implement ferrum-store adapter artifact persistence | 2.1 |
| 2.3 | Implement ferrum-adapter-fs — backup/hash/restore path | 2.2 |
| 2.4 | Implement ferrum-adapter-git — before_ref/after_ref/revert path | 2.2 |
| 2.5 | Implement ferrum-adapter-sqlite — transaction/verify/rollback | 2.2 |
| 2.6 | Integrate adapters into ferrum-gateway orchestration | 2.3, 2.4, 2.5 |
| 2.7 | Policy packs for fs/git/db engineering workflows | 2.6 |
| 2.8 | Demo — fs + db verify/compensate end-to-end demo | 2.7 |
| 2.9 | Examples repo for 3 workflow samples | 2.8 |

#### Q2 Dependency gates
- Gate D (v1.1 exit → 2.1): Proto stability from Q1 is required before adapter payload work
- Gate E (2.2 → 2.3/2.4/2.5): Store must support adapter artifacts before adapter integration
- Gate F (2.3 + 2.4 + 2.5 → 2.6): All three adapters must be real-implementation before gateway integration
- Gate G (2.6 + 2.7 → 2.8): Gateway orchestration and policy packs must be ready before demo

> **Gate naming note**: Gates A/B/C are Q1-internal (see Q1 Dependency gates above).
> Gates D/E/F/G align with `03` cross-crate gates as follows:
> - Gate D ↔ G1 (`ferrum-proto` shape lock → all crates)
> - Gate E ↔ G4 (`ferrum-store` adapter artifacts → adapter crates)
> - Gate F ↔ G5 (all adapters real → `ferrum-gateway` integration)
> - Gate G ↔ G6 (`ferrum-gateway` orchestration → PDP policy pack work)

#### Q2 Evidence expectations
| Gate | Evidence |
|---|---|
| Gate D | v1.1 exit gate evidence (test output or artifact note) |
| Gate E | ferrum-store adapter artifact persistence has at least unit-level test |
| Gate F | Each adapter has at least one integration test showing real backup/restore or rollback |
| Gate G | Operator-visible execution + lineage trace for fs mutation demo |
| Exit gate | End-to-end demo runs; verify and compensate path demonstrable for fs + db |

> **Gate E partial status (2026-04-11):** Store-level fs-first adapter artifact persistence is confirmed
> for FileWrite (prepare → persist → compensate/restore contract). This satisfies Gate E at the store
> and adapter layer. Gateway-level execute and verify HTTP surfaces (`POST /v1/executions/{id}/execute`,
> `POST /v1/executions/{id}/verify`) **exist** for the fs-first FileWrite slice per `11-gateway-execute-verify-surface-design-note.md`
> (server.rs:155–162). Git and sqlite adapters are not yet implemented; Gate E at the full Q2 adapter scope is not yet satisfied.

#### Q2 Definition of done
- All three adapters (fs, git, sqlite) have real implementations with prepare/execute/verify/compensate
- Policy packs exist for at least fs and db engineering workflows
- A demo shows verify + compensate/rollback working on a real workload
- Operator can inspect execution and lineage for all three adapter types

### Mục tiêu
Làm adapter thật cho các side effect có thể bán được sớm nhất: file, git, database.

### Kết quả bắt buộc
- fs adapter có backup / hash verify / restore path
- git adapter có before_ref / after_ref / revert-reset path
- db adapter có transaction wrapper / verify predicate / rollback
- policy packs cho engineering workflows
- demo workflow thực chạy được qua gateway

### Deliverables
- governed-engineering-changes-beta release
- sample policy bundles cho repo/file/db
- operator-readable execution and lineage traces
- verify/compensate hoạt động thật ở ít nhất fs + db

### Work packages
- path-scoped capability binding cho fs
- protected branch / ref policy cho git
- SQL class/risk mapping cho db
- controlled HTTP mutation proof-of-concept nếu còn thời gian
- examples repo cho 3 workflow mẫu

### Exit gate
Không sang Q3 nếu:
- adapter vẫn chỉ là mock/noop
- verify layer chưa đáng tin
- không demo được rollback/recovery path ở workload thực

> **V1 boundary note**: Adapter work in Q2 is explicitly post-v1 scope.
> The v1 support contract confirms all adapters are skeleton-only in v1.
> "Done when" criteria for adapter items in this plan describe the target state
> for post-v1 releases, not the current v1 support baseline.

---

## Q3 — Self-hosted commercial beta

### Mục tiêu
Đóng gói FerrumGate thành sản phẩm self-hosted private beta cho design partners.

### Kết quả bắt buộc
- Postgres support production-like
- operator UI cơ bản usable
- approval workflow usable qua UI
- observability package có logs/metrics/traces
- deployment bundle cho private environment

### Deliverables
- self-hosted-commercial-beta release
- Docker/Compose + Helm draft
- operator UI private beta
- backup/restore + incident playbook cho PostgreSQL và SQLite

### Work packages
- store abstraction mở cho sqlite + postgres
- RBAC tối thiểu cho operator UI
- evidence export bundle
- deployment docs, secrets docs, upgrade docs

### Exit gate
Không sang Q4 nếu:
- chưa có customer-pilot-ready deployment path
- operator vẫn cần CLI/raw DB để điều tra execution
- auth/ops story chưa đủ cho environment private

> **V1 boundary note**: Q3 covers self-hosted commercial beta which is explicitly
> post-v1. Multi-node, HA, and read-replica are out of scope for v1 and remain
> out of scope for Q3. The v1 support contract does not cover postgres support
> or operator UI; those are Q3 deliverables.

---

## Q4 — MCP governance beta and enterprise evidence alpha

### Mục tiêu
Mở rộng từ engineering workflows sang open runtime / MCP governance, đồng thời thêm enterprise evidence plane.

### Kết quả bắt buộc
- MCP wrapper/gateway mode beta
- tool governance policy packs
- provenance graph usable cho multi-hop queries
- tamper-evident evidence alpha
- signed approval / evidence bundle alpha

### Deliverables
- mcp-governance-beta release
- enterprise-evidence-alpha release
- runtime integration docs
- audit/export artifacts

### Work packages
- tool call -> ActionProposal mapping
- capability binding theo tool + resource + arg constraints
- runtime trust/taint propagation
- hash-chain / tamper-evident ledger alpha
- incident review workflow

### Exit gate
Q4 có thể kết thúc với alpha/beta; không ép GA. Mục tiêu là chứng minh FerrumGate có thể mở rộng thành governed execution plane cho open runtime.

> **V1 boundary note**: Q4 (MCP governance beta + enterprise evidence alpha) is entirely
> post-v1 scope. MCP/runtime integration and enterprise evidence plane are listed
> as explicitly unsupported in the v1 support contract. Do not claim any v1 support
> extension based on Q4 deliverables.

---

## V1 boundary reminder

The quarterly plan mixes two categories of work: Q1 hardening within the locked v1 boundary,
and Q2–Q4 post-v1 roadmap work. The v1 support contract
(`19-v1-single-node-support-contract.md`) is the **only authoritative boundary** for
v1 scope. Specifically:

- Q1 work is about hardening the v1 kernel, but **Q1 deliverables
  are not v1 scope expansions** — they are defect closure within the existing v1 contract
- Q2–Q4 work is entirely post-v1 and must not be described as "v1 support"
- Adapter implementations, postgres support, operator UI, MCP integration, and
  enterprise evidence are all post-v1 scope per the v1 support contract
