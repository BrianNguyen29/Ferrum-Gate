# Security Model Operator Decisions — Phase 4

> **Artifact ID**: 2026-05-20-security-model-operator-decisions
> **Date**: 2026-05-20
> **Owner**: Operator + Engineering
> **Scope**: Unblocks BLK-SEC-PH4 implementation planning. This is not production-ready evidence.

---

## Authorization source

The operator authorized engineering to proceed with feasible remaining work in the current session.
This artifact records Phase 4 decisions using the default recommendations from
`docs/production-readiness-v2/16-operator-shortcut-decision-packet.md`.

No bearer tokens, credentials, domains, service account keys, or other secrets are recorded here.

## Decisions

| # | Question | Decision | Notes |
|---|----------|----------|-------|
| Q1 | Which tenant model for first production posture? | **Option 1 — Single-tenant production** | One deployment = one tenant. No `tenant_id` schema migration in the first implementation. |
| Q2 | Is OIDC/JWT/SSO required for first production posture? | **Deferred to later phase** | Implement opaque scoped bearer tokens first; OIDC/JWT may be added as a later auth mode. |
| Q3 | Which RBAC roles should be enabled first? | **Full set** | `admin`, `operator`, `policy_author`, `auditor`, `agent`, `read_only`. |
| Q4 | Token revocation durability? | **Durable — store-backed `revoked_at`** | Revocation must survive process restart and be auditable. |
| Q5 | Maximum service-account token TTL? | **90 days** | Human/operator tokens should use shorter TTLs when practical; 90 days is a max, not a default for every token. |
| Q6 | Approve scoped token model and scope list? | **Approve as-is** | Uses `12-endpoint-to-scope-mapping.md` as the implementation contract. |

## Implementation authorization

Engineering may proceed with Phase 4 implementation bounded to:

1. Single-tenant scoped opaque bearer tokens.
2. Store-backed token records with hashed token material and durable revocation.
3. RBAC middleware enforcing the approved endpoint-to-scope mapping.
4. Admin token lifecycle APIs required by `13-token-api-contract.md`.
5. `ferrumctl admin tokens` CLI required by `14-ferrumctl-admin-tokens-cli-spec.md`.
6. Tests for SEC-1 through SEC-6 and UX-4 acceptance.

Engineering must not implement:

- Multi-tenant row-level isolation.
- PostgreSQL RLS.
- OIDC/JWT/SSO.
- Any token value logging or token value persistence in plaintext.
- Any production-ready/full-G2 claim.

## Signoff

| Field | Value |
|-------|-------|
| Operator decision | Approved defaults Q1–Q6 for bounded Phase 4 implementation |
| Operator authorization | Current-session authorization from user/operator |
| Engineering lead | AI engineering orchestrator |
| Date | 2026-05-20 |

## Non-claims

- **NOT production-ready**: These decisions authorize implementation only.
- **NOT full G2 closure**: Full G2 remains blocked by real domain, target evidence, and final signoff.
- **NOT multi-tenant**: Single-tenant is explicitly selected for the first production posture.
- **NOT OIDC/JWT**: OIDC/JWT/SSO is deferred.

---

*End of artifact — Phase 4 operator decisions.*
