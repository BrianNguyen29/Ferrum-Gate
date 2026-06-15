# S3 Adapter Design (PR4)

## Scope

- IN (implemented in this slice):
  - ADR and rollback contract design for versioning-based S3 recovery.
  - Proto types: `ActionType` variants (`S3PutObject`, `S3DeleteObject`, `S3GetObject`, `S3CopyObject`), `RollbackTarget::S3Object`, `ResourceSelector::S3Bucket`, and `CheckType` variants (`S3ObjectExists`, `S3VersionIdMatches`).
  - `crates/ferrum-adapter-s3` with:
    - `S3Config` (single-bucket allowlist, max object size, versioning requirement, optional endpoint for MinIO, optional static credentials).
    - `S3Adapter` implementing `RollbackAdapter` with **live AWS SDK execution** for PutObject, DeleteObject, GetObject, CopyObject.
    - `PlannableS3Adapter` generating execution plans with compensation placeholders.
    - `S3RollbackMetadata` struct for versioning-based recovery.
    - Unit tests for config validation, bucket/key validation, action mapping, and metadata.
    - MinIO-gated integration tests (`#[ignore]`) validating the full lifecycle.
  - Gateway tool mapping for S3 operations (`s3_put`, `s3_get`, `s3_delete`, `s3_copy`).
  - MCP resource scope parsing for `s3:put:bucket:key` and `s3:get:bucket:key` patterns.
  - Ferrumd config registration: S3 adapter and planner registered when `s3_config` is present.
  - Adapter reference documentation and ROADMAP update.

- Not yet implemented (future slice):
  - Automated CI integration tests requiring Docker/MinIO (gated/manual only).
  - Production S3 readiness certification (operator-managed credentials and endpoints required).

## Design

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| S3PutObject | Validate bucket/key, live HeadObject for `before_version_id`, live GetBucketVersioning | Live PutObject with ByteStream; captures `after_version_id` | Live S3ObjectExists via HeadObject | Live delete of `after_version_id` to restore previous version |
| S3DeleteObject | Validate bucket/key, live HeadObject for `before_version_id`, live GetBucketVersioning | Live DeleteObject; captures `delete_marker_version_id` | Live S3ObjectExists via HeadObject | Live delete of `delete_marker_version_id` to restore object |
| S3GetObject | Validate bucket/key | Live GetObject with body collect; enforces `max_object_size` | Live S3ObjectExists via HeadObject | N/A (read-only) |
| S3CopyObject | Validate bucket/key, live HeadObject for `before_version_id`, live GetBucketVersioning | Live CopyObject; captures `after_version_id` | Live S3VersionIdMatches via HeadObject | Live delete of destination `after_version_id` |

### Rollback model (versioning-based)

FerrumGate requires **S3 versioning** on the target bucket. The adapter does not create or manage buckets; it only operates on existing buckets with versioning enabled.

1. **Prepare**: validates the bucket against the allowlist, validates the key, and performs a live `GetBucketVersioning` check (if `live: true`). If `live: true` and the live check fails, prepare **fails closed**. If `live: false` (default, e.g. unit tests), prepare skips the live check and falls back to config-only validation.
2. **Execute**: performs the live S3 operation. If `live: true` and the S3 call fails, execute **fails closed** with a validation error. If `live: false`, execute falls back to shape-only metadata capture and marks `execution_groundwork: true`.
3. **Verify**: requires explicit `S3ObjectExists` or `S3VersionIdMatches` checks. If the client is configured, live `HeadObject` checks are performed; failures **fail closed**. If the client is not configured, checks fall back to shape-only validation. Fail-closed if no checks are provided.
4. **Rollback/Compensate**:
   - For **PutObject**: deletes the `after_version_id` to restore the `before_version_id` as the current version.
   - For **DeleteObject**: deletes the `delete_marker_version_id` to restore the object.
   - For **CopyObject**: deletes the destination object's new version.
   - For **GetObject**: no-op (read-only).

### Fail-closed behavior

When the S3 adapter is configured with `live: true` (which ferrumd sets automatically when `s3_config` is present):
- `prepare` fails if `GetBucketVersioning` returns that versioning is not enabled or the call fails.
- `execute` fails if the S3 SDK call (PutObject, DeleteObject, GetObject, CopyObject) fails.
- `verify` fails if `HeadObject` does not match the expected state.
- `rollback`/`compensate` attempts to delete the captured version/delete marker; if the live delete fails, it returns `recovered: false` with structured metadata.

When the adapter is configured with `live: false` (the default for `S3Config::default()` and unit tests), all phases fall back to shape-only validation and mark `execution_groundwork: true`. This allows safe unit testing without real credentials. The `live` flag must be explicitly set to `true` for the adapter to make real S3 SDK calls.

