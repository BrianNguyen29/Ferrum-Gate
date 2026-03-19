# 01 — Current state

## Repo hiện có
- docs khá đầy đủ
- contracts/schemas/openapi đã có
- crates scaffold đã có
- binaries đã có
- CI/layout scripts đã có
- sqlite persistence cho core state
- firewall MVP co trust/taint/sanitize/DLP co y nghia
- adapters that cho fs/sqlite/maildraft/git/http-initial-slice
- gateway orchestration + provenance chain co evidence thuc te
- integration tests meaningful cho happy/deny/recovery/git/http path

## Repo chưa có
- implementation parity day du cho moi adapter/runtime ngoai supported set hien tai
- http adapter full parity beyond GET-only execute/verify va conservative no-op rollback
- operator/config docs day du cho release

## Phase hợp lý nhất để tiếp tục
1. mo rong HTTP adapter qua GET/status-only initial slice hien tai ma khong broaden remote mutation recovery promises
2. tiep tuc operator/release docs va config handoff
