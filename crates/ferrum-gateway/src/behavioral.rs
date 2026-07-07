//! Behavioral anomaly profiling for the gateway.
//!
//! V1 is opt-in, in-memory, advisory-only. It detects repeated high-risk/R3
//! proposals by the same principal within a rolling window and emits:
//!   - a `BehavioralFinding` returned to the proposal evaluation path,
//!   - an audit log entry (`AuditAction::BehavioralAnomaly`),
//!   - a Prometheus counter (`ferrumgate_behavioral_anomaly_detected_total`).
//!
//! It does not alter policy decisions, auto-block, or persist state.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ferrum_proto::{RiskTier, RollbackClass};

/// Severity of an advisory behavioral anomaly finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehavioralSeverity {
    Warning,
    Critical,
}

/// An advisory finding produced by a behavioral profiler.
///
/// The `principal_id` is kept internal to the profiler; callers must not copy
/// raw identifiers into audit or provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BehavioralFinding {
    pub severity: BehavioralSeverity,
    pub(crate) principal_id: String,
    pub window_count: u32,
    pub window_secs: u64,
    pub threshold: u32,
}

/// Profiler interface for proposal-time behavioral inspection.
pub trait BehavioralProfiler: Send + Sync {
    /// Inspect a proposal and return an advisory finding if an anomaly is detected.
    fn inspect_proposal(
        &self,
        principal_id: &str,
        estimated_risk: RiskTier,
        requested_rollback_class: RollbackClass,
    ) -> Option<BehavioralFinding>;
}

/// No-op profiler used when behavioral anomaly detection is disabled.
#[derive(Debug, Default)]
pub struct NoopBehavioralProfiler;

impl BehavioralProfiler for NoopBehavioralProfiler {
    fn inspect_proposal(
        &self,
        _principal_id: &str,
        _estimated_risk: RiskTier,
        _requested_rollback_class: RollbackClass,
    ) -> Option<BehavioralFinding> {
        None
    }
}

/// Threshold-based profiler that tracks high-risk/R3 proposals per principal in
/// a rolling window.
#[derive(Debug)]
pub struct ThresholdBehavioralProfiler {
    window: Duration,
    warning_threshold: u32,
    critical_threshold: u32,
    max_actors: usize,
    max_events_per_actor: usize,
    actors: Mutex<HashMap<String, ActorWindow>>,
}

#[derive(Debug, Default)]
struct ActorWindow {
    events: VecDeque<tokio::time::Instant>,
}

