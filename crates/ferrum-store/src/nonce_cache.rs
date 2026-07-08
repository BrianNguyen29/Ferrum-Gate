//! Nonce cache implementations for agent-auth replay protection.
//!
//! `InMemoryNonceCache` preserves the original process-local behaviour and is
//! always available. `PostgresNonceCache` is only compiled when the `postgres`
//! feature is enabled and provides shared replay protection across multiple
//! gateway processes.

use crate::{NonceCache, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Process-local nonce cache with TTL and bounded capacity.
///
/// Mirrors the original `Arc<Mutex<HashMap<String, Instant>>>` behaviour used
/// by the gateway, but hides the implementation behind the `NonceCache` trait.
#[derive(Debug, Clone)]
pub struct InMemoryNonceCache {
    max_entries: usize,
    inner: Arc<Mutex<HashMap<String, Instant>>>,
}

impl InMemoryNonceCache {
    /// Create a new cache with the given capacity bound.
    pub fn new(max_entries: usize) -> Self {
        Self {
            max_entries,
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Test-only membership check that does not mutate the cache.
    #[cfg(test)]
    fn contains_for_test(&self, nonce: &str) -> bool {
        let cache = self
            .inner
            .lock()
            .expect("nonce cache lock poisoned in test");
        cache.contains_key(nonce)
    }
}

#[async_trait]
impl NonceCache for InMemoryNonceCache {
    async fn check_and_insert(&self, nonce: &str, ttl: Duration) -> Result<bool> {
        let mut cache = self
            .inner
            .lock()
            .map_err(|e| crate::StoreError::Other(format!("nonce cache lock poisoned: {e}")))?;
        let now = Instant::now();

        // Evict entries that have exceeded the TTL.
        cache.retain(|_, inserted| now.duration_since(*inserted) < ttl);

        // Enforce a hard capacity bound after TTL cleanup.
        prune_oldest(&mut cache, self.max_entries.saturating_sub(1));

        if cache.contains_key(nonce) {
            return Ok(false);
        }

        cache.insert(nonce.to_string(), now);
        Ok(true)
    }

    async fn vacuum(&self) -> Result<usize> {
        // In-memory TTL eviction already happens during `check_and_insert`, so
        // an explicit vacuum is a no-op that still reports success.
        Ok(0)
    }
}

/// Prune the oldest entries until the cache is at or below `max_entries`.
fn prune_oldest(cache: &mut HashMap<String, Instant>, max_entries: usize) {
    while cache.len() > max_entries {
        let oldest = cache
            .iter()
            .min_by_key(|(_, instant)| *instant)
            .map(|(k, _)| k.clone());
        match oldest {
            Some(key) => {
                cache.remove(&key);
            }
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn first_insert_accepted() {
        let cache = InMemoryNonceCache::new(100);
        assert!(
            cache
                .check_and_insert("nonce-1", Duration::from_secs(60))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn replay_rejected() {
        let cache = InMemoryNonceCache::new(100);
        let ttl = Duration::from_secs(60);
        assert!(cache.check_and_insert("nonce-1", ttl).await.unwrap());
        assert!(!cache.check_and_insert("nonce-1", ttl).await.unwrap());
    }

    #[tokio::test]
    async fn expired_nonce_can_be_reused() {
        let cache = InMemoryNonceCache::new(100);
        let ttl = Duration::from_millis(50);
        assert!(cache.check_and_insert("nonce-1", ttl).await.unwrap());
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(cache.check_and_insert("nonce-1", ttl).await.unwrap());
    }

    #[tokio::test]
    async fn capacity_bound_evicts_oldest() {
        let cache = InMemoryNonceCache::new(3);
        let ttl = Duration::from_secs(60);
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert!(
                cache
                    .check_and_insert(&format!("nonce-{i}"), ttl)
                    .await
                    .unwrap()
            );
        }
        // At capacity the oldest retained entries have been evicted. The three
        // newest (nonce-2, nonce-3, nonce-4) should still be present.
        assert!(!cache.contains_for_test("nonce-0"));
        assert!(!cache.contains_for_test("nonce-1"));
        assert!(cache.contains_for_test("nonce-2"));
        assert!(cache.contains_for_test("nonce-3"));
        assert!(cache.contains_for_test("nonce-4"));
    }

    #[test]
    fn prune_oldest_drops_oldest_first() {
        let mut cache = HashMap::new();
        let now = Instant::now();
        for i in 0..5 {
            cache.insert(format!("nonce-{i}"), now - Duration::from_secs(i));
        }
        prune_oldest(&mut cache, 3);
        assert_eq!(cache.len(), 3);
        assert!(!cache.contains_key("nonce-4"));
        assert!(!cache.contains_key("nonce-3"));
        assert!(cache.contains_key("nonce-0"));
        assert!(cache.contains_key("nonce-1"));
        assert!(cache.contains_key("nonce-2"));
    }
}
