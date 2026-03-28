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
//! - **`read_local_state()`** (PF2/PF6/PF7 + follower tip): Reads the three boolean
//!   flags from the `sync_state` table (migration 003) and the follower tip from
//!   `ledger_entries`. Returns an honest `LocalPreflightState` with all four fields
//!   populated.
//!
//! - **`read_leader_tip()`** (PF8): Reads from the `leader_tips` cache table
//!   (migration 002). Returns `Ok(Some(TipId))` if a tip is cached for the given
//!   leader address, `Ok(None)` if no tip is known. Returns `Err` only on database
//!   errors (fail-closed: a DB error is treated as tip unavailable, not "tip unknown").
//!
//! - **`is_leader_authorized()`** (PF4): Queries the `leader_allowlist` table
//!   (migration 004). Returns `Ok(true)` if the leader is in the allowlist with
//!   `authorized=1`, `Ok(false)` if not in the allowlist (deny-by-default).
//!   Returns `Err` only on database errors (fail-closed: a DB error is treated as
//!   unauthorized, not as an error).
//!
//! ## Implementation Note: Sync Trait + Async Store
//!
//! The `SyncPreflightRepo` trait is synchronous, but `SqliteStore` operations are
//! async. This implementation bridges the gap using `tokio::task::block_in_place` combined
//! with `Handle::current().block_on()`. This means:
//!
//! - Methods **must** be called from within a multi-threaded Tokio runtime context.
//! - Methods **must not** be called from within a `#[tokio::test]` (single-threaded)
//!   or a `#[tokio::main]` context. Use the internal `*_async` helpers directly
//!   in tests instead.
//!
//!   For production callers use the trait methods from a multi-threaded runtime.

use ferrum_ledger::LedgerEntry;
use ferrum_sync::decision::TipId;
use ferrum_sync::repo::{LocalPreflightState, SyncPreflightRepo, SyncRepoError};
use sqlx::Row;

use crate::repos::LedgerRepo;
use crate::sqlite::SqliteStore;
use crate::sqlite::leader_allowlist::LeaderAllowlist;
use crate::sqlite::leader_tip_cache::{CacheWriteError, LeaderTipCache};

/// Convert a `LedgerEntry` to a `TipId`.
fn ledger_entry_to_tip(entry: &LedgerEntry) -> TipId {
    TipId {
        sequence: entry.sequence,
        hash: entry.entry_hash.clone(),
    }
}

/// Concrete SQLite-backed `SyncPreflightRepo`.
///
/// See module-level documentation for what is real vs unsupported.
#[derive(Clone)]
pub struct SqliteSyncPreflightRepo {
    store: SqliteStore,
    leader_tip_cache: LeaderTipCache,
    leader_allowlist: LeaderAllowlist,
}

