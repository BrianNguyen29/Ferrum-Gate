# 45 — Kế hoạch Kiểm toán Tính năng FerrumGate

> **Trạng thái**: Bản nháp lập kế hoạch — chưa triển khai sản xuất
> **Phạm vi**: FerrumGate v1 single-node SQLite (RC-ready/conditional)
> **Ràng buộc**: Tài liệu này là kế hoạch kiểm tra, không phải báo cáo kết quả hoàn thành

---

## Mục đích

Tài liệu này định nghĩa kế hoạch kiểm toán toàn diện các tính năng hiện tại của FerrumGate nhằm:

- Xác định các tính năng đã được kiểm chứng, tính năng còn thiếu, và tính năng cần cải thiện
- Cung cấp bản đồ tính năng chi tiết cho quyết định sản xuất
- Làm cơ sở cho roadmap nâng cao năng lực (novelty roadmap — xem `47-novelty-roadmap.md`)
- Hỗ trợ so sánh với hệ thống intent-rebase tại `/home/uong_guyen/work/intent-rebase`

---

## Ranh giới Phi-sản xuất

| Khía cạnh | Trạng thái hiện tại | Ghi chú |
|---|---|---|
| Kiến trúc sản xuất | Phase 1 SQLite write queue | Không phù hợp cho multi-node hoặc HA |
| Trạng thái RC | RC-ready (single-node SQLite) | Cần đánh giá trước triển khai |
| Invariant | 12 VERIFIED / 0 PARTIAL / 0 INFERRED | Xem `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` |
| Phase 2 | Deferred/regressed | Perf regression trong benchmarking |
| Phase 3 (PostgreSQL) | Chưa triển khai | Cần thiết cho mở rộng quy mô |

**Lưu ý quan trọng**: Tài liệu này không khẳng định FerrumGate đã sẵn sàng sản xuất đầy đủ. Việc triển khai sản xuất chỉ được thực hiện sau khi đánh giá theo `27-production-evaluation-plan.md`.

---

## Các Pha Kiểm toán

### Pha 1 — Khảo sát và Lập bản đồ Tính năng

**Mục tiêu**: Xây dựng bản đồ đầy đủ các tính năng hiện có

**Công việc**:
1. Liệt kê tất cả API endpoint theo nhóm chức năng
2. Xác định các tính năng capability/intent/provenance/rollback
3. Đánh giá mức độ kiểm chứng (unit test, integration test, manual)
4. Ghi nhận các tính năng chưa có test coverage

**Deliverables**:
- Ma trận tính năng theo nhóm
- Bảng test coverage

---

### Pha 2 — Kiểm tra Invariant và Ràng buộc

**Mục tiêu**: Xác minh trạng thái invariant hiện tại

**Công việc**:
1. Duyệt lại ma trận kiểm chứng invariant (`26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`)
2. Ghi nhận trạng thái lịch sử của các items từng PARTIAL/INFERRED và trạng thái hiện tại sau khi đã khắc phục
3. Đề xuất hành động khắc phục cho từng item
4. Đánh giá risk level của từng invariant gap

**Deliverables**:
- Báo cáo chi tiết từng invariant gap
- Đề xuất hành động với priority ranking

---

### Pha 3 — Phân tích Khả năng Mở rộng

**Mục tiêu**: Đánh giá các điểm nghẽn và khả năng mở rộng

**Công việc**:
1. Phân tích kiến trúc write queue
2. Đánh giá SQLite bottleneck
3. Xác định các điểm cần cải thiện cho Phase 2/3
4. Đánh giá adapter surface cho capability extension

**Deliverables**:
- Báo cáo bottleneck analysis
- Danh sách ưu tiên mở rộng

---

## Nhóm Tính năng cần Kiểm toán

### Nhóm 1 — Governance Core

| Tính năng | Mô tả | Mức độ ưu tiên |
|---|---|---|
| Intent lifecycle | Tạo, duyệt, thực thi intent | Cao |
| Capability minting | Mint/revoke capability với TTL | Cao |
| Scope enforcement | Kiểm tra scope bounds | Cao |
| Single-use enforcement | Ngăn chặn reuse capability | Cao |

### Nhóm 2 — Provenance và Lineage

| Tính năng | Mô tả | Mức độ ưu tiên |
|---|---|---|
| Event emission | Phát sinh sự kiện lineage | Cao |
| Lineage query | Truy vấn chain sự kiện | Trung bình |
| Provenance chain | Liên kết action → effect | Cao |

