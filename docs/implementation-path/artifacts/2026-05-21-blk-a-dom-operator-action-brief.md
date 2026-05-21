# BLK-A-DOM Operator Action Brief — 2026-05-21

> **Status**: Operator action brief. Blocks production-ready and full G2 closure.
> **Owner**: Operator
> **Date**: 2026-05-21
> **Blocker ID**: `BLK-A-DOM`
> **Parent**: [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md)
> **Scope**: Real owned domain acquisition, DNS configuration, and revalidation.

---

## 1. Problem statement

FerrumGate v1 is currently accessible via a DuckDNS temporary domain (`ferrumgate.duckdns.org`). This was accepted as **WAIVED/CONDITIONAL** for the single-node SQLite pilot only. A real owned domain is still required before any production-ready claim or full G2 closure can be made.

**This blocker is operator-owned and cannot be resolved by engineering.**

## 2. Requirements

| Requirement | Detail | Acceptance criteria |
|-------------|--------|---------------------|
| **R1 — Domain ownership** | Operator procures a real domain name via any registrar. | Registrar receipt or WHOIS record shows operator as registrant. |
| **R2 — DNS A record** | A record for the chosen hostname points to `34.158.51.8`. | `dig +short <hostname>` returns `34.158.51.8` from multiple resolvers. |
| **R3 — HTTPS reachability** | Target host serves HTTPS 200 on the real domain. | `curl -sf https://<hostname>/v1/healthz` returns HTTP 200 with `{"status":"ok"}`. |
| **R4 — L1–L5 re-run** | Engineering re-runs target bridge L1–L5 against the real domain. | All L1–L5 checks pass; evidence artifact created. |
| **R5 — G2 re-signoff** | Operator re-signs G2.1–G2.8 with real domain evidence. | `54-operator-signoff-packet.md` updated with new evidence references. |

## 3. Exact steps

### Step 1 — Procure domain (operator)

1. Choose a domain registrar (e.g., Namecheap, Cloudflare Registrar, Google Domains).
2. Search for and purchase an available domain name.
3. Verify ownership via registrar dashboard or WHOIS.

**Evidence to capture**: Registrar receipt, WHOIS screenshot (redact PII if publishing).

### Step 2 — Configure DNS A record (operator)

1. Log in to the registrar's DNS management console.
2. Add an A record:
   - Name: `@` (apex) or chosen subdomain (e.g., `fg`)
   - Value: `34.158.51.8`
   - TTL: 300–3600 seconds
3. Wait for propagation (typically 5–60 minutes).
4. Verify from multiple locations:
   ```bash
   dig +short <hostname>
   nslookup <hostname> 8.8.8.8
   nslookup <hostname> 1.1.1.1
   ```

**Evidence to capture**: `dig` output from at least two resolvers.

### Step 3 — Verify HTTPS (operator + engineering)

1. Confirm the VM firewall allows HTTPS (port 443) to `34.158.51.8`.
2. If using a reverse proxy (nginx/Caddy), update its server_name to the new domain.
3. If using Let's Encrypt, trigger certificate issuance for the new domain.
4. Verify:
   ```bash
   curl -sf https://<hostname>/v1/healthz
   curl -sf https://<hostname>/v1/readyz/deep
   ```

**Evidence to capture**: `curl` output and HTTP status codes.

### Step 4 — Notify engineering (operator)

1. Send the new hostname to engineering.
2. Confirm the old DuckDNS domain may be deprecated or kept as fallback (operator decision).

### Step 5 — L1–L5 target bridge re-run (engineering)

1. Engineering updates `BASE_URL` in target scripts to the new domain.
2. Re-runs `check_pilot_readiness.py` against real domain.
3. Re-runs L1–L5 bridge checks (health, auth, readiness, metrics, functional).
4. Creates evidence artifact: `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-domain-evidence.md`.

**Evidence to capture**: Pass/fail log, latency samples, error counts.

### Step 6 — G2 re-signoff (operator)

1. Operator reviews the new evidence artifact.
2. Updates `54-operator-signoff-packet.md` with real domain evidence references.
3. Signs G2.1–G2.8 as **complete** (not conditional) if all checks pass.

