# I11 — Output Sanitization Gateway Wiring Design

> **Trạng thái**: Post-implementation record — v1 bounded scope implemented and verified
> **Bounded scope**: v1 gateway wiring + integration tests (7 targeted endpoints)
> **Không phải**: full production-ready claim, middleware toàn cục, thay đổi public contract

---

## 1. Tổng quan

| Thuộc tính | Chi tiết |
|---|---|
| **Invariant** | I11 — Output sanitization |
| **Current state** | `sanitize_output` implemented in `ferrum-firewall`; `GatewayRuntime` has `firewall: Arc<dyn Firewall>`; **7 gateway call sites wired** (revoke_capability, delete_policy_bundle, set_policy_bundle_active, get_execution, get_execution_lineage, query_lineage, list_bridge_tools); **2 integration tests pass**; I11 is VERIFIED per invariant matrix |
| **Goal** | Wire bounded sanitization vào gateway response path mà không phải full response middleware |
| **Production ready** | Bounded v1 scope verified — full production-ready claim remains conditional (see production evaluation plan) |

---

## 2. Hiện trạng và Bằng chứng

### 2.1 Đã có

- **`SemanticFirewall::sanitize_output`** — implemented trong `crates/ferrum-firewall/src/lib.rs`
  - Unit tests: kiểm tra control char stripping, JSON safety
  - Input: arbitrary string; Output: sanitized string (null bytes, newlines, tabs stripped)
  - Không buffer body — operates trên string slice

- **`GatewayRuntime`** đã hold `firewall: Arc<dyn Firewall>` (không nhất thiết `SemanticFirewall`)

- 7 call sites wired trong gateway handlers — I11 VERIFIED (integration tests pass)

### 2.2 Gap chính

Không có integration giữa `sanitize_output` và gateway response path. Chỉ ở trait-level, chưa wired.

---

## 3. Tùy chọn Đã Xem xét

### Option A — Full Axum response middleware layer

**Mô tả**: Global `Layer` inject sanitization vào tất cả `Response` bodies.

**Lý do reject cho v1**:
- **Blast radius**: Toàn bộ response body bị buffer và parse — overhead lớn cho tất cả endpoints
- **Body buffering**: Toàn bộ JSON body phải fit vào memory trước khi sanitize
- **Serde overhead**: Cần deserialize/reerialize toàn bộ response để sanitize
- **Too coarse**: Không cần sanitize toàn bộ response — chỉ một số field

### Option B — Per-endpoint inline calls

**Mô tả**: Gọi `sanitize_output` trực tiếp trong mỗi handler.

**Nhược điểm**:
- **Fragile**: Dễ miss endpoint khi thêm mới
- **Inconsistent**: Mỗi developer có thể sanitize khác nhau
- **Boilerplate**: Lặp lại nhiều nơi

**Kết luận**: Chấp nhận được cho v1 nếu tập trung vào bounded set.

### Option C — `SanitizedJson<T>` wrapper

**Mô tả**: Typed wrapper serializes với sanitization.

**Nhược điểm**:
- **Extra abstraction**: Thêm generic wrapper cho mỗi response type
- **Defer**: Chỉ justify khi endpoint count grows significantly

**Kết luận**: Defer cho post-v1.

### Option D — Hybrid: Error-first + Targeted high-risk endpoints *(Recommended for v1)*

**Mô tả**:
1. Sanitize reflected error messages tại actual risky construction sites hoặc với small helper
2. Sanitize targeted success responses carrying user/provenance/tool metadata

**Ưu điểm**:
- Bounded blast radius
- Clear scope — chỉ risk-critical endpoints
- Incremental — có thể extend sau
- Không overhead cho phần lớn endpoints

---

## 4. Recommended v1 Design (Hybrid)

### 4.1 Error message sanitization

- **Không** dùng global middleware
- Tại các điểm xây dựng error response có chứa user input hoặc tool output được reflect:
  - Error messages từ adapter failures
  - Error messages từ capability/intent parsing
  - Provenance event descriptions
