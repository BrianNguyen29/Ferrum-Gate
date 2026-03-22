# 01 — Current state

## Repo hiện có
- docs khá đầy đủ
- contracts/schemas/openapi đã có
- crates scaffold đã có
- binaries đã có
- CI/layout scripts đã có
- sqlite persistence cho core state
- firewall MVP co trust/taint/sanitize/DLP co y nghia
- adapters that cho fs/sqlite/maildraft/git/http-full-parity
- gateway orchestration + provenance chain co evidence thuc te
- integration tests meaningful cho happy/deny/recovery/git/http path
- durable capability persistence qua SQLite, restart-safe cho supported flow hien tai
- ferrumctl da co debug/inspect/validate slices toi thieu
- operator docs / release checklist / troubleshooting docs da duoc doi chieu voi behavior hien tai

## Repo chưa có
- implementation parity day du cho moi adapter/runtime ngoai supported set hien tai
- HTTP remote mutation recovery parity beyond conservative no-op rollback/compensate
- generic provenance query/replay/graph tooling rong hon tren persisted event graph
- serious external runtime integration boundary da duoc prove end-to-end
- in-process TLS hoac HA/multi-node control-plane story

## Phase hợp lý nhất để tiếp tục
1. tiep tuc Phase F+/U3: provenance query/read-model/replay tooling tren persisted provenance graph
2. tiep tuc operator/runtime hardening: ingress/TLS runbook, readiness diagnostics, va prod-like rollout notes
3. sau do moi mo U4 runtime integrations theo kieu vendor-neutral, dua event ben ngoai vao cung lineage

## Nguyen tac tiep tuc
- khong mo lai cac rollout slice da dong cua capability persistence
- tiep tuc bang cac slice nho, fail-closed, co evidence test/docs ro rang
- uu tien nhung muc giup repo de deploy tiep va de runtime khac tich hop tiep
