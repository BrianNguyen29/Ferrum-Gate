# ferrum-testkit

Minimal test fixtures and assertion helpers for FerrumGate workspace tests.

## Provided helpers

| Helper | Purpose |
|--------|---------|
| `test_now()` | Returns a deterministic UTC timestamp (`2024-01-01T00:00:00Z`) used as the default `now` for fixtures. |
| `sample_intent_title()` | Returns a static sample title string. |
| `sample_intent_compile_request()` | Builds a minimal `IntentCompileRequest` with placeholder principal and medium risk tier. |
| `sample_proposal_allow_response()` | Builds an `EvaluateProposalResponse` with `Decision::Allow`. |
| `sample_capability_mint_request(intent_id, proposal_id)` | Builds a `CapabilityMintRequest` with a 60-second TTL and test tool binding. |
| `assert_json_contains(haystack, needle)` | Asserts that a JSON object contains all top-level keys/values from another JSON object. Panics with a descriptive message on mismatch. |

## Fixtures

`IntentFixture`, `ProposalFixture`, and `ApprovalFixture` are builder-style fixtures
that produce minimal, valid FerrumGate protocol values. Each fixture defaults to a
deterministic `test_now()` timestamp and safe placeholder values; override fields
with the `with_*` methods as needed.

```rust
use ferrum_testkit::{ApprovalFixture, IntentFixture, ProposalFixture, test_now};

let intent = IntentFixture::new().build();
let proposal = ProposalFixture::new().build();
let approval = ApprovalFixture::new()
    .with_now(test_now())
    .build();
```

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

This crate is workspace-internal only. Without the optional `gateway` feature it
does **not** provide full integration test infrastructure, database fixtures, or
test harnesses. For full integration tests, see `crates/ferrum-integration-tests`.

Enabling the `gateway` feature adds an opt-in `SqliteGateway` harness: an
in-memory SQLite store wired to a `GatewayRuntime` for gateway-level tests. Use it
only when a test specifically needs a gateway runtime; otherwise prefer the plain
fixtures to keep tests minimal.
