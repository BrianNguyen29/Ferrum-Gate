-- Shared nonce cache for agent-auth replay protection across gateway processes.
CREATE TABLE IF NOT EXISTS nonce_cache (
    nonce TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_nonce_cache_expires_at
    ON nonce_cache (expires_at);
