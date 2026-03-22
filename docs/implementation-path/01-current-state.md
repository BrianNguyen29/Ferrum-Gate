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

## Repo chưa có
- implementation parity day du cho moi adapter/runtime ngoai supported set hien tai
- HTTP remote mutation recovery parity beyond conservative no-op rollback/compensate
- operator/config docs day du cho release

## Phase hợp lý nhất để tiếp tục
1. tiep tuc operator/release docs va config handoff
