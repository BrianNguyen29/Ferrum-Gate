CREATE TABLE IF NOT EXISTS policy_bundle_version (
    id TEXT PRIMARY KEY,
    bundle_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    content TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    created_by TEXT,
    note TEXT,
    UNIQUE(bundle_id, version)
);

CREATE INDEX IF NOT EXISTS idx_policy_bundle_version_bundle_id ON policy_bundle_version(bundle_id);
CREATE INDEX IF NOT EXISTS idx_policy_bundle_version_bundle_id_version ON policy_bundle_version(bundle_id, version);

-- Backfill: create version 1 for every existing policy bundle.
-- Idempotent: skip if bundle_id + version 1 already exists.
INSERT OR IGNORE INTO policy_bundle_version (id, bundle_id, version, content, active, created_at, created_by, note)
SELECT
    lower(hex(randomblob(16))),
    bundle_id,
    1,
    raw_json,
    active,
    created_at,
    NULL,
    'Initial version (backfilled)'
FROM policy_bundles;
