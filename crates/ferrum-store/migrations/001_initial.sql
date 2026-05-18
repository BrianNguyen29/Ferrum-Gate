CREATE TABLE IF NOT EXISTS intents (
    intent_id TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    normalized_goal TEXT NOT NULL,
    status TEXT NOT NULL,
    risk_tier TEXT NOT NULL,
    approval_mode TEXT NOT NULL,
    default_rollback_class TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_intents_status ON intents(status);
CREATE INDEX IF NOT EXISTS idx_intents_principal_id ON intents(principal_id);
CREATE INDEX IF NOT EXISTS idx_intents_expires_at ON intents(expires_at);

CREATE TABLE IF NOT EXISTS proposals (
    proposal_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    server_name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    estimated_risk TEXT NOT NULL,
    requested_rollback_class TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES intents(intent_id)
);

CREATE INDEX IF NOT EXISTS idx_proposals_intent_id ON proposals(intent_id);
CREATE INDEX IF NOT EXISTS idx_proposals_step_index ON proposals(step_index);

CREATE TABLE IF NOT EXISTS capabilities (
    capability_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    status TEXT NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    revoked_at TEXT,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES intents(intent_id),
    FOREIGN KEY (proposal_id) REFERENCES proposals(proposal_id)
);

CREATE INDEX IF NOT EXISTS idx_capabilities_intent_id ON capabilities(intent_id);
CREATE INDEX IF NOT EXISTS idx_capabilities_proposal_id ON capabilities(proposal_id);
CREATE INDEX IF NOT EXISTS idx_capabilities_status ON capabilities(status);
CREATE INDEX IF NOT EXISTS idx_capabilities_expires_at ON capabilities(expires_at);

CREATE TABLE IF NOT EXISTS executions (
    execution_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    capability_id TEXT NOT NULL,
    rollback_contract_id TEXT,
    decision TEXT NOT NULL,
    state TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    result_digest TEXT,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES intents(intent_id),
    FOREIGN KEY (proposal_id) REFERENCES proposals(proposal_id),
    FOREIGN KEY (capability_id) REFERENCES capabilities(capability_id)
);

CREATE INDEX IF NOT EXISTS idx_executions_intent_id ON executions(intent_id);
CREATE INDEX IF NOT EXISTS idx_executions_capability_id ON executions(capability_id);
CREATE INDEX IF NOT EXISTS idx_executions_state ON executions(state);

CREATE TABLE IF NOT EXISTS rollback_contracts (
    contract_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    execution_id TEXT NOT NULL,
    adapter_key TEXT NOT NULL,
    action_type TEXT NOT NULL,
    rollback_class TEXT NOT NULL,
    state TEXT NOT NULL,
    auto_commit INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES intents(intent_id),
    FOREIGN KEY (proposal_id) REFERENCES proposals(proposal_id),
    FOREIGN KEY (execution_id) REFERENCES executions(execution_id)
);

CREATE INDEX IF NOT EXISTS idx_rollback_contracts_execution_id ON rollback_contracts(execution_id);
CREATE INDEX IF NOT EXISTS idx_rollback_contracts_state ON rollback_contracts(state);

CREATE TABLE IF NOT EXISTS approvals (
    approval_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    execution_id TEXT,
    action_digest TEXT NOT NULL,
    state TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (intent_id) REFERENCES intents(intent_id),
    FOREIGN KEY (proposal_id) REFERENCES proposals(proposal_id)
);

CREATE INDEX IF NOT EXISTS idx_approvals_state ON approvals(state);
CREATE INDEX IF NOT EXISTS idx_approvals_intent_id ON approvals(intent_id);

CREATE TABLE IF NOT EXISTS provenance_events (
    event_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    intent_id TEXT,
    proposal_id TEXT,
    execution_id TEXT,
    capability_id TEXT,
    rollback_contract_id TEXT,
    policy_bundle_id TEXT,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_provenance_events_kind ON provenance_events(kind);
CREATE INDEX IF NOT EXISTS idx_provenance_events_intent_id ON provenance_events(intent_id);
CREATE INDEX IF NOT EXISTS idx_provenance_events_execution_id ON provenance_events(execution_id);
CREATE INDEX IF NOT EXISTS idx_provenance_events_capability_id ON provenance_events(capability_id);
CREATE INDEX IF NOT EXISTS idx_provenance_events_occurred_at ON provenance_events(occurred_at);

CREATE TABLE IF NOT EXISTS provenance_edges (
    edge_id INTEGER PRIMARY KEY AUTOINCREMENT,
    to_event_id TEXT NOT NULL,
    from_event_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    summary TEXT,
    FOREIGN KEY (to_event_id) REFERENCES provenance_events(event_id),
    FOREIGN KEY (from_event_id) REFERENCES provenance_events(event_id)
);

CREATE INDEX IF NOT EXISTS idx_provenance_edges_to_event_id ON provenance_edges(to_event_id);
CREATE INDEX IF NOT EXISTS idx_provenance_edges_from_event_id ON provenance_edges(from_event_id);

CREATE TABLE IF NOT EXISTS ledger_entries (
    entry_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    intent_id TEXT,
    execution_id TEXT,
    occurred_at TEXT NOT NULL,
    content_hash TEXT,
    previous_ledger_hash TEXT,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (event_id) REFERENCES provenance_events(event_id)
);

CREATE INDEX IF NOT EXISTS idx_ledger_entries_occurred_at ON ledger_entries(occurred_at);
CREATE INDEX IF NOT EXISTS idx_ledger_entries_intent_id ON ledger_entries(intent_id);
CREATE INDEX IF NOT EXISTS idx_ledger_entries_execution_id ON ledger_entries(execution_id);

CREATE TABLE IF NOT EXISTS _schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
