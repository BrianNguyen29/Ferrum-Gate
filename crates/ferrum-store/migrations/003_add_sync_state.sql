-- Sync state tables for PF2/PF6/PF7 (Sync-2 read-only preflight).
--
-- These tables provide authoritative boolean surfaces for the three remaining
-- read-only preflight checks that were previously unsupported:
--
--   PF2: has_inflight_commits  — true if the local node has commits in-flight
--   PF6: has_uncommitted_entries — true if the local ledger has uncommitted entries
--   PF7: sync_in_progress      — true if a sync session is currently active
--
-- The follower tip (also returned by read_local_state) is read directly from
-- ledger_entries, not from this table.
--
-- Design: single-row sync_state table (id=1) with three nullable integer columns.
-- NULL means "unknown/not-yet-set" which maps to false for preflight purposes.
-- In this slice, all three flags are always false because the write-path
-- that would set them to true is not yet implemented. The tables provide
-- the authoritative surface; the test helpers provide narrow runtime mutation
-- for test scenarios that need to exercise PF2/PF6/PF7 failure paths.

CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    has_inflight_commits INTEGER NOT NULL DEFAULT 0,
    has_uncommitted_entries INTEGER NOT NULL DEFAULT 0,
    sync_in_progress INTEGER NOT NULL DEFAULT 0
);

-- The table always has exactly one row after migration.
-- Insert the initial default row if not present.
INSERT OR IGNORE INTO sync_state (id, has_inflight_commits, has_uncommitted_entries, sync_in_progress)
VALUES (1, 0, 0, 0);
