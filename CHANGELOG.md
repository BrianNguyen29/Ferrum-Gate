# Changelog

**Repository**: `https://github.com/BrianNguyen29/Ferrum-Gate` | **Default version**: `0.1.0`

## v0.1.0

### Engineering Delta

- **MCP governance preview** — `crates/ferrum-integrations-mcp` local coverage hardened (239 tests)
- **Auth gate** — bearer-token auth enforced when auth_mode = "Bearer"; dev config remains auth-disabled for local development
- **Rate limiting** — configurable rate-limit middleware integrated with gateway
- **Local lifecycle/load smoke** — pre-target gate (`run_pre_target_gate.sh --full`) passes; local stress runner (`bins/ferrum-stress`) available
- **Architecture docs** — runtime configuration notes added to `docs/PRODUCTION_NOTES.md`
- **Scaffolds** — PostgreSQL/MCP bridge scaffolds present
- **Clippy cleanup** — resolved clippy warnings with behavior-neutral cleanup in `ferrum-gateway/src/server.rs` and `ferrum-integrations-mcp/src/lib.rs`

### Post-Release Operator Tooling

Added operator evidence/templates and bounded helper scripts:

- `scripts/check_pilot_readiness.py` — health endpoint probe helper
- `scripts/generate_evidence_skeleton.py` — command-output-to-markdown evidence skeleton helper
- `scripts/run_d1_d6_drills.py` — automated D1–D6 local evidence drill runner (bounded adapter-level tests, local/test-drill only, operator review required)
- `docs/guides/operator.md` — Operator procedures
- `configs/examples/*` — Operator-owned examples for backup scheduling and nginx TLS reverse proxy

### Summary of Changes

- Scope-mismatch deny implemented in PDP (`crates/ferrum-pdp/src/engine.rs:31-46`)
- Poisoned-context regression fixtures (6 tests), docs pack finalized, supported flows documented
- clippy clean (`cargo clippy --workspace --all-targets -- -D warnings` passes), evidence script present and passing, ~797 workspace tests pass

### Evidence Base

| Dimension | Result |
|-----------|--------|
| Validation | PASS — fresh validation 2026-04-28 |
| Invariant Matrix | 12 VERIFIED / 0 PARTIAL / 0 INFERRED |
| Runtime Configuration | `docs/PRODUCTION_NOTES.md` |

---
