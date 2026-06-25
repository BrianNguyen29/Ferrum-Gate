# ADR 009 — WORM Export and Portable Audit Bundle

## Status
Proposed

## Context

The current audit log is append-only with a SHA-256 hash chain (`previous_hash` linkage), providing tamper-evident detection of unauthorized changes. However, the audit log is stored in the same SQLite/PostgreSQL database as operational data, and there is no offline, portable, or write-once-read-many (WORM) export mechanism.

Operators who need stronger tamper resistance, offline forensic review, or long-term archival require a portable audit bundle that can be:
- Exported to a WORM-capable sink (e.g., S3 Object Lock, GCP Bucket Lock, Azure Immutable Blob).
- Verified independently without running `ferrumd`.
- Cryptographically anchored to an external source (e.g., timestamp authority, blockchain, or organizational notary).

## Decision

Propose a portable `ferrumctl audit export` bundle format and an optional WORM sink integration.

### 1. Portable audit bundle format
- A JSON Lines (`.jsonl`) file containing all audit entries in chronological order, each with its `hash` and `previous_hash`.
- A separate manifest file (`manifest.json`) containing:
  - Bundle version (`1`).
  - Export timestamp.
  - First and last entry hash (bookends for integrity).
  - A Merkle root of the entire chain.
- A `verify` subcommand (`ferrumctl audit verify --bundle <path>`) that checks:
  - Hash chain continuity (every `hash` matches the computed SHA-256 of the entry).
  - Merkle root matches the recomputed root.
  - No gaps or duplicate sequence numbers.
- The bundle is **not encrypted** by default (operator can encrypt at rest via filesystem or sink-level encryption).

### 2. Optional WORM sink integration
- An `AuditSink` trait with two implementations: `DatabaseSink` (current) and `WormSink`.
- `WormSink` writes to a WORM-enabled object store (S3 Object Lock in Compliance mode, or equivalent) using the same bundle format.
- A background task periodically exports the latest audit entries to the WORM sink.
- The WORM sink is **not** a real-time replacement for the database audit log; it is an eventual-consistency replica for long-term archival.
- Feature-gated: `--features worm-sink` (or similar) to avoid pulling in additional SDK dependencies by default.

## Consequences

- **Positive**: Operators can perform offline forensic review without access to the running system.
- **Positive**: WORM storage provides stronger tamper resistance than a local database alone.
- **Positive**: Merkle root enables efficient third-party verification of bundle integrity.
- **Negative**: WORM sink adds latency and cost (object store egress/ingress).
- **Negative**: Background export requires careful error handling and retry logic to avoid audit gaps.
- **Non-goal**: This does not make FerrumGate compliance-certified; it provides a building block that operators can integrate into their own compliance program.

## Acceptance criteria

1. `ferrumctl audit export` produces a valid `.jsonl` + `manifest.json` bundle from SQLite and PostgreSQL.
2. `ferrumctl audit verify` passes on a valid bundle and fails on a tampered bundle with a clear error message.
3. Bundle format is documented with schema in `docs/security/audit-bundle-format.md`.
4. `AuditSink` trait is defined and `DatabaseSink` is refactored to implement it.
5. `WormSink` is implemented behind a feature gate and tested with a local MinIO instance using Object Lock (Compliance mode).
6. WORM sink configuration is validated at startup (bucket exists, Object Lock is enabled).
7. Metrics: `ferrumgate_audit_worm_sink_exports_total`, `ferrumgate_audit_worm_sink_failures_total`, `ferrumgate_audit_bundle_verifications_total`.
8. Documentation updated: `docs/guides/security-model.md`, `docs/operations/runbook.md`, `docs/PRODUCTION_NOTES.md`.

## Non-goals

- Real-time synchronous WORM writes (would add unacceptable latency to the critical path).
- Encryption-at-rest inside the bundle (operator handles this via sink-level or filesystem encryption).
- Blockchain anchoring as a built-in feature (the Merkle root format is designed to allow external anchoring, but the anchoring itself is operator-owned).
- Replacing the database audit log with WORM storage (WORM is a replica, not a primary store).
