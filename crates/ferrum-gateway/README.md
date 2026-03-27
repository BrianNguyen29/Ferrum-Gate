# ferrum-gateway

Gateway / orchestration crate for FerrumGate.

## Current Scope

The gateway is the core orchestration layer. It wires together:

- **Policy evaluation**: trust labeling, taint scoring, contradiction checks, DLP
- **Capability lifecycle**: mint, authorize, single-use enforcement, scope mismatch deny
- **Execution orchestration**: evaluate -> mint -> prepare -> execute -> verify -> commit/rollback
- **Governance paths**: approval, draft-only, quarantine
- **Provenance emission**: execution lineage events persisted to `provenance_edges`

## Supported Resource Enforcement

Execution-time enforcement is wired for all 5 registered adapter types:

| Resource | Enforcement |
|----------|-------------|
| File | path, traversal, read-on-write binding checks |
| Http | host, method, header, binding mismatch deny |
| Sqlite | db_path, table, mutation constraints |
| Git | repo_path, ref, local-ref enforcement |
| EmailDraft | recipient, send-violation deny at prepare-time |

## Supported Flows

Evidence-backed flows (per `tests/integration_gateway_flow.rs`, `tests/integration_poisoned_context.rs`, `tests/integration_lineage_chain.rs`):

- Happy path: R0 auto-commit, R2 explicit commit, R3 RequireApproval
- Deny path: scope mismatch, proposal_id mismatch, missing intent fail-closed
- Quarantine path: high-taint + non-R0 mutation blocks execution
- Rollback/compensate: fs/sqlite/maildraft/git/http adapter-backed recovery

## Status

Phase F evidence pack complete. Primary P1 backlog: expand poisoned-context fixture breadth.

Not in scope for v1: TLS termination (external terminator required), HTTP remote mutation rollback (no-op conservative), EmailSend governed-path (explicit deny at prepare-time).

## Key Files

- `src/server.rs`: HTTP server + gateway orchestration
- `src/policy.rs`: firewall evaluation logic
- `src/capability.rs`: capability mint/authorize/enforce
- `src/adapter.rs`: adapter registry + execution routing
- `src/provenance.rs`: lineage event emission