**Evidence to capture**: Signed `54-operator-signoff-packet.md` or equivalent signoff artifact.

## 4. Evidence format

For each step, produce a dated markdown artifact in `docs/implementation-path/artifacts/`:

**Artifact 1 — Domain procurement evidence**
- Path: `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-domain-procurement-evidence.md`
- Content: Registrar name, domain name, purchase date, WHOIS snippet (redacted), receipt reference.

**Artifact 2 — DNS configuration evidence**
- Path: `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-dns-evidence.md`
- Content: A record screenshot or CLI output, `dig` results from ≥2 resolvers, propagation timestamp.

**Artifact 3 — HTTPS reachability evidence**
- Path: `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-https-evidence.md`
- Content: `curl` outputs for `/v1/healthz` and `/v1/readyz/deep`, TLS certificate info, any errors encountered.

**Artifact 4 — L1–L5 re-run evidence**
- Path: `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-closure-evidence.md`
- Content: Pre-run readiness, L1–L5 results, post-run readiness, anomalies, operator signoff block.

## 5. Consequences of non-completion

| Scenario | Impact |
|----------|--------|
| **Domain not procured** | `BLK-A-DOM` remains open indefinitely. FerrumGate cannot claim production-ready or full G2. |
| **DNS not configured correctly** | HTTPS may fail certificate validation; users see TLS errors. |
| **L1–L5 not re-run** | No evidence that the real domain path works end-to-end; prior DuckDNS evidence does not transfer. |
| **G2 not re-signed** | Full G2 closure remains incomplete; conditional pilot scope is the maximum claim. |
| **DuckDNS expires or is reclaimed** | If DuckDNS is lost before real domain is ready, target host becomes unreachable by domain name. |

## 6. Timeline decision point

| Milestone | Target date | Decision |
|-----------|-------------|----------|
| **D1 — Domain procured** | Operator decides | If deferred beyond 90 days, engineering should re-evaluate whether to maintain target-host infrastructure costs. |
| **D2 — DNS live** | Within 48h of D1 | If DNS fails to propagate, check registrar config and VM firewall rules. |
| **D3 — L1–L5 re-run** | Within 7 days of D2 | Engineering schedules re-run; if blocked by token or env issues, escalate to operator. |
| **D4 — G2 re-signoff** | Within 7 days of D3 | Operator reviews evidence and signs. If gaps found, return to engineering for re-run. |

**Fallback**: If the operator decides **not** to procure a real domain, document the decision in `docs/implementation-path/artifacts/YYYY-MM-DD-block-a-deferred-decision.md` and update `11-blockers-and-unblock-plan.md` to reflect permanent deferral. FerrumGate will remain at **conditional pilot / RC-ready** indefinitely.

## 7. Cross-references

| Document | Purpose |
|----------|---------|
| [`docs/production-readiness-v2/11-blockers-and-unblock-plan.md`](../../production-readiness-v2/11-blockers-and-unblock-plan.md) | Blocker tracking and operator decision packet |
| [`docs/production-readiness-v2/10-evidence-checklist.md`](../../production-readiness-v2/10-evidence-checklist.md) | Phase F final prerequisites |
| [`docs/implementation-path/54-operator-signoff-packet.md`](../../implementation-path/54-operator-signoff-packet.md) | G2 signoff form |
| [`docs/PRODUCTION_NOTES.md`](../../PRODUCTION_NOTES.md) | Deployment and DNS notes |
| [`docs/guides/operator.md`](../../guides/operator.md) | General operator procedures |

## 8. Non-claims

- **NOT an engineering task**: Domain procurement and DNS are operator responsibilities. Engineering cannot purchase domains on the operator's behalf.
- **NOT a production-ready claim**: This brief does not make FerrumGate production-ready.
- **NOT a guarantee of timeline**: Dates in §6 are suggestions; actual dates are operator-dependent.
- **NOT closing BLK-A-DOM**: This brief documents the path to closure; closure occurs only when R1–R5 are complete and evidence artifacts exist.
- **NOT a substitute for operator judgment**: The operator must evaluate domain choice, registrar trust, and DNS strategy independently.

---

*Brief created: 2026-05-21. BLK-A-DOM Operator Action Brief — planning artifact only.*