### Nhóm 3 — Rollback và Compensation

| Tính năng | Mô tả | Mức độ ưu tiên |
|---|---|---|
| Rollback prepare | Prepare rollback contract | Cao |
| Compensation execution | Thực thi compensate | Cao |
| R0/R1/R2/R3 classes | Phân biệt rollback class | Cao |

### Nhóm 4 — Security và Rate Limiting

| Tính năng | Mô tả | Mức độ ưu tiên |
|---|---|---|
| Bearer token auth | Xác thực token | Cao |
| Rate limiting | tower_governor integration | Cao |
| Output sanitization | Ngăn chặn injection | Trung bình |
| Taint scoring | Quarantine cho high-taint | Trung bình |

### Nhóm 5 — Operations

| Tính năng | Mô tả | Mức độ ưu tiên |
|---|---|---|
| Health endpoint | /v1/healthz, /v1/readyz | Trung bình |
| ferrumctl inspect | Công cụ inspect-only | Trung bình |
| Write queue monitoring | Queue depth, lag metrics | Thấp |
| Backup/restore | File-level SQLite backup | Thấp |

---

## Lệnh Validation

### Quality Gates

```bash
# Format check
cargo fmt --all -- --check

# Clippy check
cargo clippy --workspace --all-targets -- -D warnings

# Test suite
cargo test --workspace

# Contract consistency
python3 scripts/check_contract_consistency.py
bash scripts/validate_repo_layout.sh
```

### Invariant Verification

```bash
# Run integration tests
cargo test --workspace --test '*integration*'

# Verify lineage chain
cargo test test_lineage_chain_full_provenance_events
```

---

## Deliverables

| # | Deliverable | Định dạng | Pha |
|---|---|---|---|
| D1 | Ma trận tính năng đầy đủ | Markdown table | 1 |
| D2 | Báo cáo test coverage | Markdown | 1 |
| D3 | Chi tiết invariant gaps lịch sử và trạng thái hiện tại (`12 VERIFIED / 0 PARTIAL / 0 INFERRED`) | Markdown | 2 |
| D4 | Đề xuất khắc phục invariant gaps | Markdown + priority | 2 |
| D5 | Bottleneck analysis report ✅ | Markdown | 3 — [51-d5-bottleneck-analysis-report.md](./51-d5-bottleneck-analysis-report.md) |
| D6 | Priority list cho mở rộng | Markdown | 3 — [52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md) ✅ |

---

## Tiêu chí Hoàn thành

Kiểm toán được coi là hoàn thành khi:

- [x] Tất cả 5 nhóm tính năng đã được khảo sát
- [x] Ma trận tính năng (D1) được xác nhận đầy đủ
- [x] Tất cả invariant gaps đã được ghi nhận (D3)
- [x] Đề xuất khắc phục có priority ranking (D4)
- [x] Bottleneck analysis hoàn thành (D5) — xem [51-d5-bottleneck-analysis-report.md](./51-d5-bottleneck-analysis-report.md)
- [x] Priority list cho mở rộng được duyệt (D6) — xem [52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md)
- [x] Không có tính năng critical nào bị bỏ sót trong khảo sát

---

## Kết quả Thực hiện Pha 1 — Phase 1 D1 Executed

> **Trạng thái**: Pha 1 đã hoàn thành (2026-04-27)
> **Nguồn**: Discovery dựa trên `01-current-state.md`, `AGENTS.md`, và kiểm tra mã nguồn trực tiếp
> **Lưu ý**: Phase 2 (invariant detail) đã hoàn thành; Phase 3 D5 bottleneck analysis hoàn thành (xem [51-d5-bottleneck-analysis-report.md](./51-d5-bottleneck-analysis-report.md)); D6 priority list hoàn thành (xem [52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md))

### Ma trận Tính năng Phase 1 — Đã Thực thi

#### Nhóm 1 — Governance Core

