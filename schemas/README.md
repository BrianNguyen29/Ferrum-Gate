# Schemas

Thư mục này chứa JSON Schema cho các object cốt lõi của FerrumGate.

## Files

- `jsonschema/common.json`
- `jsonschema/intent-envelope.json`
- `jsonschema/capability-lease.json`
- `jsonschema/action-proposal.json`
- `jsonschema/provenance-event.json`
- `jsonschema/rollback-contract.json`
- `jsonschema/approval-request.json`

## Notes

- Các schema này được thiết kế để map sang `ferrum-proto`.
- Nên dùng validation ở boundary request, persistence và replay.
- Một số invariant liên object được mô tả trong `contracts/` và `docs/`, không thể encode hết ở JSON Schema.
