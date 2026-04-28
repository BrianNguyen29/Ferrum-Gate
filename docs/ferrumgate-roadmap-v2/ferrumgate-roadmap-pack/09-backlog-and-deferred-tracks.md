# 09 — Backlog and deferred tracks

## Mục đích

File này gom các hướng có tiềm năng nhưng **không phải ưu tiên ngay** trong chu kỳ roadmap 12 tháng hiện tại.

## A. Deferred but valuable

### 1. Cedar / policy engine formalization
Có giá trị lớn cho policy-as-code và analysis, nhưng không chặn wedge đầu tiên nếu explainable PDP hiện tại còn đủ.

### 2. OPA/Rego integration
Có ích cho cloud-native and org policy alignment, nhưng nên là integration path sau khi semantics lõi đã ổn.

### 3. Cryptographic capabilities
Macaroon / signed capability chain / delegated proof có tiềm năng, nhưng chưa nên thay thế priority của single-use + scope enforcement + approval digest binding.

### 4. Wasm sandbox
Rất giá trị nếu FerrumGate bước vào governed code execution. Chưa phải blocker cho wedge engineering changes nếu agent chủ yếu gọi tool/adapters.

### 5. Edge deployment
Có thể hữu ích cho ingress/policy latency sau này. Không nên ép toàn bộ control plane ra edge quá sớm.

### 6. Multi-node / HA / read-replica
Chỉ nên làm sau khi self-hosted single-node/postgres product beta đã chứng minh nhu cầu thật.

## B. Post-12-month candidates

- reversible execution planner
- cross-runtime provenance fabric
- policy marketplace / standard policy library community edition
- stronger tamper-evident ledger
- SIEM/GRC connectors
- cloud control plane hybrid model

## C. Explicit non-goals for current cycle

- full computer-use / GUI automation governance
- multi-tenant SaaS complete product
- generic compliance suite for every regulated industry
- replacing agent runtimes
- model routing/orchestration product

> **V1 boundary**: All items in section C are explicitly out of v1 scope per the
> v1 support contract. Multi-tenant SaaS and multi-node/HA are listed as "not supported"
> in v1. GUI automation governance is not in v1 scope.

## D. Revisit triggers

Chỉ kéo một mục deferred vào main roadmap khi có một trong các trigger sau:
- design partner yêu cầu rõ và sẵn sàng trả tiền
- nó unblock trực tiếp một release gate
- current architecture bắt đầu bị bó do thiếu feature này
- có bằng chứng usage cho thấy lack of this feature gây churn hoặc adoption failure

---

## V1 boundary reminder

Deferred tracks in this document are **post-v1 scope** unless they are explicitly
reclassified via a formal amendment to `19-v1-single-node-support-contract.md`.
The backlog tracks (HA, multi-node, postgres, operator UI, MCP, etc.) represent
the gap between v1 single-node and a commercially shippable product. They are
**not** v1 defects and should not be treated as v1 support obligations.
