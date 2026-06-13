# FerrumGate FAQ

Frequently asked questions about FerrumGate, its scope, and operator responsibilities.

---

## Is FerrumGate a managed service or SaaS?

No. It is open-source software you run in your own infrastructure. Single-tenant by design.

## Does FerrumGate send email?

No. The mail draft adapter manages draft create/update/delete with recipient and content binding. It does not send email.

## Is MCP HTTP/SSE supported?

stdio MCP is implemented and locally validated. Streamable HTTP / SSE and resumability are experimental and not yet production-ready.

## Does FerrumGate provide compliance certification (SOC 2, ISO 27001, etc.)?

No. FerrumGate provides audit-oriented provenance and evidence chains. Compliance certification is outside the scope of the open-source project.

## Is PostgreSQL production-HA out of the box?

PostgreSQL runtime is supported and CI live-tested. Production HA/multi-node topology, replication, and failover are operator responsibilities and are not managed by this repository.

## Can multiple tenants share one FerrumGate instance?

No. Multi-tenancy is not implemented; FerrumGate is single-tenant by design.

## What is the difference between `cargo run` and the release binary in the quickstart?

`cargo run` compiles and runs in debug mode — fine for local development. For production-like or pilot deployments, use `cargo build --release` and run the resulting `./target/release/ferrumd` binary.

## How do I report a security issue?

Please open a private security advisory via GitHub Security Advisories for this repository.

## What is the maximum capability TTL?

300 seconds (5 minutes), hardcoded in `ferrum-cap`. This is a safety limit to prevent long-lived ambient authority.

## Who owns TLS, secrets, and network policy?

The operator. FerrumGate handles intent-to-action binding, policy evaluation, capability minting, and provenance recording. TLS termination, secret management, and network policy are outside the gateway boundary.

## Who owns database HA and backup policy?

The operator. SQLite is single-node by design. PostgreSQL runtime is supported, but production HA/multi-node topology, replication, and failover are operator responsibilities.

## Are there performance benchmarks?

See [`docs/PRODUCTION_NOTES.md`](../PRODUCTION_NOTES.md) for stress-test baselines. Local validation highlights (release binary, post-write-queue):

| Scenario | Throughput | p50 Latency | Error Rate |
|----------|------------|-------------|------------|
| Health (50 workers) | ~33,000 req/s | 1.3ms | 0% |
| Execution pipeline (5 workers) | ~58 pipelines/s | 16ms | 0% |
| SQLite contention (50 workers) | ~289 req/s | 30ms | 0% |

> These are local engineering benchmarks, not production guarantees. Your results will depend on hardware, store choice, and workload shape.

---

For more questions, see the [Operator Guide](../guides/operator.md) or [Troubleshooting](../guides/troubleshooting.md).
