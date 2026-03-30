# 01 — Quickstart

## Muc tieu

Giup agent hoac engineer moi hieu nhanh:
- doc gi truoc
- bat dau tu dau
- khong duoc pha gi

## Thu tu doc

1. `00-project-canon.md`
2. `02-project-overview.md`
3. `03-architecture.md`
4. `04-runtime-flow.md`
5. `05-domain-model.md`
6. `06-constraints-and-invariants.md`
7. `09-implementation-path.md`
8. `10-crate-by-crate-plan.md`

## Happy path toi thieu cua FerrumGate

1. compile intent
2. evaluate proposal
3. mint capability
4. prepare rollback
5. execute tool/adapters
6. verify
7. commit hoac compensate / rollback
8. emit provenance chain

## Dieu khong duoc lam

- dung session nhu quyen ngam
- goi mutation ma khong qua gateway
- bo qua capability validation
- bo qua rollback prepare
- commit R3 ma khong approval / draft-only
- coi action la "xong" neu chua verify va chua co lineage

## Khoi dong nhanh

### Development mode

```bash
# Khoi dong ferrumd voi config mac dinh (SQLite in-memory)
cargo run -p ferrumd

# Hoac voi config file dev (tu dong load neu co configs/ferrumgate.dev.toml)
cargo run -p ferrumd

# Kiem tra health
ferrumctl server health

# Kiem tra lineage
ferrumctl server inspect-lineage <execution_id>
```

### Production mode

```bash
# Tao config moi hoac chinh sua configs/ferrumgate.prod.toml
# Cau hinh bearer token bao mat

# Khoi dong voi config production
cargo run -p ferrumd -- --config configs/ferrumgate.prod.toml

# Hoac qua environment variable
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
