CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_reconciliation_lease
ON lifecycle_outbox(
    status,
    reconciliation_lease_expires_at,
    reconciliation_lease_generation,
    created_at,
    outbox_id
);