| Tính năng | Trạng thái | Ghi chú / Ref |
|---|---|---|
| Intent lifecycle | ✅ Đã triển khai | `compile_intent` tại `crates/ferrum-gateway/src/server.rs`; `IntentRepo` tại `crates/ferrum-store/src/repos.rs`; tests qua store/integration |
| Capability minting | ✅ Đã triển khai | TTL/resource binding; `mint_capability` tại gateway; `crates/ferrum-cap/src/service.rs` |
| Scope enforcement | ✅ Đã triển khai | `infer_effect_type` và scope checks tại `crates/ferrum-pdp/src/engine.rs` |
| Single-use enforcement | ✅ Đã triển khai | durable mark used path tại gateway + `ferrum-cap` |
| Policy evaluation | ✅ Đã triển khai | `StaticPdpEngine` + active bundles; `evaluate_proposal` tại gateway + `ferrum-pdp` |
| Policy bundle CRUD | ✅ Đã triển khai | gateway policy bundle handlers; `PolicyBundleRepo` |
| Approval workflow | ✅ Đã triển khai (list/get/resolve) | approval binding digest vẫn inferred/deferred; handlers tại gateway; `ferrumctl resolve-approval` |

#### Nhóm 2 — Provenance & Lineage

| Tính năng | Trạng thái | Ghi chú / Ref |
|---|---|---|
| Event emission | ✅ Đã triển khai (minimum lineage) | Key events: PolicyEvaluated, CapabilityMinted, ToolCallPrepared, ToolCallExecuted, SideEffectVerified, SideEffectCommitted; integration lineage tests |
| Lineage query (execution + multi-hop) | ✅ Đã triển khai | lineage handlers tại gateway; `ferrum-graph` (BFS ancestor/descendant traversal) |
| Provenance query/ingest | ✅ Đã triển khai | gateway query/ingest handlers; `ExternalEventSource` trait + POST /v1/provenance/ingest endpoint |
| Hash chain ledger | ✅ Đã triển khai | `ferrum-store LedgerRepo`; `ferrum-ledger` SHA-256 hash chain; 13 tests |
| **Gap**: ledger hash fields không liên kết đầy đủ với gateway events | ⚠️ Gap | Cần xác minh thủ công cho một số flows |

#### Nhóm 3 — Rollback & Compensation

| Tính năng | Trạng thái | Ghi chú / Ref |
|---|---|---|
| Prepare/execute/verify/compensate lifecycle | ✅ Đã triển khai | gateway execution handlers; `ferrum-rollback` |
| R0/R1/R2/R3 classes | ✅ Đã triển khai | R2 compensation plan enforced; R3 no auto-commit |
| Plannable adapter trait + FS planner | ✅ Đã triển khai | `PlannableAdapter` trait; `PlannableFsAdapter`; 4 tests |
| **Gap**: adapter compensation guarantees phụ thuộc adapter | ⚠️ Gap | Manual verification cần thiết cho một số flows |

#### Nhóm 4 — Security & Rate Limiting

| Tính năng | Trạng thái | Ghi chú / Ref |
|---|---|---|
| Bearer token auth | ✅ Đã triển khai | `bearer` middleware tại gateway; healthz/readyz bypass |
| Rate limiting | ✅ Đã triển khai | `tower-governor`; default 2/sec per IP, burst 50 |
| **Gap**: rate-limit tests | ✅ Đã khắc phục | 2 integration tests: `test_rate_limit_returns_429_when_exceeded`, `test_rate_limit_allows_requests_under_limit` |
| Taint scoring/quarantine/contradiction | ✅ Đã triển khai | `ferrum-firewall`; `TaintScoringFirewall`; 21 tests; wired vào gateway PDP |
| Output sanitization | ✅ Đã triển khai (trait-level) | **Gap**: gateway-wide response path integration deferred |
| DLP | ❌ Stub only | `dlp_findings` stub trả về empty findings; not implemented |

#### Nhóm 5 — Operations

| Tính năng | Trạng thái | Ghi chú / Ref |
|---|---|---|
| healthz/readyz | ✅ Đã triển khai (shallow) | shallow — không sâu; readyz/deep (P3) đã triển khai với store probe |
| ferrumctl inspect suite | ✅ Đã triển khai | health, inspect-execution, inspect-approvals, inspect-lineage, inspect-provenance |
| SQLite write queue + WAL tuning | ✅ Đã triển khai | PRAGMA: synchronous=NORMAL, wal_autocheckpoint=1000, cache_size=-64000, busy_timeout=5000ms |
| Config precedence | ✅ Đã triển khai | CLI args > env vars > config file > defaults |
| Graceful shutdown | ✅ Đã triển khai | |
| Runtime bridges / cross-runtime provenance | ✅ Đã triển khai | `RuntimeBridge` trait; `McpBridge`; GET /v1/bridges endpoints |
| Stress testing | ✅ Đã triển khai | `ferrum-stress` binary |
| Backup/restore | ✅ Đã triển khai (bounded) | `ferrumctl backup create/verify/restore`; SQLite-only; offline/local; opt-in retention pruning (`--retention-days N`); no scheduling/encryption |
| PostgreSQL / multi-node / HA | ❌ Deferred | Not implemented; PostgreSQL recommended for production scale |
| cancel_execution | ✅ Implemented (post-v1 boundary) | `ferrumctl` client path now has matching `POST /v1/executions/{execution_id}/cancel` server route, handler, state guards, and integration tests |

