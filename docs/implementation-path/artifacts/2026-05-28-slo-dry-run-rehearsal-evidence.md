# SLO Sustained Dry-Run Rehearsal Evidence — 2026-05-28

> **Artifact ID**: 2026-05-28-slo-dry-run-rehearsal-evidence
> **Date**: 2026-05-28
> **Owner**: Engineering
> **Scope**: Bounded dry-run rehearsal of SLO observation procedure. Not sustained-window evidence. Not production SLO certification.
> **Parent**: [`docs/implementation-path/01-current-state.md`](../../implementation-path/01-current-state.md)

---

## 1. Non-Claims

| Claim | Status | Rationale |
|-------|--------|-----------|
| **Production-ready** | **NO** | Dry-run rehearsal only; no production traffic or real target-host sustained observation. |
| **Full G2 / operator signoff** | **NOT COMPLETE** | Engineering-owned rehearsal only. |
| **Sustained SLO window** | **NOT COMPLETE** | Rehearsal duration is ~4 minutes with 5 samples. A sustained window requires 7–30 days of observation. |
| **Canonical SLO certification** | **NOT CLAIMED** | This is a rehearsal of the observation tooling, not a canonical SLO workload run. |
| **Default-config SLO pass** | **NOT CLAIMED** | Rehearsal does not validate rate-limit configuration. |

---

## 2. Environment

| Field | Value |
|-------|-------|
| Date | 2026-05-28 |
| Start time | 2026-05-28T03:26:00Z |
| End time | 2026-05-28T03:29:59Z |
| Command | `make slo-sustained-dry-run` |
| Output directory | `/tmp/slo-obs-dryrun-20260528_032600` |
| Host scope | Local development workstation |

---

## 3. Procedure

1. Run `make slo-sustained-dry-run`.
2. The target executes a lightweight readiness probe loop against the local ferrumd instance.
3. Each sample records HTTP status, latency, and availability.
4. Results are written to the output directory with timestamped filenames.

---

## 4. Results

| Metric | Value |
|--------|-------|
| Samples | 5 |
| OK | 5 |
| Fail | 0 |
| Availability | 100.00% |
| Average latency | 42 ms |
| Duration | ~3 min 59 s |

---

## 5. Summary Status

```text
DRY-RUN / REHEARSAL — NOT VALID SLO EVIDENCE
```

This rehearsal validates that the observation scripts, output directory creation, and metrics collection pipeline function correctly. It does **not** constitute:
- A canonical SLO workload run (see `2026-05-21-canonical-slo-helm-conditional-signoff.md` for canonical certification).
- A sustained observation window (7-day or 30-day rolling).
- Target-host or production traffic validation.

---

## 6. Interpretation

- The `make slo-sustained-dry-run` target is operational and produces timestamped artifacts.
- All 5 samples passed with sub-50 ms latency, confirming the local observation pipeline is functional.
- To upgrade this to valid SLO evidence, an operator must:
  1. Run the same target against a production or staging target host.
  2. Observe for at least 7 consecutive days (pilot) or 30 days (production-candidate).
  3. Review the aggregated metrics and sign off on the evidence artifact.

---

## 7. Related Artifacts

- [`2026-05-21-canonical-slo-helm-conditional-signoff.md`](./2026-05-21-canonical-slo-helm-conditional-signoff.md) — canonical SLO certification (3 runs, max-valid PASS)
- [`2026-05-25-domainless-hardening-evidence.md`](./2026-05-25-domainless-hardening-evidence.md) — prior SLO rehearsal evidence
- [`docs/production-readiness-v2/01-slo-sla.md`](../../production-readiness-v2/01-slo-sla.md) — SLO/SLA draft
- [`docs/production-readiness-v2/slo-validation-runbook.md`](../../production-readiness-v2/slo-validation-runbook.md) — validation runbook

---

*Artifact created: 2026-05-28. SLO sustained dry-run rehearsal evidence. No production-ready claim.*
