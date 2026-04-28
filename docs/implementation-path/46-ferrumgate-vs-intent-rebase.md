# 46 — So sánh FerrumGate và Intent-Rebase

> **Trạng thái**: Bản nháp nghiên cứu — chưa có kết luận hoàn thành
> **Phạm vi**: So sánh khái niệm và kiến trúc, không kiểm tra nội bộ intent-rebase
> **Ràng buộc**: Không khẳng định internals của intent-rebase trừ khi đã được xác minh

---

## Mục đích

Tài liệu này định nghĩa phương pháp nghiên cứu và các chiều so sánh giữa:

- **FerrumGate** — Hệ thống governance với intent/capability/provenance lineage chain tại `/home/uong_guyen/work/ferrum-gate`
- **Intent-Rebase** — Hệ thống đối chiếu tại `/home/uong_guyen/work/intent-rebase`

**Mục tiêu**:
1. Xác định overlap tiềm ẩn giữa hai hệ thống
2. Định nghĩa rules để tránh overlap risk
3. Xác lập differentiation criteria cho FerrumGate
4. Cung cấp framework so sánh có kiểm chứng

---

## Ranh giới Phi-sản xuất

| Khía cạnh | Trạng thái FerrumGate | Lưu ý |
|---|---|---|
| Trạng thái | RC-ready/conditional | Single-node SQLite |
| Invariant | 12 VERIFIED / 0 PARTIAL / 0 INFERRED | Xem `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` |
| Production claim | **Không** | Chỉ conditional RC, chưa production-ready |

**Lưu ý quan trọng**: Tài liệu này không khẳng định intent-rebase có specific internals nào. Mọi so sánh phải được framed là research questions hoặc verification targets. Không claim completed findings.

---

## Phương pháp Nghiên cứu

### Bước 1 — Thu thập Thông tin Intent-Rebase

**Nguồn thông tin**:
- File structure tại `/home/uong_guyen/work/intent-rebase`
- README và tài liệu mô tả (nếu có)
- API surface (nếu có OpenAPI/spec)
- Test files (nếu có)

**Công việc**:
1. Khảo sát cấu trúc file của intent-rebase
2. Xác định các module/chức năng chính
3. Ghi nhận các keyword/term quan trọng
4. Xác định entry points và API

**Lưu ý**: Không phải kiểm tra Rust code. Chỉ thu thập thông tin qua file listing, README, và docs.

---

### Bước 2 — Xác định các Chiều So sánh

**Các chiều so sánh chính**:

| Chiều | FerrumGate | Intent-Rebase | Ghi chú |
|---|---|---|---|
| **Core abstraction** | Intent → Capability → Provenance | Cần xác minh | Research |
| **Governance model** | Intent-scoped, single-use capability | Cần xác minh | Research |
| **Rollback mechanism** | R0/R1/R2/R3 classes | Cần xác minh | Research |
| **Lineage tracking** | Full chain events | Cần xác minh | Research |
| **Storage model** | SQLite (Phase 1), PostgreSQL (Phase 3) | Cần xác minh | Research |
| **Multi-node support** | Chưa (Phase 3 deferred) | Cần xác minh | Research |

---

### Bước 3 — Xây dựng Ma trận Overlap

**Quy tắc Overlap Risk**:

| Level | Mô tả | Hành động |
|---|---|---|
| **HIGH** | Cùng core abstraction + cùng use case | Tránh hoàn toàn — FerrumGate tập trung vào differentiation |
| **MEDIUM** | Có điểm chung nhưng khác focus | Đánh dấu boundary rõ ràng |
| **LOW** | Chỉ shared primitives | Có thể hợp tác hoặc reuse |

**Rules**:
1. Nếu overlap = HIGH → FerrumGate không mở rộng vào đó
2. Nếu overlap = MEDIUM → FerrumGate định nghĩa boundary cụ thể
3. Nếu overlap = LOW → Có thể explore reuse strategy

---

### Bước 4 — Differentiation Criteria

**FerrumGate Differentiation Principles**:

| # | Nguyên tắc | Diễn giải |
|---|---|---|
| D1 | **Intent-scoped execution** | FerrumGate bắt buộc mọi action phải có intent scope rõ ràng |
| D2 | **Single-use capability** | Capability chỉ được sử dụng một lần, không reuse |
| D3 | **Provenance-first lineage** | Mọi side effect phải có provenance chain đầy đủ |
| D4 | **Rollback-by-default** | R3 auto-rollback, không auto-commit |
| D5 | **Capability TTL max 300s** | Hard limit, không exception |

**Questions cho Verification**:

| Question | Type | Target |
|---|---|---|
| Intent-Rebase có enforce intent-scoping không? | Research question | `/home/uong_guyen/work/intent-rebase` |
| Intent-Rebase có lineage chain không? | Research question | `/home/uong_guyen/work/intent-rebase` |
| Intent-Rebase có rollback mechanism không? | Research question | `/home/uong_guyen/work/intent-rebase` |
| Intent-Rebase có capability model không? | Research question | `/home/uong_guyen/work/intent-rebase` |

---

## Deliverables

| # | Deliverable | Định dạng | Trạng thái |
|---|---|---|---|
| D1 | File structure map của intent-rebase | Markdown list | Pending |
| D2 | Key terms/concepts identified | Markdown table | Pending |
| D3 | Chiều so sánh (verified facts) | Markdown table | Pending |
| D4 | Ma trận overlap risk | Markdown table | Pending |
| D5 | Differentiation statement | Markdown | Pending |
| D6 | Research questions còn lại | Markdown list | Pending |

---

## Tiêu chí Hoàn thành

So sánh được coi là hoàn thành khi:

- [ ] File structure của intent-rebase đã được khảo sát (D1)
- [ ] Key terms đã được xác định (D2)
- [ ] Ít nhất 4 chiều so sánh đã được verify bằng evidence (D3)
- [ ] Overlap risk matrix đã được xây dựng (D4)
- [ ] Differentiation statement đã được draft (D5)
- [ ] Research questions còn lại đã được ghi nhận (D6)
- [ ] Không có claim nào về internals intent-rebase mà không có evidence

---

## Tham khảo

- `docs/implementation-path/06-guardrails-and-invariants.md` — FerrumGate invariants
- `docs/implementation-path/26-EV-v1-single-node-invariant-control-test-evidence-matrix.md` — Invariant status
- `docs/implementation-path/47-novelty-roadmap.md` — Novelty roadmap (dựa trên so sánh này)
- `/home/uong_guyen/work/intent-rebase` — Intent-rebase repository (so sánh)