### Configuration

```rust
pub struct S3Config {
    pub allowed_bucket: String,       // exact match; single-bucket allowlist
    pub max_object_size: u64,         // default 100MB; max 5GB
    pub require_versioning: bool,     // default true; fail-closed if false
    pub endpoint_url: Option<String>, // e.g., "http://localhost:9000" for MinIO
    pub region: String,               // default "us-east-1"
    pub access_key_id: Option<String>,     // optional static credentials
    pub secret_access_key: Option<String>,   // optional static credentials
}
```

For MinIO, set `endpoint_url` to `http://localhost:9000` and provide `access_key_id`/`secret_access_key`. For AWS, omit `endpoint_url` and rely on the AWS credential chain (environment, IAM role, etc.).

### Validation rules

- **Bucket name**: must match AWS bucket naming rules (3-63 chars, lowercase, digits, hyphens, dots; no consecutive dots, no dot-hyphen adjacency, no IP-like strings).
- **Object key**: non-empty, max 1024 chars, no leading `/`, no `..`, no control characters (except tab, newline, carriage return).
- **Object size**: must not exceed `max_object_size`.
- **Bucket allowlist**: the target bucket must exactly match `allowed_bucket`.

### Limitations (explicit)

- Single-bucket allowlist only; no bucket creation/IAM/ACL admin.
- No multipart upload, lifecycle, presigned URLs, replication, or batch deletion.
- No real AWS credentials in the repo; all examples use placeholders or MinIO.
- If the live S3 client fails (no credentials, network unreachable), the adapter falls back to shape-only validation and marks `execution_groundwork: true`. This is a graceful degradation, not a silent failure.
- Production S3 readiness certification is not claimed; operators must manage credentials and endpoints.
- Production MCP HTTP/SSE claims are not made; stdio-only.

## Threats Addressed

| Threat | Mitigation | Limitation |
|--------|------------|------------|
| Accidental cross-bucket operations | Single-bucket allowlist | Operator must configure the correct bucket |
| Path traversal via object keys | Key validation rejects `..`, leading `/`, control chars | Does not prevent all semantic attacks |
| Overwriting without recovery | Versioning required by default | Rollback requires the bucket to have versioning enabled before the operation |
| Oversized object writes | `max_object_size` enforced at adapter boundary | Not enforced by S3 itself; adapter validates before network call |

## Threats NOT Addressed

- **Bucket misconfiguration**: if versioning is disabled after prepare, rollback may fail. The adapter validates config only at prepare time.
- **Concurrent writers**: another process modifying the same object may leave an unexpected version state.
- **AWS credential exposure**: the adapter does not store credentials; they are provided via environment or IAM role outside this scope.
- **Network partition**: if the S3 endpoint is unreachable during execute, the operation fails closed (when `live: true`).
- **Production S3 readiness**: live execution is implemented but requires operator-managed credentials and endpoints. MinIO integration tests are gated (`#[ignore]`).

## API / Usage

### Example: S3PutObject with rollback

```json
{
  "action": "s3.S3PutObject",
  "target": {
    "kind": "S3Object",
    "bucket": "my-allowed-bucket",
    "key": "config/app.yaml",
    "version_id": "abc123"
  },
  "parameters": {
    "content": "key: value\n"
  }
}
```

The adapter captures `before_version_id: "abc123"` at prepare. After execute, the new `after_version_id` is captured. If rollback is triggered, the adapter deletes the `after_version_id` to restore the previous version.

### MinIO local testing

For local development, set `endpoint_url` to `http://localhost:9000` and configure MinIO credentials via environment variables:

```bash
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_REGION=us-east-1
```

Create a versioned bucket in MinIO:

```bash
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/my-test-bucket
mc version enable local/my-test-bucket
```

## Design Extensions

1. Live AWS SDK wiring (`aws-sdk-s3` with `rustls` feature, no `native-tls`).
2. MinIO integration tests (gated behind Docker; manual only in this slice).
3. Gateway tool listing and MCP wiring for S3 operations.
4. Multi-bucket allowlist (if needed; currently single-bucket by design).

## Notes

- Live S3 execution is implemented behind the `live` config flag. When `live: true`, real AWS SDK calls are made and failures fail closed. When `live: false` (default), the adapter falls back to shape-only validation for safe unit testing.
- All real AWS/MinIO credentials remain operator-managed and out of version control.
- No automated CI tests requiring credentials or Docker are introduced in this slice.
- SOC2 / compliance certification is **not** claimed.
