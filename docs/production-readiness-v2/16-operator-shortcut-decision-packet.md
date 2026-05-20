# 16 — Operator Shortcut Decision Packet

> **Status**: Planning artifact. Condensed decision packet for operator review. No code changes.
> **Owner**: Engineering + Operator
> **Last updated**: 2026-05-20
> **Parent**: [`docs/ROADMAP.md`](../../ROADMAP.md)
> **Scope**: [`00-scope-and-nonclaims.md`](00-scope-and-nonclaims.md)
> **Depends on**: [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md), [`11-blockers-and-unblock-plan.md`](11-blockers-and-unblock-plan.md), [`15-revocation-durability-tradeoff.md`](15-revocation-durability-tradeoff.md)

---

## Goal

Provide a single-page decision packet that the operator can review, fill out, and sign to unblock BLK-SEC-PH4. This packet condenses the Phase 4 security model questions (Q1–Q6) from [`11-blockers-and-unblock-plan.md`](11-blockers-and-unblock-plan.md) into an actionable form with context and recommendations.

## How to use this packet

1. Engineering fills the **Context** and **Recommendation** columns.
2. Operator reviews each question, selects an option, and records the decision.
3. Operator signs and dates the packet.
4. The completed packet is stored in `docs/implementation-path/artifacts/` as evidence.
5. Engineering begins Phase 4 implementation only after the signed packet is received.

---

## Decision table

### Tenant / OIDC / scoped token decisions (unblocks BLK-SEC-PH4)

| # | Question | Context | Options | Engineering recommendation | Operator decision |
|---|----------|---------|---------|---------------------------|-------------------|
| **Q1** | Which tenant model for first production posture? | Single-tenant (T1) means one deployment = one tenant. No `tenant_id` in schema yet. Row-level and RLS options require large migrations. | ☐ Option 1 — Single-tenant production (one deployment = one tenant) <br> ☐ Option 2 — Row-level `tenant_id` in every table <br> ☐ Option 3 — PostgreSQL RLS | **Option 1** — minimal code change, fits self-hosted, defers SaaS complexity. T2–T5 can follow later without breaking T1. | ☐ |
| **Q2** | Is OIDC/JWT/SSO required for the first production posture, or can it be deferred? | OIDC adds significant integration work (provider config, callback handling, JWT validation, session management). | ☐ Required now <br> ☐ Deferred to later phase | **Deferred** — bearer + scoped tokens first; OIDC later as an additional auth mode. | ☐ |
| **Q3** | Which RBAC roles should be enabled in the first implementation? | The role set in `04-security-tenant-model-adr.md` is already minimal viable. Removing roles does not reduce implementation cost significantly. | ☐ Full set (admin, operator, policy_author, auditor, agent, read_only) <br> ☐ Subset (specify below) | **Full set** — the scope set is minimal viable; subsetting does not simplify the middleware. | ☐ |
| **Q4** | Should token revocation be immediate (in-memory) or durable (store-backed)? | See [`15-revocation-durability-tradeoff.md`](15-revocation-durability-tradeoff.md) for full comparison. Immediate is faster but loses revocations on restart. Durable is slightly slower but survives restart and provides audit trail. | ☐ Immediate (in-memory deny list) <br> ☐ Durable (store-backed `revoked_at`) <br> ☐ Hybrid (store + cache; see tradeoff doc) | **Durable** — the schema already has `revoked_at`; store lookup cost is negligible for pilot volume; no restart edge cases. | ☐ |
| **Q5** | What is the maximum token TTL acceptable for service-account tokens? | Human operator tokens should be short (hours–days). Service-account tokens may be longer. The max TTL is enforced at creation time. | ☐ 24 hours <br> ☐ 7 days <br> ☐ 30 days <br> ☐ 90 days <br> ☐ Other: _______ | **90 days** with mandatory rotation reminder. Allows service accounts without frequent manual intervention. | ☐ |
| **Q6** | Do you approve the scoped token model and scope list in `04-security-tenant-model-adr.md` §Scopes and `12-endpoint-to-scope-mapping.md`? | The scope list and endpoint mapping have been reviewed by engineering. Changes here affect middleware design and CLI spec. | ☐ Approve as-is <br> ☐ Approve with changes (describe below) <br> ☐ Request changes before approval | **Approve as-is** — the scope set is minimal and covers all current endpoints. | ☐ |

### Operator notes / change requests

If you selected "Approve with changes" or "Request changes" for any question, describe them here:

```







```

---

## Signoff block

By signing below, the operator confirms:
1. All questions have been reviewed.
2. Decisions are recorded in the table above.
3. Engineering may proceed with Phase 4 implementation based on these decisions.
4. Changes to these decisions after signoff may require re-scoping engineering work.

| Field | Value |
|-------|-------|
| **Operator name** | |
| **Date** | |
| **Signature** | |

## Engineering acknowledgment

By signing below, engineering confirms:
1. The decisions above will be implemented as specified.
2. If a decision requires clarification, engineering will request it before proceeding.
3. Implementation will not begin until this packet is signed by both parties.

| Field | Value |
|-------|-------|
| **Engineering lead** | |
| **Date** | |
| **Signature** | |

## Evidence artifact path

When complete, store this packet at:

```
docs/implementation-path/artifacts/YYYY-MM-DD-security-model-operator-decisions.md
```

*(Replace `YYYY-MM-DD` with the actual signoff date.)*

## Non-claims

- **NOT a contract**: This is a planning artifact. It records intent, not a legal agreement.
- **NOT immutable**: Changes after signoff are possible but may require re-scoping.
- **NOT production-ready**: This packet unblocks design implementation; it does not certify production readiness.
- **NOT multi-tenant**: Q1 default is single-tenant (T1).

## Related docs

- [`04-security-tenant-model-adr.md`](04-security-tenant-model-adr.md) — Security and tenant model ADR
- [`11-blockers-and-unblock-plan.md`](11-blockers-and-unblock-plan.md) — Full blockers and unblock plan (source of Q1–Q6)
- [`12-endpoint-to-scope-mapping.md`](12-endpoint-to-scope-mapping.md) — Endpoint-to-scope mapping (supports Q6)
- [`15-revocation-durability-tradeoff.md`](15-revocation-durability-tradeoff.md) — Revocation durability tradeoff (supports Q4)

---

*End of file — Operator Shortcut Decision Packet (planning artifact only).*
