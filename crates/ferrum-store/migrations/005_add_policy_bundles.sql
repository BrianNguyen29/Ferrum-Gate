CREATE TABLE IF NOT EXISTS policy_bundles (
    bundle_id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_policy_bundles_content_hash ON policy_bundles(content_hash);
CREATE INDEX IF NOT EXISTS idx_policy_bundles_active ON policy_bundles(active);
