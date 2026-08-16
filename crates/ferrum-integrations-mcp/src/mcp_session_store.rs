//! In-memory MCP session store for the experimental HTTP transport.
//!
//! This module provides a bounded, ephemeral session/replay skeleton. It is
//! **not** a persistent checkpoint store: sessions and replay buffers live
//! only in process memory and are lost on restart.
//!
//! Security properties:
//! - Session IDs are 32 random bytes encoded as URL-safe base64 (visible ASCII).
//! - Sessions are bound to a SHA-256 fingerprint of the bearer token; the raw
//!   token is never stored.
//! - Session IDs are never logged by this module.
//! - Replay buffers contain only serialized outbound JSON-RPC responses.
//! - Replay buffers are byte-bounded: a single event cannot exceed
//!   `max_event_bytes`, and the total retained bytes per session cannot exceed
//!   `max_total_bytes`. Oldest events are evicted to stay within bounds.

use rand::{Rng, RngExt};
use sha2::Digest;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Errors returned by session store operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// The maximum number of sessions has been reached.
    LimitReached,
    /// The session does not exist or has expired.
    NotFound,
    /// The session auth fingerprint does not match the request token.
    AuthMismatch,
    /// The requested `Last-Event-ID` is unknown or refers to an evicted event.
    StaleEventId,
    /// The outbound payload exceeds the per-event byte limit.
    PayloadTooLarge,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitReached => write!(f, "session limit reached"),
            Self::NotFound => write!(f, "session not found or expired"),
            Self::AuthMismatch => write!(f, "session auth mismatch"),
            Self::StaleEventId => write!(f, "unknown or stale Last-Event-ID"),
            Self::PayloadTooLarge => write!(f, "replay payload exceeds per-event byte limit"),
        }
    }
}

impl std::error::Error for SessionError {}

/// A single outbound event that can be replayed over an SSE stream.
#[derive(Debug, Clone)]
pub struct ReplayEvent {
    /// Monotonic event ID within the session.
    pub id: String,
    /// Serialized JSON-RPC response payload.
    pub payload: String,
}

/// An in-memory MCP session bound to an auth fingerprint.
#[derive(Debug)]
pub struct McpSession {
    auth_fingerprint: String,
    last_activity: Mutex<Instant>,
    events: Mutex<Vec<ReplayEvent>>,
    next_event_id: AtomicU64,
    first_event_id: AtomicU64,
    last_event_id: AtomicU64,
    max_event_bytes: usize,
    total_bytes: AtomicUsize,
}

impl McpSession {
    fn new(auth_fingerprint: String, max_event_bytes: usize) -> Self {
        let now = Instant::now();
        Self {
            auth_fingerprint,
            last_activity: Mutex::new(now),
            events: Mutex::new(Vec::new()),
            next_event_id: AtomicU64::new(1),
            first_event_id: AtomicU64::new(0),
            last_event_id: AtomicU64::new(0),
            max_event_bytes,
            total_bytes: AtomicUsize::new(0),
        }
    }

    /// Auth fingerprint (sha256 hex of bearer token) bound to this session.
    pub fn auth_fingerprint(&self) -> &str {
        &self.auth_fingerprint
    }

    /// Return the instant of the last activity on this session.
    pub fn last_activity(&self) -> Instant {
        *self.last_activity.lock().unwrap()
    }

    /// Record a new activity timestamp.
    fn touch(&self) {
        if let Ok(mut last) = self.last_activity.lock() {
            *last = Instant::now();
        }
    }

    /// Append a serialized outbound event to the replay buffer.
    /// Returns the assigned monotonic event ID, or `SessionError::PayloadTooLarge`
    /// if the payload exceeds the configured per-event byte limit.
    pub fn append_event(&self, payload: String) -> Result<String, SessionError> {
        let payload_bytes = payload.len();
        if payload_bytes > self.max_event_bytes {
            return Err(SessionError::PayloadTooLarge);
        }
        let id = self.next_event_id.fetch_add(1, Ordering::SeqCst);
        let id_str = id.to_string();
        {
            let mut events = self.events.lock().unwrap();
            events.push(ReplayEvent {
                id: id_str.clone(),
                payload,
            });
            let total = self.total_bytes.load(Ordering::SeqCst) + payload_bytes;
            self.total_bytes.store(total, Ordering::SeqCst);
            if self.first_event_id.load(Ordering::SeqCst) == 0 {
                self.first_event_id.store(id, Ordering::SeqCst);
            }
            self.last_event_id.store(id, Ordering::SeqCst);
        }
        self.touch();
        Ok(id_str)
    }

