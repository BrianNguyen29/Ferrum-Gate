-- H1.1a: Policy bundle persistence table
-- Stores authored intent outcome contracts for auditability and reuse.
-- bundle_id is derived deterministically from content fingerprint.

CREATE TABLE IF NOT EXISTS policy_bundles (
    bundle_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    version TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_policy_bundles_created_at ON policy_bundles(created_at DESC);
