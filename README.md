# FerrumGate

FerrumGate is an **intent-scoped reversible execution plane** for MCP agents.

This repository is a **unified project scaffold** that includes:
- Rust code scaffold (crates + binaries)
- Business and architecture documentation
- Machine-readable contracts
- OpenAPI specifications
- JSON Schemas
- Prompts for AI agents
- Configuration examples
- Automation scripts
- Test skeletons

The goal is to give **AI agents and engineers a coherent foundation to build on**, rather than assembling disjointed pieces.

## Quickstart

1. `docs/guides/README.md` — Guide index
2. `docs/guides/concepts.md` — Core concepts and architecture
3. `docs/guides/quickstart.md` — Getting started
4. `docs/PRODUCTION_NOTES.md` — Runtime configuration notes
5. `contracts/ferrumgate-agent-contract.v1.yaml` — Machine-readable agent contract
6. `prompts/agent_system.md` — System prompt for agents

## Key Capabilities

- **Governance core**: `ferrum-proto`, `ferrum-pdp`, `ferrum-cap`, `ferrum-rollback`, `ferrum-store`, `ferrum-firewall`, `ferrum-graph`, `ferrum-ledger`, `ferrum-sync`
- **Gateway orchestration**: full evaluate → mint → authorize → prepare → execute → verify → compensate flow (internal commit/rollback semantics; compensate is the v1 recovery endpoint)
- **SQLite-backed persistence**: intents, proposals, capabilities, executions, rollback contracts, provenance, approvals; write-queue eliminates lock thrash
- **Security enforcement**: trust labeler, taint scorer, contradiction checks, taint-based quarantine
- **CLI (`ferrumctl`)**: health, inspect-execution, inspect-approvals, inspect-approval, inspect-lineage, inspect-provenance, policy bundle CRUD, backup/restore
- **Verified adapter slices** (bounded local scope):
  - `ferrum-adapter-fs`: 146 tests — FileWrite/FileDelete/FileMove/FileCopy/DirCreate/DirDelete/FileAppend/FileChmod + PlannableFsAdapter
  - `ferrum-adapter-git`: 86 tests — GitCommit/GitBranchCreate/GitTagCreate/GitTagDelete/GitBranchDelete + rollback fail-closed
  - `ferrum-adapter-http`: 103 tests — HttpMutation + http.replay_v1 (POST/PUT/PATCH) + pooling/retry
  - `ferrum-adapter-sqlite`: 16 tests — transaction rollback + G-E1 verify fail-closed
  - `ferrum-adapter-maildraft`: 16 tests — create/update/delete lifecycle
- **Integration tests**: 82 tests — contracts(2) + fs-roundtrip(7) + gateway-flow(65) + lineage-chain(8)
- **CI pipeline**: cargo check, repo layout validation, contract consistency

## Architecture Overview

```
┌─────────┐     ┌──────────┐     ┌─────────┐     ┌──────────┐
│  Intent │────→│ Proposal │────→│Capability│────→│ Execution│
│ Compile │     │ Evaluate │     │  Mint   │     │ Prepare  │
└─────────┘     └──────────┘     └─────────┘     └──────────┘
                                                      │
                      ┌──────────┐                   ▼
                      │ Provenance│←────────────── Execute
                      │  /Lineage │←────────────── Verify
                      └──────────┘←────────────── Evaluate Outcome
```

All execution paths pass through the gateway. No adapter is invoked directly without a capability.

## Entry Points for Agents and Engineers

- `docs/guides/README.md` — User/operator guide index
- `docs/guides/concepts.md` — Core concepts and architecture
- `docs/guides/quickstart.md` — Getting started
- `docs/guides/operator.md` — Operator runbook
- `docs/PRODUCTION_NOTES.md` — Runtime configuration notes

## Security Model

- Intent-scoped, capability-bounded, approval-aware execution
- Rollback-classified operations with provenance tracking
- Bearer-token auth gate in bearer-auth mode
- Rate limiting integrated with the gateway
- Taint-based quarantine and contradiction detection

## License

See `LICENSE` for the project license.
