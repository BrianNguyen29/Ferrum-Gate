# 15 — Deployment and operations

## Development
- single process
- sqlite local
- memory ledger chấp nhận được

## Staging / production-like
- persistent store
- provenance bật
- rollback bật
- strict manifest pinning nên bật
- logs không lộ secrets

## Operations checklist
- policy bundle đúng environment
- rollback không bị tắt
- sanitize/DLP bật
- TTL hợp lý
- lineage query usable
