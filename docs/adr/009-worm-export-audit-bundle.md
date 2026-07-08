# ADR 009 — WORM Export and Portable Audit Bundle

## Status

Accepted (P2-1). The portable `ferrum-audit-bundle` crate and `ferrumctl audit export/verify` are implemented. The optional S3 Object Lock WORM sink is implemented behind the `worm-sink` feature gate. External anchoring remains operator-owned and is not a built-in feature.

## Context

The current audit log is append-only with a SHA-256 hash chain (`previous_hash` linkage), providing tamper-evident detection of unauthorized changes. However, the audit log is stored in the same SQLite/PostgreSQL database as operational data, and there is no offline, portable, or write-once-read-many (WORM) export mechanism.

Operators who need stronger tamper resistance, offline forensic review, or long-term archival require a portable audit bundle that can be:

- Exported to a WORM-capable sink (e.g., S3 Object Lock, GCP Bucket Lock, Azure Immutable Blob).
- Verified independently without running `ferrumd`.
- Cryptographically anchored to an external source (e.g., timestamp authority, blockchain, or organizational notary) by the operator.

## Decision

Implement a portable `ferrumctl audit export` bundle format and an optional, feature-gated WORM sink integration.

### 1. Portable audit bundle format

The canonical bundle format is implemented in the shared `ferrum-audit-bundle` crate and used by both `ferrumctl` and the WORM sink:

- A JSON Lines (`audit.jsonl`) file containing all audit entries in chronological order, each with its `content_hash` and `previous_hash`.
- A separate manifest file (`manifest.json`) containing:
  - Bundle version (`1`).
  - Export timestamp.
  - First and last entry hash (bookends for integrity).
  - A Merkle root of the entire chain.
  - Total entry count.
- A `verify` subcommand (`ferrumctl audit verify --bundle <path>`) that checks:
  - Hash chain continuity (every `content_hash` matches the computed SHA-256 of the entry).
  - Merkle root matches the recomputed root.
  - No duplicate sequence numbers.
  - Entry count matches the manifest.
- The bundle is **not encrypted** by default; operators may encrypt at rest via filesystem or sink-level encryption.

### 2. Optional WORM sink integration

The WORM sink is implemented as a background worker in `ferrum-gateway` and is only compiled when the `worm-sink` feature is enabled.

- The worker periodically scans the database audit log in ascending `id` order, builds bundles using `ferrum-audit-bundle`, and uploads them to an S3-compatible Object Lock bucket.
- Uploads use S3 Object Lock with a configurable mode (`governance` or `compliance`) and retention period.
- An optional legal hold can be applied to uploaded objects.
- The worker uses deterministic bundle keys (`prefix/audit-bundle-{first_id}-{last_id}/audit.jsonl` and `.../manifest.json`) so restarts can safely re-scan without a durable checkpoint.
- The WORM sink is **not** a real-time replacement for the database audit log; it is an eventual-consistency replica for long-term archival.
- Failures are best-effort: logged and counted via metrics; they do not block the request path or governance requests.
- Feature-gated: `worm-sink` (implies `s3`) to avoid pulling in AWS SDK dependencies by default.
- The adapter does not create buckets, enable Object Lock, manage IAM, or perform deletes/retention bypasses. The operator must provision the bucket and Object Lock configuration.

## Consequences

- **Positive**: Operators can perform offline forensic review without access to the running system.
- **Positive**: WORM-compatible S3 Object Lock storage provides stronger tamper resistance than a local database alone.
- **Positive**: Merkle root enables efficient third-party verification of bundle integrity.
- **Negative**: WORM sink adds latency and cost (object store egress/ingress).
- **Negative**: Background export requires careful error handling and retry logic to avoid audit gaps; the current implementation retries on the next interval and uses deterministic keys. With S3 Object Lock and versioning enabled, deterministic re-uploads of the same key create additional object versions rather than overwriting existing objects, so gaps on restart are avoided without requiring a durable checkpoint store.
- **Non-goal**: This does not make FerrumGate compliance-certified; it provides a building block that operators can integrate into their own compliance program.

## Acceptance criteria

1. `ferrumctl audit export` produces a valid `.jsonl` + `manifest.json` bundle from SQLite and PostgreSQL. ✅
2. `ferrumctl audit verify` passes on a valid bundle and fails on a tampered bundle with a clear error message. ✅
3. Bundle format is implemented in the shared `ferrum-audit-bundle` crate and used by both `ferrumctl` and the WORM sink. ✅
4. WORM sink is implemented behind the `worm-sink` feature gate and uses S3 Object Lock. ✅
5. WORM sink configuration is validated at startup (bucket name, prefix, retention, interval, mode). ✅
6. Metrics: `ferrumgate_audit_worm_sink_exports_total`, `ferrumgate_audit_worm_sink_failures_total`, `ferrumgate_audit_worm_sink_last_success_timestamp_seconds` are emitted. ✅
7. Safe language only: documentation describes "S3 Object Lock integration" and "WORM-compatible audit bundle sink"; no compliance, certification, or tamper-proof claims. ✅
8. Documentation and config examples updated. ✅

## Non-goals

- Real-time synchronous WORM writes (would add unacceptable latency to the critical path).
- Encryption-at-rest inside the bundle (operator handles this via sink-level or filesystem encryption).
- Blockchain or timestamp-authority anchoring as a built-in feature (the Merkle root format is designed to allow external anchoring, but the anchoring itself is operator-owned).
- Replacing the database audit log with WORM storage (WORM is a replica, not a primary store).
- Creating buckets, enabling Object Lock, managing IAM, or performing deletes/retention bypasses in the adapter.
