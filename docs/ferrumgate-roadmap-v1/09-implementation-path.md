# 09 — Implementation path

> **⚠️ Historical / Superseded**: This document describes the intended implementation path at time of planning. Phase A–F completion status is stale. Do not treat unchecked phases as open blockers.
>
> **For current Phase status**: See `docs/implementation-path/01-current-state.md` §Phase status summary.
>
> **Planning reference**: For post-v1/quarterly planning, see [`../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/README.md`](../ferrumgate-roadmap-v2/ferrumgate-roadmap-pack/README.md). This pack is non-authoritative for v1 support unless the support contract is explicitly amended.
>
> **Canonical Phase completion**:
> - Phase A (compile/shape stability): ✅ DONE
> - Phase B (SQLite storage boundary): ✅ DONE
> - Phase C (firewall MVP): ✅ DONE
> - Phase D (adapters): **PARTIAL** — verified local slices exist (fs:135, git:86, http:103, sqlite:16, maildraft:13); broader surface is post-v1
> - Phase E (gateway orchestration): ✅ DONE
> - Phase F (hardening/evidence): ✅ DONE



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

## Phase E — Gateway orchestration
Mục tiêu:
- proposal -> policy -> capability -> prepare -> execute -> verify -> commit
- provenance chain đầy đủ

## Phase F — Hardening and evidence
Mục tiêu:
- tests
- poisoned context regression
- examples
- CLI / debug flow

## Thứ tự crate nên làm
1. proto
2. store
3. pdp
4. cap
5. firewall
6. rollback
7. fs/sqlite/maildraft adapters
8. graph
9. ledger
10. gateway
11. ferrumctl
12. testkit/tests
