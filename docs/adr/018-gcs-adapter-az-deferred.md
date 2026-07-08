# ADR 018 — P2-4 GCS-First Object-Storage Adapter Split

## Status

Accepted

## Context

P2-4 originally covered cloud object-storage rollback adapters. Both Google Cloud
Storage (GCS) and Azure Blob Storage were candidates. The two services have
different versioning and precondition models:

- GCS uses object `generation` and `metageneration` for consistency and rollback.
- Azure Blob uses snapshots and blob versioning with different API semantics.

Implementing both adapters in the same slice would increase dependency weight,
SDK integration risk, and public contract churn.

## Decision

1. **GCS first**: Implement a bounded GCS rollback adapter in this slice. It is
   feature-gated behind `gcs` / `gcs-client`, follows the S3 adapter
   architecture, models `generation`/`metageneration`, and fails closed without
   generation metadata.
2. **Azure Blob deferred**: Azure Blob adapter implementation, public DTOs,
   config, and features are explicitly out of scope for this slice. They will
   be addressed in a future ADR when the GCS adapter has proven the shape and
   rollback semantics are stable.
3. **Shape-only default**: The `gcs-client` feature enables the live client path,
   but the default adapter behavior is shape-only validation with `live: false`.
   No live cloud calls are made by default.
4. **No production claim**: The GCS adapter is additive and local/bounded only;
   it is not production-ready, compliant, or WORM-certified.

## Consequences

- **Positive**: Smaller, reviewable slice with clear generation-based rollback
  semantics.
- **Positive**: Default builds do not pull in GCS SDK dependencies or require
  cloud credentials.
- **Positive**: Azure deferral avoids half-implemented public surface and
  unstable SDK integration.
- **Negative**: Azure Blob rollback remains unavailable until a follow-up slice.
- **Negative**: Live GCS SDK integration is a deliberate follow-up; the current
  `gcs-client` path is a declared seam.
