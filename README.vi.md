# FerrumGate

> **Scoped, Auditable, Reversible.**

Một governance gateway dành cho AI agents và hệ thống dùng công cụ, thay thế quyền thường trực (ambient authority) bằng capability có phạm vi rõ ràng, chỉ dùng một lần — để mọi hành động đều được kiểm tra policy, chuẩn bị rollback và ghi lại provenance.

[![ci](https://github.com/BrianNguyen29/Ferrum-Gate/actions/workflows/ci.yml/badge.svg)](https://github.com/BrianNguyen29/Ferrum-Gate/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)](https://www.rust-lang.org/)

[English](./README.md) | [Tài liệu](./docs/README.md) | [Quickstart](./docs/guides/quickstart.md) | [Operator Guide](./docs/guides/operator.md) | [Security Model](./docs/guides/security-model.md)

---

## Mục lục

- [Tại sao cần gateway?](#tại-sao-cần-gateway)
- [Dành cho ai?](#dành-cho-ai)
- [Điểm khác biệt](#điểm-khác-biệt)
- [Mô hình tin cậy](#mô-hình-tin-cậy)
- [Trạng thái triển khai](#trạng-thái-triển-khai)
- [So sánh](#so-sánh)
- [Vòng đời thực thi](#vòng-đời-thực-thi)
- [Kiến trúc](#kiến-trúc)
- [Adapter surface](#adapter-surface)
- [Quickstart](#quickstart)
- [Vận hành FerrumGate](#vận-hành-ferrumgate)
- [Hiệu năng & validation](#hiệu-năng--validation)
- [Trạng thái dự án](#trạng-thái-dự-án)
- [CLI và binaries](#cli-và-binaries)
- [FAQ](#faq)
- [Lộ trình](#lộ-trình)
- [Cấu trúc repository](#cấu-trúc-repository)
- [Lộ trình đọc tài liệu](#lộ-trình-đọc-tài-liệu)
- [Đóng góp](#đóng-góp)
- [License](#license)

---

## Tại sao cần gateway?

AI agents đang dần trở thành operator: chúng ghi file, thay đổi Git repository, gọi API, cập nhật database và soạn thông điệp vận hành. API key truyền thống và quyền tool rộng trả lời được câu hỏi "ai được gọi API?", nhưng không trả lời được:

- Agent đang cố đạt mục tiêu gì?
- Hành động này có nằm trong scope người dùng đã khai báo không?
- Policy decision nào đã cho phép hành động?
- Rollback đã được chuẩn bị trước khi execute chưa?
- Verify có chứng minh kết quả khớp intent không?
- Operator có thể dựng lại lineage sau crash hoặc incident không?

Một governance gateway đứng giữa agent và công cụ, đưa mỗi side effect vào một vòng đời có kiểm soát: khai báo intent, đánh giá policy, cấp capability có phạm vi, chuẩn bị rollback, thực thi, verify và ghi provenance. **Agent không nên có quyền thường trực và rộng trên hệ thống quan trọng.** Agent chỉ nên nhận quyền hẹp, ngắn hạn, giải thích được, cho một hành động cụ thể, với rollback và lineage được chuẩn bị trước khi side effect xảy ra.

## Dành cho ai?

| Đối tượng | FerrumGate mang lại |
|-----------|---------------------|
| **Platform teams** | Control plane có boundary rõ ràng cho agent tooling với policy bundle, auth mode và audit scaffolding. |
| **Security engineers** | Capability có phạm vi, vòng đời fail-closed, chuỗi provenance và rollback contract thay vì API key thường trực. |
| **Operators** | Health/readiness probe, metrics, backup/restore drill, CLI workflow và Helm deployment scaffolding. |
| **Integration designers** | Adapter contract cho filesystem, Git, HTTP, SQLite và mail draft với boundary control rõ ràng. |

## Điểm khác biệt

| Năng lực | FerrumGate bổ sung |
|----------|--------------------|
| Intent-first execution | Side effect được gắn với intent đã khai báo, không chỉ là một tool call thô. |
| Capability single-use | Execution cần lease ngắn hạn, có scope, ràng buộc với intent, proposal, tool và resource. |
| Rollback trước action | Recovery contract được tạo trước khi adapter thực hiện side effect. |
| Provenance-first lineage | Policy, capability, prepare, execute, verify và terminal state đều được ghi thành chuỗi audit. |
| Fail-closed lifecycle | Thiếu lineage, state transition sai, lease stale, mutating tool không rõ binding hoặc recovery incomplete đều bị chặn. |
| Operator-ready controls | Health, deep readiness, metrics, backup/restore, lifecycle outbox review, policy bundles và CLI workflow có sẵn. |

## Mô hình tin cậy

| Đảm bảo | Trách nhiệm |
|---------|-------------|
| Intent-to-action binding và policy evaluation | **FerrumGate** — ép buộc trước mọi adapter call. |
| Single-use capability minting với TTL enforcement | **FerrumGate** — tối đa 300s, hardcoded trong `ferrum-cap`. |
| Rollback prepare / verify / compensate contracts | **FerrumGate** — sinh và validate trước side effects. |
| Audit log append-only provenance chain | **FerrumGate** — hướng tới evidence; không phải WORM/compliance certification. |
| TLS termination, secret management, network policy | **Operator** — nằm ngoài boundary của gateway. |
| Database HA, backup policy, alert routing | **Operator** — SQLite là single-node; PostgreSQL runtime được hỗ trợ, nhưng production HA/multi-node topology không được quản lý bởi repository này. |
| Production acceptance testing cho môi trường của bạn | **Operator** — FerrumGate nêu rõ mức validation và trách nhiệm còn lại của operator. |

## Trạng thái triển khai

| Tier | Định nghĩa | Các thành phần hiện tại |
|------|-----------|------------------------|
| **Stable** | Core model đã triển khai, CI-tested, phù hợp cho local evaluation và controlled pilot. | Intent lifecycle, policy evaluation, capability minting, rollback prepare/verify/compensate, SQLite write queue, provenance chain, bearer/scoped/OIDC/agent auth. |
| **Implemented** | Feature-complete cho use case tiêu chuẩn; local và CI-validated. | Filesystem, HTTP, Git, SQLite, mail draft adapter; `ferrumctl` CLI; `ferrum-stress` smoke test; `ferrum-tui` dashboard; Prometheus metrics; rate limiting; Helm chart. |
| **Beta** | Functional nhưng có thể cần operator tuning hoặc có caveat đã biết. | PostgreSQL runtime — local và CI live-tested; production HA/multi-node topology không được quản lý bởi repo. |
| **Experimental** | Skeleton hoặc partial implementation; chưa sẵn sàng production. | MCP Streamable HTTP / SSE transport và resumability. |
| **Not implemented** | Single-tenant by design; không cam kết roadmap. | Multi-tenancy, managed service, gửi email (maildraft chỉ quản lý draft), compliance certification. |

> **Lưu ý trung thực**: FerrumGate không phải turnkey HA product hoặc compliance certification. Operator vẫn chịu trách nhiệm về deployment topology, TLS, secrets, backup, database HA và production acceptance testing.

## So sánh

| Chiều | Raw API Keys | Policy-as-Code (static) | Audit Logging (post-hoc) | Raw MCP | FerrumGate |
|-------|-------------|--------------------------|------------------------|---------|------------|
| Intent binding | Không | Hạn chế | Không | Không | Có — mọi action gắn với intent đã khai báo. |
| Single-use capability | Không | Không | Không | Không | Có — lease ngắn hạn, có scope cho mỗi action. |
| Policy enforcement point | Không | Config-time hoặc admission | Không | Không | Có — runtime policy evaluation trước execution. |
| Rollback preparation | Không | Không | Không | Không | Có — prepare/verify/compensate contracts. |
| Provenance lineage | Không | Không | Log một phần | Không | Có — chuỗi lifecycle đầy đủ: policy → capability → prepare → execute → verify → terminal. |
| Fail-closed unknown tools | Không | Không | Không | Không | Có — tool không rõ binding bị chặn trừ khi explicitly bound. |
| Operator controls | Không | Hạn chế | Không | Không | Có — health, readiness, metrics, CLI, backup/restore, Helm. |

## Vòng đời thực thi

```text
Intent
  -> Proposal
  -> PolicyEvaluated
  -> CapabilityMinted
  -> ActionProposalSubmitted
  -> SideEffectPrepared
  -> ToolCallPrepared
  -> ToolCallExecuted
  -> SideEffectVerified
  -> Terminal state
```

Terminal state có thể là committed, compensated, rolled back, failed hoặc recovery-incomplete. Vòng đời này được bảo vệ bằng store-backed transitions, outbox reconciliation, fencing token và lineage gate.

## Kiến trúc

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
- Các crate chính bổ sung: `ferrum-firewall` và bounded adapters (`ferrum-adapter-fs`, `ferrum-adapter-git`, `ferrum-adapter-http`, `ferrum-adapter-sqlite`, `ferrum-adapter-maildraft`).

## Adapter surface

FerrumGate có các adapter slice có boundary rõ ràng cho side effect phổ biến của agent:

| Adapter | Phạm vi |
|---------|---------|
| Filesystem | Write, delete, move, copy, append, chmod, tạo/xóa thư mục với sandbox và snapshot. |
| Git | Commit, tạo/xóa branch, tạo/xóa tag với repository-root allowlist. |
| HTTP | HTTP mutation với client rustls, không follow redirect, timeout giới hạn, SSRF guard, replay recovery contract. |
| SQLite | Mutation trên SQLite file-backed với database-root allowlist và verification gate. |
| Mail draft | Vòng đời tạo/cập nhật/xóa draft với recipient và content binding. **Không gửi email.** |

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

> **Lưu ý:** Ví dụ này cố ý ghi đè port của dev config qua `FERRUMD_BIND_ADDR`; dev config đơn lẻ sử dụng `8080`. Với deployment production-like, hãy build release binary (`cargo build --release`) và chạy `./target/release/ferrumd` với production config.

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

## Vận hành FerrumGate

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

## Hiệu năng & validation

Xem [`docs/PRODUCTION_NOTES.md`](./docs/PRODUCTION_NOTES.md) để biết chi tiết stress-test baseline, tuning SQLite write-queue và hướng dẫn scale PostgreSQL.

Highlights từ local validation (release binary, sau write-queue):

| Scenario | Throughput | p50 Latency | Error Rate |
|----------|------------|-------------|------------|
| Health (50 workers) | ~33,000 req/s | 1.3ms | 0% |
| Execution pipeline (5 workers) | ~58 pipelines/s | 16ms | 0% |
| SQLite contention (50 workers) | ~289 req/s | 30ms | 0% |

> Đây là engineering benchmark local, không phải production guarantee. Kết quả thực tế phụ thuộc vào phần cứng, store choice và workload shape.

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

## Trạng thái dự án

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

## FAQ

**Q: FerrumGate có phải managed service hoặc SaaS không?**
> Không. Đây là open-source software bạn tự chạy trên infrastructure của mình. Single-tenant by design.

**Q: FerrumGate có gửi email không?**
> Không. Mail draft adapter quản lý vòng đời tạo/cập nhật/xóa draft với recipient và content binding. Nó không gửi email.

**Q: MCP HTTP/SSE có được hỗ trợ không?**
> stdio MCP đã triển khai và locally validated. Streamable HTTP / SSE và resumability đang ở trạng thái experimental, chưa sẵn sàng production.

**Q: FerrumGate có cung cấp compliance certification (SOC 2, ISO 27001, v.v.) không?**
> Không. FerrumGate cung cấp provenance và evidence chain hướng tới audit. Compliance certification nằm ngoài phạm vi của open-source project.

**Q: PostgreSQL có production-HA out-of-the-box không?**
> PostgreSQL runtime được hỗ trợ và CI live-tested. Production HA/multi-node topology, replication và failover là trách nhiệm của operator và không được quản lý bởi repository này.

**Q: Nhiều tenant có thể chia sẻ một instance FerrumGate không?**
> Không. Multi-tenancy chưa triển khai; FerrumGate là single-tenant by design.

**Q: Sự khác biệt giữa `cargo run` và release binary trong quickstart là gì?**
> `cargo run` compile và chạy ở debug mode — phù hợp cho local development. Với deployment production-like hoặc pilot, hãy dùng `cargo build --release` và chạy binary `./target/release/ferrumd`.

**Q: Làm thế nào để báo cáo vấn đề bảo mật?**
> Vui lòng mở private security advisory qua GitHub Security Advisories cho repository này.

## Lộ trình

Hướng đi và ưu tiên ngắn hạn:

| Lĩnh vực | Hướng đi | Trạng thái |
|----------|----------|------------|
| Core governance lifecycle | Ổn định intent → policy → capability → execution → verify → provenance | Stable |
| SQLite performance | Write queue + PRAGMA tuning đã validated; operator tuning guide có sẵn | Stable |
| PostgreSQL support | Runtime và CI live tests passing; HA topology vẫn thuộc về operator | Beta |
| MCP integration | stdio tools validated; HTTP/SSE deferred | Experimental |
| Operator experience | ferrumctl, ferrum-tui, Helm chart, monitoring rules, backup/restore drills | Implemented |
| Multi-tenancy | Không nằm trong roadmap hiện tại | Not implemented |
| Compliance certification | Ngoài phạm vi open-source project | Not implemented |

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

[Apache-2.0](./LICENSE)