    /// Return events with an ID strictly greater than `last_event_id`.
    ///
    /// Returns `SessionError::StaleEventId` if `last_event_id` is greater than
    /// the last stored event or refers to an event that has already been evicted.
    pub fn events_after(
        &self,
        last_event_id: Option<&str>,
    ) -> Result<Vec<ReplayEvent>, SessionError> {
        let events = self.events.lock().unwrap();
        if let Some(raw) = last_event_id {
            let last: u64 = raw.parse().map_err(|_| SessionError::StaleEventId)?;
            let first = self.first_event_id.load(Ordering::SeqCst);
            let last_stored = self.last_event_id.load(Ordering::SeqCst);
            if last > 0 && last_stored == 0 {
                return Err(SessionError::StaleEventId);
            }
            if first > 0 && last + 1 < first {
                return Err(SessionError::StaleEventId);
            }
            if last > last_stored {
                return Err(SessionError::StaleEventId);
            }
            Ok(events
                .iter()
                .filter(|e| e.id.parse::<u64>().unwrap_or(0) > last)
                .cloned()
                .collect())
        } else {
            Ok(events.clone())
        }
    }
}

/// In-memory store for MCP sessions and their replay buffers.
#[derive(Debug)]
pub struct McpSessionStore {
    sessions: RwLock<HashMap<String, Arc<McpSession>>>,
    ttl: Duration,
    max_events: usize,
    max_sessions: usize,
    max_event_bytes: usize,
    max_total_bytes: usize,
}

impl McpSessionStore {
    /// Create a new store with the given TTL and per-session/global bounds.
    ///
    /// `max_events` is clamped to at least 1 and `max_sessions` is clamped to
    /// at least 1 to prevent accidental unbounded growth. `max_event_bytes` and
    /// `max_total_bytes` are clamped to at least 1 byte so that any non-empty
    /// payload must be at least one byte; in practice callers should set these
    /// to useful values (e.g. 1 MiB and 16 MiB).
    pub fn new(
        ttl: Duration,
        max_events: usize,
        max_sessions: usize,
        max_event_bytes: usize,
        max_total_bytes: usize,
    ) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            ttl,
            max_events: max_events.max(1),
            max_sessions: max_sessions.max(1),
            max_event_bytes: max_event_bytes.max(1),
            max_total_bytes: max_total_bytes.max(1),
        }
    }

    /// Create a new session bound to the bearer token fingerprint.
    ///
    /// Returns the newly minted session ID.
    pub fn create_session(&self, token: &str) -> Result<String, SessionError> {
        self.create_session_with_fingerprint(&auth_fingerprint(token))
    }

    /// Create a new session bound to a pre-computed auth fingerprint.
    pub fn create_session_with_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<String, SessionError> {
        let id = generate_session_id();
        self.cleanup_expired();
        {
            let mut sessions = self.sessions.write().unwrap();
            if sessions.len() >= self.max_sessions {
                return Err(SessionError::LimitReached);
            }
            sessions.insert(
                id.clone(),
                Arc::new(McpSession::new(
                    fingerprint.to_string(),
                    self.max_event_bytes,
                )),
            );
        }
        Ok(id)
    }

    /// Validate that a session exists, has not expired, and belongs to the
    /// given auth fingerprint.
    pub fn validate_session(
        &self,
        session_id: &str,
        fingerprint: &str,
    ) -> Result<Arc<McpSession>, SessionError> {
        let sessions = self.sessions.read().unwrap();
        let session = sessions
            .get(session_id)
            .cloned()
            .ok_or(SessionError::NotFound)?;
        drop(sessions);

        if session.last_activity().elapsed() > self.ttl {
            return Err(SessionError::NotFound);
        }
        if !constant_time_eq::constant_time_eq(
            session.auth_fingerprint().as_bytes(),
            fingerprint.as_bytes(),
        ) {
            return Err(SessionError::AuthMismatch);
        }
        session.touch();
        Ok(session)
    }

    /// Terminate a session idempotently.
    ///
    /// Returns `true` if the session existed and was removed, `false` otherwise.
    pub fn terminate_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(session_id).is_some()
    }

    /// Append a serialized outbound event to the session's replay buffer.
    ///
    /// Returns `SessionError::PayloadTooLarge` if the payload is larger than the
    /// configured `max_event_bytes`. If the buffer exceeds `max_total_bytes` or
    /// `max_events`, oldest events are evicted before returning.
    pub fn append_event(&self, session_id: &str, payload: String) -> Result<String, SessionError> {
        let sessions = self.sessions.read().unwrap();
        let session = sessions
            .get(session_id)
            .cloned()
            .ok_or(SessionError::NotFound)?;
        drop(sessions);

        let id = session.append_event(payload)?;
        self.enforce_bounds(&session);
        Ok(id)
    }

    /// Drop oldest events until the session is within the per-session event count
    /// and total byte limits.
    fn enforce_bounds(&self, session: &Arc<McpSession>) {
        let mut events = session.events.lock().unwrap();
        let mut total_bytes = session.total_bytes.load(Ordering::SeqCst);
        while !events.is_empty()
            && (events.len() > self.max_events || total_bytes > self.max_total_bytes)
        {
            let removed = events.remove(0);
            total_bytes = total_bytes.saturating_sub(removed.payload.len());
            if let Some(first) = events.first() {
                if let Ok(n) = first.id.parse::<u64>() {
                    session.first_event_id.store(n, Ordering::SeqCst);
                }
            } else {
                session.first_event_id.store(0, Ordering::SeqCst);
                session.last_event_id.store(0, Ordering::SeqCst);
            }
        }
        session.total_bytes.store(total_bytes, Ordering::SeqCst);
    }

    /// Remove expired sessions and return the number removed.
    pub fn cleanup_expired(&self) -> usize {
        let now = Instant::now();
        let mut sessions = self.sessions.write().unwrap();
        let before = sessions.len();
        sessions.retain(|_, session| now.duration_since(session.last_activity()) <= self.ttl);
        before - sessions.len()
    }
}

