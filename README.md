# FerrumGate Unified Project

FerrumGate la mot **intent-scoped reversible execution plane** cho MCP/tool-using agents.

Repository nay gom lai:
- Rust workspace
- docs nghiep vu va kien truc
- machine-readable contracts
- OpenAPI + JSON Schemas
- prompts cho AI agents
- configs, scripts, va integration tests
- roadmap va handoff notes

Muc tieu la de AI agents hoac engineers khac co the tiep tuc nang cap repo ma khong phai ghep lai tu nhieu bo roi rac.

## Bat dau tu dau

1. `docs/README.md`
2. `docs/00-project-canon.md`
3. `docs/01-quickstart.md`
4. `docs/04-runtime-flow.md`
5. `docs/06-constraints-and-invariants.md`
6. `docs/implementation-path/README.md`
7. `contracts/ferrumgate-agent-contract.v1.yaml`
8. `prompts/agent_system.md`

## Trang thai hien tai

Day khong con la repo skeleton thu gom tai lieu.

Supported SQLite-backed flow hien tai da co evidence cho:
- storage boundary cho intents/proposals/capabilities/executions/rollback/provenance
- gateway orchestration `evaluate -> mint -> prepare -> execute -> verify -> commit/rollback`
- durable capability persistence qua `SqliteCapabilityService`
- firewall MVP co trust/taint/sanitize/DLP va execution-time enforcement cho `File`/`Http`/`Sqlite`/`Git`/`EmailDraft`
- adapter-backed recovery evidence cho filesystem, sqlite, maildraft, git, va HTTP full-parity execute/verify
- docs/release/troubleshooting handoff cho supported flow hien tai

Repo van con open gaps, nhung chu yeu nam o tang hardening va nang cap tiep theo thay vi core flow MVP:
- generic provenance query/replay/graph tooling rong hon
- operator/runtime hardening cho non-loopback production-style rollout
- runtime integrations ben ngoai ma khong lam ro ri vendor vao core crates

## Core crates da co implementation y nghia
- `ferrum-proto`
- `ferrum-store`
- `ferrum-pdp`
- `ferrum-cap`
- `ferrum-firewall`
- `ferrum-rollback`
- `ferrum-gateway`
- `ferrumctl`
- adapter crates cho `fs`, `sqlite`, `maildraft`, `git`, `http`

## Nen doc tiep o dau neu muon hoan thien du an
- `docs/16-release-checklist.md`
- `docs/91-phase-success-criteria-and-kpis.md`
- `docs/implementation-path/01-current-state.md`
- `docs/implementation-path/08-next-issue-backlog.md`
- `docs/implementation-path/11-phase-f-evidence.md`

## Diem vao cho agent khac
- `docs/implementation-path/README.md`
- `docs/implementation-path/00-start-here.md`
