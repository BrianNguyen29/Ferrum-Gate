-- Sync state tables for PF2/PF6/PF7 (Sync-2 read-only preflight).
-- PF2: has_inflight_commits
-- PF6: has_uncommitted_entries
-- PF7: sync_in_progress
-- The follower tip is read directly from ledger_entries.

CREATE TABLE IF NOT EXISTS sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    has_inflight_commits INTEGER NOT NULL DEFAULT 0,
    has_uncommitted_entries INTEGER NOT NULL DEFAULT 0,
    sync_in_progress INTEGER NOT NULL DEFAULT 0
);

INSERT OR IGNORE INTO sync_state (id, has_inflight_commits, has_uncommitted_entries, sync_in_progress)
VALUES (1, 0, 0, 0);
