CREATE TABLE IF NOT EXISTS agent_registry (
    agent_id TEXT PRIMARY KEY,
    public_key TEXT NOT NULL,
    key_fingerprint TEXT NOT NULL UNIQUE,
    allowed_scopes TEXT NOT NULL,
    created_at TEXT NOT NULL,
    revoked_at TEXT,
    description TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_registry_key_fingerprint ON agent_registry(key_fingerprint);
CREATE INDEX IF NOT EXISTS idx_agent_registry_revoked_at ON agent_registry(revoked_at);
