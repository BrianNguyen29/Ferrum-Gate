CREATE TABLE IF NOT EXISTS quarantine_holds (
    hold_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    state TEXT NOT NULL,
    reason TEXT NOT NULL,
    matched_rule_ids TEXT NOT NULL,
    policy_bundle_id TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    resolved_at TEXT,
    resolved_by TEXT,
    resolution_reason TEXT,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES intents(intent_id),
    FOREIGN KEY (proposal_id) REFERENCES proposals(proposal_id)
);

CREATE INDEX IF NOT EXISTS idx_quarantine_holds_state ON quarantine_holds(state);
CREATE INDEX IF NOT EXISTS idx_quarantine_holds_intent_id ON quarantine_holds(intent_id);
CREATE INDEX IF NOT EXISTS idx_quarantine_holds_proposal_id ON quarantine_holds(proposal_id);
CREATE INDEX IF NOT EXISTS idx_quarantine_holds_expires_at ON quarantine_holds(expires_at);
