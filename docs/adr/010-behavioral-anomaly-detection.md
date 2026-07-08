# ADR 010 — Behavioral Anomaly Detection

## Status

Accepted (Phase 1 V1 implemented)

## Context

The current governance model relies on static policy evaluation and explicit approval gating for R3 actions. It does not learn or detect unusual agency patterns that may indicate:
- A compromised agent suddenly requesting many R3 actions outside its historical baseline.
- An insider abusing a scoped token to execute actions at unusual times or frequencies.
- Prompt injection resulting in anomalous tool/action combinations that pass static policy but violate behavioral norms.

This gap was identified in the OWASP LLM06 mapping (Excessive Agency) and the threat model (B4 — anomalous agent behavior).

## Decision

Adopt a lightweight, opt-in behavioral anomaly detection layer that operates on proposal metadata without requiring external ML services. Phase 1 V1 is intentionally narrow: advisory-only high-risk/R3 burst detection per principal within a rolling window.

### 1. Detection scope (Phase 1 V1)
- **Actor-based**: per-principal rolling count of high-risk (`estimated_risk` = `High` or `Critical`) or R3 (`requested_rollback_class` = `R3IrreversibleHighConsequence`) proposals.
- **Advisory-only**: anomalies are emitted to the audit log, Prometheus metrics, and `PolicyEvaluated` provenance metadata; they do **not** alter the policy decision, auto-block, or create approvals/quarantine holds.
- **In-memory**: all state lives in a bounded per-principal event map; no persistence, migrations, or external dependency.

### 2. Architecture
- A `BehavioralProfiler` trait with a `inspect_proposal` method.
- `NoopBehavioralProfiler` returned when the feature is disabled, preserving the default no-op behavior.
- `ThresholdBehavioralProfiler` implementation that flags anomalies when the per-principal count of qualifying proposals in a rolling window exceeds configurable warning or critical thresholds.
- No external ML model or vector database.
- Anomaly events are emitted to the audit log (`AuditAction::BehavioralAnomaly`) and as Prometheus metrics (`ferrumgate_behavioral_anomaly_detected_total{severity="warning|critical"}`).

### 3. Operator action
- When an anomaly is detected, the system does **not** auto-block the action (to avoid false-positive denial of service).
- It emits:
  - A `behavioral_anomaly` audit entry with severity `warning` or `critical` and sanitized metadata (severity, window count, window seconds, threshold). No actor IDs or raw arguments are logged.
  - A Prometheus counter with bounded `severity` labels only (no actor IDs).
  - Sanitized `behavioral_anomaly` metadata in the `PolicyEvaluated` provenance event.

### 4. Privacy and performance
- All profiling data is derived from the proposal metadata already present on the request path; no new telemetry is collected.
- The profiler runs synchronously on the proposal evaluation hot path but is bounded by `max_actors` and per-actor window pruning; disabled by default.
- Statistical models are bounded in memory (`max_actors` + rolling window per actor).

## V1 limitations
- Single-signal only: high-risk/R3 burst rate. Time-of-day, action-sequence, and tool-diversity baselines are deferred.
- No cross-actor correlation or historical baselines; each actor is counted independently.
- No lifecycle-outbox escalation or approval requirements; the detector is advisory.
- Memory-only; state is lost on restart and is not replicated across instances.
- No ML or statistical standard-deviation modeling; thresholds are simple fixed counts.

## Consequences

- **Positive**: Adds a dynamic layer to complement static policy evaluation.
- **Positive**: No external dependencies; all data is local and in-memory.
- **Negative**: Simple count-based thresholds have higher false-positive rates than ML-based approaches; operator tuning is required.
- **Negative**: Adds small per-proposal CPU/memory cost when enabled.
- **Non-goal**: This is not a replacement for policy evaluation; it is an adjunct signal.

## Acceptance criteria

1. `BehavioralProfiler` trait is defined with `NoopBehavioralProfiler` and `ThresholdBehavioralProfiler` implementations.
2. `ThresholdBehavioralProfiler` computes rolling counts over a configurable window with warning and critical thresholds.
3. Anomaly events are written to the audit log with sanitized metadata (severity, window count, threshold); no actor IDs or raw arguments.
4. Prometheus metric `ferrumgate_behavioral_anomaly_detected_total` is emitted with a bounded `severity` label (no actor ID labels).
5. Configuration is validated at startup: `behavioral_anomaly_enabled`, `behavioral_anomaly_window_secs`, `behavioral_anomaly_warning_threshold`, `behavioral_anomaly_critical_threshold`, `behavioral_anomaly_max_actors`.
6. Default configuration disables the detector; enabled detector does not change policy decisions.
7. Integration/unit tests verify detection, per-actor isolation, pruning, memory bounds, and default disabled behavior.

## Non-goals

- Real-time ML inference or external model integration (out of scope; can be added via a future adapter).
- Auto-blocking of anomalous actions (advisory-only).
- Cross-actor correlation or graph analysis (single-actor baselines only; cross-actor would require multi-tenancy design).
- Predictive modeling of future behavior (only retrospective anomaly detection).
- Persistence, migrations, or outbox escalation.
