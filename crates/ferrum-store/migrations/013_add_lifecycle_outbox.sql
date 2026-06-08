CREATE TABLE IF NOT EXISTS lifecycle_outbox (
    outbox_id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    rollback_contract_id TEXT,
    previous_execution_state TEXT,
    new_execution_state TEXT NOT NULL,
    previous_rollback_state TEXT,
    new_rollback_state TEXT,
    intended_provenance_kind TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL,
    provenance_event_id TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    reconciliation_lease_owner TEXT,
    reconciliation_lease_expires_at TEXT,
    reconciliation_lease_generation INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (execution_id) REFERENCES executions(execution_id),
    FOREIGN KEY (rollback_contract_id) REFERENCES rollback_contracts(contract_id),
    FOREIGN KEY (provenance_event_id) REFERENCES provenance_events(event_id)
);

CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_status ON lifecycle_outbox(status);
CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_execution_id ON lifecycle_outbox(execution_id);
CREATE INDEX IF NOT EXISTS idx_lifecycle_outbox_idempotency_key ON lifecycle_outbox(idempotency_key);