---

### Tổng kết Routes

#### Unauthenticated
- `GET /v1/healthz`
- `GET /v1/readyz`
- `GET /v1/readyz/deep` (deep readiness with store probe)

#### Authenticated (bearer token)
| Nhóm | Endpoints |
|---|---|
| Intent | `POST /v1/intents/compile` |
| Proposal | `POST /v1/proposals/{proposal_id}/evaluate` |
| Capability | `POST /v1/capabilities/mint`; `POST /v1/capabilities/{capability_id}/revoke` |
| Execution | `POST /v1/executions/authorize`; `GET /v1/executions/{execution_id}`; `POST /v1/executions/{execution_id}/prepare`; `POST /v1/executions/{execution_id}/execute`; `POST /v1/executions/{execution_id}/verify`; `POST /v1/executions/{execution_id}/cancel`; `POST /v1/executions/{execution_id}/compensate`; `POST /v1/executions/{execution_id}/evaluate-outcome` |
| Approval | `GET /v1/approvals`; `GET /v1/approvals/{approval_id}` |
| Provenance | `GET /v1/provenance/query`; `GET /v1/provenance/lineage`; `POST /v1/provenance/ingest` |
| Bridges | `GET /v1/bridges`; `GET /v1/bridges/{id}/tools` |
| Policy Bundle | `POST /v1/policy-bundles`; `GET /v1/policy-bundles`; `GET /v1/policy-bundles/{id}`; `PUT /v1/policy-bundles/{id}`; `DELETE /v1/policy-bundles/{id}`; `PUT /v1/policy-bundles/{id}/active` |

---

### Tổng kết Binaries

| Binary | Mô tả |
|---|---|
| `ferrumd` | Server daemon |
| `ferrumctl` | CLI: inspect/policy/server/contract utilities + cancel-execution + backup (create/verify/restore) |
| `ferrum-stress` | In-process stress testing tool |

---

### Test Coverage Summary (current workspace evidence)

> **Nguồn**: `cargo test --workspace` chạy trong phiên cập nhật M1–M3/S1 (2026-04-28)
> **Trạng thái**: Full workspace check/clippy/test pass locally với 0 failures

| Crate | Tests | Ghi chú |
|---|---|---|
| ferrum-adapter-fs | 135 | FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod + PlannableFsAdapter |
| ferrum-adapter-git | 86 | GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete |
| ferrum-adapter-http | 103 | HttpMutation + http.replay_v1 (POST/PUT/PATCH) + pooling/retry |
| ferrum-adapter-sqlite | 16 | SqlRowCountRange + transaction rollback |
| ferrum-adapter-maildraft | 16 | create/update/delete lifecycle + rollback idempotency |
| ferrum-cap | 4 | Capability TTL boundaries |
| ferrum-firewall | 21 | TaintScoringFirewall + contradiction detection + sanitizer |
| ferrum-graph | 10 | BFS ancestor/descendant traversal |
| ferrum-ledger | 13 | SHA-256 hash chain |
| ferrum-gateway | 44 | Endpoints + outcome evaluation + provenance ingest + bridges + readiness + S2 deep readiness failure-mode tests |
| ferrum-pdp | 19 | Outcome-aware governance |
| ferrum-proto | 18 | Intent validation + canonical action digest + schemas |
| ferrum-rollback | 11 | ExecutionPlan + PlannableAdapter |
| ferrum-store | 60 | SQLite persistence + StoreFacade + readiness health check |
| ferrum-sync | 65 | ExternalEventSource + RuntimeBridge + McpBridge |
| ferrumctl | 35 | CLI utilities + policy bundle CRUD + SQLite backup/restore |
| ferrumd | 6 | Daemon config + unsupported DSN guardrails |
| ferrum-stress | 0 | Stress binary compile coverage |
| ferrum-testkit | 0 | Testkit compile coverage |
| Integration tests | 82 | contracts(2) + fs-roundtrip(7) + gateway-flow(65) + lineage-chain(8) |
| **Tổng** | **~763** | includes integration tests, invalid_transitions(22), and ferrum-sync doctest |

