# 06 — Constraints and invariants

Đây là tài liệu quan trọng nhất sau project canon.

## 1. Intent invariants
- `allowed_outcomes.length >= 1`
- `expires_at > created_at`
- nếu `risk_tier = Critical` thì `approval_mode != None`
- nếu default rollback class là `R3` thì approval mode phải đủ chặt

## 2. Capability invariants
- TTL mặc định 15 giây
- TTL tối đa 300 giây trong v1
- single-use only
- `resource_bindings subset_of intent.resource_scope`
- approval binding phải khớp digest

## 3. Taint invariants
- nếu `taint_score > max_taint_score` thì action mutation không được execute
- external tool output / external metadata / untrusted lineage phải bị xem là risky

## 4. Rollback invariants
- `R2` phải có compensation plan
- `R3` không được auto-commit
- `EmailSend` luôn là `R3` trong v1

## 5. Provenance invariants
- side effect meaningful phải có minimum lineage chain
- lineage không được bị “mất event” ở các bước gate quan trọng

## 6. Output invariants
- output phải sanitize trước khi trả lên agent/user plane
- secrets/PII/internal control data không được lộ tùy tiện

## 7. System invariants
- không bypass gateway cho mutation
- không infer permission từ session continuity
- không được đổi shape object lõi mà không update docs/spec
