# HA Local ferrumd Reconnect Drill Evidence — 2026-05-26

> **Artifact ID**: 2026-05-26-ha-local-ferrumd-reconnect-evidence
> **Date**: 2026-05-26
> **Owner**: Engineering
> **Scope**: HA-B local ferrumd reconnect drill (app-level RTO measurement)
> **Constraint**: Local-only, Docker Compose, manual/optional. No production HA claim.

---

## 1. Summary

This artifact records the local HA ferrumd reconnect drill. The drill verifies that ferrumd can be restarted against a promoted standby PostgreSQL after primary failure, and measures the app-level RTO from primary stop to ferrumd ready on the standby.

---

## 2. Drill Procedure

1. Ensure HA local simulation is running (primary + standby streaming replication).
2. Build ferrumd binary (`cargo build --features postgres --package ferrumd`).
3. Start ferrumd against primary DSN (`localhost:5433`).
4. Verify `/v1/readyz/deep` returns 200.
5. Stop primary container (failure injection).
6. Promote standby via `pg_promote()`.
7. Wait for standby to exit recovery mode.
8. Stop ferrumd.
9. Restart ferrumd against standby DSN (`localhost:5434`).
10. Verify `/v1/readyz/deep` returns 200.
11. Verify `/v1/healthz` returns 200 (lightweight smoke).
12. Measure RTO from step 5 to step 10.

---

## 3. Evidence

| Check | Result |
|-------|--------|
| HA local simulation running | PASS |
| ferrumd binary available | PASS |
| ferrumd ready against primary | PASS |
| Primary stopped | PASS |
| Standby promoted | PASS |
| Standby exited recovery | PASS |
| ferrumd restarted against standby | PASS |
| ferrumd ready after reconnect | PASS |
| Lightweight smoke request passes | PASS |
| App-level RTO measured | PASS |

**RTO**: Measured from primary stop to ferrumd ready on standby. Typical local measurement: ~5–15 seconds (depends on Docker/container timings).

Latest verification run (`make ha-local-ferrumd-reconnect-drill`, 2026-05-26):

- Result: `HA LOCAL FERRUMD RECONNECT DRILL: ALL CHECKS PASSED`
- App-level RTO: `5 s`
- Post-reconnect `/v1/readyz/deep`: HTTP 200
- Post-reconnect `/v1/healthz`: HTTP 200

---

## 4. Boundary and Non-Claims

- **Local-only**: This drill runs on a single host using Docker Compose.
- **Manual promotion**: Standby is promoted manually via `pg_promote()`. No automated failover.
- **App-level RTO only**: Measures ferrumd restart + readyz probe, not end-to-end client request recovery.
- **No production HA claim**: This is procedure rehearsal, not evidence of production readiness.
- **No multi-host**: Both primary and standby run on the same Docker host.

---

## 5. Related Artifacts

- [`2026-05-26-ha-local-failover-simulation-evidence.md`](./2026-05-26-ha-local-failover-simulation-evidence.md) — HA local failover drill evidence.
- [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](../../production-readiness-v2/00a-domainless-readiness-tier.md) — Canonical three-tier model.

---

*Artifact created: 2026-05-26. HA local ferrumd reconnect drill evidence. No production-ready claim.*
