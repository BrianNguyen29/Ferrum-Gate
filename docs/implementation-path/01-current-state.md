# 01 — Current state

## Repo hiện có
- docs khá đầy đủ
- contracts/schemas/openapi đã có
- crates scaffold đã có
- binaries đã có
- CI/layout scripts đã có
- sqlite persistence cho core state
- firewall MVP co trust/taint/sanitize/DLP co y nghia
- adapters that cho fs/sqlite/maildraft/git
- gateway orchestration + provenance chain co evidence thuc te
- integration tests meaningful cho happy/deny/recovery/git path

## Repo chưa có
- implementation parity day du cho moi adapter/runtime ngoai supported set hien tai
- http adapter va remote mutation recovery semantics ro rang
- operator/config docs day du cho release

## Phase hợp lý nhất để tiếp tục
1. lam http adapter voi fail-closed semantics ro rang
2. quyet dinh recovery boundary cho remote/destructive HTTP mutations
3. tiep tuc operator/release docs va config handoff
