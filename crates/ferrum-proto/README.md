# ferrum-proto

Source-of-truth Rust types cho FerrumGate.

Bao gồm:
- strong ids
- common enums
- intent types
- capability types
- provenance types
- rollback types
- API request/response types

Nguyên tắc:
- kiểu dữ liệu phải map được sang `schemas/` và `openapi/`
- mọi thay đổi shape phải update cả docs/contracts
