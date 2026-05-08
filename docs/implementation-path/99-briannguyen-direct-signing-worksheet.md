# 99 — BrianNguyen Direct Signing Worksheet

**Mục đích / Purpose**: Trang tính này tổng hợp tất cả các trường cần điền/ký trực tiếp cho Phase 3A/3B/3C/3D và G2 readiness. Hoàn thành trang tính này **không thay thế** việc cập nhật và ký các tài liệu canonical (docs 54/59/63/65).

**Trạng thái / Status**: **CHƯA KÝ — UNSIGNED**. Tài liệu này chỉ là bản chuẩn bị. Operator phải ký các phần tương ứng trong doc 54 và các canonical docs khác sau khi điền đầy đủ.

**Ràng buộc / Constraints**:
- Không điền secrets thật vào tài liệu này hoặc bất kỳ tài liệu nào trong repo
- Không ký thay operator
- Không claim G2 complete hoặc production-ready
- Trang tính này **không cập nhật** docs 54/59/63/65 — giá trị phải được copy-forward thủ công

---

## PHẦN 0 — XÁC NHẬN CHƯA KÝ / UNSIGNED CONFIRMATION

> **Tài liệu này CHƯA ĐƯỢC KÝ**. Không có signature nào dưới đây có giá trị pháp lý.
> Việc ký chỉ có hiệu lực khi được thực hiện trên doc 54 và các canonical docs tương ứng.

---

## PHẦN 1 — THÔNG TIN OPERATOR / Operator Identity

**Reference**: doc 65 §A, doc 54 §Signature

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Operator name | Tên operator | _______________________________ |
| Operator role/title | Chức danh | _______________________________ |
| Operator email | Email | _______________________________ |
| Supervisor/countersigner name (if required) | Người ký đối.counter | _______________________________ |
| Date of worksheet completion | Ngày hoàn thành | _______________ |

---

## PHẦN 2 — PHASE 3A/3B/3C/3D EVIDENCE ACCEPTANCE

**Reference**: doc 97 (Phase 3A/3B/3C), doc 98 (Phase 3D G2 readiness)

### 2.1 — Phase 3A GCP Non-Prod VM Target

**Reference**: doc 97 §Phase 3A Summary

| Check | Vietnamese Label | Xác nhận / Confirm |
|-------|-----------------|---------------------|
| VM `ferrumgate-nonprod` đang chạy | VM đang chạy | [ ] Đã xác nhận / Confirmed |
| `ferrumgate.service` active | Service active | [ ] Đã xác nhận / Confirmed |
| `ferrumgate-backup.timer` enabled | Backup timer enabled | [ ] Đã xác nhận / Confirmed |
| `/v1/healthz` trả về 200 (với token) | Health probe OK | [ ] Đã xác nhận / Confirmed |
| `/v1/readyz` trả về 200 (với token) | Readyz probe OK | [ ] Đã xác nhận / Confirmed |
| Manual backup tạo file hợp lệ | Backup hoạt động | [ ] Đã xác nhận / Confirmed |

**Ghi chú / Notes**: _______________________________

### 2.2 — Phase 3B TLS/nip.io/Caddy

**Reference**: doc 97 §Phase 3B Summary

| Check | Vietnamese Label | Xác nhận / Confirm |
|-------|-----------------|---------------------|
| Caddy active | Caddy đang chạy | [ ] Đã xác nhận / Confirmed |
| TLS certificate được provision qua Let's Encrypt | TLS OK | [ ] Đã xác nhận / Confirmed |
| `https://34-158-51-8.nip.io/v1/healthz` trả về 200 | HTTPS Health OK | [ ] Đã xác nhận / Confirmed |
| `GET /v1/approvals` không có token → 401 | Auth 401 OK | [ ] Đã xác nhận / Confirmed |
| `GET /v1/approvals` với VM-local token → 200 | Auth 200 OK | [ ] Đã xác nhận / Confirmed |

**Ghi chú / Notes**: _______________________________

### 2.3 — Phase 3C Live Rehearsal (Fail-Closed Script)

**Reference**: doc 97 §Phase 3C Summary, doc 96 (live ops packet)

