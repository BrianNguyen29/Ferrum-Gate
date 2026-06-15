# Manual MCP and MinIO Smoke Checklist

> **Status**: Manual checklist — no automated CI integration. No real AWS credentials required.
> **Scope**: Validate existing stdio MCP flow and S3 adapter groundwork against a local MinIO instance.

---

## Prerequisites

- [ ] FerrumGate built: `cargo build --workspace`
- [ ] MinIO running locally (e.g., Docker or binary)
- [ ] `mc` (MinIO Client) installed
- [ ] MCP client configured for stdio transport (e.g., Claude Desktop, Cursor, or manual `stdio` test)

---

## Step 1: Start MinIO

```bash
# Docker example
docker run -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data --console-address ":9001"
```

Access the console at `http://localhost:9001` (credentials: `minioadmin` / `minioadmin`).

---

## Step 2: Create a Versioned Bucket

```bash
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/my-test-bucket
mc version enable local/my-test-bucket
```

---

## Step 3: Start ferrumd (In-Memory Dev Mode)

```bash
# Use the dev config (auth disabled, in-memory SQLite)
cargo run --bin ferrumd
```

Verify health:
```bash
curl http://127.0.0.1:18080/v1/healthz
```

---

## Step 4: MCP stdio Smoke

### 4.1 Start the MCP server

```bash
cargo run --bin ferrum-mcp-server
```

> Note: `ferrum-mcp-server` is a binary in the `crates/ferrum-integrations-mcp` crate. If the binary name differs, use the crate's actual binary name.

### 4.2 Send a `tools/list` request

Via stdio (line-delimited JSON):

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list"}
```

Expected: response containing `ferrum_gate_health`, `ferrum_gate_submit_intent`, etc.

### 4.3 Send a `ping` request

```json
{"jsonrpc":"2.0","id":2,"method":"ping"}
```

Expected: `{"jsonrpc":"2.0","id":2,"result":{}}`

### 4.4 Submit an intent via MCP

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ferrum_gate_submit_intent","arguments":{"description":"Test S3 adapter intent","actor_id":"test-operator"}}}
```

Expected: accepted intent with `intent_id`.

### 4.5 Query lineage

```json
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ferrum_gate_query_lineage","arguments":{"execution_id":"<from previous step>"}}}
```

Expected: lineage chain with at least `IntentSubmitted` and `PolicyEvaluated` events.

---

## Step 5: S3 Adapter Groundwork Validation (Manual)

> **Note**: Live S3 network execution is **not** implemented in this slice. The following steps validate the adapter's validation, planning, and metadata capture only.

### 5.1 Verify unit tests pass

```bash
cargo test -p ferrum-adapter-s3
```

Expected: 26 tests passed, 0 failed.

### 5.2 Verify adapter compiles with MinIO endpoint config

The adapter accepts an `endpoint_url` in `S3Config` (e.g., `http://localhost:9000`). This is validated at compile time but no network connection is made in this slice.

```rust
let config = S3Config {
    allowed_bucket: "my-test-bucket".to_string(),
    endpoint_url: Some("http://localhost:9000".to_string()),
    ..Default::default()
};
assert!(config.validate().is_ok());
```

### 5.3 Verify plan generation

```rust
use ferrum_adapter_s3::PlannableS3Adapter;
use ferrum_rollback::PlannableAdapter;

let adapter = PlannableS3Adapter;
let plan = adapter.generate_plan(
    &ActionType::S3PutObject,
    &RollbackTarget::S3Object {
        bucket: "my-test-bucket".to_string(),
        key: "test.txt".to_string(),
        version_id: None,
    },
).await.unwrap();
assert!(plan.is_some());
```

---

## Step 6: Fail-Closed Checks

- [ ] Mutating MCP tools fail closed without a valid bearer token (if auth is enabled).
- [ ] S3 adapter rejects a disallowed bucket name.
- [ ] S3 adapter rejects an invalid object key (`../etc/passwd`).
- [ ] S3 adapter rejects an unsupported action type (`FileWrite`).
- [ ] S3 `verify` fails closed when no `verify_checks` are provided.
- [ ] S3 `rollback`/`compensate` returns `recovered: false` with structured metadata (network deferred).

---

## Step 7: Cleanup

```bash
# Remove test bucket (optional)
mc rm --recursive --force local/my-test-bucket
mc rb local/my-test-bucket

# Stop ferrumd and MinIO
```

---

## Sign-Off Criteria

| Check | Status |
|-------|--------|
| MCP stdio `tools/list` returns expected tools | ☐ |
| MCP stdio `ping` returns `{}` | ☐ |
| Intent submission via MCP returns `intent_id` | ☐ |
| `ferrum-adapter-s3` unit tests pass (26/26) | ☐ |
| `cargo check --workspace` is clean | ☐ |
| S3 adapter rejects invalid bucket/key/unsupported action | ☐ |
| S3 adapter `rollback` returns `recovered: false` with metadata | ☐ |
| No real AWS credentials used in repo or test | ☐ |

---

## Notes

- **Production MCP HTTP/SSE is NOT claimed** in this repo. Only stdio transport is validated.
- **Production S3 readiness is NOT claimed** in this slice. Only groundwork (validation, planning, metadata) is present.
- Do not commit AWS/MinIO credentials to version control.

## Related docs

- [`mcp-integration.md`](./mcp-integration.md) — MCP stdio setup and tool list.
- [`adapter-reference.md`](./adapter-reference.md) — S3 adapter operations and rollback behavior.
- [`../architecture/s3-adapter-design.md`](../architecture/s3-adapter-design.md) — S3 ADR.
