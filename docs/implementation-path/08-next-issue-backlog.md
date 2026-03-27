# 08 — Next issue backlog

## Completed foundation
- P0: workspace compile sạch
- P0: store sqlite MVP
- P0: tests smoke
- P1: firewall rule-based
- P1: fs/sqlite/maildraft adapters
- P1: git adapter local rollback + gateway wiring
- P1: git after_ref verify handoff fix
- P1: gateway happy path
- P1: ledger hash chain (DONE - initial integration slice complete per `12-ledger-hash-chain-execution-plan.md`)
  - Live append-time verification DONE per `17-ledger-live-hash-verification-execution-plan.md` (Commits A-C)
  - Cross-node sync: Sync-0 safety-contract discovery/plan started per `18-cross-node-ledger-sync-plan.md`
  - Future: ledger read-model, cross-node sync (beyond Sync-0) remain P2
- P1: ferrumctl debug/inspect/validate slices
- P1: http adapter full parity (GET/POST/PUT/PATCH/DELETE + body/header/query binding + auth) + gateway wiring
- P1: durable capability persistence + startup reconciliation + restart integration coverage

## P2
- provenance query/read model: **DONE (core surface implemented)**
  - `POST /v1/provenance/query` expanded with filters on `intent_id`, `proposal_id`, `execution_id`, `capability_id`, event kind, terminal state, time range, cursor pagination
  - `ferrum-graph` read-model helpers implemented: `terminal_events`, `walk_backwards_from`, `walk_forwards_from`
  - Evidence: `crates/ferrum-proto/src/provenance.rs:86`, `crates/ferrum-store/src/sqlite/provenance.rs:142`, `crates/ferrum-gateway/src/server.rs:2192`, `crates/ferrum-graph/src/lib.rs:52`, `tests/integration_provenance_query.rs`
  - Future P2: advanced replay/fabric tooling, cross-node sync
- operator/runtime hardening: **DONE**
  - ghi ro prod-style ingress/TLS deployment story thanh runbook thao tac duoc
  - them diagnostics cho effective config/startup guard/readiness de giam "why did ferrumd refuse to start" debugging time
  - doi chieu lai quickstart + troubleshooting + deployment docs voi production-like bearer-auth rollout
  - Evidence: `13-operator-runtime-hardening-execution-plan.md` (all items complete)

## P3
- runtime integration boundary: **DONE (proof slice complete)**
  - Observation-only MCP bridge (`McpBridge`) with explicit anchor ingest; no auto-correlation, no retries, no per-call source_system override per `crates/ferrum-integrations-mcp/src/bridge.rs`
  - E2e lineage proof: internal + external events share same execution chain per `tests/integration_mcp_bridge.rs:253` (`test_mcp_bridge_ingest_creates_linked_external_event`) and `tests/integration_mcp_bridge.rs:399` (`test_mcp_bridge_ingest_multiple_event_types`)
  - Future P3: full MCP transport loop, auto anchor resolution, persistent dedupe, background replay worker, multiple simultaneous vendor bridges
- recovery/hardening follow-up: **PLANNED**
  - HTTP mutation recovery boundary analysis + EmailSend governed-path evaluation
  - Plan: `16-recovery-hardening-follow-up-execution-plan.md`
