# Implementation path

Thư mục này là điểm vào duy nhất để agent khác tiếp tục triển khai.

## Đọc theo thứ tự
1. `00-start-here.md`
2. `01-current-state.md`
3. `02-execution-order.md`
4. `03-phase-plan.md`
5. `04-crate-by-crate-tasks.md`
6. `05-done-criteria.md`
7. `06-guardrails-and-invariants.md`
8. `07-agent-handoff-prompt.md`
9. `08-next-issue-backlog.md`
10. `09-phase-checklists.md`
11. `10-crate-dependency-map.md`
12. `11-remaining-tasks.md`
12. `12-sync-3a-probe-api-boundary.md`
12a. `12a-sync-3a1-read-only-transport-probe.md`
15. `15-ferrumctl-more-useful-execution-plan.md`
23. `23-production-readiness-assessment.md`
25. `25-EV-v1-single-node-rc-evidence.md`
26. [26-EV-v1-single-node-invariant-control-test-evidence-matrix.md](26-EV-v1-single-node-invariant-control-test-evidence-matrix.md)
27. [27-production-evaluation-plan.md](27-production-evaluation-plan.md)
30. `30-production-roadmap.md`
31. [31-release-paths-todo.md](31-release-paths-todo.md)
32. `32-feature-completeness-audit.md`
33. `33-feature-completion-backlog.md`
44. `44-v1-review-readiness-template.md`
45. [45-current-feature-audit.md](45-current-feature-audit.md)
47. `47-novelty-roadmap.md`
51. `51-d5-bottleneck-analysis-report.md`
52. `52-d6-priority-expansion-list.md`
53. `53-rc-tag-checklist.md`
54. `54-operator-signoff-packet.md`
55. `55-phase-3-go-no-go-review.md`
56. [56-adapter-compensation-evidence-matrix.md](56-adapter-compensation-evidence-matrix.md)
57. [57-workload-compensation-drill-plan.md](57-workload-compensation-drill-plan.md)
58. [58-workload-compensation-drill-evidence-template.md](58-workload-compensation-drill-evidence-template.md)
59. [59-pilot-readiness-evidence-packet.md](59-pilot-readiness-evidence-packet.md)
60. [60-bounded-hardening-examples.md](60-bounded-hardening-examples.md)
61. [61-path-2-execution-plan.md](61-path-2-execution-plan.md)
62. [62-path-2-operator-runbook.md](62-path-2-operator-runbook.md)
63. [63-path-2-target-environment-spec.md](63-path-2-target-environment-spec.md)
64. [64-local-staging-simulation-guide.md](64-local-staging-simulation-guide.md)
65. `65-path-2-target-questionnaire.md`
66. [66-path-2-operator-handoff.md](66-path-2-operator-handoff.md)
67. [67-production-readiness-roadmap.md](67-production-readiness-roadmap.md)
68. [68-path-2-operator-handoff-packet.md](68-path-2-operator-handoff-packet.md) — **concise operator quick-reference**
69. [69-local-dummy-target-values.md](69-local-dummy-target-values.md) — **LOCAL-TEST ONLY**: dummy values for rehearsal (NOT operator evidence)
69a. [path2-dummy-rehearsal-bundle/](path2-dummy-rehearsal-bundle/) — **LOCAL-TEST ONLY**: dummy rehearsal bundle template + `scripts/run_dummy_path2_rehearsal.sh` orchestration (NOT operator evidence; does NOT modify canonical docs 54/58/59/63/65)
70. [70-security-hardening-local-only-plan.md](70-security-hardening-local-only-plan.md) — Security hardening proposals, local-only audit commands, token rotation procedure
70a. [artifacts/2026-05-08-local-baseline.md](artifacts/2026-05-08-local-baseline.md) — **LOCAL ONLY**: full repo-side baseline after hygiene/doctest fixes (NOT target evidence; G2 remains pending)
70a.1. [artifacts/2026-05-08-dummy-values-and-mcp-validation.md](artifacts/2026-05-08-dummy-values-and-mcp-validation.md) — **LOCAL-TEST/DUMMY ONLY**: fresh dummy Path 2 rehearsal + MCP smoke validation (NOT target evidence; G2 remains pending)
70a.2. [artifacts/2026-05-08-sqlite3-enabled-dummy-rehearsal.md](artifacts/2026-05-08-sqlite3-enabled-dummy-rehearsal.md) — **LOCAL-TEST/DUMMY ONLY**: sqlite3-enabled dummy rehearsal with restore data comparison pass (NOT target evidence; G2 remains pending)
70b. [71-path-2-target-values-intake-packet.md](71-path-2-target-values-intake-packet.md) — **INTAKE ONLY**: concise operator target-values checklist before filling docs 63/65 (NOT evidence; NOT signoff)
71. [71-mcp-server-feasibility-and-design.md](71-mcp-server-feasibility-and-design.md) — MCP server design and todo-list (post-v1 scope; v1.4 MCP Governance Beta)
72. [72-mcp-server-phase-a-implementation-plan.md](72-mcp-server-phase-a-implementation-plan.md) — Phase A–C implementation plan/tracker: crate skeleton + stdio transport (Phases A, B, C complete; D-0 ready to implement; D-1 deferred)
73. [73-mcp-server-phase-d-implementation-plan.md](73-mcp-server-phase-d-implementation-plan.md) — Phase D-0 read-only REST client plan (9 tools mapped to gateway REST routes) + D-1 deferred governance pipeline
74. [74-mcp-server-phase-d1-governance-design.md](74-mcp-server-phase-d1-governance-design.md) — Phase D-1 governance pipeline design: auth, policy eval, capability, rollback, provenance (design complete; implementation deferred)
75. [75-mcp-server-phase-d1-stage2-governance-pipeline-plan.md](75-mcp-server-phase-d1-stage2-governance-pipeline-plan.md) — Phase D-1 Stage 2 plan: endpoint/DTO map, sequential ID flow, provenance strategy, blockers (implementation GATED pending design review)
76. [76-mcp-server-d1-action-proposal-mapping-design.md](76-mcp-server-d1-action-proposal-mapping-design.md) — D-1.3.2 ActionProposal mapping design: field mapping chain, missing-field derivation rules, B-MAP-1..B-MAP-7 blockers, sequential ID correction (implementation BLOCKED)
77. [77-mcp-server-d1-3-2a-pure-mapping-helpers-plan.md](77-mcp-server-d1-3-2a-pure-mapping-helpers-plan.md) — D-1.3.2a pure mapping helpers plan: allowed/forbidden boundaries, helper function signatures, TODO marker policy, test plan (implemented and verified; D1.3.2b remains gated)
78. [78-mcp-server-d1-3-2b-mapping-completion-review.md](78-mcp-server-d1-3-2b-mapping-completion-review.md) — D-1.3.2b mapping-completion review packet: reconcile doc/code drift, decide C-E6/C-RISK/C-RB/C-PRIN/C-RAW, keep REST/mutating execution blocked
60. 79. [79-mcp-server-d1-3-3-preflight.md](79-mcp-server-d1-3-3-preflight.md) — D-1.3.3 preflight packet: P1-P4 blockers for side-effecting REST wiring gate (BLOCKED until P1-P4 approved)
61. [80-mcp-server-d1-3-4-evaluate-preflight.md](80-mcp-server-d1-3-4-evaluate-preflight.md) — D-1.3.4 evaluate-only gate: E1-E2 approved, low-level HTTP client implemented (tool dispatch and D1.4+ remain blocked)
62. [81-mcp-server-d1-4-capability-authorize-preflight.md](81-mcp-server-d1-4-capability-authorize-preflight.md) — D-1.4 capability/authorize: I5-I7 approved, low-level mint/authorize HTTP client implemented; tool dispatch and D1.5+ remain blocked
63. [82-mcp-server-d1-5-prepare-rollback-preflight.md](82-mcp-server-d1-5-prepare-rollback-preflight.md) — D-1.5 prepare/rollback: low-level HTTP client implemented; execute/verify/compensate and D1.6+ remain blocked
64. [83-mcp-server-d1-6-execute-verify-compensate-preflight.md](83-mcp-server-d1-6-execute-verify-compensate-preflight.md) — D-1.6 execute/verify/compensate: low-level REST clients implemented and auto_commit verify risk resolved; tool dispatch and D1.7+ remain blocked
65. [84-mcp-server-d1-7-tool-dispatch-preflight.md](84-mcp-server-d1-7-tool-dispatch-preflight.md) — D1.7 tool dispatch: 8 lifecycle tools wired (submit, evaluate, mint, authorize, prepare, execute, verify, compensate); historical pre-approve/reject gate superseded by doc90/doc91
66. [85-mcp-server-d1-8-output-sanitization-preflight.md](85-mcp-server-d1-8-output-sanitization-preflight.md) — D1.8 output sanitization preflight: firewall gap analysis, single-point choke point, sensitive fields, no-over-redaction, implementation options A+C (oracle-approved 2026-05-07)
67. [86-mcp-server-d1-9-dlp-field-redaction-preflight.md](86-mcp-server-d1-9-dlp-field-redaction-preflight.md) — D1.9 DLP/field redaction preflight: dlp_findings stub, sanitize_output control-char-only, FirewallContext gap, implementation options A–E, Explorer recommends Option B targeting raw_arguments/metadata first (Phase 1 + Phase 2 implemented)
68. [87-mcp-server-d1-9-3-dlp-integration-validation.md](87-mcp-server-d1-9-3-dlp-integration-validation.md) — D1.9.3 integration validation: mockito-based E2E tests through handle_tools_call_with_client choke point, Phase 1+2 key redaction, path-aware compensation_plan[].args, no-over-redaction, large nested response guard
68a. [88-mcp-server-d1-10-full-pipeline-validation.md](88-mcp-server-d1-10-full-pipeline-validation.md) — D1.10 full-pipeline validation: mockito-based sequential 8-step lifecycle test through handle_tools_call_with_client, ID chaining, D1.8/D1.9 redaction inheritance, no-over-redaction
69. [89-mcp-server-d1-11-live-local-smoke.md](89-mcp-server-d1-11-live-local-smoke.md) — D1.11 live-local smoke: bounded lifecycle dispatch checks (submit/evaluate/mint/list) in run_mcp_lifecycle_smoke.sh, soft-pass semantics for gateway errors, non-claims
70. [90-mcp-approve-reject-enable-plan.md](90-mcp-approve-reject-enable-plan.md) — MCP approve/reject enablement: implemented locally; smoke 15/15; G2/target evidence/pilot/signoff not claimed
71. [91-proposal-todo-status-after-mcp-approve-reject.md](91-proposal-todo-status-after-mcp-approve-reject.md) — Proposal todo/status after approve/reject enablement: completed/deferred/blocked record
72. [92-path-2-target-intake-next-actions.md](92-path-2-target-intake-next-actions.md) — **Recommended next action**: Path 2 target intake actionable plan; phases A-F with owners, blockers, stop conditions
71a. [artifacts/2026-05-08-mcp-approve-reject-smoke-15-15.md](artifacts/2026-05-08-mcp-approve-reject-smoke-15-15.md) — **LOCAL ONLY**: post-approve/reject MCP smoke 15/15 (NOT target evidence; G2 remains pending)
71b. [artifacts/2026-05-08-path2-phase-a-pre-target-gate.md](artifacts/2026-05-08-path2-phase-a-pre-target-gate.md) — **LOCAL ONLY**: Path 2 Phase A pre-target gate pass (NOT target evidence; G2 remains pending)
71c. [93-local-path2-target-profile-plan.md](93-local-path2-target-profile-plan.md) — **LOCAL ONLY / CI evidence**: local Path 2 target profile alternative when real target values unavailable; script + documentation + [CI workflow](../../.github/workflows/local-profile-evidence.yml) (NOT G2/production/target evidence)
71d. [artifacts/2026-05-08-local-path2-target-profile.md](artifacts/2026-05-08-local-path2-target-profile.md) — **LOCAL ONLY**: local target profile run result (NOT target evidence; G2 remains pending)
82. [94-gcp-compute-phase3a-nonprod-target-plan.md](94-gcp-compute-phase3a-nonprod-target-plan.md) — **GCP ONLY / NON-PROD**: Phase 3A operator-owned GCP Compute non-prod target plan; scripts under `scripts/gcp/`; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
82a. [artifacts/2026-05-08-gcp-phase3a-nonprod-target.md](artifacts/2026-05-08-gcp-phase3a-nonprod-target.md) — **GCP ONLY / NON-PROD**: Phase 3A GCP Compute run result; VM/service/probes/auth/backup verified; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
83. [95-gcp-compute-phase3b-domain-tls-plan.md](95-gcp-compute-phase3b-domain-tls-plan.md) — **GCP ONLY / NON-PROD**: Phase 3B operator-owned GCP TLS/nip.io/Caddy plan; scripts `phase3b_configure_tls_caddy.sh`, `phase3b_destroy_tls_caddy.sh`; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff; nip.io temporary only
83a. [artifacts/2026-05-08-gcp-phase3b-domain-tls.md](artifacts/2026-05-08-gcp-phase3b-domain-tls.md) — **GCP ONLY / NON-PROD**: Phase 3B TLS run result; Caddy HTTPS via temporary `34-158-51-8.nip.io`; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
84. [96-gcp-compute-phase3c-live-ops-packet.md](96-gcp-compute-phase3c-live-ops-packet.md) — **GCP ONLY / NON-PROD**: Phase 3C live ops packet; bounded rehearsal runbook, manual runbook, token security protocol; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
84a. [artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md](artifacts/2026-05-08-gcp-phase3c-live-rehearsal.md) — **GCP ONLY / NON-PROD**: Phase 3C live rehearsal artifact; orchestrator-gathered evidence, script validation, auth probes, backup trigger; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
85. [97-phase3ab-operator-review-packet.md](97-phase3ab-operator-review-packet.md) — **GCP ONLY / NON-PROD**: Phase 3A/3B/3C/3D unsigned operator review packet; prepared for BrianNguyen; blank signature fields; NOT signed; references Phase 3D G2 readiness; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
86. [98-phase3d-g2-readiness-checklist.md](98-phase3d-g2-readiness-checklist.md) — **GCP ONLY / NON-PROD**: Phase 3D G2 readiness checklist mapping G2.1-G2.8; conservative conclusion G2 NOT complete; ready for operator review only; UNSIGNED; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
86a. [artifacts/2026-05-08-gcp-phase3d-g2-readiness.md](artifacts/2026-05-08-gcp-phase3d-g2-readiness.md) — **GCP ONLY / NON-PROD**: Phase 3D evidence artifact; restore drill, metrics snapshot, TLS/auth, Phase 3C smoke, light workload smoke; G2 gate readiness summary; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
87. [99-briannguyen-direct-signing-worksheet.md](99-briannguyen-direct-signing-worksheet.md) — **OPERATOR USE**: BrianNguyen fillable signing worksheet for Phase 3A/3B/3C/3D evidence review and G2 readiness; blank/fillable; copy-forward mapping to canonical docs 54/59/63/65; UNSIGNED; NOT a substitute for signing canonical docs; NOT production-ready, NOT G2 complete, NOT pilot authorized, NOT operator signoff
88. [99-phase3e-sqlite-pilot-evidence-plan.md](99-phase3e-sqlite-pilot-evidence-plan.md) — **GCP ONLY / NON-PROD**: Phase 3E SQLite pilot evidence plan; read-only evidence gathering script; signed conditional single-node SQLite pilot only; NOT production-ready, NOT full G2/full production pilot authorization, NOT Phase 3E operator signoff; no GCP config mutations
88a. [artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md](artifacts/2026-05-09-gcp-phase3e-sqlite-pilot-evidence.md) — **GCP ONLY / NON-PROD**: Phase 3E evidence artifact scaffold; placeholder for evidence to be filled after running script; signed conditional single-node SQLite pilot; NOT production-ready, NOT full G2/full production pilot authorization, NOT Phase 3E operator signoff

## Luật ưu tiên
Khi mâu thuẫn, ưu tiên:
1. `../ferrumgate-roadmap-v1/19-v1-single-node-support-contract.md`
2. `01-current-state.md`
3. `31-release-paths-todo.md`
4. `23-production-readiness-assessment.md`
5. `../ferrumgate-roadmap-v1/06-constraints-and-invariants.md`
6. file còn lại trong thư mục này

`../ferrumgate-roadmap-v1/00-project-canon.md` là historical/superseded cho v1 hiện tại;
không dùng làm nguồn quyết định feature/status khi mâu thuẫn với các file trên.
