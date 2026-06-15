# Architecture Documentation

Design documents covering audit integrity, provenance reconciliation, and execution lifecycle internals.

---

## Index

| Document | Summary |
|----------|---------|
| [Tamper-Evident Audit Design](tamper-evident-audit-design.md) | SHA-256 content hash chain, hourly Merkle roots, and Ed25519-signed checkpoints for the audit log. |
| [Execution/Provenance Outbox and Reconciliation](execution-provenance-outbox-reconciliation.md) | Lifecycle outbox pattern that reconciles execution state, provenance events, and audit rows after crashes or partial failures. |
| [S3 Adapter Design](s3-adapter-design.md) | Versioning-based rollback semantics for S3-compatible object operations (single-bucket allowlist, MinIO support, bounded scope). |

## Related concepts

- Core concepts (intents, proposals, capabilities, provenance, rollback classes): [`docs/guides/concepts.md`](../guides/concepts.md)
