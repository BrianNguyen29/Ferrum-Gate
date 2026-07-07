CREATE TABLE IF NOT EXISTS mfa_agent_lockouts (
    agent_id TEXT PRIMARY KEY,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,
    last_failed_at TEXT,
    lockout_count INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