- Gọi `sanitize_output` inline hoặc qua small helper (`sanitize_error_message`)

### 4.2 Targeted success endpoint sanitization

Chỉ sanitize response cho các endpoints mang metadata nhạy cảm:

| Endpoint | Lý do |
|---|---|
| `revoke_capability` | Reflects capability metadata; potential user input in capability ID |
| `delete_policy_bundle` | Reflects policy bundle name/description |
| `set_policy_bundle_active` | Reflects bundle activation status + metadata |
| `get_execution` | Reflects tool call outputs + provenance chain |
| `get_execution_lineage` | Reflects full lineage event chain |
| `query_lineage` | Reflects lineage query results |
| `list_bridge_tools` | Reflects tool names + descriptions từ bridge |

### 4.3 Sanitization rules

- **Giữ nguyên JSON structure** — không thay đổi key names, types, nesting
- **Chỉ sanitize string values** — strip null bytes `\x00`, control chars `\x01-\x1F` (except `\t\n`)
- **Không sanitize numeric/bool/null values**
- **Không buffer full response** — operate on individual string fields

### 4.4 Non-goals (v1)

- ❌ Full response middleware
- ❌ Tất cả endpoints
- ❌ Request sanitization
- ❌ DLP
- ❌ Production output security claim
- ❌ Thay đổi public contract

---

## 5. Implementation Handoff

### 5.1 Khi nào I11 chuyển VERIFIED

I11 chỉ được đánh dấu VERIFIED khi:
1. Sanitization đã wired vào gateway call sites
2. Integration tests pass cho:
   - Provenance/lineage response chứa control char → sanitized output
   - Reflected error message stripping control char
3. Workspace check/clippy/test pass

### 5.2 Fixer handoff

- **Owner**: Fixer
- **Trigger**: Sau khi design note approved
- **Scope**:
  - Thêm `use ferrum_firewall::SemanticFirewall` vào gateway handlers
  - Inline `sanitize_output` call tại risk-critical construction sites
  - **Không** implement full middleware
  - Thêm 2 integration tests (xem 6.2)

### 5.3 Open points before implementation

1. **Exact error helper style**: Inline sanitize tại risky sites vs helper function
2. **Exact endpoint final scope**: Có thể thêm/bớt tùy feedback

---

## 6. Test Plan

### 6.1 Keep existing tests

- `ferrum-firewall` unit tests cho `sanitize_output` — giữ nguyên
- Không remove bất kỳ existing test nào

### 6.2 New integration tests (required for VERIFIED)

**Test 1 — Provenance/lineage response sanitization**
```
Input: lineage response chứa tool output với control chars (e.g., "value\x00with\x01null")
Expected: sanitized output không chứa \x00, \x01
```

**Test 2 — Reflected error message sanitization**
```
Input: error message reflect user input chứa control chars
Expected: control chars stripped; error still returns 4xx
```

### 6.3 Post-implementation validation

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Run affected integration tests
cargo test --workspace --test '*integration*' sanitize
```

---

## 7. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| 2026-04-27 | Chọn hybrid (D) cho v1 | Bounded blast radius, clear scope, incremental |
| 2026-04-27 | Reject full middleware (A) cho v1 | Overhead, body buffering, too coarse |
| 2026-04-27 | Defer SanitizedJson<T> (C) | Chỉ justify khi endpoint count grows |
| 2026-04-27 | Production deferred | Cần integration tests + validation trước |

---

## 8. References

- `crates/ferrum-firewall/src/lib.rs` — `SemanticFirewall::sanitize_output`
- `crates/ferrum-gateway/src/server.rs` — GatewayRuntime + handlers
- `crates/ferrum-gateway/src/error.rs` — Error type construction
- `docs/implementation-path/45-current-feature-audit.md` — I11 VERIFIED status (2026-04-27)
- `docs/implementation-path/26-v1-single-node-invariant-control-test-evidence-matrix.md` — Invariant matrix
