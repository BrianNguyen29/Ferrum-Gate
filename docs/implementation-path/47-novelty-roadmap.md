# 47 — Roadmap Nâng cao Năng lực (Novelty Roadmap)

> **Trạng thái**: Bản nháp lập kế hoạch — chưa triển khai sản xuất
> **Phạm vi**: FerrumGate v1 single-node SQLite (RC-ready/conditional)
> **Cơ sở**: Dựa trên kết quả kiểm toán tính năng (`45-current-feature-audit.md`) và so sánh intent-rebase (`46-ferrumgate-vs-intent-rebase.md`)

---

## Vị trí của FerrumGate

### Core Positioning

FerrumGate là hệ thống **governance** tập trung vào:

- **Intent-scoped execution** — Mọi action phải có intent scope rõ ràng
- **Single-use capability** — Capability chỉ được sử dụng một lần, không reuse
- **Provenance-first lineage** — Mọi side effect phải có provenance chain đầy đủ
- **Rollback-by-default** — R3 auto-rollback, không auto-commit
- **Capability TTL max 300s** — Hard limit

### Current Posture

| Khía cạnh | Trạng thái |
|---|---|
| Architecture | Single-node SQLite với write queue |
| RC status | RC-ready (conditional) — chưa production-ready |
| Invariant | 12 VERIFIED / 0 PARTIAL / 0 INFERRED |
| Phase 2 | Deferred/regressed |
| Phase 3 (PostgreSQL) | Planned, chưa triển khai |

---

## Nguyên tắc Novelty

### Nguyên tắc 1 — Không Overlap với Intent-Rebase

Mọi mở rộng phải tránh overlap với hệ thống intent-rebase tại `/home/uong_guyen/work/intent-rebase`.

**Kiểm tra overlap trước khi đề xuất**:
- Nếu tính năng mới có HIGH overlap với intent-rebase → Từ chối
- Nếu tính năng mới có MEDIUM overlap → Định nghĩa boundary rõ ràng
- Nếu tính năng mới có LOW overlap → Có thể explore

### Nguyên tắc 2 — Giữ vững Invariants

Không được phép phá vỡ các invariants đã VERIFIED. Các invariants PARTIAL cần được ưu tiên khắc phục trước novelty.

### Nguyên tắc 3 — Không vội vàng Production

FerrumGate hiện tại là **RC-ready/conditional single-node SQLite**. Không được claim đã hoàn thành production-ready. Mọi mở rộng phải ghi rõ production posture.

### Nguyên tắc 4 — Intent-Scoped, Capability-First

Mọi mở rộng phải tuân thủ:
- Intent-scoping bắt buộc
- Single-use capability
- Provenance emission
- Rollback prepare trước side effect

---

## Ứng viên Mở rộng (Non-Overlapping)

### Nhóm A — Storage Scaling

| # | Ứng viên | Mô tả | Overlap potential | Priority |
|---|---|---|---|---|
| A1 | PostgreSQL migration (Phase 3) | Hỗ trợ multi-node, HA | LOW — khác storage layer | Cao |
| A2 | Read replica support | Scale reads độc lập với writes | LOW — infrastructure | Trung bình |
| A3 | Connection pooling | Tối ưu hóa DB connections | LOW — infrastructure | Trung bình |

**Lý do không overlap**: Nhóm A tập trung vào infrastructure scaling, không thay đổi core abstraction của FerrumGate.

---

### Nhóm B — Observability

| # | Ứng viên | Mô tả | Overlap potential | Priority |
|---|---|---|---|---|
| B1 | Metrics export (Prometheus) | Export queue depth, latency | LOW — operations | Cao |
| B2 | Structured logging | Cải thiện log readability | LOW — operations | Trung bình |
| B3 | Trace propagation | End-to-end tracing | LOW — operations | Trung bình |

**Lý do không overlap**: Observability là operations concern, không thuộc core governance của intent-rebase.

---

### Nhóm C — API Extensions

| # | Ứng viên | Mô tả | Overlap potential | Priority |
|---|---|---|---|---|
| C1 | Bulk intent operations | Tạo nhiều intents cùng lúc | MEDIUM — cần boundary rõ | Thấp |
| C2 | Async intent submission | Non-blocking intent creation | MEDIUM — cần boundary rõ | Thấp |
| C3 | Webhook notifications | Notify external systems | LOW — infrastructure | Trung bình |

**Lý do cẩn thận**: API extensions có thể overlap với intent-rebase nếu cùng workflow pattern. Cần định nghĩa boundary.

---

### Nhóm D — Adapter Ecosystem

| # | Ứng viên | Mô tả | Overlap potential | Priority |
|---|---|---|---|---|
| D1 | Thêm adapter types | Mở rộng adapter surface | LOW — FerrumGate-owned | Cao |
| D2 | Adapter registry | Quản lý adapter versioning | LOW — infrastructure | Trung bình |
| D3 | Adapter testing framework | Standardized adapter tests | LOW — testing | Trung bình |

