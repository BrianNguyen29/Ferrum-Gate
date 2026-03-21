# 09 — Implementation path

## Phase A — Compile and shape stability
Mục tiêu:
- workspace compile sạch
- root Cargo / members / deps ổn
- proto shapes sync với schemas/contracts/openapi

## Phase B — Storage boundary
Mục tiêu:
- `ferrum-store` có repos thật
- persist intents/capabilities/executions/rollback/provenance

## Phase C — Firewall MVP
Mục tiêu:
- trust labels
- taint scoring
- contradiction checks
- sanitize output
- DLP cơ bản

## Phase D — Adapter-backed rollback
Mục tiêu:
- fs adapter thật
- sqlite adapter thật
- maildraft adapter thật
- git adapter thật
- http adapter full parity (GET/POST/PUT/PATCH/DELETE + body/header/query binding + auth) thật

## Phase E — Gateway orchestration
Mục tiêu:
- proposal -> policy -> capability -> prepare -> execute -> verify -> commit
- provenance chain đầy đủ

## Phase F — Hardening and evidence
Mục tiêu:
- tests
- poisoned context regression
- supported flows / open gaps handoff
- examples
- CLI / debug flow

## Thứ tự crate nên làm
1. proto
2. store
3. pdp
4. cap
5. firewall
6. rollback
7. fs/sqlite/maildraft/git/http adapters
8. graph
9. ledger
10. gateway
11. ferrumctl
12. testkit/tests
