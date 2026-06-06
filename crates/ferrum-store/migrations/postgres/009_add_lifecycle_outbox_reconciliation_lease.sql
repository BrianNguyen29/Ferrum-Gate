ALTER TABLE lifecycle_outbox
ADD COLUMN IF NOT EXISTS reconciliation_lease_owner TEXT;

ALTER TABLE lifecycle_outbox
ADD COLUMN IF NOT EXISTS reconciliation_lease_expires_at TEXT;

CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_reconciliation_lease
    ON lifecycle_outbox(status, reconciliation_lease_expires_at, created_at, outbox_id);
