# Artifact: 2026-05-17 Block A DuckDNS Conditional Pilot Waiver

> **Type**: Conditional waiver artifact (operator acceptance, not a readiness claim)
> **Date**: 2026-05-17
> **Scope**: DuckDNS domain use accepted for FerrumGate v1 conditional single-node SQLite pilot only
> **Status**: WAIVED / CONDITIONAL. No production-ready claim. No full G2 closure claim.
> **Secret handling**: No API keys, tokens, or credentials are recorded in this artifact.

---

## Summary

This artifact records the operator's formal acceptance of a **conditional waiver** for Block A (real owned domain) for the FerrumGate v1 single-node SQLite pilot. The pilot will use the existing DuckDNS endpoint (`ferrumgate.duckdns.org`) as the public-facing domain for the duration of the conditional pilot only.

**Block A = WAIVED / CONDITIONAL for single-node SQLite pilot only.**

A **real owned domain remains required for production-ready posture or full G2 closure**.

---

## Operator Acceptance

The operator explicitly accepts the following conditions for the conditional pilot:

1. **DuckDNS is not a production-owned domain**. It is a free dynamic DNS service with no SLA, no ownership guarantees, and no support contract.
2. **The pilot scope is strictly bounded**: single-node SQLite deployment only. No multi-node, no PostgreSQL, no HA.
3. **Production-ready remains NO** for the duration of this waiver.
4. **Full G2 operator signoff remains incomplete** until a real owned domain is procured and configured.
5. **Risk is bounded**: the pilot is exploratory/validation only; no production workloads or sensitive data should be processed through the DuckDNS endpoint.

### Operator Acknowledgment

Operator acknowledges and accepts the DuckDNS conditional pilot waiver terms above.

| Field | Value |
|-------|-------|
| Operator | BrianNguyen |
| Acknowledgment date | 2026-05-17 |
| Scope | FerrumGate v1 conditional single-node SQLite pilot only |
| Accepted status | Block A = WAIVED / CONDITIONAL, not CLOSED |
| Explicit non-claim | `production-ready = NO`; full G2 closure remains incomplete |

---

## Scope

| Item | Status | Bound |
|------|--------|-------|
| DuckDNS endpoint (`ferrumgate.duckdns.org`) as public-facing domain | **Accepted for pilot** | Single-node SQLite pilot only |
| HTTPS/TLS via Caddy + Let's Encrypt on DuckDNS | **Accepted for pilot** | Valid while DuckDNS record persists |
| G-A1/G-A2/G-A3 evidence against DuckDNS | **Accepted as proxy evidence** | Real domain evidence still required for full closure |
| Live readiness probes against DuckDNS | **Accepted for pilot** | `check_pilot_readiness.py` shallow/deep/metrics PASS recorded |

---

## Non-Goals (Explicitly Excluded)

| Claim | Status | Rationale |
|-------|--------|-----------|
| Production-ready | **NO** | DuckDNS waiver does not make FerrumGate production-ready. |
| Full G2 closure | **NO** | G2.1–G2.8 are signed for conditional pilot only; full G2 requires a real owned domain. |
| Real domain procurement | **Deferred** | Operator will procure a real owned domain post-pilot or before declaring production-ready. |
| Multi-node / PostgreSQL / HA | **NO** | Out of v1 scope regardless of domain status. |
| DuckDNS as long-term production domain | **NO** | DuckDNS is accepted only for the bounded conditional pilot period. |

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| DuckDNS record expires or is reclaimed | Low | High (loss of public endpoint) | Operator monitors record; migration runbook to real domain is ready (`scripts/gcp/phase3g_configure_real_domain.sh`) |
| Let's Encrypt rate limits on DuckDNS | Low | Medium | Caddy handles auto-renewal; rate limits are per-domain and unlikely for low-traffic pilot |
| External dependency (DuckDNS service outage) | Low | Medium | Pilot is non-critical; local IP (`34.158.51.8`) remains reachable directly if DNS fails |
| DNS hijacking / typo-squatting on DuckDNS subdomain | Very Low | Medium | Subdomain is specific (`ferrumgate`); no sensitive data in pilot; TLS certificate pinning adds layer |
| Pilot misinterpreted as production-ready | Low (process) | High (reputational/operational) | Explicit waiver artifact; status docs show `production-ready = NO`; no G2 completion claimed |

