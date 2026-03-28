//! Concrete SQLite-backed implementation of `SyncPreflightRepo`.
//!
//! This module wires `ferrum-sync`'s `SyncPreflightRepo` trait to the real
//! `SqliteStore` backend. It provides honest, fail-closed semantics:
//!
//! ## What Is Real (Backed by Actual Schema)
//!
//! - **`verify_local_chain()`** (PF1 + PF5): Calls `SqliteStore::verify_ledger_chain()`
//!   for chain integrity (PF1) and queries `LedgerRepo::get_latest()` for ledger
//!   readability (PF5). An empty ledger is valid (readable, no chain to verify).
//!
//! ## What Is Unsupported (Returns Explicit Err)
//!
//! - **`read_local_state()`** (PF2/PF6/PF7 + tip): The current schema has no tables
//!   for tracking in-flight commits (PF2), uncommitted local entries (PF6), or sync
//!   sessions (PF7). Returns `Err(SyncRepoError::InternalError)`.
//!
//!   The follower tip *could* be read from the `ledger_entries` table, but the PF2/PF6/PF7
//!   booleans would need to be fabricated. We refuse to return made-up domain values.
//!
//!   The full `LocalPreflightState` is unsupported until schema tables exist.
//!
//! - **`is_leader_authorized()`** (PF4): No capability or authorization model exists
//!   for sync leader identity. Returns `Err(SyncRepoError::InternalError)`.
//!
//! - **`read_leader_tip()`** (PF8): No leader tip cache table exists. Returns
//!   `Err(SyncRepoError::InternalError)`.
//!
//! ## Implementation Note: Sync Trait + Async Store
//!
//! The `SyncPreflightRepo` trait is synchronous, but `SqliteStore` operations are
//! async. This implementation bridges the gap using `tokio::task::block_in_place` combined
//! with `Handle::current().block_on()`. This means:
//!
//! - Methods **must** be called from within a multi-threaded Tokio runtime context.
//! - Methods **must not** be called from within a `#[tokio::test]` (single-threaded)
//!   or a `#[tokio::main]` context. Use the internal `_verify_local_chain_async`
//!   helper directly in tests instead.
//!
//!   For production callers use the trait methods from a multi-threaded runtime.

use ferrum_sync::decision::TipId;
use ferrum_sync::repo::{LocalPreflightState, SyncPreflightRepo, SyncRepoError};

use crate::repos::LedgerRepo;
use crate::sqlite::SqliteStore;

/// Concrete SQLite-backed `SyncPreflightRepo`.
///
/// See module-level documentation for what is real vs unsupported.
#[derive(Clone)]
pub struct SqliteSyncPreflightRepo {
    store: SqliteStore,
}

impl SqliteSyncPreflightRepo {
    /// Create a new `SqliteSyncPreflightRepo` backed by the given `SqliteStore`.
    ///
    /// The store must have had `apply_embedded_migrations()` called on it
    /// before use.
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }

    /// Internal async implementation of `verify_local_chain`.
    ///
    /// Exposed for direct testing without needing a multi-threaded runtime.
    /// Production callers should use the sync trait method.
    pub async fn verify_local_chain_async(&self) -> Result<(), SyncRepoError> {
        // PF1: chain integrity
        if let Err(e) = self.store.verify_ledger_chain().await {
            return Err(SyncRepoError::ChainIntegrityFailed {
                reason: format!("verify_ledger_chain failed: {e}"),
            });
        }

        // PF5: ledger readability (get_latest must succeed, even if empty)
        if let Err(e) = self.store.ledger().get_latest().await {
            return Err(SyncRepoError::LedgerNotReadable {
                reason: format!("get_latest failed: {e}"),
            });
        }

        Ok(())
    }
}

