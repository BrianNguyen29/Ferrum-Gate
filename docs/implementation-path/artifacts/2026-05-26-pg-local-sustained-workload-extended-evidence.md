# PG Local Sustained Workload Extended Evidence — 2026-05-26

> **Artifact ID**: 2026-05-26-pg-local-sustained-workload-extended-evidence
> **Date**: 2026-05-26
> **Owner**: Engineering
> **Scope**: Extended local PostgreSQL sustained workload drill (120s @ 1 rps)
> **Constraint**: Local-only, Docker Compose, manual/optional. No production-ready claim.

---

## 1. Summary

This artifact records the extended local PostgreSQL sustained workload drill. Compared with the default 30s @ 1 rps drill, this extended run uses 120s @ 1 rps (~120 requests) to provide additional local confidence in PostgreSQL-backed ferrumd stability under a modest sustained load.

---

## 2. Drill Configuration

| Parameter | Value |
|-----------|-------|
| Duration | 120 seconds |
| Rate | 1 request per second |
| Expected requests | ~120 |
| Adapter mix | fs / sqlite / maildraft (default offline-safe mix) |
| DSN | `postgres://ferrumgate_dev:ferrumgate_dev_password@localhost:5432/ferrumgate_p2_test` |

Environment override used:
```bash
SUSTAINED_PHASES='[{"name":"extended","duration_sec":120,"rate_rps":1.0}]'
```

---

## 3. Evidence

| Check | Result |
|-------|--------|
| PostgreSQL container healthy | PASS |
| ferrum-migrate binary available | PASS |
| ferrumd binary available | PASS |
| Synthetic SQLite fixture created | PASS |
| Migration 10/10 count+hash match | PASS |
| ferrumd readyz/deep 200 against PG | PASS |
| Workload generator completed | PASS |
| Post-workload readyz/deep 200 | PASS |
| Post-workload /v1/metrics returned body | PASS |
| PG pool metrics present in /v1/metrics | PASS |
| Workload results: no errors, all 2xx | PASS |

Latest verification run (`make pg-sustained-workload-extended`, 2026-05-26):

- Result: `PG SUSTAINED WORKLOAD DRILL: ALL CHECKS PASSED`
- Extended phase: `110` requests completed successfully (`110` HTTP 200 responses, `0` non-2xx responses, `0` errors)
- Post-workload `/v1/readyz/deep`: HTTP 200

---

## 4. Boundary and Non-Claims

- **Local-only**: Runs against local Docker PostgreSQL only.
- **Bounded runtime**: 120 seconds is still a bounded local run, not a sustained SLO observation window.
- **Light load**: 1 rps is a light local load, not representative of production traffic.
- **No production-ready claim**: This drill validates local runtime stability only.
- **No PostgreSQL production claim**: Production PG deployment remains deferred.

---

## 5. Related Artifacts

- [`2026-05-26-pg-local-sustained-workload-evidence.md`](./2026-05-26-pg-local-sustained-workload-evidence.md) — Default 30s sustained workload evidence.
- [`2026-05-26-pg-local-batch-timer-evidence.md`](./2026-05-26-pg-local-batch-timer-evidence.md) — Full pg-local-batch evidence.
- [`docs/production-readiness-v2/00a-domainless-readiness-tier.md`](../../production-readiness-v2/00a-domainless-readiness-tier.md) — Canonical three-tier model.

---

*Artifact created: 2026-05-26. Extended PG local sustained workload evidence. No production-ready claim.*