| Check | Vietnamese Label | Xác nhận / Confirm |
|-------|-----------------|---------------------|
| Script `phase3c_live_rehearsal.sh` chạy và PASS | Script passed | [ ] Đã xác nhận / Confirmed |
| `caddy.service` active | Caddy active | [ ] Đã xác nhận / Confirmed |
| `ferrumgate.service` active | Ferrumgate active | [ ] Đã xác nhận / Confirmed |
| `ferrumgate-backup.timer` enabled | Backup timer enabled | [ ] Đã xác nhận / Confirmed |
| Firewall rules đúng (22, 19080 allowlist; 80, 443 public) | Firewall OK | [ ] Đã xác nhận / Confirmed |
| Auth probe: no token → 401, with token → 200 | Auth probes OK | [ ] Đã xác nhận / Confirmed |

**Ghi chú / Notes**: _______________________________

### 2.4 — Phase 3D G2 Readiness Evidence

**Reference**: doc 98 (G2 readiness checklist), doc 98 artifact

#### Restore Drill

| Trường / Field | Giá trị cần điền / Value |
|----------------|---------------------------|
| Latest backup file | `ferrumgate_20260508_154446.db` (đã xác nhận / confirmed) |
| Restore copy created | _______________________________ |
| `PRAGMA integrity_check` result | _______________________________ |
| Table count | _______________________________ |
| Restore copy removed | [ ] Có / Yes  [ ] Không / No |

#### Metrics Snapshot

| Metric | Giá trị / Value |
|--------|-----------------|
| `ferrumgate_store_health_up` | _______________________________ |
| `ferrumgate_write_queue_depth` | _______________________________ |
| `readyz/deep` 503 count | _______________________________ |

#### G2 Gate Readiness (từ doc 98 / from doc 98)

| Gate | Trạng thái doc 98 / Status in doc 98 | Operator xác nhận / Operator confirms |
|------|--------------------------------------|--------------------------------------|
| G2.1 Target workload model | `operator-required` | [ ] Đã xem xét / Reviewed |
| G2.2 Bearer auth + TLS + firewall | `ready` | [ ] Đã xác nhận / Confirmed |
| G2.3 Backup schedule evidence | `partial` | [ ] Đã xem xét / Reviewed |
| G2.4 Restore drill | `ready` | [ ] Đã xác nhận / Confirmed |
| G2.5 RPO/RTO acceptance | `operator-required` | [ ] Đã xem xét / Reviewed |
| G2.6 Production evaluation framework | `partial` | [ ] Đã xem xét / Reviewed |
| G2.7 Accepted-risk review | `partial` | [ ] Đã xem xét / Reviewed |
| G2.8 Compensate noop risk | `partial` | [ ] Đã xem xét / Reviewed |

---

## PHẦN 3 — TARGET ENVIRONMENT FIELDS (cho docs 63/65)

**Reference**: doc 63 (target environment spec), doc 65 (target questionnaire)

### 3.1 — Host and Network / Máy chủ và Mạng

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Target host / IP | Host/IP mục tiêu | _______________________________ |
| SSH host | SSH host | _______________________________ |
| SSH user | SSH user | _______________________________ |
| SSH key path | Đường dẫn SSH key | _______________________________ |
| FQDN / domain cho TLS | Domain TLS | _______________________________ |
| Network zone (DMZ/internal) | Zone mạng | _______________________________ |

### 3.2 — TLS / Domain

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Public domain cho ferrumgate | Domain công khai | _______________________________ |
| TLS certificate type (letsencrypt/certbot/existing CA) | Loại TLS cert | _______________________________ |
| DNS A record trỏ đến target host | DNS A record | [ ] Đã xác nhận / Confirmed |

### 3.3 — Storage and Backup / Lưu trữ và Backup

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| SQLite store path | Đường dẫn SQLite store | _______________________________ |
| Backup output directory | Thư mục backup | _______________________________ |
| Backup retention policy (days) | Retention (ngày) | _______________________________ |
| Backup schedule | Lịch backup | _______________________________ |

### 3.4 — Workload Model (G2.1)

**Reference**: doc 54 Template 1 — Workload Model

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Expected sustained write rate (max 300 writes/s cho Phase 1) | Write rate dự kiến | _____ writes/s |
| Expected peak write rate | Peak write rate | _____ writes/s |
| Expected daily write volume | Daily write volume | _____ writes/day |
| SQLite single-node capacity assessment | Đánh giá SQLite | [ ] Fits ≤300 writes/s  [ ] Exceeds 300 writes/s |
| Single-node topology confirmed | Single-node confirmed | [ ] Yes  [ ] No |

**Signoff phrase / Câu xác nhận**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

