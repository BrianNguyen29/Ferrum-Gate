# ADR 010 — Behavioral Anomaly Detection

## Status
Proposed

## Context

The current governance model relies on static policy evaluation and explicit approval gating for R3 actions. It does not learn or detect unusual agency patterns that may indicate:
- A compromised agent suddenly requesting many R3 actions outside its historical baseline.
- An insider abusing a scoped token to execute actions at unusual times or frequencies.
- Prompt injection resulting in anomalous tool/action combinations that pass static policy but violate behavioral norms.

This gap was identified in the OWASP LLM06 mapping (Excessive Agency) and the threat model (B4 — anomalous agent behavior).

## Decision

Propose a lightweight, opt-in behavioral anomaly detection layer that operates on audit/provenance data without requiring external ML services.

### 1. Detection scope
- **Actor-based**: per-actor (agent, operator, service account) baselines for action frequency, R3 rate, and tool/action diversity.
- **Time-based**: detection of actions outside historical time windows (e.g., an operator account active at 3 AM when historically inactive).
- **Sequence-based**: detection of unusual action sequences (e.g., rapid policy bundle creation followed by immediate activation).

### 2. Architecture
- A `BehavioralProfiler` trait that consumes audit/provenance events and maintains a lightweight statistical model (e.g., rolling window counts, simple histograms).
- A `ThresholdDetector` implementation that flags anomalies when a metric exceeds N standard deviations from the rolling mean over a configurable window (default 7 days).
- No external ML model or vector database; the implementation uses in-memory or SQLite-resident statistics.
- Anomaly events are emitted to the audit log and as Prometheus metrics (`ferrumgate_behavioral_anomaly_detected_total`).

### 3. Operator action
- When an anomaly is detected, the system does **not** auto-block the action (to avoid false-positive denial of service).
- Instead, it:
  - Emits a `behavioral_anomaly` audit entry with severity `warning` or `critical`.
  - Increments the Prometheus metric.
  - Optionally (configurable) requires R2/R3 actions to additionally require approval, even if they would normally be auto-approved.

### 4. Privacy and performance
- All profiling data is derived from the existing audit log; no new telemetry is collected.
- The profiler runs asynchronously, decoupled from the request hot path.
- Statistical models are bounded in memory (fixed-size circular buffers, not unbounded growth).

## Consequences

- **Positive**: Adds a dynamic layer to complement static policy evaluation.
- **Positive**: No external dependencies; all data is local.
- **Negative**: Simple statistical models have higher false-positive rates than ML-based approaches; operator tuning is required.
- **Negative**: Adds CPU/memory cost for the background profiling task.
- **Non-goal**: This is not a replacement for policy evaluation; it is an adjunct signal.

## Acceptance criteria

1. `BehavioralProfiler` trait is defined with `ingest(event)` and `evaluate(actor_id, proposal) -> AnomalyScore` methods.
2. `ThresholdDetector` implementation computes rolling mean and standard deviation over a configurable window.
3. Anomaly events are written to the audit log with actor, metric, observed value, and expected range.
4. Prometheus metric `ferrumgate_behavioral_anomaly_detected_total` is emitted with labels for `actor_type` and `severity`.
5. Configuration is validated at startup: `behavioral_detection_enabled` (bool), `behavioral_window_days` (u32, default 7), `behavioral_threshold_sigma` (f64, default 3.0).
6. When `behavioral_detection_enabled=true` and an anomaly is detected for an R2/R3 action, the action is escalated to `needs_operator_review` in the lifecycle outbox.
7. Documentation updated: `docs/guides/security-model.md`, `docs/operations/runbook.md`, `docs/security/threat-model-stride.md`.
8. Integration tests simulate anomalous patterns and verify detection.

## Non-goals

- Real-time ML inference or external model integration (out of scope; can be added via a future adapter).
- Auto-blocking of anomalous actions (escalation only, to avoid false-positive DoS).
- Cross-actor correlation or graph analysis (single-actor baselines only; cross-actor would require multi-tenancy design).
- Predictive modeling of future behavior (only retrospective anomaly detection).
