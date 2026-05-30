CREATE TABLE IF NOT EXISTS audit_checkpoints (
    window_start TEXT PRIMARY KEY,
    merkle_root TEXT NOT NULL,
    entry_count INTEGER NOT NULL,
    signer_id TEXT NOT NULL,
    signer_key_fingerprint TEXT NOT NULL,
    signed_at TEXT NOT NULL,
    signature TEXT NOT NULL,
    public_key TEXT NOT NULL
);
