# Tamper-Evident Audit Design

## Status

Minimal viable implementation (MVP) — covers linear hash chain over audit log entries, remote verification via API/CLI, and safe legacy entry handling. Merkle roots per time window, signed checkpoints, and external anchoring are explicitly deferred.

## Scope

- IN (implemented):
  - Deterministic SHA-256 content hash per audit log entry.
  - `previous_hash` linkage forming a linear chain.
  - `AuditLogRepo::verify_chain()` for SQLite and PostgreSQL.
  - Gateway endpoint `GET /v1/admin/audit/verify` (requires `admin:audit` scope).
  - `ferrumctl audit verify` remote mode.
  - Legacy entries without hash fields are skipped during verification.
  - Recomputation check: stored `content_hash` is recomputed from canonical fields on verify.

- OUT / deferred:
  - Merkle tree root per time window.
  - Signed checkpoints.
  - Audit export bundle (`ferrumctl audit export`).
  - External anchoring (e.g., blockchain, timestamp authority).
  - WORM sink integration.
  - Local DB direct-verify mode in CLI (can be added later without server).

## Canonical Serialization

The content hash covers the following fields in fixed order via `serde_json::json!`:

```json
{
  "actor_id": "...",
  "action": "token_create",
  "resource_type": "token",
  "resource_id": "...",
  "result": "ok",
  "metadata": {...},
  "created_at": "2024-01-01T00:00:00+00:00"
}
```

Excluded from hashing:
- `id` (auto-generated)
- `content_hash`
- `previous_hash`

The JSON blob is serialized to UTF-8 bytes and hashed with SHA-256; the result is hex-encoded.

## Chain Rules

1. **Genesis**: the first entry with a `content_hash` must have `previous_hash = NULL`.
2. **Link**: each subsequent hashed entry's `previous_hash` must equal the prior hashed entry's `content_hash`.
3. **Gap tolerance**: legacy entries without `content_hash` are skipped. The chain resumes at the next hashed entry, linking to the last hashed entry before the gap.
4. **Recomputation**: on verify, each entry's `content_hash` is recomputed from stored fields. A mismatch signals tampering even if `previous_hash` linkage is intact.

## Legacy Entry Handling

Existing audit log rows created before this feature have `content_hash = NULL` and `previous_hash = NULL`. They are ignored by `verify_chain`. New entries appended after migration automatically compute and store hashes. There is no backfill requirement; the chain starts at the first post-migration entry.

## Threats Addressed

| Threat | Mitigation | Limitation |
|--------|------------|------------|
| Tamper with audit log content | Recomputed content hash detects modification | Attacker with DB write access can recompute hashes for the last entry; earlier entries break the chain |
| Delete or reorder entries | Linear `previous_hash` linkage detects gaps/reordering | Attacker with DB write access can rebuild the chain if they control all subsequent entries |
| Append fraudulent entries | Chain extends only from the latest hash; appending requires DB write access | Does not prevent unauthorized append if DB credentials are compromised |

## Threats NOT Addressed

- **Privileged attacker with full DB access**: can truncate the table, rebuild hashes, and produce a valid-looking chain. Mitigation requires external anchoring or WORM storage (deferred).
- **Clock manipulation**: `created_at` is part of the canonical hash; changing the timestamp changes the hash, which is detectable.
- **Collision resistance**: SHA-256 is used; no custom collision resistance claim beyond the hash function itself.

## API / CLI

### Remote Verify

```bash
ferrumctl audit verify
```

Calls `GET /v1/admin/audit/verify` and prints:
- `VALID` if the chain is intact.
- `INVALID` with error detail if tampering or a break is detected.

### Gateway Endpoint

- `GET /v1/admin/audit/verify`
- Scope required: `admin:audit`
- Returns `AuditLogVerifyResponse`:
  - `valid: bool`
  - `total_entries: usize`
  - `hashed_entries: usize`
  - `error: Option<String>`

## Future Work

1. Merkle root per time window (e.g., hourly) for batch verification.
2. Signed checkpoint exported to offline storage.
3. `ferrumctl audit export` producing a portable verification bundle.
4. Optional external anchoring to a transparency log or blockchain.
5. Local direct-verify mode for operators with file-system access to the SQLite database.

## Non-Claims

- This design does **not** provide Byzantine-fault tolerance.
- It does **not** replace a dedicated SIEM or compliance platform.
- It does **not** prevent a privileged attacker with full database access from rewriting history if they recompute the entire chain.
- SOC2 / formal compliance certification is **not** claimed.
