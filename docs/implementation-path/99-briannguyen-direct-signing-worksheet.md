# 99 — BrianNguyen Direct Signing Worksheet

**Mục đích / Purpose**: Trang tính này tổng hợp tất cả các trường cần điền/ký trực tiếp cho Phase 3A/3B/3C/3D và G2 readiness. Hoàn thành trang tính này **không thay thế** việc cập nhật và ký các tài liệu canonical (docs 54/59/63/65).

**Trạng thái / Status**: **SIGNED by BrianNguyen on 09/05/2026**. Operator đã ký worksheet này vào ngày 09/05/2026. Tuy nhiên, việc ký worksheet này KHÔNG thay thế việc ký các canonical docs. Giá trị phải được copy-forward vào docs 54/59/63/65 và các canonical docs đó phải được ký.

**Ràng buộc / Constraints**:
- Không điền secrets thật vào tài liệu này hoặc bất kỳ tài liệu nào trong repo
- Không ký thay operator
- Không claim G2 complete hoặc production-ready
- Trang tính này **không cập nhật** docs 54/59/63/65 — giá trị phải được copy-forward thủ công

---

## PHẦN 0 — XÁC NHẬN ĐÃ KÝ / SIGNED CONFIRMATION

> **Tài liệu này ĐÃ ĐƯỢC KÝ bởi BrianNguyen vào ngày 09/05/2026**.
> Tuy nhiên, signature trên worksheet này KHÔNG thay thế việc ký các canonical docs (54/59/63/65).
> Operator phải copy các giá trị đã ký vào các canonical docs và ký các canonical docs đó để hoàn tất G2.

---

## PHẦN 1 — THÔNG TIN OPERATOR / Operator Identity

**Reference**: doc 65 §A, doc 54 §Signature

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Operator name | Tên operator | BrianNguyen |
| Operator role/title | Chức danh | Owner/Operator |
| Operator email | Email | (withheld — operator-managed) |
| Supervisor/countersigner name (if required) | Người ký đối.counter | N/A |
| Date of worksheet completion | Ngày hoàn thành | 09/05/2026 |

---

## PHẦN 2 — PHASE 3A/3B/3C/3D EVIDENCE ACCEPTANCE

**Reference**: doc 97 (Phase 3A/3B/3C), doc 98 (Phase 3D G2 readiness)

### 2.1 — Phase 3A GCP Non-Prod VM Target

**Reference**: doc 97 §Phase 3A Summary

| Check | Vietnamese Label | Xác nhận / Confirm |
|-------|-----------------|---------------------|
| VM `ferrumgate-nonprod` đang chạy | VM đang chạy | [x] Evidence observed: RUNNING, IP `34.158.51.8` |
| `ferrumgate.service` active | Service active | [x] Evidence observed: `active` |
| `ferrumgate-backup.timer` enabled | Backup timer enabled | [x] Evidence observed: `enabled` |
| `/v1/healthz` trả về 200 (với token) | Health probe OK | [x] Evidence observed: HTTP 200 |
| `/v1/readyz` trả về 200 (với token) | Readyz probe OK | [x] Evidence observed: HTTP 200 |
| Manual backup tạo file hợp lệ | Backup hoạt động | [x] Evidence observed: `ferrumgate_20260508_154446.db` |

**Ghi chú / Notes**: Prefilled from Phase 3A/3C/3D non-prod evidence; operator must review before canonical signoff.

### 2.2 — Phase 3B TLS/nip.io/Caddy

**Reference**: doc 97 §Phase 3B Summary

| Check | Vietnamese Label | Xác nhận / Confirm |
|-------|-----------------|---------------------|
| Caddy active | Caddy đang chạy | [x] Evidence observed: `active` |
| TLS certificate được provision qua Let's Encrypt | TLS OK | [x] Evidence observed: Caddy HTTPS via `34-158-51-8.nip.io` |
| `https://34-158-51-8.nip.io/v1/healthz` trả về 200 | HTTPS Health OK | [x] Evidence observed: HTTP 200 |
| `GET /v1/approvals` không có token → 401 | Auth 401 OK | [x] Evidence observed: HTTP 401 |
| `GET /v1/approvals` với VM-local token → 200 | Auth 200 OK | [x] Evidence observed: HTTP 200 |

**Ghi chú / Notes**: Prefilled from Phase 3B/3C/3D non-prod evidence. `nip.io` remains temporary and not a production domain.

