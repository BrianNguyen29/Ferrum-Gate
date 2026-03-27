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
- generic provenance query/replay/graph tooling rong hon tren persisted event graph (P2)
- in-process TLS hoac HA/multi-node control-plane story

## Ratified boundaries (not gaps)
- HTTP rollback la intentional no-op; remote mutation can yeu cau manual R3 compensation (ratified in `16a-slice-16-a-boundary-ratification.md`)
- EmailSend governed-path la explicit deny tai prepare-time (allow_send=true -> PolicyDenied 403); ratified in `16a-slice-16-a-boundary-ratification.md`
- MCP bridge proof slice da hoan thanh (P3 DONE); full MCP transport loop con la P3 backlog

## Phase hợp lý nhất để tiếp tục

Theo `23-production-readiness-assessment.md`, uu tien hien tai la:
1. P1 single-node hardening: observability baseline (tracing + Prometheus metrics), TLS/ingress docs, operational runbook
2. P2 provenance tooling: advanced replay/fabric tooling tren persisted event graph
3. P2 sync prep: complete Sync-3a.1 probe API boundary, then begin Sync-1 protocol implementation

## Nguyen tac tiep tuc
- khong mo lai cac rollout slice da dong cua capability persistence
- tiep tuc bang cac slice nho, fail-closed, co evidence test/docs ro rang
- uu tien nhung muc giup repo de deploy tiep va de runtime khac tich hop tiep
