ALTER TABLE mfa_credentials ADD COLUMN failed_attempts INTEGER DEFAULT 0;
ALTER TABLE mfa_credentials ADD COLUMN locked_until TEXT;
ALTER TABLE mfa_credentials ADD COLUMN last_failed_at TEXT;
ALTER TABLE mfa_credentials ADD COLUMN lockout_count INTEGER DEFAULT 0;
