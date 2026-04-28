# 12 — Persistence and data model

## `ferrum-store` phải lưu
- intents
- proposals
- capabilities
- executions
- rollback contracts
- approvals
- provenance events
- provenance edges
- ledger entries

## Nguyên tắc
- IDs ổn định
- state transitions explicit
- không rewrite lineage
- query được theo execution_id / intent_id / capability_id

## Bảng tối thiểu
- intents
- proposals
- capabilities
- executions
- rollback_contracts
- approvals
- provenance_events
- provenance_edges
- ledger_entries
