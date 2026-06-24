# ADR 004 — S3 Feature Gate and Live Mode Semantics

## Status
Accepted

## Context

The S3 adapter depends on the AWS SDK (`aws-sdk-s3`, `aws-config`, `aws-smithy-types`), which adds significant compile time and dependency weight. Not all users need S3 integration. Additionally, S3 operations should not silently fail or execute against unconfigured buckets.

## Decision

1. **Feature gate**: The AWS SDK is optional behind the `s3-client` feature in `ferrum-adapter-s3`. Without this feature, the adapter compiles but cannot perform live S3 calls; it returns `execution_groundwork=true` metadata.
2. **Live mode**: S3 operations only execute when `live: true` is set in `s3_config`. When `live: false` (default), the adapter validates inputs and returns metadata without calling S3.
3. **Bucket allowlist**: Only a single `allowed_bucket` is permitted per adapter instance. Operations against other buckets are rejected at validation time.
4. **Fail closed**: If the S3 client is unavailable or the operation fails, the adapter returns an error rather than silently succeeding.

## Consequences

- **Positive**: Users without S3 needs do not pay the AWS SDK compile-time cost.
- **Positive**: Default configuration is safe; live S3 mutations require explicit opt-in.
- **Negative**: S3 integration tests require both the `s3-client` feature and a live MinIO/S3 endpoint.
- **Negative**: Rollback/compensate requires the `s3-client` feature for live delete operations.
