ALTER TABLE lifecycle_outbox
ADD COLUMN IF NOT EXISTS reconciliation_lease_generation BIGINT NOT NULL DEFAULT 0;

ALTER TABLE lifecycle_outbox
ALTER COLUMN reconciliation_lease_expires_at TYPE TIMESTAMPTZ
USING reconciliation_lease_expires_at::TIMESTAMPTZ;

DROP INDEX IF EXISTS idx_lifecycle_outbox_reconciliation_lease;

CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_reconciliation_lease
    ON lifecycle_outbox(
        status,
        reconciliation_lease_expires_at,
        reconciliation_lease_generation,
        created_at,
        outbox_id
    );