### 2.3 — Phase 3C Live Rehearsal (Fail-Closed Script)

**Reference**: doc 97 §Phase 3C Summary, doc 96 (live ops packet)

| Check | Vietnamese Label | Xác nhận / Confirm |
|-------|-----------------|---------------------|
| Script `phase3c_live_rehearsal.sh` chạy và PASS | Script passed | [x] Evidence observed: `PASSED: All checks succeeded` |
| `caddy.service` active | Caddy active | [x] Evidence observed: `active` |
| `ferrumgate.service` active | Ferrumgate active | [x] Evidence observed: `active` |
| `ferrumgate-backup.timer` enabled | Backup timer enabled | [x] Evidence observed: `enabled` |
| Firewall rules đúng (22, 19080 allowlist; 80, 443 public) | Firewall OK | [x] Evidence observed: 22/19080 allowlisted, 80/443 public |
| Auth probe: no token → 401, with token → 200 | Auth probes OK | [x] Evidence observed: 401/200 |

**Ghi chú / Notes**: Prefilled from Phase 3C full/read-only rehearsals. Operator must decide whether public 80/443 exposure is acceptable for continued non-prod demo.

### 2.4 — Phase 3D G2 Readiness Evidence

**Reference**: doc 98 (G2 readiness checklist), doc 98 artifact

#### Restore Drill

| Trường / Field | Giá trị cần điền / Value |
|----------------|---------------------------|
| Latest backup file | `ferrumgate_20260508_154446.db` (đã xác nhận / confirmed) |
| Restore copy created | `ferrumgate_restore_drill_20260508_165658.db` |
| `PRAGMA integrity_check` result | `ok` |
| Table count | `14` |
| Restore copy removed | [x] Có / Yes  [ ] Không / No |

#### Metrics Snapshot

| Metric | Giá trị / Value |
|--------|-----------------|
| `ferrumgate_store_health_up` | `1` |
| `ferrumgate_write_queue_depth` | `0` |
| `readyz/deep` 503 count | `0` |

#### G2 Gate Readiness (từ doc 98 / from doc 98)

| Gate | Trạng thái doc 98 / Status in doc 98 | Operator xác nhận / Operator confirms |
|------|--------------------------------------|--------------------------------------|
| G2.1 Target workload model | `operator-required` | [ ] Đã xem xét / Reviewed |
| G2.2 Bearer auth + TLS + firewall | `ready` | [x] Evidence ready; operator review still required |
| G2.3 Backup schedule evidence | `partial` | [ ] Đã xem xét / Reviewed |
| G2.4 Restore drill | `ready` | [x] Evidence ready; operator review still required |
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
| Target host / IP | Host/IP mục tiêu | `34.158.51.8` (GCP non-prod) |
| SSH host | SSH host | `ferrumgate-nonprod` |
| SSH user | SSH user | `ubuntu` |
| SSH key path | Đường dẫn SSH key | `/home/uong_guyen/.ssh/google_compute_engine` |
| FQDN / domain cho TLS | Domain TLS | `34-158-51-8.nip.io` (temporary non-prod) |
| Network zone (DMZ/internal) | Zone mạng | GCP custom VPC `ferrumgate-nonprod-vpc`, zone `asia-southeast1-a` |

### 3.2 — TLS / Domain

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Public domain cho ferrumgate | Domain công khai | `34-158-51-8.nip.io` (temporary; replace with real domain before production) |
| TLS certificate type (letsencrypt/certbot/existing CA) | Loại TLS cert | Let's Encrypt via Caddy automatic HTTPS |
| DNS A record trỏ đến target host | DNS A record | [x] Confirmed for `34-158-51-8.nip.io` → `34.158.51.8` |

### 3.3 — Storage and Backup / Lưu trữ và Backup

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| SQLite store path | Đường dẫn SQLite store | `/var/lib/ferrumgate/data/ferrumgate.db` |
| Backup output directory | Thư mục backup | `/var/lib/ferrumgate/backups` |
| Backup retention policy (days) | Retention (ngày) | 7 days + offsite copy required before final production pilot |
| Backup schedule | Lịch backup | 15-minute systemd timer configured on non-prod VM: `OnUnitActiveSec=15min`; timer `enabled` and `active` |

### 3.4 — Workload Model (G2.1)

