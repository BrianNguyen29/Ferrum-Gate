# 2026-05-16 — C1 Keyless Backup Recovery and Block B Status

> **Status**: Audit-trail artifact. Operator-owned evidence. No production-ready claim.
> **Purpose**: Record the 2026-05-16 C1 keyless backup scope update, zone-capacity recovery, keyless verification, and Block B SendGrid smoke-test state.
> **Scope**: Single-node SQLite v1 conditional pilot. Non-production VM only.
> **Constraint**: `production-ready = NO`. Block A remains blocked. No secrets.

---

## 1. Block C — C1 Keyless Backup Path Execution

### 1.1 Path Selection

Operator selected **Path C1** (stop-start VM with GCS write scopes, keyless).
Path C2 (key-based) was not required.

### 1.2 Pre-Flight

| Item | Value |
|------|-------|
| VM | `ferrumgate-nonprod` |
| Zone | `asia-southeast1-a` |
| Project | `fairy-b13f4` |
| Static IP | `34.158.51.8` (preserved throughout) |
| Pre-C1 snapshot | `ferrumgate-pre-c1-keyless-20260516` |
| Original machine type | `e2-medium` |

**Pre-C1 scope state**:
- VM service account: `905477274418-compute@developer.gserviceaccount.com`
- OAuth scopes included `devstorage.read_only`
- `devstorage.read_write` and `cloud-platform` were **ABSENT**

### 1.3 Scope Update

```bash
# Command executed (placeholder values preserved)
gcloud compute instances set-service-account ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --service-account=905477274418-compute@developer.gserviceaccount.com \
  --scopes=storage-rw,logging-write,monitoring-write,pubsub,service-control,trace
```

**Result**: Scope update succeeded. VM metadata confirmed new scopes include `https://www.googleapis.com/auth/devstorage.read_write` plus logging, monitoring, pubsub, service-control, and trace scopes.

### 1.4 Restart Attempts and Recovery

| Attempt | Machine Type | Result |
|---------|-------------|--------|
| 1 | `e2-medium` (original) | `ZONE_RESOURCE_POOL_EXHAUSTED` — zone capacity insufficient |
| 2 | `e2-small` | `ZONE_RESOURCE_POOL_EXHAUSTED` — zone capacity insufficient |
| 3 | `n2-standard-2` | **START SUCCEEDED** — VM RUNNING |

**Recovery action**: Changed machine type from `e2-medium` to `n2-standard-2` to satisfy zone capacity constraints.

**Note**: `n2-standard-2` is a temporary recovery state. Operator should cost-review and revert to `e2-medium` (or `e2-small`) when zone capacity permits.

### 1.5 Post-Recovery Verification

| Check | Method | Result |
|-------|--------|--------|
| VM state | `gcloud compute instances describe` | RUNNING |
| Static IP | `networkInterfaces[0].accessConfigs[0].natIP` | `34.158.51.8` preserved |
| ferrumgate.service | `systemctl status ferrumgate` | active (running) |
| Local readyz | `curl http://localhost:19080/v1/readyz` | HTTP 200 |
| Local readyz/deep | `curl http://localhost:19080/v1/readyz/deep` | HTTP 200 |
| Public HTTPS readyz | `curl https://ferrumgate.duckdns.org/v1/readyz` | HTTP 200 |
| Metadata scopes | `curl -s "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/scopes" -H "Metadata-Flavor: Google"` | Includes `devstorage.read_write` |

### 1.6 Keyless GCS Probe

Executed with isolated environment to ensure no key file or env var influenced the result:

```bash
# Empty isolated HOME, no GOOGLE_APPLICATION_CREDENTIALS
HOME=/tmp/empty-home-$$ mkdir -p /tmp/empty-home-$$
HOME=/tmp/empty-home-$$ gsutil ls gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/
# Result: PASS — listed objects without error

printf "keyless probe\n" > /tmp/ferrumgate-keyless-probe.txt
HOME=/tmp/empty-home-$$ gsutil cp /tmp/ferrumgate-keyless-probe.txt gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/keyless-probe-20260516.txt
# Result: PASS — write succeeded without error

HOME=/tmp/empty-home-$$ gsutil rm gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/keyless-probe-20260516.txt
# Result: PASS — remote probe cleanup succeeded

rm -rf /tmp/empty-home-$$ /tmp/ferrumgate-keyless-probe.txt
# Result: PASS — cleanup complete
```

**Conclusion**: Keyless GCS access works. VM identity alone is sufficient for read and write operations on the backup bucket.

### 1.7 Offsite Sync Verification

```bash
# Executed offsite backup sync script
sudo /usr/local/sbin/ferrumgate-offsite-backup-sync.sh
```

| Metric | Value |
|--------|-------|
| Sync method | `gsutil rsync` |
| Objects copied | 1 |
| Size | 15.3 MiB |
| Return code | `OFFSITE_SYNC_RC=0` |
| Result | **PASS** |

