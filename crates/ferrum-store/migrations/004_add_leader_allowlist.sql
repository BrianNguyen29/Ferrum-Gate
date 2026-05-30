-- PF4: Local leader allowlist for sync authorization.
-- Deny-by-default: missing entry => unauthorized (Ok(false), not error)

CREATE TABLE IF NOT EXISTS leader_allowlist (
    leader_address TEXT PRIMARY KEY,
    authorized INTEGER NOT NULL DEFAULT 0 CHECK (authorized IN (0, 1)),
    added_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_leader_allowlist_authorized ON leader_allowlist(authorized);
