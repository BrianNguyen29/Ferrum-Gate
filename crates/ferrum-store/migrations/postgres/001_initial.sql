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
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_executions_intent_id ON executions(intent_id);
CREATE INDEX IF NOT EXISTS idx_executions_capability_id ON executions(capability_id);
CREATE INDEX IF NOT EXISTS idx_executions_state ON executions(state);

CREATE TABLE IF NOT EXISTS proposals (
    proposal_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    server_name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    estimated_risk TEXT NOT NULL,
    requested_rollback_class TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_proposals_intent_id ON proposals(intent_id);

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
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_capabilities_intent_id ON capabilities(intent_id);

CREATE TABLE IF NOT EXISTS rollback_contracts (
    contract_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    execution_id TEXT NOT NULL,
    adapter_key TEXT NOT NULL,
    action_type TEXT NOT NULL,
    rollback_class TEXT NOT NULL,
    state TEXT NOT NULL,
    auto_commit BOOLEAN NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rollback_contracts_execution_id ON rollback_contracts(execution_id);

CREATE TABLE IF NOT EXISTS approvals (
    approval_id TEXT PRIMARY KEY,
    intent_id TEXT NOT NULL,
    proposal_id TEXT NOT NULL,
    execution_id TEXT,
    action_digest TEXT NOT NULL,
    state TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_approvals_proposal_id ON approvals(proposal_id);
CREATE INDEX IF NOT EXISTS idx_approvals_state_created_at ON approvals(state, created_at DESC, approval_id DESC);

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

CREATE INDEX IF NOT EXISTS idx_provenance_events_occurred_at ON provenance_events(occurred_at ASC);

CREATE TABLE IF NOT EXISTS provenance_edges (
    to_event_id TEXT NOT NULL,
    from_event_id TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    summary TEXT,
    CONSTRAINT fk_provenance_edges_to_event
        FOREIGN KEY (to_event_id) REFERENCES provenance_events(event_id),
    CONSTRAINT fk_provenance_edges_from_event
        FOREIGN KEY (from_event_id) REFERENCES provenance_events(event_id)
);

CREATE INDEX IF NOT EXISTS idx_provenance_edges_to_event_id ON provenance_edges(to_event_id);
CREATE INDEX IF NOT EXISTS idx_provenance_edges_from_event_id ON provenance_edges(from_event_id);

CREATE TABLE IF NOT EXISTS ledger_entries (
    entry_id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    intent_id TEXT,
    execution_id TEXT,
    occurred_at TEXT NOT NULL,
    content_hash TEXT,
    previous_ledger_hash TEXT,
    raw_json TEXT NOT NULL,
    CONSTRAINT fk_ledger_entries_event
        FOREIGN KEY (event_id) REFERENCES provenance_events(event_id)
);

CREATE INDEX IF NOT EXISTS idx_ledger_entries_occurred_at ON ledger_entries(occurred_at);
CREATE INDEX IF NOT EXISTS idx_ledger_entries_intent_id ON ledger_entries(intent_id);
CREATE INDEX IF NOT EXISTS idx_ledger_entries_execution_id ON ledger_entries(execution_id);

CREATE TABLE IF NOT EXISTS policy_bundles (
    bundle_id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT false,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_policy_bundles_content_hash ON policy_bundles(content_hash);
CREATE INDEX IF NOT EXISTS idx_policy_bundles_active ON policy_bundles(active);

CREATE TABLE IF NOT EXISTS scoped_tokens (
    token_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    role TEXT NOT NULL,
    scopes TEXT NOT NULL,
    description TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT,
    revoked_reason TEXT,
    rotated_from TEXT,
    token_lookup_hash TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    token_salt TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_scoped_tokens_actor_id ON scoped_tokens(actor_id);
CREATE INDEX IF NOT EXISTS idx_scoped_tokens_role ON scoped_tokens(role);
CREATE INDEX IF NOT EXISTS idx_scoped_tokens_revoked_at ON scoped_tokens(revoked_at);
CREATE INDEX IF NOT EXISTS idx_scoped_tokens_token_lookup_hash ON scoped_tokens(token_lookup_hash);

CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    result TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
CREATE INDEX IF NOT EXISTS idx_audit_log_resource_type ON audit_log(resource_type);
CREATE INDEX IF NOT EXISTS idx_audit_log_resource_id ON audit_log(resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_created_at ON audit_log(created_at DESC);

CREATE TABLE IF NOT EXISTS _schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
