-- Intentionally no FK to agents in this slice; add composite index for get_active_for_agent.
CREATE INDEX IF NOT EXISTS idx_mfa_credentials_active_lookup ON mfa_credentials(agent_id, status, revoked_at, created_at DESC);