impl SyncPreflightRepo for SqliteSyncPreflightRepo {
    fn verify_local_chain(&self) -> Result<(), SyncRepoError> {
        // Bridge sync trait -> async store using block_in_place.
        // Requires a multi-threaded Tokio runtime (not single-threaded test).
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.verify_local_chain_async())
        })
    }

    fn read_local_state(&self) -> Result<LocalPreflightState, SyncRepoError> {
        // UNSUPPORTED: The current schema has no tables for:
        // - PF2 (in-flight commits): no `inflight_commits` or equivalent table
        // - PF6 (uncommitted local entries): no `uncommitted_entries` marker
        // - PF7 (sync session tracking): no `sync_sessions` table
        //
        // Returning a fabricated `LocalPreflightState` with `false` for all
        // boolean flags would be dishonest and violate fail-closed semantics.
        // We refuse to return made-up domain values for checks that have no
        // authoritative backend surface.
        //
        // The follower tip *could* be read from `ledger_entries`, but since
        // PF2/PF6/PF7 cannot be populated honestly, the entire state snapshot
        // is unsupported.
        Err(SyncRepoError::InternalError {
            reason: "read_local_state unsupported: current schema has no tables for \
                     PF2 (in-flight commits), PF6 (uncommitted entries), or \
                     PF7 (sync sessions). Cannot populate LocalPreflightState \
                     honestly."
                .to_string(),
        })
    }

    fn is_leader_authorized(&self, _leader_identity: &str) -> Result<bool, SyncRepoError> {
        // UNSUPPORTED: No capability model or authorization table exists for
        // sync leader identity. The `capabilities` table in the current schema
        // tracks tool-use capabilities, not sync leader authorization.
        Err(SyncRepoError::InternalError {
            reason: "is_leader_authorized unsupported: no capability model or \
                     authorization table for sync leader identity in current schema."
                .to_string(),
        })
    }

    fn read_leader_tip(&self, _leader_identity: &str) -> Result<Option<TipId>, SyncRepoError> {
        // UNSUPPORTED: No leader tip cache table exists in the current schema.
        // The `ledger_entries` table stores local entries only.
        Err(SyncRepoError::InternalError {
            reason: "read_leader_tip unsupported: no leader tip cache table in \
                     current schema."
                .to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an in-memory SQLite store with migrations applied.
    async fn make_store() -> SqliteStore {
        let store = SqliteStore::connect("sqlite::memory:")
            .await
            .expect("connect");
        store.apply_embedded_migrations().await.expect("migrations");
        store
    }

    // =========================================================================
    // verify_local_chain — real backend (tested via async helper)
    // =========================================================================

    #[tokio::test]
    async fn verify_local_chain_async_succeeds_on_empty_ledger() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.verify_local_chain_async().await;
        assert!(result.is_ok(), "empty ledger should verify ok");
    }

    #[tokio::test]
    async fn verify_local_chain_async_after_append() {
        use ferrum_proto::{ActorRef, ActorType, HashChainRef, ObjectRef, ObjectType};

        let store = make_store().await;

        // Append a single entry to create a non-empty ledger
        let event = ferrum_proto::ProvenanceEvent {
            event_id: ferrum_proto::EventId::new(),
            kind: ferrum_proto::ProvenanceEventKind::UserGoalReceived,
            occurred_at: chrono::Utc::now(),
            actor: ActorRef {
                actor_type: ActorType::System,
                actor_id: "test".to_string(),
                display_name: None,
            },
            object: ObjectRef {
                object_type: ObjectType::Intent,
                object_id: "test-intent".to_string(),
                summary: None,
            },
            intent_id: Some(ferrum_proto::IntentId::new()),
            proposal_id: None,
            execution_id: None,
            capability_id: None,
            rollback_contract_id: None,
            policy_bundle_id: None,
            trust_labels: vec![],
            sensitivity_labels: vec![],
            parent_edges: vec![],
            hash_chain: HashChainRef {
                content_hash: None,
                manifest_hash: None,
                policy_bundle_hash: None,
                previous_ledger_hash: None,
            },
            metadata: ferrum_proto::JsonMap::new(),
        };
        store.ledger().append_event(&event).await.unwrap();

        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.verify_local_chain_async().await;
        assert!(result.is_ok(), "single-entry ledger should verify ok");
    }

    // =========================================================================
    // Unsupported methods — fail-closed (sync methods, no runtime needed)
    // =========================================================================

    #[test]
    fn read_local_state_returns_unsupported_err() {
        // This is a sync method that just returns Err, no runtime needed.
        // We need a store, but can't use make_store (async) from sync test.
        // Use a simple runtime block.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(make_store());
        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.read_local_state();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{err}").contains("unsupported"));
        assert!(format!("{err}").contains("PF2"));
    }

    #[test]
    fn is_leader_authorized_returns_unsupported_err() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(make_store());
        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.is_leader_authorized("some-leader");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{err}").contains("unsupported"));
    }

    #[test]
    fn read_leader_tip_returns_unsupported_err() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(make_store());
        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.read_leader_tip("some-leader");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{err}").contains("unsupported"));
    }

    // =========================================================================
    // Dyn trait dispatch (sync, uses block_in_place so needs multi-threaded rt)
    // =========================================================================

    #[test]
    fn concrete_repo_works_through_dyn() {
        // Use a multi-threaded runtime so block_in_place works.
        // Keep the runtime alive for the duration of the test since
        // verify_local_chain() uses block_in_place internally.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo: Box<dyn SyncPreflightRepo> = Box::new(SqliteSyncPreflightRepo::new(store));
            assert!(repo.verify_local_chain().is_ok());
            assert!(repo.read_local_state().is_err());
        });
    }
}
