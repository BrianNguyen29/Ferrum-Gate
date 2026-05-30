-- Leader tip cache for PF8 (Sync-2 read-only preflight).
-- Stores the last-known leader tip retrieved via transport probe.

CREATE TABLE IF NOT EXISTS leader_tips (
    leader_address TEXT PRIMARY KEY,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    hash TEXT NOT NULL,
    fetched_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_leader_tips_fetched_at ON leader_tips(fetched_at);