---

## Evidence Boundaries

### What This Waiver Supports

- Block A is **WAIVED / CONDITIONAL** for the single-node SQLite pilot.
- Safe probes executed against `ferrumgate.duckdns.org` (shallow/deep/metrics PASS) are accepted as proxy readiness evidence.
- The pilot may proceed with DuckDNS as the public endpoint under bounded scope.
- All other blocks (B, C) remain CLOSED.

### What This Waiver Does NOT Support

| Claim | Status | Rationale |
|-------|--------|-----------|
| Production-ready | **NO** | DuckDNS is not a production domain; production-ready requires real owned domain + full G2. |
| Full G2 operator signoff | **NO** | G2 is conditional/signed for pilot only; full closure requires real domain evidence (G-A1/G-A2/G-A3). |
| Long-term DuckDNS use | **NO** | Waiver is conditional and time-bounded to the pilot phase. |
| HA / multi-node / PostgreSQL | **NO** | Out of v1 scope independent of domain status. |
| Block A CLOSED | **NO** | Block A is **WAIVED / CONDITIONAL**, not CLOSED. Real domain closure remains pending. |

---

## Status Summary

| Block | Status | Rationale |
|-------|--------|-----------|
| **Block A — Real owned domain** | **WAIVED / CONDITIONAL** | DuckDNS accepted by operator on 2026-05-17 for single-node SQLite pilot only; real domain still required for production-ready or full G2 closure |
| **Block B — Off-VM alerting** | **CLOSED** | G-B1/G-B2/G-B3/G-B4 all satisfied and acknowledged |
| **Block C — Keyless backup** | **CLOSED** | C1 verified; residual key removed; offsite sync confirmed |
| **Production-ready** | **NO** | Block A is waived, not closed; full G2 incomplete; DuckDNS not production-grade |

---

## Required Future Action to Close Block A

To move Block A from **WAIVED / CONDITIONAL** to **CLOSED**, the operator must:

1. Procure a real owned domain.
2. Configure a DNS A record pointing to `34.158.51.8`.
3. Execute the Block A runbook: `bash scripts/gcp/phase3g_configure_real_domain.sh --confirm ...`
4. Produce pass evidence for G-A1, G-A2, and G-A3 against the real domain.
5. Re-sign or refresh the operator signoff packet (`54-operator-signoff-packet.md`) with real domain closure date.

---

## Cross-References

| Document | Purpose |
|----------|---------|
| `docs/implementation-path/01-current-state.md` | Canonical current state and blocker status |
| `docs/implementation-path/67-production-readiness-roadmap.md` | Production readiness roadmap and blocker details |
| `docs/implementation-path/122-completion-roadmap-and-hardening-tracker.md` | Completion tracker and 10-item follow-up list |
| `docs/implementation-path/artifacts/2026-05-17-all-paths-execution-evidence.md` | Path 1/2/3 execution evidence (safe probes against DuckDNS) |
| `docs/implementation-path/artifacts/2026-05-17-bridge-to-live-runbook.md` | L1–L5 live gate runbook (safe-by-default) |
| `docs/implementation-path/artifacts/2026-05-17-bridge-l0-preflight-evidence.md` | Bridge L0 pre-flight evidence packet |
| `docs/implementation-path/artifacts/2026-05-17-operator-unblock-packet.md` | Operator unblock packet with Block A procedure |
| `docs/implementation-path/artifacts/2026-05-17-escalation-matrix-acknowledgment.md` | Block B escalation matrix acknowledgment |
| `docs/implementation-path/artifacts/2026-05-17-sendgrid-rotation-evidence.md` | Block B SendGrid rotation evidence |
| `docs/implementation-path/artifacts/2026-05-15-r4-production-blocker-execution-runbook.md` | Exact command sequences for Blocks A/B/C |

---

## Document History

| Date | Change | Author |
|---|---|---|
| 2026-05-17 | Initial DuckDNS conditional pilot waiver created and operator acknowledgment recorded | Engineering |

---

*Artifact created: 2026-05-17. Conditional waiver — no production-ready claim, no full G2 closure claim, no secrets.*