### 1.8 Key-File Residue Audit (Pre-Removal State)

> **Note**: This section records the state observed during initial C1 recovery, before the follow-up removal documented in §1.10.

| Path | Status (initial) | Note |
|------|-----------------|------|
| `/etc/ferrumgate/gcs-service-account.json` | **PRESENT** | Old key-file path from prior key-based attempts. Removed during 2026-05-16 follow-up (see §1.10). |
| `/etc/ferrumgate/secrets/gcs-service-account.json` | **ABSENT** | Correct — no key file in secrets directory |
| `/root/.boto` | **ABSENT** | Correct — no gsutil boto config with embedded credentials |

**Follow-up (completed)**: Remove `/etc/ferrumgate/gcs-service-account.json` after confirming keyless backup is stable. Removed 2026-05-16. See §1.10 for removal evidence.

### 1.9 SSH Firewall

During live follow-up work, SSH access timed out with the previous firewall range. The firewall was temporarily opened to `14.239.184.129/32` to restore access. After work completed, the firewall was restored to `118.69.4.63/32`.

| Step | Firewall Range | Note |
|------|---------------|------|
| Initial (pre-follow-up) | `118.69.4.63/32` | Restored after C1 recovery |
| Temporary (live work) | `14.239.184.129/32` | Opened due to SSH timeout |
| Final (post-follow-up) | `118.69.4.63/32` | Restored after work complete |

### 1.10 Residual Key Removal

#### Pre-Removal Checks

| Check | Result |
|-------|--------|
| ferrumgate.service | active (running) |
| LOCAL_READYZ | HTTP 200 |
| `/etc/ferrumgate/gcs-service-account.json` | **PRESENT** |
| `/etc/ferrumgate/secrets/gcs-service-account.json` | **ABSENT** |
| `/root/.boto` | **ABSENT** |
| Reference scan (`/etc/systemd`, `/etc/cron.d`, `/etc/cron.daily`, `/usr/local/sbin`, `/usr/local/bin`) | No references to `/etc/ferrumgate/gcs-service-account.json` |

#### Removal

```bash
sudo rm -f /etc/ferrumgate/gcs-service-account.json
```

| Check | Result |
|-------|--------|
| OLD_KEY_AFTER | **ABSENT** |

#### Post-Removal Keyless Verification

Executed with isolated environment (empty HOME, no key env):

| Probe | Result |
|-------|--------|
| KEYLESS_LS (`gsutil ls` backup bucket) | **PASS** |
| KEYLESS_CP (`gsutil cp` test object) | **PASS** |
| KEYLESS_CLEANUP (remove test object) | **PASS** |
| Offsite sync (`ferrumgate-offsite-backup-sync.sh`) | **PASS** — `OFFSITE_SYNC_RC=0` |

### 1.11 e2 Revert Attempt and n2 Rollback

Operator attempted to revert the temporary `n2-standard-2` machine type back to the original `e2-medium` for cost optimization.

| Step | Command / Action | Result |
|------|-----------------|--------|
| Preflight | VM RUNNING on `n2-standard-2`; static IP `34.158.51.8`; scopes include `devstorage.read_write` | ✅ Ready |
| Pre-revert snapshot | `ferrumgate-pre-e2-revert-20260516` | ✅ Created |
| Stop VM | `gcloud compute instances stop ferrumgate-nonprod` | ✅ Succeeded |
| Set machine type | `gcloud compute instances set-machine-type ferrumgate-nonprod --machine-type=e2-medium` | ✅ Succeeded |
| Start VM (e2-medium) | `gcloud compute instances start ferrumgate-nonprod` | ❌ `ZONE_RESOURCE_POOL_EXHAUSTED` |
| Rollback machine type | `gcloud compute instances set-machine-type ferrumgate-nonprod --machine-type=n2-standard-2` | ✅ Succeeded |
| Start VM (n2-standard-2) | `gcloud compute instances start ferrumgate-nonprod` | ✅ Succeeded |
| External IP check | `networkInterfaces[0].accessConfigs[0].natIP` | `34.158.51.8` preserved |

#### Post-Rollback Health

| Check | Result |
|-------|--------|
| VM state | RUNNING on `n2-standard-2` |
| ferrumgate.service | active (running) |
| LOCAL_READYZ | HTTP 200 |
| LOCAL_DEEP | HTTP 200 |
| PUBLIC_READYZ | HTTP 200 |
| Residual key (`/etc/ferrumgate/gcs-service-account.json`) | **ABSENT** |

**Conclusion**: `e2-medium` remains unavailable due to zone capacity. `n2-standard-2` continues as the temporary operational machine type. Revert to `e2-medium` is deferred until zone capacity permits.

---

## 2. Block B — SendGrid Smoke-Test Status

### 2.1 Service State

| Component | Status |
|-----------|--------|
| `prometheus-alertmanager.service` | active (running) |
| AlertManager health endpoint | HTTP 200 |

### 2.2 Configuration State

