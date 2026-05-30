CREATE TABLE IF NOT EXISTS audit_merkle_roots (
    window_start TEXT PRIMARY KEY,
    root TEXT NOT NULL,
    entry_count INTEGER NOT NULL,
    computed_at TEXT NOT NULL
);