impl SqliteSyncPreflightRepo {
    /// Create a new `SqliteSyncPreflightRepo` backed by the given `SqliteStore`.
    ///
    /// The store must have had `apply_embedded_migrations()` called on it
    /// before use.
    pub fn new(store: SqliteStore) -> Self {
        Self {
            leader_tip_cache: LeaderTipCache::new(store.pool().clone()),
            leader_allowlist: LeaderAllowlist::new(store.pool().clone()),
            store,
        }
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

    /// Internal async implementation of `verify_local_chain_for_hash_path_valid`.
    ///
    /// Exposed for direct testing without needing a multi-threaded runtime.
    /// Production callers should use the sync trait method.
    ///
    /// Fail-closed: returns `false` on any error.
    pub async fn verify_local_chain_for_hash_path_valid_async(&self) -> bool {
        self.verify_local_chain_async().await.is_ok()
    }

    /// Internal async implementation of `read_local_state`.
    ///
    /// Reads PF2/PF6/PF7 booleans from `sync_state` (migration 003) and the
    /// follower tip from `ledger_entries`. Exposed for direct testing without
    /// needing a multi-threaded runtime.
    pub async fn read_local_state_async(&self) -> Result<LocalPreflightState, SyncRepoError> {
        // Read the three boolean flags from sync_state.
        // The table always has exactly one row (id=1) after migration.
        let row = sqlx::query(
            "SELECT has_inflight_commits, has_uncommitted_entries, sync_in_progress FROM sync_state WHERE id = 1",
        )
        .fetch_optional(self.store.pool())
        .await
        .map_err(|e| SyncRepoError::InternalError {
            reason: format!("sync_state query failed: {}", e),
        })?;

        let (has_inflight_commits, has_uncommitted_entries, sync_in_progress) = match row {
            Some(r) => {
                let has_inflight: i32 = r.get("has_inflight_commits");
                let has_uncommitted: i32 = r.get("has_uncommitted_entries");
                let sync_active: i32 = r.get("sync_in_progress");
                (has_inflight != 0, has_uncommitted != 0, sync_active != 0)
            }
            // Empty sync_state: fail-closed — missing state is a db integrity error,
            // not a clean idle state that silently passes preflight.
            None => {
                return Err(SyncRepoError::InternalError {
                    reason: "sync_state row missing (id=1); cannot read PF2/PF6/PF7 flags"
                        .to_string(),
                });
            }
        };

        // Read the follower tip from ledger_entries (latest entry by entry_id).
        let follower_tip = self
            .store
            .ledger()
            .get_latest()
            .await
            .map_err(|e| SyncRepoError::InternalError {
                reason: format!("get_latest for follower tip failed: {}", e),
            })?
            .map(|entry| ledger_entry_to_tip(&entry));

        Ok(LocalPreflightState {
            follower_tip,
            has_inflight_commits,
            has_uncommitted_entries,
            sync_in_progress,
        })
    }

    /// Set the sync_state flags (PF2/PF6/PF7) for test scenarios.
    ///
    /// This is a narrow, test-only helper that directly manipulates the
    /// `sync_state` table. Do NOT use in production code; the write-path
    /// that would normally set these flags is not yet implemented.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the database write fails.
    #[cfg(test)]
    pub async fn set_sync_flags_test_only(
        &self,
        has_inflight_commits: bool,
        has_uncommitted_entries: bool,
        sync_in_progress: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE sync_state SET has_inflight_commits = ?, has_uncommitted_entries = ?, sync_in_progress = ? WHERE id = 1",
        )
        .bind(if has_inflight_commits { 1 } else { 0 })
        .bind(if has_uncommitted_entries { 1 } else { 0 })
        .bind(if sync_in_progress { 1 } else { 0 })
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    /// Write a leader tip to the cache (for test scenarios).
    ///
    /// This is a narrow, test-only helper that wraps `LeaderTipCache::write`.
    /// In tests, call this to seed the cache before running preflight checks.
    /// (The production transport probe path is a future slice and does not
    /// yet call this helper.)
    pub async fn write_leader_tip_test_only(
        &self,
        leader_address: &str,
        tip: &TipId,
    ) -> Result<(), CacheWriteError> {
        self.leader_tip_cache.write(leader_address, tip).await
    }

    /// Delete a cached leader tip (for test scenarios).
    ///
    /// This is a narrow, test-only helper that wraps `LeaderTipCache::delete`.
    pub async fn delete_leader_tip_test_only(
        &self,
        leader_address: &str,
    ) -> Result<(), sqlx::Error> {
        self.leader_tip_cache.delete(leader_address).await
    }

    /// Access the leader tip cache for direct cache operations.
    ///
    /// This is exposed so that `sync_service` can call `write()` after a
    /// successful probe without going through the full `SqliteSyncPreflightRepo`
    /// construction path.
    pub fn leader_tip_cache(&self) -> &LeaderTipCache {
        &self.leader_tip_cache
    }

    /// Internal async implementation of `is_leader_authorized` (PF4).
    ///
    /// Reads from the `leader_allowlist` table (migration 004).
    /// Returns `Ok(true)` if the leader is in the allowlist with `authorized=1`,
    /// `Ok(false)` if the leader is not in the allowlist (deny-by-default).
    /// Returns `Err` on database errors (fail-closed).
    ///
    /// Exposed for direct testing without needing a multi-threaded runtime.
    /// Production callers should use the sync trait method.
    pub async fn is_leader_authorized_async(
        &self,
        leader_address: &str,
    ) -> Result<bool, SyncRepoError> {
        self.leader_allowlist
            .is_authorized(leader_address)
            .await
            .map_err(|e| SyncRepoError::InternalError {
                reason: format!("leader_allowlist query failed: {}", e),
            })
    }

    /// Authorize a leader address in the allowlist (for test scenarios).
    ///
    /// This is a narrow, test-only helper that wraps `LeaderAllowlist::authorize`.
    /// In tests, call this to seed the allowlist before running preflight checks.
    /// (The production transport adapter path is a future slice.)
    pub async fn authorize_leader_test_only(
        &self,
        leader_address: &str,
    ) -> Result<(), sqlx::Error> {
        self.leader_allowlist.authorize(leader_address).await
    }

    /// Remove a leader from the allowlist (for test scenarios).
    ///
    /// This is a narrow, test-only helper that wraps `LeaderAllowlist::deauthorize`.
    pub async fn deauthorize_leader_test_only(
        &self,
        leader_address: &str,
    ) -> Result<(), sqlx::Error> {
        self.leader_allowlist.deauthorize(leader_address).await
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
        // Bridge sync trait -> async store using block_in_place.
        // Requires a multi-threaded Tokio runtime (not single-threaded test).
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.read_local_state_async())
        })
        .map_err(|e| SyncRepoError::InternalError {
            reason: format!("read_local_state failed: {}", e),
        })
    }

    fn is_leader_authorized(&self, leader_address: &str) -> Result<bool, SyncRepoError> {
        // Bridge sync trait -> async store using block_in_place.
        // Requires a multi-threaded Tokio runtime (not single-threaded test).
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.is_leader_authorized_async(leader_address))
        })
    }

    fn read_leader_tip(&self, leader_address: &str) -> Result<Option<TipId>, SyncRepoError> {
        // Bridge sync trait -> async store using block_in_place.
        // Requires a multi-threaded Tokio runtime (not single-threaded test).
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.leader_tip_cache.read(leader_address))
        })
        .map_err(|e| SyncRepoError::InternalError {
            reason: format!("leader_tip_cache read failed: {}", e),
        })
    }

    fn verify_local_chain_for_hash_path_valid(&self) -> bool {
        // Fail-closed: any error means hash_path_valid = false.
        // This includes chain integrity failures (PF1) and ledger unreadable (PF5).
        self.verify_local_chain().is_ok()
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
    // read_local_state — real backend (async helper only; sync method uses block_in_place)
    // =========================================================================

    #[tokio::test]
    async fn read_local_state_async_empty_ledger_all_false() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);
        let state = repo
            .read_local_state_async()
            .await
            .expect("read_local_state_async");

        assert!(
            state.follower_tip.is_none(),
            "empty ledger should have no follower tip"
        );
        assert!(
            !state.has_inflight_commits,
            "PF2: no in-flight commits on clean ledger"
        );
        assert!(
            !state.has_uncommitted_entries,
            "PF6: no uncommitted entries on clean ledger"
        );
        assert!(
            !state.sync_in_progress,
            "PF7: no sync in progress on clean ledger"
        );
        assert!(
            state.is_local_state_clean(),
            "clean idle state should pass is_local_state_clean"
        );
    }

    #[tokio::test]
    async fn read_local_state_async_with_follower_tip() {
        use ferrum_proto::{ActorRef, ActorType, HashChainRef, ObjectRef, ObjectType};

        let store = make_store().await;

        // Append an entry so the ledger has a tip
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
        let state = repo
            .read_local_state_async()
            .await
            .expect("read_local_state_async");

        assert!(
            state.follower_tip.is_some(),
            "ledger with entries should have a follower tip"
        );
        assert!(!state.has_inflight_commits, "PF2 should be false");
        assert!(!state.has_uncommitted_entries, "PF6 should be false");
        assert!(!state.sync_in_progress, "PF7 should be false");
    }

    #[tokio::test]
    async fn read_local_state_async_pf2_flag_set() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);

        // Set PF2 flag (in-flight commits)
        repo.set_sync_flags_test_only(true, false, false)
            .await
            .expect("set_sync_flags");

        let state = repo
            .read_local_state_async()
            .await
            .expect("read_local_state_async");
        assert!(
            state.has_inflight_commits,
            "PF2 should be true after flag set"
        );
        assert!(!state.has_uncommitted_entries, "PF6 should be false");
        assert!(!state.sync_in_progress, "PF7 should be false");
        assert!(
            !state.is_local_state_clean(),
            "PF2 set should make state not clean"
        );
    }

    #[tokio::test]
    async fn read_local_state_async_pf6_flag_set() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);

        // Set PF6 flag (uncommitted entries)
        repo.set_sync_flags_test_only(false, true, false)
            .await
            .expect("set_sync_flags");

        let state = repo
            .read_local_state_async()
            .await
            .expect("read_local_state_async");
        assert!(!state.has_inflight_commits, "PF2 should be false");
        assert!(
            state.has_uncommitted_entries,
            "PF6 should be true after flag set"
        );
        assert!(!state.sync_in_progress, "PF7 should be false");
        assert!(
            !state.is_local_state_clean(),
            "PF6 set should make state not clean"
        );
    }

    #[tokio::test]
    async fn read_local_state_async_pf7_flag_set() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);

        // Set PF7 flag (sync in progress)
        repo.set_sync_flags_test_only(false, false, true)
            .await
            .expect("set_sync_flags");

        let state = repo
            .read_local_state_async()
            .await
            .expect("read_local_state_async");
        assert!(!state.has_inflight_commits, "PF2 should be false");
        assert!(!state.has_uncommitted_entries, "PF6 should be false");
        assert!(state.sync_in_progress, "PF7 should be true after flag set");
        assert!(
            !state.is_local_state_clean(),
            "PF7 set should make state not clean"
        );
    }

    // =========================================================================
    // is_leader_authorized — real backend via async helper (PF4 allowlist)
    // =========================================================================

    #[tokio::test]
    async fn is_leader_authorized_async_returns_false_for_unknown_leader() {
        // Deny-by-default: unknown leader => Ok(false), not Err
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.is_leader_authorized_async("unknown-leader").await;
        assert!(
            result.is_ok(),
            "is_leader_authorized_async must not return Err for unknown leader"
        );
        assert!(!result.unwrap(), "unknown leader must be denied (false)");
    }

    #[tokio::test]
    async fn is_leader_authorized_async_returns_true_for_authorized_leader() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);

        // Authorize a leader via test helper
        repo.authorize_leader_test_only("leader:9000")
            .await
            .expect("authorize_leader");

        let result = repo.is_leader_authorized_async("leader:9000").await;
        assert!(result.is_ok());
        assert!(result.unwrap(), "authorized leader must return true");
    }

    #[tokio::test]
    async fn is_leader_authorized_async_returns_false_for_deauthorized_leader() {
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);

        // Authorize then deauthorize
        repo.authorize_leader_test_only("leader:9000")
            .await
            .expect("authorize_leader");
        repo.deauthorize_leader_test_only("leader:9000")
            .await
            .expect("deauthorize_leader");

        let result = repo.is_leader_authorized_async("leader:9000").await;
        assert!(result.is_ok());
        assert!(!result.unwrap(), "deauthorized leader must return false");
    }

    #[tokio::test]
    async fn is_leader_authorized_async_returns_false_for_explicitly_unauthorized_entry() {
        // Even if the leader exists in the table with authorized=0
        let store = make_store().await;
        let pool = store.pool().clone();
        let repo = SqliteSyncPreflightRepo::new(store);

        // Manually insert a row with authorized=0 via raw SQL
        sqlx::query(
            "INSERT OR REPLACE INTO leader_allowlist (leader_address, authorized, added_at) VALUES (?, 0, ?)",
        )
        .bind("leader:9000")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("manual insert");

        let result = repo.is_leader_authorized_async("leader:9000").await;
        assert!(result.is_ok());
        assert!(
            !result.unwrap(),
            "explicitly unauthorized (authorized=0) leader must return false"
        );
    }

    // =========================================================================
    // is_leader_authorized — sync method (uses block_in_place, needs multi-threaded rt)
    // =========================================================================

    #[test]
    fn is_leader_authorized_sync_returns_false_for_unknown_leader() {
        // Uses block_in_place so needs multi-threaded runtime
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);
            // Deny-by-default: unknown leader => Ok(false)
            let result = repo.is_leader_authorized("unknown-leader");
            assert!(
                result.is_ok(),
                "is_leader_authorized must not return Err for unknown leader"
            );
            assert!(!result.unwrap(), "unknown leader must be denied");
        });
    }

    #[test]
    fn is_leader_authorized_sync_returns_true_for_authorized_leader() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);

            repo.authorize_leader_test_only("leader:9000")
                .await
                .expect("authorize_leader");

            let result = repo.is_leader_authorized("leader:9000");
            assert!(result.is_ok());
            assert!(result.unwrap(), "authorized leader must return true");
        });
    }

    #[test]
    fn read_leader_tip_returns_none_when_not_cached() {
        // read_leader_tip is a sync method (uses block_in_place internally).
        // It needs a multi-threaded runtime.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);
            // read_leader_tip is sync, not async
            let result = repo.read_leader_tip("unknown-leader");
            assert!(
                result.is_ok(),
                "read_leader_tip should succeed for unknown leader (returns None)"
            );
            assert!(
                result.unwrap().is_none(),
                "unknown leader should return None tip"
            );
        });
    }

    // =========================================================================
    // Dyn trait dispatch (sync, uses block_in_place so needs multi-threaded rt)
    // =========================================================================

    #[test]
    fn concrete_repo_works_through_dyn() {
        // Use a multi-threaded runtime so block_in_place works.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo: Box<dyn SyncPreflightRepo> = Box::new(SqliteSyncPreflightRepo::new(store));
            assert!(repo.verify_local_chain().is_ok());
            // read_local_state now returns Ok (PF2/PF6/PF7 all false on clean ledger)
            assert!(repo.read_local_state().is_ok());
            let state = repo.read_local_state().unwrap();
            assert!(state.is_local_state_clean());
        });
    }

    // =========================================================================
    // Full preflight roundtrip via async helpers (PF2/PF6/PF7 flags in preflight path)
    // =========================================================================

    // ---------------------------------------------------------------------------
    // Full preflight roundtrip — uses #[test] with multi-threaded rt because
    // these tests call sync methods (read_leader_tip, is_leader_authorized) that
    // use block_in_place internally.
    // ---------------------------------------------------------------------------

    #[test]
    fn full_preflight_passes_with_clean_state_and_cached_tip() {
        use ferrum_sync::decision::TipId;
        use ferrum_sync::preflight::{PreflightResult, build_preflight_input, run_preflight};

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);

            // Authorize the leader for PF4
            repo.authorize_leader_test_only("leader-1")
                .await
                .expect("authorize_leader");

            // Verify chain passes
            let chain_ok = repo.verify_local_chain_async().await.is_ok();
            assert!(chain_ok, "chain should verify ok on clean ledger");

            // Read local state (PF2/PF6/PF7 all false on clean ledger)
            let local_state = repo
                .read_local_state_async()
                .await
                .expect("read_local_state");
            assert!(
                local_state.is_local_state_clean(),
                "clean state should pass local checks"
            );

            // PF4: authorized leader
            let leader_authorized = repo
                .is_leader_authorized("leader-1")
                .expect("is_leader_authorized");

            // Cache a leader tip so PF8 passes
            let leader_tip = TipId {
                sequence: 10,
                hash: "leaderhash".to_string(),
            };
            repo.write_leader_tip_test_only("leader-1", &leader_tip)
                .await
                .expect("write_leader_tip");

            // PF8: cached tip available -> true
            let leader_tip_result = repo.read_leader_tip("leader-1");
            let leader_tip_available = leader_tip_result.expect("read_leader_tip").is_some();

            let input = build_preflight_input(
                &local_state,
                chain_ok,
                true, // leader_identity_known (external)
                leader_authorized,
                leader_tip_available,
            );

            // All checks should pass
            let result = run_preflight(&input);
            assert_eq!(
                result,
                PreflightResult::Pass,
                "All preflight checks should pass with clean state, authorized leader, and cached tip"
            );
        });
    }

    #[test]
    fn full_preflight_pf2_fails_when_inflight_commits_set() {
        use ferrum_sync::decision::TipId;
        use ferrum_sync::preflight::{
            PreflightCheckCode, PreflightResult, build_preflight_input, run_preflight,
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);

            // Authorize the leader for PF4
            repo.authorize_leader_test_only("leader-1")
                .await
                .expect("authorize_leader");

            // Set PF2 flag to true (in-flight commits exist)
            repo.set_sync_flags_test_only(true, false, false)
                .await
                .expect("set_sync_flags");

            let chain_ok = repo.verify_local_chain_async().await.is_ok();
            let local_state = repo
                .read_local_state_async()
                .await
                .expect("read_local_state");
            assert!(local_state.has_inflight_commits, "PF2 flag should be set");

            // PF4: authorized leader
            let leader_authorized = repo
                .is_leader_authorized("leader-1")
                .expect("is_leader_authorized");

            // Cache a leader tip so PF8 passes
            let leader_tip = TipId {
                sequence: 10,
                hash: "leaderhash".to_string(),
            };
            repo.write_leader_tip_test_only("leader-1", &leader_tip)
                .await
                .expect("write_leader_tip");

            let leader_tip_available = repo
                .read_leader_tip("leader-1")
                .expect("read_leader_tip")
                .is_some();

            let input = build_preflight_input(
                &local_state,
                chain_ok,
                true,
                leader_authorized,
                leader_tip_available,
            );

            let result = run_preflight(&input);
            assert_eq!(
                result,
                PreflightResult::Fail(PreflightCheckCode::PF2),
                "PF2 should fail when has_inflight_commits is true"
            );
        });
    }

    #[test]
    fn full_preflight_pf6_fails_when_uncommitted_entries_set() {
        use ferrum_sync::decision::TipId;
        use ferrum_sync::preflight::{
            PreflightCheckCode, PreflightResult, build_preflight_input, run_preflight,
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);

            // Authorize the leader for PF4
            repo.authorize_leader_test_only("leader-1")
                .await
                .expect("authorize_leader");

            // Set PF6 flag to true (uncommitted entries exist)
            repo.set_sync_flags_test_only(false, true, false)
                .await
                .expect("set_sync_flags");

            let chain_ok = repo.verify_local_chain_async().await.is_ok();
            let local_state = repo
                .read_local_state_async()
                .await
                .expect("read_local_state");
            assert!(
                local_state.has_uncommitted_entries,
                "PF6 flag should be set"
            );

            // PF4: authorized leader
            let leader_authorized = repo
                .is_leader_authorized("leader-1")
                .expect("is_leader_authorized");

            // Cache a leader tip so PF8 passes
            let leader_tip = TipId {
                sequence: 10,
                hash: "leaderhash".to_string(),
            };
            repo.write_leader_tip_test_only("leader-1", &leader_tip)
                .await
                .expect("write_leader_tip");

            let leader_tip_available = repo
                .read_leader_tip("leader-1")
                .expect("read_leader_tip")
                .is_some();

            let input = build_preflight_input(
                &local_state,
                chain_ok,
                true,
                leader_authorized,
                leader_tip_available,
            );

            let result = run_preflight(&input);
            assert_eq!(
                result,
                PreflightResult::Fail(PreflightCheckCode::PF6),
                "PF6 should fail when has_uncommitted_entries is true"
            );
        });
    }

    #[test]
    fn full_preflight_pf7_fails_when_sync_in_progress_set() {
        use ferrum_sync::decision::TipId;
        use ferrum_sync::preflight::{
            PreflightCheckCode, PreflightResult, build_preflight_input, run_preflight,
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);

            // Authorize the leader for PF4
            repo.authorize_leader_test_only("leader-1")
                .await
                .expect("authorize_leader");

            // Set PF7 flag to true (sync in progress)
            repo.set_sync_flags_test_only(false, false, true)
                .await
                .expect("set_sync_flags");

            let chain_ok = repo.verify_local_chain_async().await.is_ok();
            let local_state = repo
                .read_local_state_async()
                .await
                .expect("read_local_state");
            assert!(local_state.sync_in_progress, "PF7 flag should be set");

            // PF4: authorized leader
            let leader_authorized = repo
                .is_leader_authorized("leader-1")
                .expect("is_leader_authorized");

            // Cache a leader tip so PF8 passes
            let leader_tip = TipId {
                sequence: 10,
                hash: "leaderhash".to_string(),
            };
            repo.write_leader_tip_test_only("leader-1", &leader_tip)
                .await
                .expect("write_leader_tip");

            let leader_tip_available = repo
                .read_leader_tip("leader-1")
                .expect("read_leader_tip")
                .is_some();

            let input = build_preflight_input(
                &local_state,
                chain_ok,
                true,
                leader_authorized,
                leader_tip_available,
            );

            let result = run_preflight(&input);
            assert_eq!(
                result,
                PreflightResult::Fail(PreflightCheckCode::PF7),
                "PF7 should fail when sync_in_progress is true"
            );
        });
    }

    #[test]
    fn full_preflight_pf4_fails_when_leader_not_authorized() {
        use ferrum_sync::decision::TipId;
        use ferrum_sync::preflight::{
            PreflightCheckCode, PreflightResult, build_preflight_input, run_preflight,
        };

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let store = make_store().await;
            let repo = SqliteSyncPreflightRepo::new(store);

            // Do NOT authorize the leader; deny-by-default => PF4 fails

            let chain_ok = repo.verify_local_chain_async().await.is_ok();
            let local_state = repo
                .read_local_state_async()
                .await
                .expect("read_local_state");
            assert!(
                local_state.is_local_state_clean(),
                "local state should be clean"
            );

            // PF4: leader NOT authorized (deny-by-default)
            let leader_authorized = repo
                .is_leader_authorized("leader-1")
                .expect("is_leader_authorized");
            assert!(
                !leader_authorized,
                "leader should NOT be authorized by default"
            );

            // Cache a leader tip so PF8 passes
            let leader_tip = TipId {
                sequence: 10,
                hash: "leaderhash".to_string(),
            };
            repo.write_leader_tip_test_only("leader-1", &leader_tip)
                .await
                .expect("write_leader_tip");

            let leader_tip_available = repo
                .read_leader_tip("leader-1")
                .expect("read_leader_tip")
                .is_some();

            let input = build_preflight_input(
                &local_state,
                chain_ok,
                true,
                leader_authorized,
                leader_tip_available,
            );

            let result = run_preflight(&input);
            assert_eq!(
                result,
                PreflightResult::Fail(PreflightCheckCode::PF4),
                "PF4 should fail when leader is not authorized"
            );
        });
    }

    // =========================================================================
    // verify_local_chain_for_hash_path_valid — real SQLite backend
    // =========================================================================

    #[tokio::test]
    async fn hash_path_valid_true_on_empty_ledger() {
        // An empty ledger has a valid (trivially empty) chain
        let store = make_store().await;
        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.verify_local_chain_for_hash_path_valid_async().await;
        assert!(result, "empty ledger should have hash_path_valid=true");
    }

    #[tokio::test]
    async fn hash_path_valid_true_on_valid_single_entry_ledger() {
        use ferrum_proto::{ActorRef, ActorType, HashChainRef, ObjectRef, ObjectType};

        let store = make_store().await;

        // Append a genesis entry to create a non-empty valid ledger
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
        let result = repo.verify_local_chain_for_hash_path_valid_async().await;
        assert!(
            result,
            "valid single-entry ledger should have hash_path_valid=true"
        );
    }

    #[tokio::test]
    async fn hash_path_valid_false_on_tampered_ledger() {
        use ferrum_proto::{ActorRef, ActorType, HashChainRef, ObjectRef, ObjectType};

        let store = make_store().await;

        // Append a genesis entry
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

        // Tamper with the entry's hash column directly in the database
        let pool = store.pool();
        sqlx::query("UPDATE ledger_entries SET content_hash = 'tampered_hash' WHERE entry_id = 1")
            .execute(pool)
            .await
            .expect("tamper update should succeed");

        let repo = SqliteSyncPreflightRepo::new(store);
        let result = repo.verify_local_chain_for_hash_path_valid_async().await;
        assert!(
            !result,
            "tampered ledger should have hash_path_valid=false (fail-closed)"
        );
    }

    // =========================================================================
    // hash_path_valid wiring to DecisionInput — real backend integration
    // =========================================================================

    #[tokio::test]
    async fn hash_path_valid_wiring_real_repo_success() {
        // Leader ahead + real local chain valid -> SYNC
        use ferrum_proto::{ActorRef, ActorType, HashChainRef, ObjectRef, ObjectType};
        use ferrum_sync::decision::{DecisionInput, Sync1Decision, TipId, decide};

        let store = make_store().await;

        // Create a valid ledger with genesis entry
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

        // Get follower tip
        let follower_tip = repo
            .read_local_state_async()
            .await
            .expect("read_local_state")
            .follower_tip
            .expect("follower should have tip after append");

        // Leader is ahead
        let leader_tip = TipId {
            sequence: follower_tip.sequence + 50,
            hash: "leader_hash_ahead".to_string(),
        };

        // Compute hash_path_valid from real local chain verification
        let hash_path_valid = repo.verify_local_chain_for_hash_path_valid_async().await;
        assert!(
            hash_path_valid,
            "valid chain should give hash_path_valid=true"
        );

        let input = DecisionInput {
            follower_tip: Some(follower_tip),
            leader_tip: Some(leader_tip),
            hash_path_valid,
        };

        let decision = decide(&input);
        assert_eq!(
            decision,
            Sync1Decision::Sync,
            "leader ahead + hash_path_valid=true -> SYNC"
        );
    }

    #[tokio::test]
    async fn hash_path_valid_wiring_real_repo_fail_closed() {
        // Leader ahead + tampered local chain -> Abort(A3)
        use ferrum_proto::{ActorRef, ActorType, HashChainRef, ObjectRef, ObjectType};
        use ferrum_sync::decision::{DecisionInput, TipId, decide};
        use ferrum_sync::error::Sync1AbortCode;

        let store = make_store().await;

        // Create a valid ledger then tamper with it
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

        // Tamper with the ledger
        let pool = store.pool();
        sqlx::query("UPDATE ledger_entries SET content_hash = 'tampered_hash' WHERE entry_id = 1")
            .execute(pool)
            .await
            .expect("tamper update should succeed");

        let repo = SqliteSyncPreflightRepo::new(store);

        // Get follower tip (still readable even if tampered)
        let follower_tip = repo
            .read_local_state_async()
            .await
            .expect("read_local_state")
            .follower_tip
            .expect("follower should have tip");

        let leader_tip = TipId {
            sequence: follower_tip.sequence + 50,
            hash: "leader_hash_ahead".to_string(),
        };

        // hash_path_valid from real query should be false (fail-closed)
        let hash_path_valid = repo.verify_local_chain_for_hash_path_valid_async().await;
        assert!(
            !hash_path_valid,
            "tampered chain should give hash_path_valid=false"
        );

        let input = DecisionInput {
            follower_tip: Some(follower_tip),
            leader_tip: Some(leader_tip),
            hash_path_valid,
        };

        let decision = decide(&input);
        assert!(
            decision.is_abort(),
            "leader ahead + hash_path_valid=false -> abort"
        );
        assert_eq!(
            decision.abort_code(),
            Some(Sync1AbortCode::A3),
            "should abort with A3 (C2 violation)"
        );
    }
}