---

### Gaps và Hạn chế Phase 1

| # | Gap | Risk Level | Hành động |
|---|---|---|---|
| G1 | Gateway events không liên kết đầy đủ với ledger hash fields | Medium | Duyệt thủ công + integration test bổ sung |
| G2 | Adapter compensation guarantees phụ thuộc adapter (không đồng nhất) | Medium | ✅ Evidence matrix complete — xem `56-adapter-compensation-evidence-matrix.md`; production use vẫn cần workload-specific drill/operator acceptance |
| G3 | Rate limiting không có dedicated test suite | Low | ✅ Đã bổ sung 3 rate-limit integration tests (per-IP isolation, recovery, concurrent burst) — M2 IMPLEMENTED |
| G4 | Output sanitization — gateway-wide response path chưa integration | Medium | ✅ Bounded wiring hoàn thành per design note 48 — M1 IMPLEMENTED (docs-only design) |
| G5 | DLP stub only (không có triển khai thực) | Low | Stub — post-v1 scope — S1 DOCS-ONLY |
| G6 | Backup/restore ✅ Đã triển khai (P5 bounded) | — | ✅ Hoàn thành — bounded SQLite-only offline workflow với safety guardrails |
| G7 | PostgreSQL / multi-node / HA chưa triển khai | High (cho scale) | Deferred — PostgreSQL recommended cho production |
| G8 | cancel_execution CLI-only, không có HTTP endpoint | Low | ✅ M3 IMPLEMENTED — route/handler/state/provenance/tests added |
| G9 | healthz/readyz shallow (không deep health check) | Low | ✅ S2 improved — 2026-04-28: /v1/readyz/deep với store probe + bounded failure-mode tests (503/degraded/healthy=false/component error) |
| S4 | Policy bundle / bridge support boundary clarity | Low | ✅ Clarified — 2026-04-28: Doc 19 §2.4 explicitly lists policy bundle (6 routes) and bridge (2 routes) as implemented but outside v1 support contract; Doc 33 S4 status updated |

---

### Hành động Tiếp theo (Next Actions)

#### Phase 1 D1 — Hoàn thành ✅
- Ma trận tính năng đã được khảo sát và ghi nhận
- Test coverage đã được xác minh
- Gaps đã được ghi nhận

#### Phase 2 D3 + D4 — Đã Thực thi ✅

> **Trạng thái**: Phase 2 audit hoàn thành (2026-04-27)
> **Nguồn**: Kiểm tra mã nguồn trực tiếp + `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`
> **Lưu ý**: Kiểm toán D3/D4 đã hoàn thành — việc triển khai các bản sửa lỗi vẫn là **pending**

---

### Ma trận Invariant Gap — Phase 2 D3 Executed

| # | Invariant | Trạng thái | Risk | Fix Class | Priority |
|---|---|---|---|---|---|
| I11 | Output sanitization (gateway wiring) | VERIFIED | MED | Bounded wiring + design choice | 1 |
| I6 | Approval binding matches action digest | VERIFIED | MED | Bounded test addition (P3) | 2 |
| I7 | High taint blocks risky mutation | VERIFIED | LOW-MED | Bounded test addition | 3 |
| I1 | Intent envelope validity | VERIFIED | LOW | Bounded hygiene fix | 4 |

---

### Chi tiết Invariant Gaps — Phase 2 D3 Detail

#### I5 — Scope cannot expand beyond intent (VERIFIED)

| Khía cạnh | Chi tiết |
|---|---|
| **Hiện trạng** | `validate_resource_bindings_subset_of_scope` implemented in `crates/ferrum-gateway/src/server.rs`; invoked in `authorize_execution`; 16 unit tests + 2 integration test cases cover empty, subset, exact, superset, and disjoint scenarios |
| **Evidence refs** | `crates/ferrum-gateway/src/server.rs:validate_resource_bindings_subset_of_scope`; `crates/ferrum-integration-tests/src/integration_gateway_flow.rs` dedicated I5 tests: `test_i5_scope_validation_resource_bindings_exceed_intent_scope`, `test_i5_scope_validation_resource_bindings_within_intent_scope` |
| **Lưu ý** | Conservative prefix matching used (superset scope with prefix overlap is rejected — not a blocker for v1) |
| **Status** | **VERIFIED** — bounded fix completed |

