# FerrumGate

**Nền tảng thực thi có kiểm soát, có thể hoàn tác, dành cho AI agents và hệ thống dùng công cụ.**

[English](./README.md) | [Tài liệu](./docs/README.md) | [Quickstart](./docs/guides/quickstart.md) | [Operator Guide](./docs/guides/operator.md) | [Security Model](./docs/guides/security-model.md)

FerrumGate là một governance gateway nằm giữa agent tự động và các công cụ mà agent muốn sử dụng. Thay vì cho agent gọi tool trực tiếp bằng quyền rộng, FerrumGate đưa mỗi side effect vào một vòng đời có kiểm soát: khai báo intent, đánh giá policy, cấp capability có phạm vi, chuẩn bị rollback, thực thi, verify kết quả và ghi provenance để audit.

Ý tưởng sản phẩm rất rõ: **agent không nên có quyền thường trực và rộng trên hệ thống quan trọng**. Agent chỉ nên nhận quyền hẹp, ngắn hạn, giải thích được, cho một hành động cụ thể, với rollback và lineage được chuẩn bị trước khi side effect xảy ra.

## Vì Sao FerrumGate Cần Tồn Tại

AI agents đang dần trở thành operator: chúng ghi file, thay đổi Git repository, gọi API, cập nhật database và soạn thông điệp vận hành. API key truyền thống và quyền tool rộng không đủ cho mô hình này. Chúng trả lời được câu hỏi "ai được gọi API?", nhưng không trả lời được:

- Agent đang cố đạt mục tiêu gì?
- Hành động này có nằm trong scope người dùng đã khai báo không?
- Policy decision nào đã cho phép hành động?
- Rollback đã được chuẩn bị trước khi execute chưa?
- Verify có chứng minh kết quả khớp intent không?
- Operator có thể dựng lại lineage sau crash hoặc incident không?

FerrumGate được thiết kế xoay quanh các câu hỏi đó.

## Điểm Khác Biệt

| Năng lực | FerrumGate bổ sung |
|----------|--------------------|
| Intent-first execution | Side effect được gắn với intent đã khai báo, không chỉ là một tool call thô. |
| Capability single-use | Execution cần lease ngắn hạn, có scope, ràng buộc với intent, proposal, tool và resource. |
| Rollback trước action | Recovery contract được tạo trước khi adapter thực hiện side effect. |
| Provenance-first lineage | Policy, capability, prepare, execute, verify và terminal state đều được ghi thành chuỗi audit. |
| Fail-closed lifecycle | Thiếu lineage, state transition sai, lease stale, mutating tool không rõ binding hoặc recovery incomplete đều bị chặn. |
| Operator-ready controls | Health, deep readiness, metrics, backup/restore, lifecycle outbox review, policy bundles và CLI workflow có sẵn. |

## Giá Trị Cốt Lõi

- **Quyền tối thiểu**: không cấp quyền rộng và thường trực cho agent; mọi hành động đều có scope và TTL.
- **Reversibility by design**: rollback class và recovery path là một phần của execution contract.
- **Traceability over trust**: quyết định quan trọng được ghi thành provenance, không phụ thuộc trí nhớ vận hành.
- **Fail closed under ambiguity**: trạng thái không rõ, stale, incomplete hoặc chưa verify được xem là không an toàn.
- **Operational honesty**: FerrumGate hướng tới vận hành production, nhưng tài liệu luôn nêu rõ mức validation và trách nhiệm còn lại của operator.

## Vòng Đời Thực Thi

```text
Intent
  -> Proposal
  -> PolicyEvaluated
  -> CapabilityMinted
  -> SideEffectPrepared
  -> ToolCallPrepared
  -> ToolCallExecuted
  -> SideEffectVerified
  -> Terminal state
```

Terminal state có thể là committed, compensated, rolled back, failed hoặc recovery-incomplete. Vòng đời này được bảo vệ bằng store-backed transitions, outbox reconciliation, fencing token và lineage gate.

## Kiến Trúc

```text
Agent / Client
    |
    v
Ferrum Gateway
    |-- policy evaluation
    |-- capability minting
    |-- authorization
    |-- rollback prepare
    |-- adapter execution
    |-- verification
    |-- provenance and lineage
    |
    v
Store: SQLite hoặc PostgreSQL
```

Các crate chính:

- `ferrum-gateway`: HTTP API, auth, route, lifecycle orchestration, metrics.
- `ferrum-proto`: domain types và API models dùng chung.
- `ferrum-pdp`: policy decision point và policy bundle evaluation.
- `ferrum-cap`: capability minting, single-use lease, TTL enforcement.
- `ferrum-rollback`: prepare, execute, verify, rollback, compensate contracts.
- `ferrum-store`: SQLite/PostgreSQL persistence, migrations, reconciliation, audit state.
- `ferrum-ledger` và `ferrum-graph`: nền tảng tamper-evident và lineage graph.
- `ferrum-sync` và `ferrum-integrations-mcp`: tích hợp MCP và runtime bridge.

## Adapter Surface

FerrumGate có các adapter slice có boundary rõ ràng cho side effect phổ biến của agent:

