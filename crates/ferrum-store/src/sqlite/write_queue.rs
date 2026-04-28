//! Write queue for serializing SQLite writes through a single writer task.
//!
//! This eliminates SQLite's single-writer contention by funneling all write
//! operations through one mpsc channel processed by a dedicated task.

use crate::error::StoreError;
use crate::repos::LedgerEntry;
use crate::sqlite::{
    SqliteApprovalRepo, SqliteCapabilityRepo, SqliteExecutionRepo, SqliteIntentRepo,
    SqliteLedgerRepo, SqliteProposalRepo, SqliteProvenanceRepo, SqliteRollbackRepo,
};
use crate::{
    ApprovalRepo, CapabilityRepo, ExecutionRepo, IntentRepo, LedgerRepo, ProposalRepo,
    ProvenanceRepo, Result, RollbackRepo,
};
use ferrum_proto::{
    ActionProposal, ApprovalId, ApprovalRequest, ApprovalState, CapabilityId, CapabilityLease,
    CapabilityStatus, EventId, ExecutionId, ExecutionRecord, ExecutionState, IntentEnvelope,
    IntentId, IntentStatus, ProvenanceEdge, ProvenanceEvent, RollbackContract, RollbackContractId,
    RollbackState,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sqlx::SqlitePool;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::JoinHandle;

/// Payload types for write operations.
#[derive(Debug)]
pub enum WriteOp {
    // Intent operations
    InsertIntent {
        data: IntentEnvelope,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateIntent {
        data: IntentEnvelope,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateIntentStatus {
        intent_id: IntentId,
        status: IntentStatus,
        reply: oneshot::Sender<Result<()>>,
    },

    // Proposal operations
    InsertProposal {
        data: ActionProposal,
        reply: oneshot::Sender<Result<()>>,
    },

    // Capability operations
    InsertCapability {
        data: CapabilityLease,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateCapability {
        data: CapabilityLease,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateCapabilityStatus {
        capability_id: CapabilityId,
        status: CapabilityStatus,
        reply: oneshot::Sender<Result<()>>,
    },

    // Execution operations
    InsertExecution {
        data: ExecutionRecord,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateExecution {
        data: ExecutionRecord,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateExecutionState {
        execution_id: ExecutionId,
        state: ExecutionState,
        reply: oneshot::Sender<Result<()>>,
    },

    // Rollback operations
    InsertRollback {
        data: RollbackContract,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateRollback {
        data: RollbackContract,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateRollbackState {
        contract_id: RollbackContractId,
        state: RollbackState,
        reply: oneshot::Sender<Result<()>>,
    },

    // Approval operations
    InsertApproval {
        data: ApprovalRequest,
        reply: oneshot::Sender<Result<()>>,
    },
    UpdateApproval {
        data: ApprovalRequest,
        reply: oneshot::Sender<Result<()>>,
    },
    ResolveApproval {
        approval_id: ApprovalId,
        state: ApprovalState,
        reply: oneshot::Sender<Result<()>>,
    },

    // Provenance operations
    AppendProvenanceEvent {
        data: ProvenanceEvent,
        reply: oneshot::Sender<Result<()>>,
    },
    AppendProvenanceEdges {
        to_event_id: EventId,
        edges: Vec<ProvenanceEdge>,
        reply: oneshot::Sender<Result<()>>,
    },

    // Ledger operations
    AppendLedger {
        entry: LedgerEntry,
        reply: oneshot::Sender<Result<()>>,
    },

    /// Fire-and-forget operation: executes but does not send a reply.
    /// Used for background operations like revoke.
    FireAndForget(Box<WriteOp>),
}

/// Write queue sender for dispatching write operations.
#[derive(Clone)]
pub struct WriteQueue {
    sender: mpsc::Sender<WriteOp>,
}

impl WriteQueue {
    /// Send a write operation and wait for the result.
    pub async fn send(&self, op: WriteOp) -> crate::Result<()> {
        let (reply, recv) = oneshot::channel();
        let op_with_reply = Self::attach_reply(op, reply);

        self.sender
            .send(op_with_reply)
            .await
            .map_err(|_| StoreError::Other("write queue closed".to_string()))?;

        recv.await
            .map_err(|_| StoreError::Other("write operation cancelled".to_string()))?
    }

    /// Send a fire-and-forget operation without waiting for a reply.
    pub async fn fire_and_forget(&self, op: WriteOp) -> crate::Result<()> {
        self.try_fire_and_forget(op)
    }

    /// Non-blocking fire-and-forget: returns immediately.
    /// If the queue is full, the operation is dropped silently (best-effort).
    pub fn try_fire_and_forget(&self, op: WriteOp) -> crate::Result<()> {
        let fire_and_forget = WriteOp::FireAndForget(Box::new(op));
        match self.sender.try_send(fire_and_forget) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("write queue full, dropping fire-and-forget operation");
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(StoreError::Other("write queue closed".to_string()))
            }
        }
    }

    fn attach_reply(op: WriteOp, reply: oneshot::Sender<crate::Result<()>>) -> WriteOp {
        match op {
            WriteOp::InsertIntent { data, .. } => WriteOp::InsertIntent { data, reply },
            WriteOp::UpdateIntent { data, .. } => WriteOp::UpdateIntent { data, reply },
            WriteOp::UpdateIntentStatus {
                intent_id, status, ..
            } => WriteOp::UpdateIntentStatus {
                intent_id,
                status,
                reply,
            },
            WriteOp::InsertProposal { data, .. } => WriteOp::InsertProposal { data, reply },
            WriteOp::InsertCapability { data, .. } => WriteOp::InsertCapability { data, reply },
            WriteOp::UpdateCapability { data, .. } => WriteOp::UpdateCapability { data, reply },
            WriteOp::UpdateCapabilityStatus {
                capability_id,
                status,
                ..
            } => WriteOp::UpdateCapabilityStatus {
                capability_id,
                status,
                reply,
            },
            WriteOp::InsertExecution { data, .. } => WriteOp::InsertExecution { data, reply },
            WriteOp::UpdateExecution { data, .. } => WriteOp::UpdateExecution { data, reply },
            WriteOp::UpdateExecutionState {
                execution_id,
                state,
                ..
            } => WriteOp::UpdateExecutionState {
                execution_id,
                state,
                reply,
            },
            WriteOp::InsertRollback { data, .. } => WriteOp::InsertRollback { data, reply },
            WriteOp::UpdateRollback { data, .. } => WriteOp::UpdateRollback { data, reply },
            WriteOp::UpdateRollbackState {
                contract_id, state, ..
            } => WriteOp::UpdateRollbackState {
                contract_id,
                state,
                reply,
            },
            WriteOp::InsertApproval { data, .. } => WriteOp::InsertApproval { data, reply },
            WriteOp::UpdateApproval { data, .. } => WriteOp::UpdateApproval { data, reply },
            WriteOp::ResolveApproval {
                approval_id, state, ..
            } => WriteOp::ResolveApproval {
                approval_id,
                state,
                reply,
            },
            WriteOp::AppendProvenanceEvent { data, .. } => {
                WriteOp::AppendProvenanceEvent { data, reply }
            }
            WriteOp::AppendProvenanceEdges {
                to_event_id, edges, ..
            } => WriteOp::AppendProvenanceEdges {
                to_event_id,
                edges,
                reply,
            },
            WriteOp::AppendLedger { entry, .. } => WriteOp::AppendLedger { entry, reply },
            WriteOp::FireAndForget(inner) => WriteOp::FireAndForget(inner),
        }
    }
}

/// Execute a single WriteOp and send result through the reply channel.
async fn execute_write_op(pool: &SqlitePool, op: WriteOp) -> Result<()> {
    match op {
        WriteOp::InsertIntent { data, reply } => {
            let repo = SqliteIntentRepo::new(pool.clone());
            let result = repo.insert(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateIntent { data, reply } => {
            let repo = SqliteIntentRepo::new(pool.clone());
            let result = repo.update(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateIntentStatus {
            intent_id,
            status,
            reply,
        } => {
            let repo = SqliteIntentRepo::new(pool.clone());
            let result = repo.update_status(intent_id, status).await;
            let _ = reply.send(result);
        }
        WriteOp::InsertProposal { data, reply } => {
            let repo = SqliteProposalRepo::new(pool.clone());
            let result = repo.insert(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::InsertCapability { data, reply } => {
            let repo = SqliteCapabilityRepo::new(pool.clone());
            let result = repo.insert(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateCapability { data, reply } => {
            let repo = SqliteCapabilityRepo::new(pool.clone());
            let result = repo.update(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateCapabilityStatus {
            capability_id,
            status,
            reply,
        } => {
            let repo = SqliteCapabilityRepo::new(pool.clone());
            let result = repo.update_status(capability_id, status).await;
            let _ = reply.send(result);
        }
        WriteOp::InsertExecution { data, reply } => {
            let repo = SqliteExecutionRepo::new(pool.clone());
            let result = repo.insert(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateExecution { data, reply } => {
            let repo = SqliteExecutionRepo::new(pool.clone());
            let result = repo.update(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateExecutionState {
            execution_id,
            state,
            reply,
        } => {
            let repo = SqliteExecutionRepo::new(pool.clone());
            let result = repo.update_state(execution_id, state).await;
            let _ = reply.send(result);
        }
        WriteOp::InsertRollback { data, reply } => {
            let repo = SqliteRollbackRepo::new(pool.clone());
            let result = repo.insert(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateRollback { data, reply } => {
            let repo = SqliteRollbackRepo::new(pool.clone());
            let result = repo.update(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateRollbackState {
            contract_id,
            state,
            reply,
        } => {
            let repo = SqliteRollbackRepo::new(pool.clone());
            let result = repo.update_state(contract_id, state).await;
            let _ = reply.send(result);
        }
        WriteOp::InsertApproval { data, reply } => {
            let repo = SqliteApprovalRepo::new(pool.clone());
            let result = repo.insert(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::UpdateApproval { data, reply } => {
            let repo = SqliteApprovalRepo::new(pool.clone());
            let result = repo.update(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::ResolveApproval {
            approval_id,
            state,
            reply,
        } => {
            let repo = SqliteApprovalRepo::new(pool.clone());
            let result = repo.resolve(approval_id, state).await;
            let _ = reply.send(result);
        }
        WriteOp::AppendProvenanceEvent { data, reply } => {
            let repo = SqliteProvenanceRepo::new(pool.clone());
            let result = repo.append_event(&data).await;
            let _ = reply.send(result);
        }
        WriteOp::AppendProvenanceEdges {
            to_event_id,
            edges,
            reply,
        } => {
            let repo = SqliteProvenanceRepo::new(pool.clone());
            let result = repo.append_edges(to_event_id, &edges).await;
            let _ = reply.send(result);
        }
        WriteOp::AppendLedger { entry, reply } => {
            let repo = SqliteLedgerRepo::new(pool.clone());
            let result = repo.append(&entry).await;
            let _ = reply.send(result);
        }
        WriteOp::FireAndForget(inner) => {
            // Unwrap nested FireAndForget ops and execute the innermost operation
            let mut op = *inner;
            loop {
                match op {
                    WriteOp::FireAndForget(inner_box) => {
                        op = *inner_box;
                    }
                    _ => {
                        // Use Box::pin to satisfy recursion requirement
                        let future = Box::pin(execute_write_op(pool, op));
                        future.await?;
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Channel capacity for the write queue.
pub const WRITE_QUEUE_CAPACITY: usize = 256;

/// Shared state for the write queue writer task.
pub(crate) struct WriterState {
    /// Flag to signal the writer to stop accepting new operations.
    shutdown_requested: AtomicBool,
    /// Notification that the writer has completed its shutdown.
    shutdown_complete: Notify,
}

impl WriterState {
    fn new() -> Self {
        Self {
            shutdown_requested: AtomicBool::new(false),
            shutdown_complete: Notify::new(),
        }
    }

    pub(crate) fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
    }

    fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::SeqCst)
    }

    fn notify_shutdown_complete(&self) {
        self.shutdown_complete.notify_one();
    }
}

/// Spawn the write queue writer task.
///
/// Returns the WriteQueue sender, a JoinHandle for the writer task,
/// and a shared state handle for signaling shutdown.
pub(crate) fn spawn_writer_task(
    pool: SqlitePool,
) -> (WriteQueue, JoinHandle<()>, Arc<WriterState>) {
    let (tx, rx) = mpsc::channel::<WriteOp>(WRITE_QUEUE_CAPACITY);
    let queue = WriteQueue { sender: tx };
    let state = Arc::new(WriterState::new());
    let state_clone = state.clone();

    let handle = tokio::spawn(async move {
        writer_loop(pool, rx, state_clone).await;
    });

    (queue, handle, state)
}

/// Writer task main loop.
async fn writer_loop(pool: SqlitePool, mut rx: mpsc::Receiver<WriteOp>, state: Arc<WriterState>) {
    while let Some(op) = rx.recv().await {
        // Check for shutdown signal - reject new ops but drain the queue
        if state.is_shutdown_requested() {
            // Drain remaining operations before shutdown
            tracing::debug!("write queue draining remaining operations during shutdown");
            while let Some(drain_op) = rx.recv().await {
                if let Err(e) = execute_write_op(&pool, drain_op).await {
                    tracing::warn!(error = %e, "write queue drain operation failed");
                }
            }
            break;
        }
        // Execute the write operation
        if let Err(e) = execute_write_op(&pool, op).await {
            tracing::warn!(error = %e, "write queue operation failed");
        }
    }
    state.notify_shutdown_complete();
    tracing::info!("write queue writer task shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteStore;

    #[tokio::test]
    async fn test_write_queue_single_insert() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let (queue, _handle, _state) = spawn_writer_task(store.pool().clone());

        let intent = IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: ferrum_proto::TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };

        let (reply, _) = oneshot::channel();
        let op = WriteOp::InsertIntent {
            data: intent.clone(),
            reply,
        };

        queue.send(op).await.unwrap();

        // Give time for the write to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify the intent was inserted
        let fetched = store.intents().get(intent.intent_id).await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_write_queue_fire_and_forget() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let (queue, _handle, _state) = spawn_writer_task(store.pool().clone());

        let intent = IntentEnvelope {
            intent_id: ferrum_proto::IntentId::new(),
            principal_id: ferrum_proto::PrincipalId::new(),
            session_id: None,
            channel_id: None,
            title: "test".to_string(),
            goal: "test goal".to_string(),
            normalized_goal: "test goal".to_string(),
            allowed_outcomes: vec![],
            forbidden_outcomes: vec![],
            resource_scope: vec![],
            risk_tier: ferrum_proto::RiskTier::Low,
            approval_mode: ferrum_proto::ApprovalMode::None,
            default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
            time_budget: ferrum_proto::TimeBudget {
                max_duration_ms: 30000,
                max_steps: 8,
                max_retries_per_step: 1,
            },
            trust_context: ferrum_proto::TrustContextSummary {
                input_labels: vec![],
                sensitivity_labels: vec![],
                taint_score: 0,
                contains_external_metadata: false,
                contains_tool_output: false,
                contains_untrusted_text: false,
            },
            derived_from_event_ids: vec![],
            tags: vec![],
            metadata: ferrum_proto::JsonMap::new(),
            status: ferrum_proto::IntentStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
        };

        let op = WriteOp::InsertIntent {
            data: intent.clone(),
            reply: oneshot::channel().0,
        };

        // Fire and forget - should not block
        queue.fire_and_forget(op).await.unwrap();

        // Give time for the write to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify the intent was inserted
        let fetched = store.intents().get(intent.intent_id).await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_write_queue_ordering() {
        let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
        store.apply_embedded_migrations().await.unwrap();

        let (queue, _handle, _state) = spawn_writer_task(store.pool().clone());

        // Insert 5 intents
        for i in 0..5 {
            let intent = IntentEnvelope {
                intent_id: ferrum_proto::IntentId::new(),
                principal_id: ferrum_proto::PrincipalId::new(),
                session_id: None,
                channel_id: None,
                title: format!("test{}", i),
                goal: format!("test goal {}", i),
                normalized_goal: format!("test goal {}", i),
                allowed_outcomes: vec![],
                forbidden_outcomes: vec![],
                resource_scope: vec![],
                risk_tier: ferrum_proto::RiskTier::Low,
                approval_mode: ferrum_proto::ApprovalMode::None,
                default_rollback_class: ferrum_proto::RollbackClass::R0NativeReversible,
                time_budget: ferrum_proto::TimeBudget {
                    max_duration_ms: 30000,
                    max_steps: 8,
                    max_retries_per_step: 1,
                },
                trust_context: ferrum_proto::TrustContextSummary {
                    input_labels: vec![],
                    sensitivity_labels: vec![],
                    taint_score: 0,
                    contains_external_metadata: false,
                    contains_tool_output: false,
                    contains_untrusted_text: false,
                },
                derived_from_event_ids: vec![],
                tags: vec![],
                metadata: ferrum_proto::JsonMap::new(),
                status: ferrum_proto::IntentStatus::Active,
                created_at: chrono::Utc::now(),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(15),
            };

            let (reply, _) = oneshot::channel();
            let op = WriteOp::InsertIntent {
                data: intent.clone(),
                reply,
            };

            queue.send(op).await.unwrap();
        }

        // Give time for all writes to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // All 5 should be present
        let all_intents = store
            .intents()
            .list_by_status(ferrum_proto::IntentStatus::Active)
            .await
            .unwrap();
        assert_eq!(all_intents.len(), 5);
    }
}
