# Slice 37 — P2.6 EmailSend Adapter Contract Draft

**Date:** 2026-04-04
**Type:** Preflight contract draft (post-v1 planning)
**Status:** DRAFT — not for implementation

---

## Overview

This document is a **draft contract sketch** for a future `EmailSend` adapter that would handle real email send operations (as opposed to the current maildraft adapter which only handles draft creation/deletion).

**Important:** This is **not** an implementation plan. This is a preflight contract draft to guide future analysis. EmailSend implementation is explicitly **post-v1** and requires separate Slice proposal approval.

The maildraft adapter handles **draft-only** (`allow_send=false`) operations and will **never** support send semantics. A dedicated EmailSend adapter is required for send-capable bindings.

---

## Design Principles

1. **Separate from maildraft**: EmailSend is a distinct capability from EmailDraft. The maildraft adapter contract is for draft artifacts; a future EmailSend adapter would be for live send operations.

2. **Explicit R3 classification**: All send operations are R3 by nature (irreversible once sent). The adapter must enforce this classification.

3. **Provider-agnostic core**: The adapter interface should not assume specific email provider semantics. Provider-specific behavior belongs in a provider layer.

4. **Fail-closed on send errors**: Send failures should result in clear error states, not silent degradation.

5. **No "unsend" guarantee claim**: Email providers generally do not support true unsend. The adapter contract must not claim what cannot be delivered.

---

## Draft Adapter Contract: EmailSend

### Adapter Identity

| Field | Value |
|-------|-------|
| `ADAPTER_KIND` | `"ferrum-adapter-emailsend"` |
| `ADAPTER_KEY` | `"emailsend"` |

### Action Types

| Action | Rollback Class | Notes |
|--------|----------------|-------|
| `EmailSend` | R3Irreversible | Send email; no automatic undo |

### Standard Rollback Adapter Interface

```rust
pub struct EmailSendAdapter {
    key: &'static str,
    provider: Arc<dyn EmailProvider>,
    store: SqliteEmailSendLogStore,
}

#[async_trait]
impl RollbackAdapter for EmailSendAdapter {
    fn key(&self) -> &'static str {
        self.key
    }

    async fn prepare(
        &self,
        request: &RollbackPrepareRequest,
    ) -> Result<PrepareReceipt, AdapterError> {
        // Validate: this is an EmailSend action (not EmailDraft)
        // Capture provider config from request for execute-time use
        // R3 enforcement: auto_commit must be false
        if request.auto_commit {
            return Err(AdapterError::Validation(
                "EmailSend does not support auto-commit (R3)".to_string(),
            ));
        }

        Ok(PrepareReceipt {
            accepted: true,
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn execute(
        &self,
        contract: &RollbackContract,
        payload: &serde_json::Value,
    ) -> Result<ExecuteReceipt, AdapterError> {
        // Extract send fields from payload
        let to: Vec<String> = payload
            .get("to")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let subject = payload.get("subject").and_then(|v| v.as_str()).unwrap_or("");
        let body = payload.get("body").and_then(|v| v.as_str()).unwrap_or("");

        // Execute send via provider
        let provider_result = self.provider.send(to.clone(), subject, body).await?;

        // Log send operation for audit/recovery analysis
        let log_entry = SendLogEntry {
            execution_id: contract.execution_id.clone(),
            message_id: provider_result.message_id.clone(),
            provider_ref: provider_result.provider_ref,
            sent_at: chrono::Utc::now(),
        };
        self.store.log_send(&log_entry)?;

        Ok(ExecuteReceipt {
            external_id: Some(provider_result.message_id),
            result_digest: Some(format!("sent:{}", to.len())),
            adapter_metadata: JsonMap::new(),
        })
    }

    async fn verify(
        &self,
        contract: &RollbackContract,
    ) -> Result<VerifyReceipt, AdapterError> {
        // EmailSend verification: check that send was logged
        // NOTE: This does NOT guarantee delivery (email is fire-and-forget by nature)
        let logged = self.store.get_log_by_execution(&contract.execution_id)?;
        match logged {
            Some(entry) => Ok(VerifyReceipt {
                verified: true, // Send was executed and logged
                adapter_metadata: serde_json::json!({
                    "message_id": entry.message_id,
                    "sent_at": entry.sent_at,
                }),
            }),
            None => Ok(VerifyReceipt {
                verified: false,
                adapter_metadata: JsonMap::new(),
            }),
        }
    }

    async fn compensate(
        &self,
        contract: &RollbackContract,
    ) -> Result<RecoveryReceipt, AdapterError> {
        // EmailSend compensation: conservative no-op
        // True "unsend" is generally not available from email providers
        // Compensate logs the compensation attempt for audit purposes
        Ok(RecoveryReceipt {
            recovered: false, // Cannot undo send
            metadata: JsonMap::json({
                "compensate": "no-op",
                "reason": "EmailSend is R3: no automatic undo available",
            }),
        })
    }

    async fn rollback(&self, contract: &RollbackContract) -> Result<RecoveryReceipt, AdapterError> {
        // Same as compensate for EmailSend
        self.compensate(contract).await
    }
}
```