#### I7 — High taint blocks risky mutation (VERIFIED)

| Khía cạnh | Chi tiết |
|---|---|
| **Hiện trạng** | Full pipeline verified: `evaluate_proposal` → `build_firewall_context` → `TaintScoringFirewall.compute_taint_score` → `TrustContextSummary` → `StaticPdpEngine.evaluate` → `Decision::Quarantine` |
| **Evidence refs** | `crates/ferrum-pdp/src/engine.rs:202-215` (taint quarantine logic); `crates/ferrum-gateway/src/server.rs:377-393` (firewall taint scoring wired to PDP); `crates/ferrum-firewall/src/lib.rs:105-141` (compute_taint_score); `integration_gateway_flow.rs:test_i7_e2e_static_pdp_quarantine_on_high_taint` (real StaticPdpEngine E2E test) |
| **Test** | New integration test uses `StaticPdpEngine` (not `InjectablePdpEngine`) with intent having `input_labels: [ExternalWeb]` → `is_external=true` → `trust_score=30` → taint contribution +50; proposal has `privileged=true` metadata → +20; total taint score = 70 triggers quarantine |
| **Risk/Priority** | LOW-MED / #4 |
| **Fix Classification** | Bounded test addition |
| **Next Actions** | None — I7 now VERIFIED |
| **Status** | **VERIFIED** — bounded test addition completed |

#### I11 — Output sanitization (VERIFIED)

| Khía cạnh | Chi tiết |
|---|---|
| **Hiện trạng** | Bounded wiring hoàn thành: `sanitize_output` đã được wire vào 7 targeted endpoints theo design note 48 |
| **Evidence refs** | `crates/ferrum-gateway/src/server.rs`: revoke_capability, delete_policy_bundle, set_policy_bundle_active, get_execution, get_execution_lineage, query_lineage, list_bridge_tools; `crates/ferrum-integration-tests/src/integration_gateway_flow.rs`: test_i11_sanitizes_execution_response_with_control_characters, test_i11_sanitizes_error_response_for_invalid_bundle_id |
| **Wired Endpoints** | revoke_capability, delete_policy_bundle, set_policy_bundle_active, get_execution, get_execution_lineage, query_lineage, list_bridge_tools |
| **Risk/Priority** | MED / #1 |
| **Fix Classification** | Bounded wiring — design note 48 đã implement |
| **Design** | Hybrid v1: sanitize reflected error messages + targeted high-risk endpoints — reject full middleware; defer SanitizedJson<T> wrapper |
| **Tests** | 2 integration tests pass: provenanc/lineage sanitization + error message sanitization |
| **Production Ready** | **Không** — deferred; bounded v1 implementation only |
| **Status** | **VERIFIED** — bounded wiring completed + integration tests pass |

#### I1 — Intent envelope validity (VERIFIED)

| Khía cạnh | Chi tiết |
|---|---|
| **Hiện trạng** | `IntentEnvelope::validate()` implemented in `crates/ferrum-proto/src/intent.rs`; wired in `compile_intent` at `crates/ferrum-gateway/src/server.rs`; 4 unit tests cover valid, empty outcomes, expires_at==created_at, expires_at<created_at |
| **Evidence refs** | `crates/ferrum-proto/src/intent.rs` validate impl + unit tests; `crates/ferrum-gateway/src/server.rs` compile_intent I1 guard |
| **Risk/Priority** | LOW / #5 |
| **Fix Classification** | Bounded hygiene fix |
| **Next Actions** | None — I1 now VERIFIED |
| **Status** | **VERIFIED** — bounded hygiene fix completed |

#### I6 — Approval binding matches action digest (VERIFIED)

