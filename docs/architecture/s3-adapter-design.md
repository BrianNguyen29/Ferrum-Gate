# S3 Adapter Design (PR4)

## Scope

- IN (implemented in this slice):
  - ADR and rollback contract design for versioning-based S3 recovery.
  - Proto types: `ActionType` variants (`S3PutObject`, `S3DeleteObject`, `S3GetObject`, `S3CopyObject`), `RollbackTarget::S3Object`, and `CheckType` variants (`S3ObjectExists`, `S3VersionIdMatches`).
  - `crates/ferrum-adapter-s3` with:
    - `S3Config` (single-bucket allowlist, max object size, versioning requirement, optional endpoint for MinIO).
    - `S3Adapter` implementing `RollbackAdapter` with full validation and metadata capture.
    - `PlannableS3Adapter` generating execution plans with compensation placeholders.
    - `S3RollbackMetadata` struct for versioning-based recovery.
    - Unit tests for config validation, bucket/key validation, action mapping, and metadata.
  - Adapter reference documentation and ROADMAP update.
  - Manual MinIO/MCP smoke checklist (gated, no CI credentials).

- Not yet implemented (future slice):
  - Live AWS SDK or MinIO client wiring (`aws-sdk-s3` or equivalent with `rustls`).
  - Actual network execution in `execute`, `rollback`, and `compensate`.
  - Gateway tool listing and MCP wiring for S3 operations.
  - Automated integration tests requiring Docker/MinIO.

## Design

### Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| S3PutObject | Validate bucket/key, capture `before_version_id` | Put object (network deferred) | S3VersionIdMatches | Delete new version to restore previous |
| S3DeleteObject | Validate bucket/key, capture `before_version_id` | Delete object (network deferred) | S3ObjectExists (absent) | Delete delete marker to restore object |
| S3GetObject | Validate bucket/key | Get object (network deferred) | S3ObjectExists | N/A (read-only) |
| S3CopyObject | Validate bucket/key, capture `before_version_id` | Copy object (network deferred) | S3VersionIdMatches (dest) | Delete destination new version |

### Rollback model (versioning-based)

FerrumGate requires **S3 versioning** on the target bucket. The adapter does not create or manage buckets; it only operates on existing buckets with versioning enabled.

1. **Prepare**: validates the bucket against the allowlist, validates the key, and captures the current `version_id` (if the object exists) as `before_version_id`. For CopyObject, the source object is validated; the destination object may not exist yet.
2. **Execute**: performs the S3 operation. For mutating operations, the result includes the new `version_id` or `delete_marker_version_id` as `after_version_id`.
3. **Verify**: requires explicit `S3ObjectExists` or `S3VersionIdMatches` checks. Fail-closed if no checks are provided.
4. **Rollback/Compensate**:
   - For **PutObject**: delete the `after_version_id` to restore the `before_version_id` as the current version.
   - For **DeleteObject**: delete the `delete_marker_version_id` to restore the object.
   - For **CopyObject**: delete the destination object's new version.
   - For **GetObject**: no-op (read-only).

### Configuration

```rust
pub struct S3Config {
    pub allowed_bucket: String,       // exact match; single-bucket allowlist
    pub max_object_size: u64,         // default 100MB; max 5GB
    pub require_versioning: bool,     // default true; fail-closed if false
    pub endpoint_url: Option<String>, // e.g., "http://localhost:9000" for MinIO
    pub region: String,               // default "us-east-1"
}
```

### Validation rules

- **Bucket name**: must match AWS bucket naming rules (3-63 chars, lowercase, digits, hyphens, dots; no consecutive dots, no dot-hyphen adjacency, no IP-like strings).
- **Object key**: non-empty, max 1024 chars, no leading `/`, no `..`, no control characters (except tab, newline, carriage return).
- **Object size**: must not exceed `max_object_size`.
- **Bucket allowlist**: the target bucket must exactly match `allowed_bucket`.

### Limitations (explicit)

- No bucket creation, deletion, IAM, ACL, or policy admin.
- No multipart upload, lifecycle, presigned URLs, replication, or batch deletion.
- No real AWS credentials in the repo; all examples use placeholders or MinIO.
- Network execution is deferred to a future slice; the current slice validates and captures metadata only.
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
- **Network partition**: if the S3 endpoint is unreachable during execute, the operation fails closed.
- **Production S3 readiness**: this slice is groundwork; live network tests require a future slice with credentials and endpoint.

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

- This design does **not** claim production-ready S3 integration. It is groundwork for a future slice.
- All real AWS/MinIO credentials remain operator-managed and out of version control.
- No automated CI tests requiring credentials or Docker are introduced in this slice.
- SOC2 / compliance certification is **not** claimed.
