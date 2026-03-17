# 11 — Testing strategy

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
