# 16 — Release checklist

## Contract integrity
- [x] contracts cập nhật (`python3 scripts/check_contract_consistency.py` => `VALIDATION PASSED`)
- [x] schemas cập nhật
- [x] openapi cập nhật (`openapi/ferrumgate-control-api.v1.yaml` parsed and matches current routes)
- [x] docs cập nhật (`docs/01-quickstart.md`, `docs/14-api-and-contracts-map.md`, `docs/15-deployment-and-operations.md`, `docs/17-troubleshooting.md`)

## Workspace quality
- [x] cargo check pass (`cargo check --workspace`)
- [x] fmt pass (`cargo fmt --all --check`)
- [x] clippy pass (`cargo clippy --workspace -- -D warnings`)
- [x] test pass (`cargo test --workspace`)

## Behavior quality
- [x] scope mismatch deny test
- [x] single-use capability test
- [x] R3 no auto-commit test
- [x] rollback/compensate test
- [x] poisoned context test

## Operator readiness
- [x] config docs đúng (config precedence, auth mode, startup guard documented)
- [x] CLI hữu dụng tối thiểu (`ferrumctl server health/inspect-*` documented and implemented)
- [x] lineage usable
- [x] approval flow documented (state transitions, CLI examples, and resolve-approval command)
- [x] runbooks updated (`runbooks/ops-tls-ingress-runbook.md` — TLS/ingress production runbook, `runbooks/ops-approval-workflow-runbook.md` — approval workflow)

## CLI / docs parity
- [x] every `ferrumctl server` subcommand is documented in the relevant docs section
- [x] every documented `ferrumctl server` flag matches the actual flag in `--help`
- [x] approval state transitions in docs match runtime (`Authorized` on approve, `Denied` on deny)
- [x] new runbooks are referenced from `runbooks/README.md`
