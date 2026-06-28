CREATE TABLE IF NOT EXISTS mfa_credentials (
    mfa_factor_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    factor_type TEXT NOT NULL,
    status TEXT NOT NULL,
    encrypted_secret TEXT NOT NULL,
    secret_nonce TEXT NOT NULL,
    encryption_key_id TEXT NOT NULL,
    label TEXT,
    created_at TEXT NOT NULL,
    verified_at TEXT,
    last_used_at TEXT,
    last_used_counter BIGINT,
    revoked_at TEXT,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mfa_credentials_agent_id ON mfa_credentials(agent_id);
CREATE INDEX IF NOT EXISTS idx_mfa_credentials_status_revoked ON mfa_credentials(status, revoked_at);