/// Generate a secure random 32-byte session ID using URL-safe base64.
fn generate_session_id() -> String {
    let bytes = rand::rng().random::<[u8; 32]>();
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

/// Compute a stable fingerprint of a bearer token.
///
/// The raw token is never stored; only the SHA-256 hex digest is kept.
pub fn auth_fingerprint(token: &str) -> String {
    let hash = sha2::Sha256::digest(token.as_bytes());
    hex::encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_returns_unique_id() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id1 = store.create_session("token-a").unwrap();
        let id2 = store.create_session("token-b").unwrap();
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_validate_session_requires_matching_fingerprint() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token-a").unwrap();
        assert!(
            store
                .validate_session(&id, &auth_fingerprint("token-a"))
                .is_ok()
        );
        assert!(matches!(
            store.validate_session(&id, &auth_fingerprint("token-b")),
            Err(SessionError::AuthMismatch)
        ));
    }

    #[test]
    fn test_validate_session_unknown_returns_not_found() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        assert!(matches!(
            store.validate_session("does-not-exist", &auth_fingerprint("x")),
            Err(SessionError::NotFound)
        ));
    }

    #[test]
    fn test_session_expires_after_ttl() {
        let store = McpSessionStore::new(
            Duration::from_millis(1),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(matches!(
            store.validate_session(&id, &auth_fingerprint("token")),
            Err(SessionError::NotFound)
        ));
    }

    #[test]
    fn test_create_session_respects_max_sessions() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            2,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        store.create_session("a").unwrap();
        store.create_session("b").unwrap();
        assert!(matches!(
            store.create_session("c"),
            Err(SessionError::LimitReached)
        ));
    }

    #[test]
    fn test_append_and_replay_events() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token").unwrap();
        let e1 = store.append_event(&id, r#"{"id":1}"#.to_string()).unwrap();
        let e2 = store.append_event(&id, r#"{"id":2}"#.to_string()).unwrap();

        let session = store
            .validate_session(&id, &auth_fingerprint("token"))
            .unwrap();
        let all = session.events_after(None).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, e1);
        assert_eq!(all[1].id, e2);

        let after_first = session.events_after(Some(&e1)).unwrap();
        assert_eq!(after_first.len(), 1);
        assert_eq!(after_first[0].id, e2);
    }

    #[test]
    fn test_replay_unknown_event_id_returns_error() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token").unwrap();
        store.append_event(&id, r#"{}"#.to_string()).unwrap();

        let session = store
            .validate_session(&id, &auth_fingerprint("token"))
            .unwrap();
        assert!(matches!(
            session.events_after(Some("99")),
            Err(SessionError::StaleEventId)
        ));
    }

    #[test]
    fn test_replay_buffer_drops_oldest_events() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            2,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token").unwrap();
        let e1 = store.append_event(&id, r#"{"n":1}"#.to_string()).unwrap();
        let e2 = store.append_event(&id, r#"{"n":2}"#.to_string()).unwrap();
        let e3 = store.append_event(&id, r#"{"n":3}"#.to_string()).unwrap();

        let session = store
            .validate_session(&id, &auth_fingerprint("token"))
            .unwrap();
        let all = session.events_after(None).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, e2);
        assert_eq!(all[1].id, e3);

        // Event 1 was evicted; replaying from before it is stale.
        assert!(matches!(
            session.events_after(Some("0")),
            Err(SessionError::StaleEventId)
        ));

        // Replaying from the evicted event itself still gives the remaining events.
        let after_e1 = session.events_after(Some(&e1)).unwrap();
        assert_eq!(after_e1.len(), 2);
        assert_eq!(after_e1[0].id, e2);
        assert_eq!(after_e1[1].id, e3);
    }

    #[test]
    fn test_terminate_session_idempotent() {
        let store = McpSessionStore::new(
            Duration::from_secs(300),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token").unwrap();
        assert!(store.terminate_session(&id));
        assert!(!store.terminate_session(&id));
        assert!(matches!(
            store.validate_session(&id, &auth_fingerprint("token")),
            Err(SessionError::NotFound)
        ));
    }

    #[test]
    fn test_cleanup_expired_removes_stale_sessions() {
        let store = McpSessionStore::new(
            Duration::from_millis(1),
            100,
            100,
            1024 * 1024,
            16 * 1024 * 1024,
        );
        let id = store.create_session("token").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        let removed = store.cleanup_expired();
        assert_eq!(removed, 1);
        assert!(matches!(
            store.validate_session(&id, &auth_fingerprint("token")),
            Err(SessionError::NotFound)
        ));
    }

    #[test]
    fn test_reject_oversized_event_payload() {
        let store = McpSessionStore::new(Duration::from_secs(300), 100, 100, 10, 1024);
        let id = store.create_session("token").unwrap();
        assert!(matches!(
            store.append_event(&id, "12345678901".to_string()),
            Err(SessionError::PayloadTooLarge)
        ));

        // A payload exactly at the limit is accepted.
        let e1 = store.append_event(&id, "1234567890".to_string()).unwrap();
        let session = store
            .validate_session(&id, &auth_fingerprint("token"))
            .unwrap();
        let all = session.events_after(None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, e1);
    }

    #[test]
    fn test_evict_oldest_events_when_total_bytes_exceeded() {
        // Each small payload is 7 bytes ("{"n":X}"). Cap total bytes at 10 so
        // two events fit and a third evicts the oldest.
        let store = McpSessionStore::new(Duration::from_secs(300), 100, 100, 100, 14);
        let id = store.create_session("token").unwrap();
        let e1 = store.append_event(&id, r#"{"n":1}"#.to_string()).unwrap();
        let e2 = store.append_event(&id, r#"{"n":2}"#.to_string()).unwrap();
        let e3 = store.append_event(&id, r#"{"n":3}"#.to_string()).unwrap();

        let session = store
            .validate_session(&id, &auth_fingerprint("token"))
            .unwrap();
        let all = session.events_after(None).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, e2);
        assert_eq!(all[1].id, e3);

        // Replaying from the evicted event still yields the retained events.
        let after_e1 = session.events_after(Some(&e1)).unwrap();
        assert_eq!(after_e1.len(), 2);
        assert_eq!(after_e1[0].id, e2);
        assert_eq!(after_e1[1].id, e3);
    }

    #[test]
    fn test_large_event_evicts_self_when_total_bytes_too_small() {
        // A payload of 10 bytes with a total cap of 5 bytes means the event
        // is accepted by the per-event limit but immediately evicted by the
        // total byte cap. The replay buffer ends up empty.
        let store = McpSessionStore::new(Duration::from_secs(300), 100, 100, 100, 5);
        let id = store.create_session("token").unwrap();
        let _e1 = store
            .append_event(&id, "1234567890".to_string())
            .expect("append within per-event limit succeeds");

        let session = store
            .validate_session(&id, &auth_fingerprint("token"))
            .unwrap();
        let all = session.events_after(None).unwrap();
        assert!(all.is_empty());
    }
}
