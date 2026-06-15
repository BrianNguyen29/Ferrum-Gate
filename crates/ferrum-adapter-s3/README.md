# Ferrum-adapter-s3

S3-compatible object adapter for FerrumGate with versioning-based rollback.

## Status

Implemented (experimental). Live S3 execution is supported behind the `live` config flag.
MinIO integration tests are gated (`#[ignored]`) and require a local Docker container.

## Supported operations

| Operation | Prepare | Execute | Verify | Rollback |
|-----------|---------|---------|--------|----------|
| S3PutObject | Validate bucket/key, live `GetBucketVersioning` when `live: true` | Live `PutObject` with ByteStream; captures `after_version_id` | Live `HeadObject` check (or shape-only fallback) | Deletes `after_version_id` to restore previous version |
| S3DeleteObject | Validate bucket/key, live `GetBucketVersioning` when `live: true` | Live `DeleteObject`; captures `delete_marker_version_id` | Live `HeadObject` check (or shape-only fallback) | Deletes `delete_marker_version_id` to restore object |
| S3GetObject | Validate bucket/key | Live `GetObject` with body collect; enforces `max_object_size` | Live `HeadObject` check (or shape-only fallback) | N/A (read-only) |
| S3CopyObject | Validate bucket/key, live `GetBucketVersioning` when `live: true` | Live `CopyObject`; captures `after_version_id` | Live `HeadObject` check (or shape-only fallback) | Deletes destination `after_version_id` |

## Rollback behavior

- **PutObject**: Deletes the new version ID to restore the previous version.
- **DeleteObject**: Deletes the delete marker to restore the object.
- **CopyObject**: Deletes the destination object's new version.
- **GetObject**: No-op (read-only).

## Fail-closed behavior

When `live: true` (set by ferrumd when `s3_config` is present):
- `prepare` fails if `GetBucketVersioning` returns that versioning is not enabled or the call fails.
- `execute` fails if the S3 SDK call fails.
- `verify` fails if `HeadObject` does not match the expected state.
- `rollback`/`compensate` attempts to delete the captured version/delete marker; if the live delete fails, it returns `recovered: false` with structured metadata.

When `live: false` (the default for `S3Config::default()` and unit tests), all phases fall back to shape-only validation and mark `execution_groundwork: true`. This allows safe unit testing without real credentials.

## Configuration

```toml
[server.s3_config]
allowed_bucket = "my-bucket"
max_object_size = 104857600        # 100 MB
require_versioning = true
endpoint_url = "http://localhost:9000"  # optional (MinIO)
region = "us-east-1"
access_key_id = "minioadmin"       # optional (static credentials)
secret_access_key = "minioadmin"   # optional (static credentials)
```

## MinIO testing

```bash
# 1. Start MinIO
docker run -d -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data --console-address ":9001"

# 2. Create a versioned bucket
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/ferrum-test-bucket
mc version enable local/ferrum-test-bucket

# 3. Run the gated integration tests
cargo test -p ferrum-adapter-s3 --test minio_integration -- --ignored
```

## Limitations

- Single-bucket allowlist only; no bucket creation/IAM/ACL admin.
- Max object size is enforced at the adapter boundary, not by S3 itself.
- Multipart upload, lifecycle, presigned URLs, replication, and batch deletion are out of scope.
- MinIO integration tests are gated (`#[ignore]`) and require a local Docker container.
- No production S3 readiness certification is claimed; operators must manage credentials and endpoints.

## Related docs

- [`docs/architecture/s3-adapter-design.md`](../../docs/architecture/s3-adapter-design.md) — ADR and design details.
- [`docs/guides/adapter-reference.md`](../../docs/guides/adapter-reference.md) — Adapter reference with risk class mapping.
- [`docs/guides/mcp-s3-smoke-checklist.md`](../../docs/guides/mcp-s3-smoke-checklist.md) — Manual smoke checklist.