| Khía cạnh | Chi tiết |
|---|---|
| **Hiện trạng** | `validate_approval_binding_digest` implemented in `crates/ferrum-gateway/src/server.rs:770-853`; 8 integration tests cover all failure modes + single-use |
| **Evidence refs** | `crates/ferrum-gateway/src/server.rs:validate_approval_binding_digest`; `integration_gateway_flow.rs:3881-4886` (8 I6 tests); ADR-49 P3/P5 complete |
| **Tests** | test_i6_none_binding_skips_validation, test_i6_valid_binding_succeeds, test_i6_pending_approval_denied, test_i6_digest_mismatch_denied, test_i6_expired_binding_denied, test_i6_approval_not_found_denied, test_i6_chain_broken_digest_mismatch_between_approval_and_binding, test_i6_single_use_with_valid_approval_binding |
| **Risk/Priority** | MED / #3 |
| **Fix Classification** | Bounded test addition (P3) — all 8 integration tests pass |
| **ADR** | **[ADR-49: I6 Approval Binding Digest Validation](./49-i6-approval-binding-digest-adr.md)** — P1+P2+P3+P5 implemented; P4 optional/not required |
| **Decision Summary** | - Enforce I6 at `authorize_execution` after I5 scope validation, before `mark_capability_used_durable`<br>- Canonical digest: SHA-256 over deterministic JSON of intent_id, proposal_id, tool_name, server_name, raw_arguments (key-sorted), expected_effect, estimated_risk, requested_rollback_class<br>- 5 checks when `approval_binding=Some`: approval exists, state=Granted, not expired, binding.digest==approval.digest, computed.digest==binding.digest<br>- `approval_binding=None` skips check for backwards compatibility |
| **Next Actions** | None — I6 now VERIFIED; production deployment still deferred |
| **Status** | **VERIFIED** — bounded test addition completed |

---

### Phase 2 D4 — Đề xuất Khắc phục Invariant Gaps

| Priority | Invariant | Recommended Next | Fix Class | Production Ready |
|---|---|---|---|---|
| 1 | I11 (Output sanitization wiring) | Chọn middleware/per-endpoint design + integration test | Bounded wiring | **Không** — design choice required |
| 2 | (I6 VERIFIED) | — | — | — |
| 3 | (I1 VERIFIED) | — | — | — |
| 4 | (I7 VERIFIED) | — | — | — |

**Lưu ý quan trọng**: Tất cả bản sửa lỗi trên vẫn là **pending implementation**. Không có bản sửa lỗi nào được triển khai sản xuất. Việc triển khai chỉ được thực hiện sau khi đánh giá theo `27-production-evaluation-plan.md`.

---

### Phase 3 D5 — Complete ✅
- [x] Bottleneck analysis report complete. See [51-d5-bottleneck-analysis-report.md](./51-d5-bottleneck-analysis-report.md) — covers all 10 mapped bottleneck domains.

### Phase 3 D6 — Complete ✅
- [x] Priority list cho mở rộng (PostgreSQL, multi-node/HA). See [52-d6-priority-expansion-list.md](./52-d6-priority-expansion-list.md).
- **See also**: [33-feature-completion-backlog.md](./33-feature-completion-backlog.md) — Must/Should/Production-only categorization of all incomplete/partial features

#### Deferred / Post-v1
- [ ] PostgreSQL adapter cho production scale
- [ ] Multi-node / HA architecture
- [ ] Full backup/restore (scheduling, retention, encryption — not in v1)
- [ ] DLP triển khai đầy đủ (nếu cần)
- [ ] Rate limiting test suite
- [ ] Output sanitization gateway-wide integration
- [ ] Deep health check endpoints

---

## Tham khảo

- `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` — Ma trận invariant
- `docs/implementation-path/27-production-evaluation-plan.md` — Evaluation framework
- `docs/implementation-path/47-novelty-roadmap.md` — Novelty roadmap (đầu ra của kiểm toán này)
- `docs/implementation-path/01-current-state.md` — Trạng thái hiện tại (test coverage, phase status)
- `docs/implementation-path/32-feature-completeness-audit.md` — Route/API reconciliation (this doc complements the Phase 1 feature matrix with v1 boundary audit)
- `docs/implementation-path/33-feature-completion-backlog.md` — Must/Should/Production-only backlog for incomplete/partial features
- `docs/implementation-path/56-adapter-compensation-evidence-matrix.md` — G2 adapter compensation gap evidence
- `docs/implementation-path/57-workload-compensation-drill-plan.md` — Operator drill plan for compensation verification
- `AGENTS.md` — Trạng thái xác minh (check/clippy/test pass)
- `docs/PRODUCTION_NOTES.md` — Stress test evidence
