# ferrum-testkit

Minimal test fixtures and assertion helpers for FerrumGate workspace tests.

## Provided helpers

| Helper | Purpose |
|--------|---------|
| `sample_intent_title()` | Returns a static sample title string. |
| `sample_intent_compile_request()` | Builds a minimal `IntentCompileRequest` with placeholder principal and medium risk tier. |
| `sample_proposal_allow_response()` | Builds an `EvaluateProposalResponse` with `Decision::Allow`. |
| `sample_capability_mint_request(intent_id, proposal_id)` | Builds a `CapabilityMintRequest` with a 60-second TTL and test tool binding. |
| `assert_json_contains(haystack, needle)` | Asserts that a JSON object contains all top-level keys/values from another JSON object. Panics with a descriptive message on mismatch. |

## Usage

```rust
use ferrum_testkit::{sample_intent_compile_request, assert_json_contains};
use serde_json::json;

let req = sample_intent_compile_request();
assert_eq!(req.title, "Create invoice email draft");

let response = json!({"status": "ok", "count": 42});
assert_json_contains(&response, &json!({"status": "ok"}));
```

## Scope

This crate is workspace-internal only. It does **not** provide full integration test infrastructure, database fixtures, or test harnesses. For integration tests, see `crates/ferrum-integration-tests`.
