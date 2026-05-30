CREATE TABLE IF NOT EXISTS scoped_tokens (
    token_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    role TEXT NOT NULL,
    scopes TEXT NOT NULL,
    description TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT,
    revoked_reason TEXT,
    rotated_from TEXT,
    token_lookup_hash TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    token_salt TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scoped_tokens_actor_id ON scoped_tokens(actor_id);
CREATE INDEX IF NOT EXISTS idx_scoped_tokens_role ON scoped_tokens(role);
CREATE INDEX IF NOT EXISTS idx_scoped_tokens_revoked_at ON scoped_tokens(revoked_at);
CREATE INDEX IF NOT EXISTS idx_scoped_tokens_token_lookup_hash ON scoped_tokens(token_lookup_hash);
