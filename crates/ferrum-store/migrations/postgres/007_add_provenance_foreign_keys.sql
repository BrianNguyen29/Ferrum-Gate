DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_provenance_edges_to_event'
    ) THEN
        ALTER TABLE provenance_edges
               ADD CONSTRAINT fk_provenance_edges_to_event
               FOREIGN KEY (to_event_id)
               REFERENCES provenance_events(event_id);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_provenance_edges_from_event'
    ) THEN
        ALTER TABLE provenance_edges
               ADD CONSTRAINT fk_provenance_edges_from_event
               FOREIGN KEY (from_event_id)
               REFERENCES provenance_events(event_id);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_ledger_entries_event'
    ) THEN
        ALTER TABLE ledger_entries
               ADD CONSTRAINT fk_ledger_entries_event
               FOREIGN KEY (event_id)
               REFERENCES provenance_events(event_id);
    END IF;
END $$;
