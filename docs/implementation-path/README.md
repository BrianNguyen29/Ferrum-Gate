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
70. [70-security-hardening-local-only-plan.md](70-security-hardening-local-only-plan.md) — Security hardening proposals, local-only audit commands, token rotation procedure
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
65. [84-mcp-server-d1-7-tool-dispatch-preflight.md](84-mcp-server-d1-7-tool-dispatch-preflight.md) — D1.7 tool dispatch: 8 lifecycle tools wired (submit, evaluate, mint, authorize, prepare, execute, verify, compensate); approve/reject permanently blocked (backend absent)
66. [85-mcp-server-d1-8-output-sanitization-preflight.md](85-mcp-server-d1-8-output-sanitization-preflight.md) — D1.8 output sanitization preflight: firewall gap analysis, single-point choke point, sensitive fields, no-over-redaction, implementation options A+C (oracle-approved 2026-05-07)
67. [86-mcp-server-d1-9-dlp-field-redaction-preflight.md](86-mcp-server-d1-9-dlp-field-redaction-preflight.md) — D1.9 DLP/field redaction preflight: dlp_findings stub, sanitize_output control-char-only, FirewallContext gap, implementation options A–E, Explorer recommends Option B targeting raw_arguments/metadata first (Phase 1 + Phase 2 implemented)
68. [87-mcp-server-d1-9-3-dlp-integration-validation.md](87-mcp-server-d1-9-3-dlp-integration-validation.md) — D1.9.3 integration validation: mockito-based E2E tests through handle_tools_call_with_client choke point, Phase 1+2 key redaction, path-aware compensation_plan[].args, no-over-redaction, large nested response guard

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
