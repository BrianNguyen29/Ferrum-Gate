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
  - Future: live hash verification, read-model, cross-node sync remain P2
- P1: ferrumctl debug/inspect/validate slices
- P1: http adapter full parity (GET/POST/PUT/PATCH/DELETE + body/header/query binding + auth) + gateway wiring
- P1: durable capability persistence + startup reconciliation + restart integration coverage

## P2
- provenance query/read model:
  - mo rong `POST /v1/provenance/query` tu minimal fail-closed endpoint thanh query surface co filter thuc dung theo `intent_id`, `proposal_id`, `execution_id`, event kind, va terminal state
  - them read-model helpers trong `ferrum-graph` cho multi-hop lineage/event-edge traversal tren du lieu da persist
  - them integration coverage cho provenance query va terminal recovery lineage vuot qua minimum chain
- operator/runtime hardening:
  - ghi ro prod-style ingress/TLS deployment story thanh runbook thao tac duoc
  - them diagnostics cho effective config/startup guard/readiness de giam "why did ferrumd refuse to start" debugging time
  - doi chieu lai quickstart + troubleshooting + deployment docs voi production-like bearer-auth rollout

## P3
- runtime integration boundary:
  - xac dinh model map external runtime/tool events vao FerrumGate provenance graph ma khong lam leak vendor assumptions vao core crates
  - chon 1 integration serioius dau tien (vi du MCP/runtime event bridge) va prove duoc internal + external events cung nam tren mot execution lineage
- recovery/hardening follow-up:
  - neu mo rong HTTP mutation recovery, lam ro boundary/an toan truoc; khong duoc silently claim rollback parity cho remote side effects
  - danh gia xem `EmailSend` co can tro thanh supported governed path hay tiep tuc explicit out-of-scope cho v1
