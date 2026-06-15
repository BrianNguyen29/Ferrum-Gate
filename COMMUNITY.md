# Community

Ways to participate in FerrumGate without writing core Rust code.

## Feedback and questions

- **Bug reports** — Use the bug report issue template. Include reproduction steps, expected vs actual behavior, and environment details.
- **Adapter requests** — Use the adapter request template. Describe the use case, risk class, and rollback behavior you need.
- **Security concerns** — Use the security concern template or open a private GitHub Security Advisory.
- **Docs improvements** — Use the docs improvement template. Point to the file/section and suggest the change.

## Discussion venues

- GitHub Issues — for concrete, actionable items with templates above.
- GitHub Discussions — for general questions, ideas, and non-actionable conversation.

## Contribution norms

- See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for commit style, PR checklist, and conventions.
- See [`AGENTS.md`](./AGENTS.md) if you are an AI assistant working in this repository.
- Pick one crate or document boundary at a time.
- Do not change contracts/schemas without updating docs and tests.
- Preserve intent / capability / provenance / rollback invariants.

## Scope boundaries

FerrumGate is honest about what it does and does not provide:
- **Not** a managed service or SaaS.
- **Not** a turnkey HA product.
- **Not** compliance-certified (SOC 2, ISO 27001, etc.).
- **Not** multi-tenant.
- MCP HTTP/SSE/resumability is **not** production-ready.

## Related docs

- [`docs/ROADMAP.md`](./docs/ROADMAP.md) — Current and future priorities.
- [`docs/guides/faq.md`](./docs/guides/faq.md) — Frequently asked questions.
- [`SECURITY.md`](./SECURITY.md) — Security disclosure policy.