---

## Draft EmailProvider Trait

```rust
/// Email provider abstraction (not implemented in this contract draft)
#[async_trait]
pub trait EmailProvider: Send + Sync {
    /// Send an email and return the provider's message reference
    async fn send(
        &self,
        to: Vec<String>,
        subject: &str,
        body: &str,
    ) -> Result<ProviderSendResult, ProviderError>;

    /// Check if a message can be revoked (rarely supported)
    async fn can_revoke(&self, message_id: &str) -> bool;

    /// Attempt to revoke a sent message (if supported by provider)
    async fn revoke(&self, message_id: &str) -> Result<(), ProviderError>;
}

/// Result of a successful send operation
pub struct ProviderSendResult {
    pub message_id: String,      // Global unique message ID
    pub provider_ref: String,    // Provider's internal reference
}

/// Provider-specific errors
#[derive(Debug)]
pub enum ProviderError {
    Transient(String),    // Retryable
    Permanent(String),    // Non-retryable (bad address, etc.)
    Auth(String),        // Authentication failure
    Network(String),     // Network connectivity issue
}
```

---

## SQLite Send Log Schema (Draft)

```sql
CREATE TABLE emailsend_log (
    log_id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    provider_ref TEXT,
    recipients TEXT NOT NULL,  -- JSON array
    subject TEXT NOT NULL,
    sent_at TEXT NOT NULL,
    compensate_attempted INTEGER DEFAULT 0,
    compensate_result TEXT,
    FOREIGN KEY (execution_id) REFERENCES executions(execution_id)
);

CREATE INDEX idx_emailsend_execution_id ON emailsend_log(execution_id);
CREATE INDEX idx_emailsend_message_id ON emailsend_log(message_id);
```

---

## Payload Shape (Draft)

### Execute Payload

```json
{
  "to": ["recipient@example.com"],
  "cc": [],
  "bcc": [],
  "subject": "Email subject",
  "body": "Email body (plaintext or HTML)",
  "reply_to": "sender@example.com",
  "attachments": []
}
```

### Verify Receipt Metadata

```json
{
  "message_id": "provider-generated-message-id",
  "sent_at": "2026-04-04T12:00:00Z"
}
```

### Compensate/Rollback Receipt Metadata

```json
{
  "compensate": "no-op",
  "reason": "EmailSend is R3: no automatic undo available"
}
```

---

## Comparison: maildraft vs. EmailSend

| Aspect | maildraft (v1) | EmailSend (future) |
|--------|---------------|-------------------|
| Scope | Draft create/delete | Live send |
| `allow_send` | `false` only | `true` (new binding type) |
| Rollback Class | R2Compensatable | R3Irreversible |
| Adapter Key | `"maildraft"` | `"emailsend"` |
| Compensation | Delete draft | No-op (R3) |
| Provider Integration | None | Required |
| Auto-commit | Supported | Not supported |

---

## Open Questions (Not Resolved in This Draft)

1. **Provider selection**: How does the adapter select which email provider to use? Per-intent configuration? Global default?
2. **Credential management**: How are SMTP/API credentials managed? Secret store integration?
3. **Send verification**: What does "verified" mean for email send? Delivery receipt? Read receipt? None?
4. **Rate limiting**: How is send rate limited to prevent abuse?
5. **Audit trail**: What metadata must be persisted for compliance?

---

## Relationship to Other Documents

| Document | Role |
|----------|------|
| `docs/implementation-path/36-p2-6-emailsend-governed-path-entry-analysis.md` | Governed-path entry requirements |
| `docs/implementation-path/16a-slice-16-a-boundary-ratification.md` | Current deny boundary ratified |
| `docs/13-adapter-contracts.md` | Existing adapter contracts (maildraft, fs, sqlite, git, http) |

---

## Status

This is a **preflight draft**. It is **not** approved for implementation. The actual EmailSend adapter implementation requires:

1. Phase 2 send-semantics safety analysis completion
2. R3 binding extension proposal approval
3. Evidence pack review
4. Explicit Slice approval to advance from preflight to implementation

**Post-v1 only.**
