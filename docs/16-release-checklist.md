# 16 — Release checklist

## v1 Scope Freeze — Single-Node Only

**v1 closure is scoped to single-node deployment only.** The following are
explicitly out of scope for v1 and are not blockers for v1 RC sign-off:

- Multi-node sync (write-path, two-way merge, consensus)
- HA / multi-leader replication
- In-process TLS termination (external terminator required)
- Distributed trace context (W3C TraceContext)
- Alerting rules
- Generic provenance replay/fabric tooling

The supported v1 surface is: SQLite-backed single-node gateway flow with
filesystem, SQLite, maildraft, git, and HTTP adapters; firewall enforcement;
capability mint/authorize/execute; R0/R2/R3 governance paths; rollback and
compensation; provenance lineage chain. See
`docs/implementation-path/23-production-readiness-assessment.md` Section 1 for
the full supported surface with evidence links.

**RC evidence artifact:** `docs/implementation-path/25-v1-single-node-rc-evidence.md`
- Current verdict: **READY TO CLOSE** — single-node v1 (all checklist items green: clippy, tests, startup guard, smoke server, readiness endpoint, metrics auth, SQLite backup/integrity)
- Post-v1 items (multi-node sync, HA, in-process TLS, distributed trace context, alerting rules) remain out of scope and are not v1 blockers.

## Contract integrity
- [x] contracts updated (`python3 scripts/check_contract_consistency.py` => `VALIDATION PASSED`)
- [x] schemas updated
- [x] openapi updated (`openapi/ferrumgate-control-api.v1.yaml` parsed and matches current routes)
- [x] docs updated (`docs/01-quickstart.md`, `docs/14-api-and-contracts-map.md`, `docs/15-deployment-and-operations.md`, `docs/17-troubleshooting.md`)

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
- [x] config docs correct (config precedence, auth mode, startup guard documented)
- [x] CLI useful minimum (`ferrumctl server health/inspect-*` documented and implemented)
- [x] lineage usable
- [x] approval flow documented (state transitions, CLI examples, and resolve-approval command)
- [x] runbooks updated (`runbooks/ops-tls-ingress-runbook.md` — TLS/ingress production runbook, `runbooks/ops-approval-workflow-runbook.md` — approval workflow)

## CLI / docs parity
- [x] every `ferrumctl server` subcommand is documented in the relevant docs section
- [x] every documented `ferrumctl server` flag matches the actual flag in `--help`
- [x] approval state transitions in docs match runtime (`Authorized` on approve, `Denied` on deny)
- [x] new runbooks are referenced from `runbooks/README.md`
