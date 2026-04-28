# 11 — Testing strategy

> **⚠️ Historical / Planning-era**: This document describes the intended testing strategy at time of planning. Some items listed as mandatory for v1 are now verified or have been superseded by P6/P7 validation evidence.
>
> **For current test coverage**: See `docs/implementation-path/01-current-state.md` §Test coverage matrix. Current workspace has ~761 tests all passing.



## Test layers
- unit
- contract conformance
- integration
- poisoned context
- lineage/replay

## Bắt buộc trong v1
- capability TTL test
- capability single-use test
- scope mismatch deny test
- R3 no auto-commit test
- fs rollback test
- sqlite rollback test
- maildraft draft-only test
- gateway happy path test
- quarantine path test
- provenance minimum-chain test

## Nguyên tắc test
- mutation tests phải assert recovery path
- gateway tests phải assert decision + provenance
- lineage tests phải assert đủ minimum chain
