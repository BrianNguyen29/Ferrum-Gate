-- H1.1c: Add supersedes_bundle_id column for lineage tracking.
-- A bundle can only be deleted if no other bundle supersedes it.
-- ON DELETE RESTRICT enforces this at the DB level even without app-level checks.

ALTER TABLE policy_bundles
ADD COLUMN supersedes_bundle_id TEXT REFERENCES policy_bundles(bundle_id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_policy_bundles_supersedes ON policy_bundles(supersedes_bundle_id);
