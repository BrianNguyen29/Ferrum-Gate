-- PF4: Local leader allowlist for sync authorization.
--
-- This table implements the PF4 capability model enforcement as a bounded
-- local allowlist keyed by `leader_address`. The contract is:
--
--   - Canonical key: `leader_address`
--   - Local allowlist (no external capability broker)
--   - Deny-by-default: missing entry => unauthorized (Ok(false), not error)
--   - Fail-closed on DB/read errors => Err
--
-- This table is NOT the generic capabilities lease table; it is a separate,
-- narrow sync-specific allowlist. The write/apply path is not yet implemented;
-- for now the table starts empty and all leaders are denied.
--
-- Design:
--   leader_address: TEXT PRIMARY KEY (stable transport-boundary identifier)
--   authorized:    INTEGER NOT NULL DEFAULT 0 (0=false, 1=true; BOOLEAN in SQLite)
--   added_at:       TEXT NOT NULL (ISO8601 UTC timestamp)
--
-- The table starts empty. `authorize_leader_test_only()` seeds entries for
-- test scenarios. The production transport adapter that populates this table
-- is deferred (Sync-3+ territory).

CREATE TABLE IF NOT EXISTS leader_allowlist (
    leader_address TEXT PRIMARY KEY,
    authorized INTEGER NOT NULL DEFAULT 0 CHECK (authorized IN (0, 1)),
    added_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_leader_allowlist_authorized ON leader_allowlist(authorized);