**Reference**: doc 54 Template 1 — Workload Model

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| Expected sustained write rate (max 300 writes/s cho Phase 1) | Write rate dự kiến | ≤300 writes/s |
| Expected peak write rate | Peak write rate | ≤300 writes/s |
| Expected daily write volume | Daily write volume | ≤1M writes/day |
| SQLite single-node capacity assessment | Đánh giá SQLite | [x] Fits ≤300 writes/s  [ ] Exceeds 300 writes/s — operator selected ≤300 writes/s |
| Single-node topology confirmed | Single-node confirmed | [x] Yes  [ ] No — conditional single-node pilot only |

**Signoff phrase / Câu xác nhận**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

### 3.5 — RPO/RTO Acceptance (G2.5)

**Reference**: doc 54 §3

| Trường / Field | Vietnamese Label | Giá trị cần điền / Value to Fill |
|----------------|-----------------|-----------------------------------|
| RPO accepted (time since last backup = max data loss) | RPO chấp nhận được | 15 minutes |
| RTO accepted (restore time + restart + verification) | RTO chấp nhận được | 15 minutes |
| RPO acceptable for target workload SLA | RPO phù hợp SLA | [x] Yes  [ ] No — operator selected RPO 15m |
| RTO acceptable for target workload SLA | RTO phù hợp SLA | [x] Yes  [ ] No — operator selected RTO 15m |

**Signoff phrase / Câu xác nhận**: "Operator confirms RPO/RTO fit for the target workload."

---

## PHẦN 4 — G2 GATE CHECKLIST VÀ SIGNATURE FIELDS

**Reference**: doc 54 (operator signoff packet), doc 98 (G2 readiness checklist)

### G2.1 — Workload Model

**Status**: `operator-required`

Checklist:
- [x] Sustained write rate modeled (≤300 writes/s for Phase 1 SQLite)
- [x] Single-node topology confirmed acceptable
- [x] Workload model attached (if applicable)

**Signoff phrase**: "Operator has modeled production workload against SQLite single-node constraints and confirmed fit."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.2 — Bearer Auth + TLS + Firewall

**Status**: `ready` (GCP non-prod evidence confirmed)

Checklist:
- [x] Bearer token configured (`auth_mode = "Bearer"`)
- [x] TLS termination at reverse proxy confirmed
- [x] Firewall rules reviewed (non-prod rehearsal acceptable)
- [x] Production TLS/domain plan defined (nip.io not for production)

**Signoff phrase**: "Operator has configured bearer auth and confirmed TLS termination is handled by the reverse proxy."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.3 — Backup Schedule Evidence

**Status**: `partial`

Checklist:
- [x] Production backup schedule target defined: 15m cadence, 7 days + offsite copy required
- [x] Backup schedule evidence attached: timer updated to `OnUnitActiveSec=15min`; `enabled` + `active`
- [x] Backup timer/timer schedule confirmed for target non-prod environment; offsite copy still required before final production pilot

**Signoff phrase**: "Operator has implemented backup schedule external to FerrumGate."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.4 — Restore Drill

**Status**: `ready` (GCP non-prod drill passed)

Checklist:
- [x] Restore drill performed in production-adjacent environment
- [x] `PRAGMA integrity_check` passed on restored DB
- [x] Execution lineage queryable after restore
- [x] Approval queue readable after restore

**Signoff phrase**: "Operator has performed a restore drill, confirmed RPO/RTO fit for the target workload, and backup retention policy is operator-defined."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.5 — RPO/RTO Acceptance

**Status**: `operator-required`

Checklist:
- [x] RPO formally accepted for target workload: 15 minutes
- [x] RTO formally accepted for target workload: 15 minutes
- [x] RPO/RTO acceptance documented in this worksheet

**Signoff phrase**: "Operator confirms RPO/RTO fit for the target workload."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.6 — Production Evaluation Framework

**Status**: `partial` (repo-side tests passed; operator framework pending)

Checklist:
- [x] Dimension 1 — Performance: [ ] SATISFIED  [x] CONDITIONAL (conditional single-node pilot scope)
- [x] Dimension 2 — Security: [ ] SATISFIED  [x] CONDITIONAL (nip.io temporary; production needs real domain)
- [x] Dimension 3 — Reliability: [ ] SATISFIED  [x] CONDITIONAL (single-node SQLite; no HA)
- [x] Dimension 4 — Operations: [ ] SATISFIED  [x] CONDITIONAL (manual backup; no automated recovery)
- [x] Dimension 5 — Release Confidence: [ ] SATISFIED  [x] CONDITIONAL (RC-ready; pilot pending)
- [x] All critical items SATISFIED or CONDITIONAL (with controls)? — YES, CONDITIONAL accepted for conditional single-node pilot