impl ActorWindow {
    fn prune(&mut self, now: tokio::time::Instant, window: Duration) {
        while let Some(&t) = self.events.front() {
            if now.duration_since(t) >= window {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
}

impl ThresholdBehavioralProfiler {
    /// Create a new threshold profiler.
    ///
    /// `window` is the rolling observation window. `warning_threshold` and
    /// `critical_threshold` are inclusive counts of qualifying proposals within
    /// the window that trigger a warning or critical finding, respectively.
    /// `max_actors` bounds the number of principals tracked in memory.
    ///
    /// Each actor's event queue is hard-capped at `critical_threshold + 1`
    /// events so that memory is bounded even when events arrive faster than the
    /// rolling window prunes them.
    pub fn new(
        window: Duration,
        warning_threshold: u32,
        critical_threshold: u32,
        max_actors: usize,
    ) -> Self {
        Self {
            window,
            warning_threshold,
            critical_threshold,
            max_actors,
            max_events_per_actor: critical_threshold as usize + 1,
            actors: Mutex::new(HashMap::with_capacity(max_actors.min(1024))),
        }
    }

    /// Returns true if the proposal qualifies for profiling (high-risk or R3).
    fn is_target(risk: &RiskTier, rollback: &RollbackClass) -> bool {
        matches!(risk, RiskTier::High | RiskTier::Critical)
            || matches!(rollback, RollbackClass::R3IrreversibleHighConsequence)
    }
}

impl BehavioralProfiler for ThresholdBehavioralProfiler {
    fn inspect_proposal(
        &self,
        principal_id: &str,
        estimated_risk: RiskTier,
        requested_rollback_class: RollbackClass,
    ) -> Option<BehavioralFinding> {
        if !Self::is_target(&estimated_risk, &requested_rollback_class) {
            return None;
        }

        let now = tokio::time::Instant::now();
        let mut actors = self.actors.lock().unwrap();

        // Bound total actor memory. Evict an arbitrary actor when at capacity.
        if !actors.contains_key(principal_id) && actors.len() >= self.max_actors {
            if let Some(oldest) = actors.keys().next().cloned() {
                actors.remove(&oldest);
            }
        }

        let actor = actors.entry(principal_id.to_string()).or_default();
        actor.prune(now, self.window);
        actor.events.push_back(now);
        // Hard per-actor event cap: keep the most recent events and drop older
        // ones so memory is bounded even if high-risk proposals arrive faster
        // than the rolling window expires them.
        while actor.events.len() > self.max_events_per_actor {
            actor.events.pop_front();
        }
        let count = actor.events.len() as u32;

        if count >= self.critical_threshold {
            Some(BehavioralFinding {
                severity: BehavioralSeverity::Critical,
                principal_id: principal_id.to_string(),
                window_count: count,
                window_secs: self.window.as_secs(),
                threshold: self.critical_threshold,
            })
        } else if count >= self.warning_threshold {
            Some(BehavioralFinding {
                severity: BehavioralSeverity::Warning,
                principal_id: principal_id.to_string(),
                window_count: count,
                window_secs: self.window.as_secs(),
                threshold: self.warning_threshold,
            })
        } else {
            None
        }
    }
}

/// Build a profiler from server configuration.
///
/// Returns a `NoopBehavioralProfiler` when the feature is disabled, so the
/// default configuration has no behavioral change.
pub fn build_profiler(config: &crate::ServerConfig) -> Arc<dyn BehavioralProfiler> {
    if config.behavioral_anomaly_enabled {
        Arc::new(ThresholdBehavioralProfiler::new(
            Duration::from_secs(config.behavioral_anomaly_window_secs),
            config.behavioral_anomaly_warning_threshold,
            config.behavioral_anomaly_critical_threshold,
            config.behavioral_anomaly_max_actors,
        ))
    } else {
        Arc::new(NoopBehavioralProfiler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn high_risk_proposal() -> (RiskTier, RollbackClass) {
        (RiskTier::High, RollbackClass::R0NativeReversible)
    }

    fn r3_proposal() -> (RiskTier, RollbackClass) {
        (RiskTier::Low, RollbackClass::R3IrreversibleHighConsequence)
    }

    fn low_risk_proposal() -> (RiskTier, RollbackClass) {
        (RiskTier::Low, RollbackClass::R0NativeReversible)
    }

    #[test]
    fn noop_profiler_never_finds_anomaly() {
        let profiler = NoopBehavioralProfiler;
        let (risk, rollback) = high_risk_proposal();
        for i in 0..100 {
            assert!(
                profiler
                    .inspect_proposal("principal-1", risk.clone(), rollback.clone())
                    .is_none(),
                "noop should never fire on event {}",
                i
            );
        }
    }

    #[test]
    fn threshold_profiler_finds_warning_and_critical() {
        let profiler = ThresholdBehavioralProfiler::new(Duration::from_secs(60), 3, 5, 10);
        let (risk, rollback) = high_risk_proposal();

        for i in 1..=5 {
            let finding = profiler.inspect_proposal("principal-1", risk.clone(), rollback.clone());
            if i < 3 {
                assert!(finding.is_none(), "expected no finding at count {}", i);
            } else if i < 5 {
                assert_eq!(
                    finding.as_ref().map(|f| f.severity),
                    Some(BehavioralSeverity::Warning),
                    "expected warning at count {}",
                    i
                );
            } else {
                assert_eq!(
                    finding.as_ref().map(|f| f.severity),
                    Some(BehavioralSeverity::Critical),
                    "expected critical at count {}",
                    i
                );
            }
        }

        // Per-actor isolation: a different principal starts from zero.
        assert!(
            profiler
                .inspect_proposal("principal-2", risk.clone(), rollback.clone())
                .is_none()
        );
    }

    #[test]
    fn threshold_profiler_prunes_stale_events() {
        let profiler = ThresholdBehavioralProfiler::new(Duration::from_millis(10), 2, 3, 10);
        let (risk, rollback) = high_risk_proposal();

        // Two events are within the window; the second triggers a warning.
        assert!(
            profiler
                .inspect_proposal("p", risk.clone(), rollback.clone())
                .is_none()
        );
        let finding = profiler.inspect_proposal("p", risk.clone(), rollback.clone());
        assert_eq!(
            finding.map(|f| f.severity),
            Some(BehavioralSeverity::Warning)
        );

        // Wait for the window to elapse; the next event should be the only one.
        std::thread::sleep(Duration::from_millis(15));
        assert!(
            profiler
                .inspect_proposal("p", risk.clone(), rollback.clone())
                .is_none()
        );
    }

    #[test]
    fn threshold_profiler_bounds_actor_count() {
        let profiler = ThresholdBehavioralProfiler::new(Duration::from_secs(60), 1, 2, 2);
        let (risk, rollback) = high_risk_proposal();

        // One qualifying event per actor is enough to trigger a warning.
        profiler.inspect_proposal("actor-a", risk.clone(), rollback.clone());
        profiler.inspect_proposal("actor-b", risk.clone(), rollback.clone());
        // A third actor evicts one of the previous entries; the map never
        // exceeds the configured bound.
        profiler.inspect_proposal("actor-c", risk.clone(), rollback.clone());

        let actors = profiler.actors.lock().unwrap();
        assert_eq!(actors.len(), 2, "actor map must respect max_actors bound");
    }

    #[test]
    fn threshold_profiler_bounds_per_actor_event_count() {
        let profiler = ThresholdBehavioralProfiler::new(Duration::from_secs(60), 1, 2, 10);
        let (risk, rollback) = high_risk_proposal();

        // Fire many more qualifying proposals than the critical threshold.
        for _ in 0..100 {
            profiler.inspect_proposal("p", risk.clone(), rollback.clone());
        }

        let actors = profiler.actors.lock().unwrap();
        let actor = actors.get("p").expect("actor should exist");
        assert_eq!(
            actor.events.len(),
            3,
            "per-actor event queue must be hard-capped at critical_threshold + 1"
        );
    }

    #[test]
    fn threshold_profiler_ignores_low_risk_proposals() {
        let profiler = ThresholdBehavioralProfiler::new(Duration::from_secs(60), 1, 2, 10);
        let (risk, rollback) = low_risk_proposal();

        for _ in 0..10 {
            assert!(
                profiler
                    .inspect_proposal("p", risk.clone(), rollback.clone())
                    .is_none()
            );
        }
    }

    #[test]
    fn threshold_profiler_detects_r3_irrespective_of_risk_tier() {
        let profiler = ThresholdBehavioralProfiler::new(Duration::from_secs(60), 1, 2, 10);
        let (risk, rollback) = r3_proposal();

        let finding = profiler.inspect_proposal("p", risk.clone(), rollback.clone());
        assert_eq!(
            finding.map(|f| f.severity),
            Some(BehavioralSeverity::Warning)
        );
    }
}
