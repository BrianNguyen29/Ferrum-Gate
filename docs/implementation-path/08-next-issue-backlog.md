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
- P1: ledger hash chain
- P1: ferrumctl debug/inspect/validate slices
- P1: http adapter initial slice + gateway wiring

## P2
- lineage query: da co execution lineage (`GET /v1/provenance/lineage/{execution_id}`) va provenance query endpoint (`POST /v1/provenance/query`) fail-closed; generic query/replay/graph tooling rong hon van la backlog
- http adapter parity beyond GET/status-only execute/verify; explicit R3 boundary for mutating HTTP methods is now enforced server-side and rollback remains conservative no-op
