# Tamper-Evident Audit Design

## Scope

- IN (implemented):
  - Deterministic SHA-256 content hash per audit log entry.
  - `previous_hash` linkage forming a linear chain.
  - `AuditLogRepo::verify_chain()` for SQLite and PostgreSQL.
  - Gateway endpoint `GET /v1/admin/audit/verify` (requires `admin:audit` scope).
  - `ferrumctl audit verify` remote mode.
  - `ferrumctl admin audit export --bundle <dir>`: portable `.jsonl` + `manifest.json` export using the existing admin export endpoint.
  - `ferrumctl admin audit verify --bundle <dir>`: local bundle verification (hash chain, Merkle root, duplicate detection, tamper detection) without server access.
  - Legacy entries without hash fields are skipped during verification.
  - Recomputation check: stored `content_hash` is recomputed from canonical fields on verify.
  - Merkle root per hourly UTC-aligned window:
    - Domain-separated SHA-256 Merkle tree (`0x00` leaf prefix, `0x01` internal prefix).
    - Odd-count duplication at each level.
    - Deterministic ordering by `id ASC`.
    - Excludes legacy entries missing `content_hash`.
    - Cached in `audit_merkle_roots` table (idempotent insert).
    - Gateway endpoints `GET /v1/admin/audit/merkle-verify?window_start=...` and `GET /v1/admin/audit/merkle-roots`.
    - CLI commands `ferrumctl admin audit merkle-verify` and `ferrumctl admin audit merkle-roots`.
  - Ed25519-signed checkpoint over Merkle root:
    - Canonical SHA-256 payload hash: alphabetically sorted compact JSON `{entry_count, merkle_root, signed_at, window_start}`.
    - Ed25519 signature over payload hash; Base64-encoded signature and public key stored.
    - SHA-256 fingerprint of public key stored as `signer_key_fingerprint`.
    - Stored in `audit_checkpoints` table with `window_start` as primary key (one checkpoint per window).
    - Gateway endpoints:
      - `POST /v1/admin/audit/checkpoints` — create checkpoint (verifies signature against submitted Merkle root before storing).
      - `GET /v1/admin/audit/checkpoints/{window_start}/verify` — verify stored checkpoint: recompute Merkle root, recompute payload hash, verify Ed25519 signature.
      - `GET /v1/admin/audit/checkpoints` — list checkpoints with cursor-based pagination.
    - CLI commands: `ferrumctl admin audit checkpoint-sign`, `checkpoint-verify`, `checkpoint-list`.

- Not yet implemented:
  - External anchoring (e.g., blockchain, timestamp authority).
  - WORM sink integration.
  - ~~Local DB direct-verify mode in CLI~~ (Done via `verify --bundle`).

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

- **Privileged attacker with full DB access**: can truncate the table, rebuild hashes, and produce a valid-looking chain. Mitigation requires external anchoring or WORM storage (not yet implemented).
- **Clock manipulation**: `created_at` is part of the canonical hash; changing the timestamp changes the hash, which is detectable.
- **Collision resistance**: SHA-256 is used; no custom collision resistance claim beyond the hash function itself.

## API / CLI

### Remote Verify

```bash
ferrumctl admin audit verify
```

Calls `GET /v1/admin/audit/verify` and prints:
- `VALID` if the chain is intact.
- `INVALID` with error detail if tampering or a break is detected.

### Bundle Export

```bash
ferrumctl admin audit export --bundle /tmp/audit-bundle
```

Fetches the full audit log from the server as NDJSON, writes `audit.jsonl` and `manifest.json` into the specified directory. The manifest includes:
- Bundle version (`1`).
- Export timestamp.
- First and last entry `content_hash`.
- Merkle root of the hashed entry chain.
- Total entry count.

### Bundle Verify

```bash
ferrumctl admin audit verify --bundle /tmp/audit-bundle
```

Reads the local bundle and verifies:
- Hash chain continuity (`previous_hash` linkage).
- Content hash recomputation (tamper detection per entry).
- Merkle root recomputation.
- No duplicate sequence numbers (`id`).
- Entry count matches manifest.

Prints `VALID` with manifest details, or a clear error on any failure. Does not require server access.

### Merkle Root Verify

```bash
ferrumctl admin audit merkle-verify --window-start 2024-01-01T00:00:00Z
```

Calls `GET /v1/admin/audit/merkle-verify?window_start=...` and prints the root hash and entry count for the requested hourly window. Returns 400 if `window_start` is not aligned to the hour.

### Merkle Root List

```bash
ferrumctl admin audit merkle-roots --limit 50
```

Calls `GET /v1/admin/audit/merkle-roots` and lists cached roots with cursor-based pagination.

### Checkpoint Sign

```bash
ferrumctl admin audit checkpoint-sign --window-start 2024-01-01T00:00:00Z --signer-id operator-1 --private-key <base64-ed25519-private-key>
```

Generates an Ed25519 signature over the canonical checkpoint payload and submits it via `POST /v1/admin/audit/checkpoints`. The server verifies the signature against the submitted Merkle root and entry count before storing. Returns `201 Created` on success; `400` if the Merkle root does not match the server-computed root for the window.

### Checkpoint Verify

```bash
ferrumctl admin audit checkpoint-verify --window-start 2024-01-01T00:00:00Z
```

Calls `GET /v1/admin/audit/checkpoints/{window_start}/verify` and:
- Recomputes the Merkle root for the window.
- Recomputes the canonical payload hash.
- Verifies the stored Ed25519 signature against the public key.
- Prints `VALID` or `INVALID` with detailed error.

### Checkpoint List

```bash
ferrumctl admin audit checkpoint-list --limit 50
```

Calls `GET /v1/admin/audit/checkpoints` and lists signed checkpoints with cursor-based pagination.

### Gateway Endpoints

- `GET /v1/admin/audit/verify` — Scope: `admin:audit`
- `GET /v1/admin/audit/merkle-verify?window_start=...` — Scope: `admin:audit`
- `GET /v1/admin/audit/merkle-roots?cursor=&limit=` — Scope: `admin:audit`
- `POST /v1/admin/audit/checkpoints` — Scope: `admin:audit`
- `GET /v1/admin/audit/checkpoints/{window_start}/verify` — Scope: `admin:audit`
- `GET /v1/admin/audit/checkpoints?cursor=&limit=` — Scope: `admin:audit`

## Design Extensions

1. ~~Merkle root per time window (e.g., hourly) for batch verification.~~ (Done)
2. ~~Signed checkpoint with Ed25519 signature over Merkle root.~~ (Done)
3. ~~`ferrumctl audit export` producing a portable verification bundle.~~ (Done)
4. Optional external anchoring to a transparency log or blockchain.
5. ~~Local direct-verify mode for operators with file-system access to the SQLite database.~~ (Done via `verify --bundle`).

## Notes

- This design does **not** provide Byzantine-fault tolerance.
- It does **not** replace a dedicated SIEM or compliance platform.
- It does **not** prevent a privileged attacker with full database access from rewriting history if they recompute the entire chain.
- SOC2 / formal compliance certification is **not** claimed.
