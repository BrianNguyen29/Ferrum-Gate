# 08 — Agent execution rules

## Purpose

File này dành cho AI agents hoặc engineer thực thi roadmap. Mục tiêu là giúp agent không phá scope, không làm sai thứ tự và không overclaim kết quả.

## 1. Đọc trước khi làm

Agent phải đọc theo thứ tự:
1. project canon
2. architecture
3. runtime flow
4. domain model
5. constraints and invariants
6. implementation path
7. crate-by-crate plan
8. file roadmap tương ứng trong pack này

## 2. Luật cứng

- Không bypass gateway cho mutation
- Không infer permission từ session continuity
- Không reuse capability
- Không auto-commit action R3
- Không thêm route nếu chưa có semantics thật
- Không đổi shape object lõi mà không cập nhật docs/spec/contracts/schemas/openapi
- Không đánh dấu adapter là production-ready nếu chưa có integration tests thật
- Không trả raw internal control data ra user plane

> **V1 boundary rule**: Any agent operating in the v1 support scope must check
> `19-v1-single-node-support-contract.md` before making claims about support boundaries.
> Adapter implementations and routes not listed in the v1 support contract are
> post-v1 scope, even if code for them exists in the repo.

## 3. Quy tắc làm task

Mỗi task phải ghi rõ:
- quarter hiện tại
- release target
- crate liên quan
- API liên quan (nếu có)
- invariant bị tác động
- tests cần thêm
- docs cần update

## 4. Quy tắc chia task

Một task chỉ nên rơi vào một trong các loại sau:
- crate internal refactor
- invariant closure
- API/documentation sync
- adapter implementation
- integration test
- operator/deployment surface

Không giao task “làm hết adapter” hoặc “xây UI hoàn chỉnh” trong một lần.

## 5. Định nghĩa done

Một task chỉ coi là done nếu:
- code compile
- test pass
- docs liên quan đã update
- nếu đổi API/schema thì openapi/contracts/schemas đã sync
- nếu đổi mutation path thì recovery/provenance tests đã update

## 6. Khi nào phải dừng lại

Agent phải dừng và báo risk nếu:
- gặp mâu thuẫn giữa docs và runtime route map
- không thể chứng minh recovery semantics cho adapter mới
- phát hiện accepted risk làm sai product claim
- thay đổi yêu cầu làm lệch support contract hoặc release scope

> **V1 boundary rule for stopping**: "support contract" here refers specifically to
> `19-v1-single-node-support-contract.md`. If a task or PR would expand v1 support scope
> without a formal amendment to that document, the agent must stop and flag this
> clearly. The roadmap pack describes planned post-v1 work; it does not create v1
> support obligations.

## 7. Quy tắc theo quý

### Q1
Ưu tiên correctness hơn feature.
Không thêm nhiều surface mới nếu invariant cũ còn hở.

### Q2
Ưu tiên adapter thật + demo workflow.
Không phân tán sang MCP hoặc evidence plane quá sớm.

### Q3
Ưu tiên productization self-hosted.
Không nhảy thẳng vào cloud-first hay multi-tenant SaaS.

### Q4
Ưu tiên runtime governance và evidence.
Không ôm quá nhiều framework/runtime cùng lúc.

## 8. Quy tắc review PR

PR cần được review theo 5 câu hỏi:
1. Có làm mạnh hơn intent boundary không?
2. Có làm chặt hơn capability boundary không?
3. Có cải thiện recovery path thật không?
4. Có giữ hoặc tăng provenance completeness không?
5. Có overclaim support scope không?

> **V1 boundary check for PR review**: Item 5 includes checking whether the PR
> implies a scope expansion of v1 support without a corresponding formal amendment
> to `19-v1-single-node-support-contract.md`. If the PR changes the supported route
> set, CLI surface, or known limitations described in the v1 support contract,
> the change must be gated on an explicit v1 contract amendment — not just a
> roadmap plan doc.
