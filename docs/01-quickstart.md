# 01 — Quickstart

## Mục tiêu

Giúp agent hoặc engineer mới hiểu nhanh:
- đọc gì trước
- bắt đầu từ đâu
- không được phá gì

## Thứ tự đọc

1. `00-project-canon.md`
2. `02-project-overview.md`
3. `03-architecture.md`
4. `04-runtime-flow.md`
5. `05-domain-model.md`
6. `06-constraints-and-invariants.md`
7. `09-implementation-path.md`
8. `10-crate-by-crate-plan.md`

## Happy path tối thiểu của FerrumGate

1. compile intent
2. evaluate proposal
3. mint capability
4. prepare rollback
5. execute tool/adapters
6. verify
7. commit hoặc compensate / rollback
8. emit provenance chain

## Điều không được làm

- dùng session như quyền ngầm
- gọi mutation mà không qua gateway
- bỏ qua capability validation
- bỏ qua rollback prepare
- commit R3 mà không approval / draft-only
- coi action là “xong” nếu chưa verify và chưa có lineage