| Item | Status |
|------|--------|
| SendGrid API key secret file | PRESENT at `/etc/ferrumgate/secrets/sendgrid-api-key` (content not disclosed) |
| AlertManager config | Contains SendGrid webhook receiver configuration |

### 2.3 Synthetic Alert Test

| Step | Result |
|------|--------|
| POST synthetic alert to AlertManager | HTTP 200 |
| Alert visible in AlertManager API | CONFIRMED |

### 2.4 SendGrid Inbox-Check Alert

Operator initiated an inbox-check alert to verify end-to-end delivery.

| Field | Value |
|-------|-------|
| TEST_ID | `fg-inbox-check-20260516-052910` |
| START_AT_UTC | `2026-05-16T05:29:10Z` |
| `prometheus-alertmanager.service` | active (running) |
| AlertManager health endpoint | HTTP 200 |
| Separate sendgrid/alert bridge service unit | Not listed (no dedicated bridge unit) |
| Synthetic alert POST | HTTP 200 |
| ALERT_VISIBLE in AlertManager API | YES |

**Operator confirmation (2026-05-16)**: Operator confirmed receipt of the inbox-check alert matching `TEST_ID=fg-inbox-check-20260516-052910`. The confirmation covers at least one contact/channel; secondary-contact confirmation remains pending unless separately verified.

**Operator-provided email content** (redacted — no full email address or recipient identifier):

| Field | Value |
|-------|-------|
| Subject | `FerrumGate Alert: FerrumGateInboxDeliveryCheck` |
| Status | `resolved` |
| Severity | `warning` |
| Service | `ferrumgate` |
| Body excerpt | `FerrumGate inbox delivery check` / `Synthetic non-secret alert for operator inbox confirmation` |

**Non-claim**: This artifact records operator-confirmed inbox receipt for at least one contact. It does **not** assert confirmed delivery to both primary and secondary contacts. Full G-B1/G-B2 closure requires explicit confirmation for each configured contact.

---

## 3. Block A — Real Owned Domain

**Status**: **BLOCKED** — no change.

- VM continues to use DuckDNS (`ferrumgate.duckdns.org`)
- Operator does not yet have a real owned domain
- DNS A record for a real domain pointing to `34.158.51.8` has not been configured
- Block A remains a P0 blocker before any external production exposure

---

## 4. Follow-Up Items

### Completed

| # | Item | Completed | Evidence |
|---|------|-----------|----------|
| 2 | Remove residual `/etc/ferrumgate/gcs-service-account.json` | 2026-05-16 | Post-removal keyless probe PASS; offsite sync PASS (rc=0) |
| — | SSH firewall restoration | 2026-05-16 | Restored to `118.69.4.63/32` after temporary opening |

### Active / Remaining

| # | Item | Owner | Priority | Status |
|---|------|-------|----------|--------|
| 1 | Revert VM from `n2-standard-2` to `e2-medium` when zone capacity permits | Operator | P1 — cost optimization | ☐ Deferred — `ZONE_RESOURCE_POOL_EXHAUSTED` for e2-medium and e2-small on 2026-05-16; n2-standard-2 remains temporary operational type |
| 3 | Confirm Block B inbox delivery (G-B1/G-B2) with real recipient | Operator | P0 — before unattended operation | 🟡 Partial — operator confirmed receipt of TEST_ID `fg-inbox-check-20260516-052910` for at least one contact; secondary-contact confirmation remains pending unless separately verified |
| 4 | Procure real domain and configure DNS A record for Block A | Operator | P0 — before external exposure | ☐ Blocked — no real domain available |
| 5 | Cost-review `n2-standard-2` vs `e2-medium` sustained pricing | Operator | P2 — budget | ☐ Pending zone capacity resolution |

---

## 5. Non-Claims

- **NOT production-ready**: This artifact records non-production VM evidence only.
- **NOT full production posture**: Block A (real domain) and Block B inbox delivery remain blocked.
- **NOT PostgreSQL production**: Remains deferred; single-node SQLite only.
- **NOT HA/multi-node**: Out of v1 scope.
- **NOT full inbox delivery confirmed**: Operator confirmed receipt for at least one contact. Delivery to all configured contacts (including secondary) is a separate pending gate.
- **NOT permanent machine type**: `n2-standard-2` is a temporary recovery state.

---

## 6. Cross-References

| Artifact | Purpose |
|----------|---------|
| `67-production-readiness-roadmap.md` | Updated blocker statuses and evidence gates |
| `artifacts/2026-05-15-r4-production-blocker-execution-runbook.md` | Block C exact commands and rollback |
| `artifacts/2026-05-15-r2-key-based-backup-risk-acceptance.md` | C1/C2 decision matrix and risk acceptance |
| `artifacts/2026-05-15-r1-alerting-rotation-policy.md` | Block B rotation policy |

---

*Artifact created: 2026-05-16. C1 keyless backup recovery and Block B status — audit trail only. No production-ready claim.*