| Adapter | Phạm vi |
|---------|---------|
| Filesystem | Write, delete, move, copy, append, chmod, tạo/xóa thư mục với sandbox và snapshot. |
| Git | Commit, tạo/xóa branch, tạo/xóa tag với repository-root allowlist. |
| HTTP | HTTP mutation với client rustls, không follow redirect, timeout giới hạn, SSRF guard, replay recovery contract. |
| SQLite | Mutation trên SQLite file-backed với database-root allowlist và verification gate. |
| Mail draft | Vòng đời tạo/cập nhật/xóa draft với recipient và content binding. |

Mutating tool không rõ binding sẽ fail closed, trừ khi proposal cung cấp typed adapter/action binding rõ ràng.

## Quickstart

Yêu cầu:

- Rust stable
- `cargo`
- `make`
- `curl`

Chạy gateway local:

```bash
FERRUMD_BIND_ADDR=127.0.0.1:18080 \
cargo run -p ferrumd -- --config configs/ferrumgate.dev.toml
```

Kiểm tra liveness:

```bash
curl http://127.0.0.1:18080/v1/healthz
```

Kết quả mong đợi:

```json
{"status":"ok"}
```

Luồng demo đầy đủ:

- [FerrumGate in 10 Minutes](./docs/guides/quickstart.md)
- [API guide](./docs/guides/api.md)
- [MCP integration](./docs/guides/mcp-integration.md)
- [Demo flows](./docs/guides/demo-flows.md)

## Vận Hành FerrumGate

Không dùng development config cho môi trường exposed. Với deployment production-like, hãy bật bearer auth, chọn store phù hợp, cấu hình adapter allowlist, dùng deep readiness, metrics và backup/restore drill.

Tài liệu quan trọng:

- [Operator Guide](./docs/guides/operator.md)
- [Runtime Configuration Notes](./docs/PRODUCTION_NOTES.md)
- [Hosted Deployment](./docs/guides/hosted-deployment.md)
- [Zero-Downtime Upgrade](./docs/guides/zero-downtime-upgrade.md)
- [Troubleshooting](./docs/guides/troubleshooting.md)
- [Monitoring Config](./configs/monitoring/README.md)
- [Helm Chart](./deploy/helm/ferrumgate/README.md)

Các mặc định hướng production:

- bearer-auth mode cho exposed deployment
- deep readiness endpoint kiểm tra store, queue và pool
- Prometheus metrics
- release governance CI gate
- hardcoded secret scan
- dependency audit gate
- PostgreSQL live tests
- boundary control cho filesystem, Git và SQLite adapter

## Trạng Thái Dự Án

FerrumGate là một dự án engineering đang phát triển, đã có gateway hoạt động, adapter, persistence, CLI tooling, tests, docs và deployment scaffold. Dự án phù hợp cho local evaluation, integration design, security review và controlled pilot.

FerrumGate **không tự động đồng nghĩa với compliance certification hoặc turnkey HA product**. Operator vẫn chịu trách nhiệm về deployment topology, TLS termination, secret management, backup policy, alert routing, database HA và production acceptance testing trong môi trường của mình.

## CLI Và Binaries

| Binary | Mục đích |
|--------|----------|
| `ferrumd` | Gateway daemon. |
| `ferrumctl` | CLI cho health, readiness, audit, policy, approvals, lifecycle outbox, backup/restore. |
| `ferrum-migrate` | Hỗ trợ migration SQLite sang PostgreSQL. |
| `ferrum-stress` | Stress/smoke scenarios có output machine-readable. |
| `ferrum-tui` | Terminal dashboard cho operator. |

## Validation

Các gate local thường dùng:

```bash
make fmt
make check
make lint
make test
make docs
make validate
make audit
make secret-scan
```

CI chạy layout validation, contract consistency checks, formatting, workspace check, clippy, tests, release governance và PostgreSQL live tests.

## Cấu Trúc Repository

```text
bins/                 ferrumd, ferrumctl, ferrum-migrate, ferrum-stress, ferrum-tui
crates/               Rust workspace crates
configs/              dev/prod/example runtime configuration
contracts/            machine-readable agent and integrator contracts
docs/                 guides, architecture, security, operations, diagrams
openapi/              control API specification
schemas/              JSON Schemas for core contracts
deploy/helm/          Kubernetes chart
scripts/              validation, drills, backup, governance, smoke tests
site/                 static documentation site scaffold
```

## Lộ Trình Đọc Tài Liệu

Nếu mới bắt đầu, nên đọc theo thứ tự:

1. [Concepts](./docs/guides/concepts.md)
2. [Quickstart](./docs/guides/quickstart.md)
3. [Adapter Reference](./docs/guides/adapter-reference.md)
4. [Security Model](./docs/guides/security-model.md)
5. [Operator Guide](./docs/guides/operator.md)
6. [Production Notes](./docs/PRODUCTION_NOTES.md)

## Đóng Góp

Xem [CONTRIBUTING.md](./CONTRIBUTING.md). Hãy giữ thay đổi đúng scope, bảo toàn các invariant intent/capability/provenance/rollback và cập nhật docs/tests khi contract hoặc schema thay đổi.

## License

Xem [LICENSE](./LICENSE).
