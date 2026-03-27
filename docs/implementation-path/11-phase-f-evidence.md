# 11 — Phase F evidence handoff

## 1. Scope of this handoff

Tai lieu nay khoa lai phan evidence/docs cho Phase F theo pham vi branch hien tai.

"Supported" trong tai lieu nay co nghia:
- da co test tu dong trong repo
- di qua governance chain cua gateway o muc repo hien tai
- co provenance/lineage evidence du de truy nguyen

"Supported" khong mac dinh co nghia:
- moi adapter/resource type trong project canon deu da duoc harden ngang nhau
- moi flow production/operator da san sang
- moi open gap trong release checklist da duoc dong

## 2. Evidence sources trong branch hien tai

- `tests/integration_gateway_flow.rs`: happy path, deny, quarantine, rollback, compensate, approval, draft-only, single-use capability, scope mismatch deny, execution binding enforcement cho ca 5 loai `File`/`Http`/`Sqlite`/`Git`/`EmailDraft`, va fs/sqlite/maildraft/git/http-full-parity adapter-backed recovery evidence.
- `tests/integration_poisoned_context.rs`: curated poisoned-context regression suite cho trust labeling, taint propagation, quarantine, read-only fail-closed, va MCP scope fail-closed.
- `tests/integration_lineage_chain.rs`: minimum lineage chain, persisted lineage edges, execution lineage endpoint, terminal rollback lineage.
- `docs/16-release-checklist.md`: release-facing readiness checklist.
- `docs/91-phase-success-criteria-and-kpis.md`: phase-level release gate va KPI snapshot.

## 3. Supported flows hien tai

### 3.1 Gateway governance flow co evidence ro

Nhung flow duoi day da co automated evidence trong repo:

- compile intent -> derive trust context -> persist intent/provenance
- evaluate proposal voi cac ket qua `Allow`, `Deny`, `Quarantine`, `RequireApproval`, `AllowDraftOnly`
- mint capability -> authorize execution -> prepare execution
- execute -> verify -> auto-commit cho flow `R0NativeReversible`
- execute -> verify -> explicit commit cho flow khong auto-commit (co bang chung truc tiep cho R2 va R3)
- rollback va compensate terminal recovery paths o muc gateway orchestration/state/provenance, va fs/sqlite/maildraft/git/http-full-parity adapter-backed recovery cho file create/delete + overwrite/restore row state + draft create/delete + git ref restore + HTTP GET/POST/PUT/PATCH/DELETE verify + rollback no-op recovery

### 3.2 Policy/firewall hardening da co evidence

- compile-time trust labeling va taint scoring
- read-only contradiction blocking
- MCP scope violation denial
- poisoned-context quarantine/deny behavior
- execution-time HTTP binding enforcement
- execution-time File binding enforcement
- execution-time Sqlite binding enforcement
- execution-time Git binding enforcement
- execution-time EmailDraft binding enforcement

### 3.3 Provenance / lineage evidence da co

- minimum lineage chain theo `docs/04-runtime-flow.md`
- `parent_edges` duoc persist vao `provenance_edges`
- lineage edge co the query lai qua store repo
- `GET /v1/provenance/lineage/{execution_id}` da reconstruct duoc execution lineage theo event graph da persist
- `POST /v1/provenance/query` fail-closed minimal endpoint da co; generic provenance query/replay fabric/graph tooling rong hon van la open gap
- terminal events cho commit va rollback da co integration evidence

### 3.4 Approval / draft-only scope hien tai

- approval allow path da duoc cover
- approval denial path da duoc cover
- draft-only dry-run success da duoc cover
- draft-only non-dry-run denial da duoc cover

## 4. Open gaps can giu ro trong handoff

Nhung muc duoi day van la open gap hoac gioi han evidence, khong nen bi hieu nham la "done":

- supported flow evidence hien tap trung vao gateway + store + firewall path duoc test trong repo; chua phai tuyen bo parity cho moi adapter/runtime ben ngoai
- lineage query da co muc toi thieu o muc execution lineage endpoint va provenance query fail-closed endpoint; generic provenance query, replay/query fabric, va graph tooling rong hon van la backlog
- adapter-backed rollback/compensate evidence hien da co truc tiep cho filesystem, sqlite, maildraft draft-only, git local-ref, va HTTP full-parity path (GET/POST/PUT/PATCH/DELETE execute/verify, body/header/query digest binding, dedicated auth); `EmailSend` va HTTP remote mutation recovery parity van chua duoc tuyen bo parity; LUU Y: `allow_send=true` EmailDraft bindings bay gio duoc explicitly denied tai gateway prepare-time (PolicyDenied 403), thay vi silently fall-through to noop nhu truoc do - day la improvement ve fail-closed semantics; HTTP rollback/compensate van conservative no-op.
- runtime config docs va CLI/debug flow toi thieu da co them mot nhip thuc dung; TLS termination van can external terminator (P1); capability persistence bay gio da duoc durable qua SQLite
- docs nay khong thay the backlog; cac nang cap tiep theo van nen tiep tuc track o `docs/implementation-path/08-next-issue-backlog.md`

## 5. Cach doc repo sau handoff nay

Neu can tiep tuc Phase F/C theo nhung gap con lai, thu tu doc nhanh nen la:

1. `docs/91-phase-success-criteria-and-kpis.md`
2. `docs/16-release-checklist.md`
3. `docs/implementation-path/08-next-issue-backlog.md`
4. `tests/integration_gateway_flow.rs`
5. `tests/integration_poisoned_context.rs`
6. `tests/integration_lineage_chain.rs`

## 6. Recommended next slices

See `23-production-readiness-assessment.md` for the full phased hardening plan. In brief:
- P1: Observability baseline (tracing + Prometheus), TLS/ingress runbook, operational runbook (backup/restore, capacity)
- P2: Advanced provenance replay/fabric tooling; complete Sync-3a.1 probe API boundary, begin Sync-1 implementation
- P3: Full MCP transport loop (future)