### 3.5 — RPO/RTO Acceptance (G2.5)

**Reference**: doc 54 §3

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| RPO accepted (time since last backup = max data loss) | RPO chấp nhận được | _____ |
| RTO accepted (restore time + restart + verification) | RTO chấp nhận được | _____ |
| RPO acceptable for target workload SLA | RPO phù hợp SLA | [ ] Yes  [ ] No |
| RTO acceptable for target workload SLA | RTO phù hợp SLA | [ ] Yes  [ ] No |

**Signoff phrase / Câu xác nhận**: "Operator confirms RPO/RTO fit for the target workload."

---

## PHẦN 4 — G2 GATE CHECKLIST VÀ SIGNATURE FIELDS

**Reference**: doc 54 (operator signoff packet), doc 98 (G2 readiness checklist)

### G2.1 — Workload Model

**Status**: `operator-required`

Checklist:
- [ ] Sustained write rate modeled (≤300 writes/s for Phase 1 SQLite)
- [ ] Single-node topology confirmed acceptable
- [ ] Workload model attached (if applicable)

**Signoff phrase**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

Operator signature: _______________________________ Date: _______________

### G2.2 — Bearer Auth + TLS + Firewall

**Status**: `ready` (GCP non-prod evidence confirmed)

Checklist:
- [ ] Bearer token configured (`auth_mode = "Bearer"`)
- [ ] TLS termination at reverse proxy confirmed
- [ ] Firewall rules reviewed (non-prod rehearsal acceptable)
- [ ] Production TLS/domain plan defined (nip.io not for production)

**Signoff phrase**: "Operator has configured bearer auth and confirmed TLS termination is handled by the reverse proxy."

Operator signature: _______________________________ Date: _______________

### G2.3 — Backup Schedule Evidence

**Status**: `partial`

Checklist:
- [ ] Production backup schedule defined (frequency, retention, offsite)
- [ ] Backup schedule evidence attached
- [ ] Backup timer/timer schedule confirmed for target environment

**Signoff phrase**: "Operator has implemented backup schedule external to FerrumGate."

Operator signature: _______________________________ Date: _______________

### G2.4 — Restore Drill

**Status**: `ready` (GCP non-prod drill passed)

Checklist:
- [ ] Restore drill performed in production-adjacent environment
- [ ] `PRAGMA integrity_check` passed on restored DB
- [ ] Execution lineage queryable after restore
- [ ] Approval queue readable after restore

**Signoff phrase**: "Operator has performed a restore drill, confirmed RPO/RTO fit for the target workload, and backup retention policy is operator-defined."

Operator signature: _______________________________ Date: _______________

### G2.5 — RPO/RTO Acceptance

**Status**: `operator-required`

Checklist:
- [ ] RPO formally accepted for target workload
- [ ] RTO formally accepted for target workload
- [ ] RPO/RTO acceptance documented

**Signoff phrase**: "Operator confirms RPO/RTO fit for the target workload."

Operator signature: _______________________________ Date: _______________

### G2.6 — Production Evaluation Framework

**Status**: `partial` (repo-side tests passed; operator framework pending)

Checklist:
- [ ] Dimension 1 — Performance: SATISFIED / CONDITIONAL / NOT MET
- [ ] Dimension 2 — Security: SATISFIED / CONDITIONAL / NOT MET
- [ ] Dimension 3 — Reliability: SATISFIED / CONDITIONAL / NOT MET
- [ ] Dimension 4 — Operations: SATISFIED / CONDITIONAL / NOT MET
- [ ] Dimension 5 — Release Confidence: SATISFIED / CONDITIONAL / NOT MET
- [ ] All critical items SATISFIED or CONDITIONAL (with controls)?

**Signoff phrase**: "All critical items SATISFIED or CONDITIONAL."

Operator signature: _______________________________ Date: _______________

### G2.7 — Accepted-Risk Review

**Status**: `partial`

Checklist:
- [ ] Weak Spot 1 — Rollback class handling: reviewed and accepted
- [ ] Weak Spot 2 — Draft-only revalidation: reviewed and accepted
- [ ] Weak Spot 3 — Scope-bounds enforcement: reviewed and accepted
- [ ] Weak Spot 4 — Provenance completeness: reviewed and accepted
- [ ] Additional accepted risks from `19-v1-single-node-support-contract.md` §4: reviewed

**Signoff phrase**: "All weak spots reviewed and accepted risks acknowledged."

