# 06 — Testing and quality gates

## Testing doctrine

FerrumGate không được coi là “đúng” chỉ vì compile pass. Mọi mutation path phải được chứng minh bằng test behavior + provenance + recovery.

## Test layers

- unit
- contract conformance
- integration
- poisoned context
- lineage/replay
- deployment smoke tests

## Q1 quality gates

### Invariant closure
- [ ] intent invariants
- [ ] capability invariants
- [ ] taint invariants
- [ ] rollback invariants
- [ ] provenance invariants
- [ ] output invariants
- [ ] system invariants

### Must-pass tests
- [ ] capability TTL test
- [ ] capability single-use end-to-end test
- [ ] scope mismatch deny test
- [ ] R3 no auto-commit integration test
- [ ] provenance minimum-chain integration test
- [ ] draft-only prepare revalidation test
- [ ] prepare-step rollback class propagation test

### Evidence required
- cargo check/fmt/clippy/test artifacts
- release checklist update
- invariant matrix update

## Q2 quality gates

### Adapter integration tests
- [ ] fs backup/restore success test
- [ ] fs verify failure test
- [ ] git revert/reset recovery test
- [ ] db rollback on predicate mismatch test
- [ ] engineering workflow happy path tests
- [ ] deny/quarantine tests for high-risk mutation

### Policy tests
- [ ] protected path deny
- [ ] protected branch approval
- [ ] destructive SQL approval or deny
- [ ] external mutation R3 escalation where applicable

### Demo readiness checks
- [ ] one workflow per adapter can be replayed from docs/examples

## Q3 quality gates

### Deployment tests
- [ ] self-hosted boot test with sqlite
- [ ] self-hosted boot test with postgres
- [ ] auth/bearer path test
- [ ] backup/restore drill
- [ ] UI smoke tests

### Operator flow tests
- [ ] approvals list/detail usable
- [ ] lineage detail visible
- [ ] evidence export smoke test

## Q4 quality gates

### Runtime/MCP tests
- [ ] tool call -> proposal mapping tests
- [ ] capability binding on tool args tests
- [ ] trust/taint propagation tests
- [ ] runtime-level quarantine/approval tests

### Evidence plane tests
- [ ] ledger integrity/hash chain alpha tests
- [ ] signed approval/evidence artifact tests

## Release stop conditions

Stop a release if any of the following is true:
- a mutation path has no recovery-path assertion
- lineage minimum chain is incomplete without explicit accepted-risk note
- adapter is marketed as real while tests only cover skeleton/noop behavior
- docs/spec/openapi/schemas drift from runtime
- operator cannot determine why action was denied/quarantined/approved