**Lý do không overlap**: Adapter ecosystem là FerrumGate-specific, không có trong intent-rebase (nếu xác minh được).

---

## Danh sách Từ chối / Tránh

### Từ chối (Reject List)

| # | Lý do từ chối | Ghi chú |
|---|---|---|
| R1 | Intent rebasing/reconciliation logic | HIGH overlap với intent-rebase |
| R2 | Generic workflow orchestration | HIGH overlap — FerrumGate chỉ governance |
| R3 | Generic lineage aggregation | HIGH overlap với intent-rebase |
| R4 | Multi-tenant isolation ở core layer | Phức tạp, chưa cần cho single-node |
| R5 | Generic rollback planning engine | HIGH overlap với intent-rebase |

### Tránh (Avoid List)

| # | Lý do tránh | Thay thế |
|---|---|---|
| A1 | Tự implement distributed consensus | Dùng PostgreSQL HA thay thế |
| A2 | Generic capability delegation | Giữ single-use, không mở rộng delegation |
| A3 | Caching layer cho capability lookup | Phức tạp hóa không cần thiết |
| A4 | GraphQL API surface | Giữ REST, không mở rộng GraphQL |

---

## Tiêu chí Ưu tiên

### Scoring Matrix

| Criteria | Weight | Description |
|---|---|---|
| **Production need** | 30% | Mức độ cần thiết cho production deployment |
| **Invariant improvement** | 25% | Cải thiện được PARTIAL/INFERRED invariants |
| **Overlap risk** | 20% | Risk overlap với intent-rebase |
| **Implementation complexity** | 15% | Effort để implement |
| **Rollback safety** | 10% | Không phá vỡ invariants hiện tại |

### Priority Ranking (Draft)

| Priority | Ứng viên | Score estimate |
|---|---|---|
| **P1** | PostgreSQL migration (A1) | Cao — production cần thiết |
| **P1** | Metrics export (B1) | Cao — operations cần thiết |
| **P2** | Read replica (A2) | Trung bình — scaling |
| **P2** | Adapter types expansion (D1) | Trung bình — capability |
| **P3** | Webhook notifications (C3) | Trung bình — integration |
| **P4** | Structured logging (B2) | Thấp — nice to have |
| **P5** | Bulk operations (C1) | Thấp — complexity cao |

---

## Trình tự Đề xuất (Sequencing)

### Giai đoạn 1 — Nền tảng (Foundation)

**Mục tiêu**: Cải thiện FerrumGate infrastructure mà không mở rộng scope

**Ưu tiên**:
1. **B1 (Metrics export)** — Cần thiết cho production monitoring
2. **A1 (PostgreSQL prep)** — Chuẩn bị migration path
3. **D1 (Adapter types)** — Mở rộng capability surface nhẹ

**Deliverables**:
- Metrics endpoint hoạt động
- PostgreSQL schema review
- ít nhất 1 new adapter type

---

### Giai đoạn 2 — Mở rộng có Kiểm soát (Controlled Expansion)

**Mục tiêu**: Mở rộng FerrumGate trong khi giữ invariants

**Ưu tiên**:
4. **A2 (Read replica)** — Scale reads
5. **C3 (Webhooks)** — Integration capability
6. **B2 (Structured logging)** — Operations improvement

**Deliverables**:
- Read replica configuration guide
- Webhook integration
- Structured log format

---

### Giai đoạn 3 — Nâng cao (Advanced)

**Mục tiêu**: Các tính năng phức tạp hơn, cần xác minh overlap

**Ưu tiên**:
7. **D2 (Adapter registry)** — Quản lý adapter versioning
8. **B3 (Trace propagation)** — End-to-end tracing
9. **D3 (Adapter testing)** — Framework

**Deliverables**:
- Adapter registry design doc
- Trace propagation spec
- Adapter testing framework

---

### Giai đoạn 4 — Xem xét (Consideration)

**Cần xác minh thêm overlap trước khi tiến hành**:

10. **C1 (Bulk operations)** — Cần boundary definition
11. **C2 (Async submission)** — Cần boundary definition

---

## Tiêu chí Hoàn thành

Roadmap được coi là hoàn thành khi:

- [ ] Tất cả ứng viên đã được đánh giá overlap risk
- [ ] Reject/avoid list đã được xác nhận
- [ ] Priority ranking đã được duyệt
- [ ] Sequencing đã được định nghĩa
- [ ] Không có claim production-ready không có cơ sở
- [ ] Invariants PARTIAL/INFERRED đã có trong P1-P2

---

## Tham khảo

- `docs/implementation-path/45-current-feature-audit.md` — Feature audit plan
- `docs/implementation-path/46-ferrumgate-vs-intent-rebase.md` — Comparison framework
- `docs/implementation-path/26-v1-single-node-invariant-control-test-evidence-matrix.md` — Invariant status
- `docs/implementation-path/27-production-evaluation-plan.md` — Production evaluation
- `/home/uong_guyen/work/intent-rebase` — Intent-rebase (so sánh)