**Signoff phrase**: "All critical items CONDITIONAL — accepted for conditional single-node pilot scope."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.7 — Accepted-Risk Review

**Status**: `partial`

Checklist:
- [x] Weak Spot 1 — Rollback class handling: reviewed and accepted as-is for conditional pilot
- [x] Weak Spot 2 — Draft-only revalidation: reviewed and accepted as-is for conditional pilot
- [x] Weak Spot 3 — Scope-bounds enforcement: reviewed and accepted as-is for conditional pilot
- [x] Weak Spot 4 — Provenance completeness: reviewed and accepted as-is for conditional pilot
- [x] Additional accepted risks from `19-v1-single-node-support-contract.md` §4: reviewed and accepted as-is

**Signoff phrase**: "All weak spots reviewed and accepted risks acknowledged."

Operator signature: BrianNguyen Date: 09/05/2026

### G2.8 — Compensate Noop Risk Acceptance

**Status**: `partial`

Checklist:
- [x] Compensate behavior matrix completed
- [x] Noop-backed adapters identified
- [x] Manual verification procedure defined for noop-backed compensate
- [x] Compensate noop risk accepted as-is for conditional pilot

**G2.8 Scope Note**: Compensate behavior verified via existing local integration evidence (`compensate_execution_flow` test PASS). For conditional single-node pilot: adapters covered by existing local evidence; noop-backed/limited external undo accepted with manual verification procedure. Production adapters may require additional verification before deployment.

**Signoff phrase**: "Operator accepts compensate noop risk with manual verification procedure for conditional single-node pilot scope."

Operator signature: BrianNguyen Date: 09/05/2026
---

## PHẦN 5 — FINAL OPERATOR SIGN-OFF / KÝ XÁC NHẬN CUỐI CÙNG

**Reference**: doc 54 Pilot Acceptance Statement

> **Pilot Acceptance Statement**: "I, [Operator Name], acting in my capacity as [Role], have evaluated FerrumGate v1 single-node SQLite against the production evaluation plan (`27-production-evaluation-plan.md`). I have reviewed and accepted all accepted risks documented in `19-v1-single-node-support-contract.md` §4 and the Weak Spots documented in `26-EV-v1-single-node-invariant-control-test-evidence-matrix.md`. I confirm the workload fits within Phase 1 SQLite constraints, all G2 gates have been satisfied, and I accept the conditional production posture as described in `23-production-readiness-assessment.md`. I authorize the limited production pilot deployment as described in `31-release-paths-todo.md` §Path 2."

**Caveat**: G2 gates are only satisfied when ALL individual gate signoff fields in Phần 4 above are signed. This worksheet alone does not constitute G2 completion.

### Final Signoff

Operator name: BrianNguyen

Operator role/title: Operator/Owner

Date: 09/05/2026

**Operator acceptance statement**: ACCEPT

Operator signature: BrianNguyen Date: 09/05/2026

Supervisor/countersigner (if required): N/A Date: _______________

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
> - Full production-ready status
> - Full G2 complete (conditional single-node pilot scope only)
> - Pilot authorization (canonical docs must still be signed)
> - Production PostgreSQL/HA readiness
>
> **Conditional pilot scope / Phạm vi pilot có điều kiện**:
> - Conditional single-node SQLite pilot authorization only
> - nip.io temporary domain not for production
> - Backup: 15-minute timer, 7 days + offsite required before production
> - Noop-backed compensate accepted for conditional pilot with manual verification
> - All G2 gates signed conditionally for single-node SQLite pilot
>
> **Chỉ có hiệu lực đầy đủ khi / Fully valid only when**:
> - Canonical docs 54/59/63/65 được copy giá trị từ worksheet này và ký / Values copied from this worksheet to canonical docs 54/59/63/65 and signed
> - Operator chấp nhận tất cả accepted risks / Operator accepts all accepted risks

---

## Document History

| Date | Change |
|---|---|
| 2026-05-08 | Initial BrianNguyen direct signing worksheet. UNSIGNED. For Phase 3A/3B/3C/3D evidence review and G2 readiness preparation only. |
| 09/05/2026 | Signed by BrianNguyen. Header updated to SIGNED. Countersigner set to N/A. G2.6 clarified as CONDITIONAL. G2.8 scope note added. Copy-forward values ready for canonical docs 54/59/63/65. |
