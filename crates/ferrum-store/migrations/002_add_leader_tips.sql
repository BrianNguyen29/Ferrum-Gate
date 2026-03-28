-- Leader tip cache for PF8 (Sync-2 read-only preflight).
-- Stores the last-known leader tip retrieved via transport probe.
--
-- Minimum honest fields:
--   leader_address: primary key (how we identify/look up the leader)
--   sequence: leader tip sequence number
--   hash: leader tip hash (Sha256Hex)
--   fetched_at: ISO8601 timestamp when this tip was fetched from the leader
--
-- This table is populated by the transport layer (Sync-3 probe) and consumed
-- by the preflight checker (PF8) to determine if a leader tip is available.
-- It does NOT store leader identity separately from leader_address in this slice:
-- we use leader_address as the identity key, consistent with current transport
-- semantics in ProbeFacadeRequest::leader_address.

CREATE TABLE IF NOT EXISTS leader_tips (
    leader_address TEXT PRIMARY KEY,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    hash TEXT NOT NULL,
    fetched_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_leader_tips_fetched_at ON leader_tips(fetched_at);