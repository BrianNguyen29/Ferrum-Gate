# 05a — Policy Version History Design

> **Status**: Design artifact. POL-5 design-first; implementation NOT STARTED.
> **Owner**: Engineering
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)

---

## Goal

Define the design for policy bundle version history, diff, and rollback so that POL-5 can be implemented without mid-stream architectural decisions.

## Current state

- Policy bundles have CRUD but no history table.
- Active bundle switch emits a provenance event (`test_policy_bundle_active_switch_emits_provenance`).
- No previous versions are retained.
- Rollback is not possible.

## Design decisions

### 1. Version table

A new `policy_bundle_version` table stores immutable snapshots:

```
policy_bundle_version
  id            UUID PK
  bundle_id     UUID FK -> policy_bundle.id
  version       INTEGER  (monotonic per bundle_id)
  content       JSONB    (full bundle snapshot)
  active        BOOLEAN  (exactly one active per bundle_id)
  created_at    TIMESTAMPTZ
  created_by    TEXT     (actor identifier; nullable until auth model lands)
  note          TEXT     (optional human description)
```

- `CREATE` on a bundle inserts a new version row with `version = max(version)+1`.
- `UPDATE` on a bundle creates a new version row; old row remains immutable.
- `DELETE` on a bundle soft-deletes the bundle record but keeps all version rows.
- `SET ACTIVE` updates `active` on the target version and clears `active` on the prior active version.

### 2. Rollback semantics

- **Rollback target**: any previous version by `version` number.
- **Rollback operation**: creates a NEW version row whose `content` is a copy of the target version’s `content`, with `version = max(version)+1` and a generated `note` (e.g., `Rollback to v3`).
- **Rollback is NOT an in-place revert**: this preserves auditability and avoids mutating history.
- **Rollback provenance event**: emits `PolicyBundleRolledBack` with `from_version`, `to_version`, `bundle_id`, `actor`, `timestamp`.
- **Rollback eligibility**:
  - Cannot rollback to a version whose `content` is identical to current active content.
  - Soft-deleted bundles cannot be rolled back unless restored first.

### 3. Diff strategy

- **Storage diff**: compare two version `content` JSONB blobs at the DB layer.
- **API diff**: `GET /v1/policy-bundles/{id}/diff?from={v}&to={v}` returns a structural diff (added/removed/changed rules, conditions, metadata).
- **CLI diff**: `ferrumctl policy diff --bundle-id {id} [--from v] [--to v]` renders the diff in unified or JSON format.
- **Diff implementation**: server-side JSON diff using a deterministic deep-diff algorithm; no external dependency beyond `serde_json`.

### 4. Migration / backfill plan

- **Step 1**: Create `policy_bundle_version` table via `ferrum-migrate`.
- **Step 2**: Backfill: for each existing `policy_bundle`, insert one version row (`version = 1`) with current `content` and `active = true`.
- **Step 3**: Update bundle CRUD to write version rows on create/update.
- **Step 4**: Add rollback and diff endpoints.
- **Step 5**: Add CLI commands for diff and rollback.
- **Backfill is idempotent**: running the backfill migration twice is a no-op (upsert by bundle_id + version).

### 5. Provenance event for rollback

```
event_type: PolicyBundleRolledBack
payload:
  bundle_id: UUID
  from_version: INTEGER
  to_version: INTEGER
  to_version_is_copy: true   # always true under this design
  actor: TEXT
  timestamp: TIMESTAMPTZ
```

- Emitted by the rollback endpoint before the new version is committed.
- If rollback fails after emission, the provenance event remains as an auditable record of the attempt.

### 6. Acceptance mapping

| Criterion | How this design satisfies it |
|-----------|------------------------------|
| POL-5: Rollback restores prior active bundle | Rollback creates a new version with copied content from target version and sets it active. |
| POL-5: Rollback is auditable | `PolicyBundleRolledBack` provenance event emitted with from/to versions and actor. |
| POL-5: History is immutable | Old version rows are never modified; rollback creates a new version. |
| POL-5: Diff is available | `GET /v1/policy-bundles/{id}/diff` and `ferrumctl policy diff` render structural diffs. |

## Implementation tasks (post-design)

- [ ] Create `policy_bundle_version` migration.
- [ ] Backfill existing bundles.
- [ ] Update bundle create/update to insert version rows.
- [ ] Implement `GET /v1/policy-bundles/{id}/versions`.
- [ ] Implement `GET /v1/policy-bundles/{id}/diff`.
- [ ] Implement `POST /v1/policy-bundles/{id}/rollback`.
- [ ] Implement `ferrumctl policy diff`.
- [ ] Implement `ferrumctl policy rollback`.
- [ ] Add integration tests for version history, diff, and rollback.
- [ ] Add rollback provenance event test.

## Non-claims

- **NOT implemented**: This doc is a design artifact only.
- **NOT production-ready**: Version history does not change the production-ready posture.
- **NOT a versioning scheme**: Version numbers are monotonic integers, not SemVer.
- **NOT multi-tenant aware**: `tenant_id` is not in this design; defer to Phase 4 tenant model.

## Related docs

- [`05-policy-authoring-ux-plan.md`](05-policy-authoring-ux-plan.md) — Parent plan for policy authoring UX.
- [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) — Tenant model may add `tenant_id` and `actor_id` constraints later.
