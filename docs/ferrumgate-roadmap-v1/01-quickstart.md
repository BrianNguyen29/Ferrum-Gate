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
- coi action là "xong" nếu chưa verify và chưa có lineage

## Khởi động nhanh

### Development mode

```bash
# Khởi động ferrumd với config mặc định (SQLite in-memory)
cargo run -p ferrumd

# Hoặc với config file dev (tự động load nếu có configs/ferrumgate.dev.toml)
cargo run -p ferrumd

# Kiểm tra health
ferrumctl server health

# Kiểm tra lineage
ferrumctl server inspect-lineage <execution_id>
```

### Production mode

```bash
# Tạo config mới hoặc chỉnh sửa configs/ferrumgate.prod.toml
# Cấu hình bearer token bảo mật

# Khởi động với config production
cargo run -p ferrumd -- --config configs/ferrumgate.prod.toml

# Hoặc qua environment variable
FERRUMD_CONFIG=configs/ferrumgate.prod.toml cargo run -p ferrumd
```

### Configuration precedence

1. CLI arguments (highest priority)
2. Environment variables (`FERRUMD_*`)
3. Config file
4. Defaults (lowest priority)

## CLI Commands

```bash
# Health check
ferrumctl server health

# Inspect execution
ferrumctl server inspect-execution <execution_id>

# List pending approvals
ferrumctl server inspect-approvals

# Inspect single approval
ferrumctl server inspect-approval <approval_id>

# Get execution lineage
ferrumctl server inspect-lineage <execution_id>

# Get execution lineage as DOT (Graphviz)
ferrumctl server inspect-lineage <execution_id> --format dot

# Get execution lineage as DOT and save to file
ferrumctl server inspect-lineage <execution_id> --format dot --output lineage.dot

# Query provenance (intent-id only; richer filtering available via HTTP at POST /v1/provenance/query)
ferrumctl server inspect-provenance --intent-id <id>
```

Environment variables:
- `FERRUMCTL_SERVER_URL`: Server URL (default: http://127.0.0.1:8080)
- `FERRUMCTL_BEARER_TOKEN`: Bearer token for authentication