Operator signature: _______________________________ Date: _______________

### G2.8 — Compensate Noop Risk Acceptance

**Status**: `partial`

Checklist:
- [ ] Compensate behavior matrix completed
- [ ] Noop-backed adapters identified
- [ ] Manual verification procedure defined for noop-backed compensate
- [ ] Compensate noop risk accepted

**Signoff phrase**: "Operator accepts compensate noop risk with manual verification procedure."

Operator signature: _______________________________ Date: _______________

---

## PHẦN 5 — FINAL OPERATOR SIGN-OFF / KÝ XÁC NHẬN CUỐI CÙNG

**Reference**: doc 54 Pilot Acceptance Statement

> **Pilot Acceptance Statement**: "I, [Operator Name], acting in my capacity as [Role], have evaluated FerrumGate v1 single-node SQLite against the production evaluation plan (`27-production-evaluation-plan.md`). I have reviewed and accepted all accepted risks documented in `19-v1-single-node-support-contract.md` §4 and the Weak Spots documented in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`. I confirm the workload fits within Phase 1 SQLite constraints, all G2 gates have been satisfied, and I accept the conditional production posture as described in `23-production-readiness-assessment.md`. I authorize the limited production pilot deployment as described in `31-release-paths-todo.md` §Path 2."

**Caveat**: G2 gates are only satisfied when ALL individual gate signoff fields in Phần 4 above are signed. This worksheet alone does not constitute G2 completion.

### Final Signoff

Operator name: _______________________________

Operator role/title: _______________________________

Date: _______________

**Operator acceptance statement**: _______________________________

Operator signature: _______________________________ Date: _______________

Supervisor/countersigner (if required): _______________________________ Date: _______________

---

## PHẦN 6 — COPY-FORWARD MAPPING / ÁNH XẠ COPY-FORWARD

**Quan trọng / Important**: Việc hoàn thành trang tính này **KHÔNG tự động cập nhật** các canonical docs. Operator phải copy giá trị thủ công vào các tài liệu bên dưới sau khi ký.

| Canonical Doc | Nội dung cần copy từ worksheet này / Content to copy from this worksheet |
|-------------|--------------------------------------------------------|
| `54-operator-signoff-packet.md` | Tất cả G2 gate signoff fields (Phần 4), Final signoff (Phần 5), Workload model (G2.1), RPO/RTO (G2.5), Accepted-risk checklist (G2.7), Compensate matrix (G2.8) |
| `58-workload-compensation-drill-evidence-template.md` | Restore drill results (Phần 2.4), G2.4 signoff |
| `59-pilot-readiness-evidence-packet.md` | G2.1-G2.8 evidence sections; all operator signatures from Phần 4 |
| `63-path-2-target-environment-spec.md` | Target environment fields (Phần 3.1–3.3): host/IP, SSH, domain, SQLite path, backup dir, retention, schedule |
| `65-path-2-target-questionnaire.md` | Operator identity (Phần 1), TLS/domain fields (Phần 3.2), workload model (Phần 3.4), RPO/RTO (Phần 3.5) |

**Sau khi copy vào canonical docs / After copying to canonical docs**:
1. Ký các canonical docs đã cập nhật / Sign the updated canonical docs
2. Xóa hoặc lưu trữ bản worksheet này một cách an toàn / Securely store or discard this worksheet
3. Đảm bảo không có secrets thật trong bất kỳ tài liệu nào / Ensure no real secrets in any document

---

## PHẦN 7 — NON-CLAIMS PRESERVED / CÁC TUYÊN BỐ ĐƯỢC GIỮ NGUYÊN

> **Trang tính này KHÔNG claim / This worksheet does NOT claim**:
> - Production-ready status
> - G2 complete
> - Pilot authorization
> - Operator signoff (cho đến khi các canonical docs được ký / until canonical docs are signed)
>
> **Chỉ có hiệu lực khi / Only valid when**:
> - Tất cả G2 gates trong Phần 4 được ký bởi operator / All G2 gates in Phần 4 are signed by operator
> - Canonical docs 54/59/63/65 được cập nhật và ký / Canonical docs 54/59/63/65 are updated and signed
> - Operator chấp nhận tất cả accepted risks / Operator accepts all accepted risks

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial BrianNguyen direct signing worksheet. UNSIGNED. For Phase 3A/3B/3C/3D evidence review and G2 readiness preparation only. |
