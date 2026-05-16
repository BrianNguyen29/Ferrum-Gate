# R2 — Key-Based Backup Risk Acceptance (Block C Backup)

> **Status**: Operator-owned appendix. NOT production-ready. NOT keyless backup executed.
> **Purpose**: Document the VM OAuth scope blocker, two resolution paths (C1 stop-start VM with GCS write scopes, C2 accept key-based risk with rotation), and the risk acceptance procedure.
> **Scope**: Single-node SQLite v1 conditional pilot. Docs-only; no secrets; no live mutation.
> **Blocked until**: Operator selects C1 or C2 and provides required inputs.

---

## 1. Current State

### 1.1 VM OAuth Scope Inspection Evidence

| Field | Value | Notes |
|-------|-------|-------|
| VM | `ferrumgate-nonprod` | GCP Compute VM |
| Project | `fairy-b13f4` | GCP project |
| Zone | `asia-southeast1-a` | GCP zone |
| External IP | `34.158.51.8` | Static IP |
| VM service account | `905477274418-compute@developer.gserviceaccount.com` | Default compute SA |
| OAuth scopes | `devstorage.read_only` present | **WRITE scopes NOT present** |
| `devstorage.read_write` | **ABSENT** | Blocks keyless GCS write |
| `cloud-platform` | **ABSENT** | Blocks broad GCP API access |

**Evidence command**:

```bash
gcloud compute instances describe ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --format='table(serviceAccounts)'
```

**Result**: The VM's attached service account does **not** have OAuth scopes that allow GCS write.
`gsutil rsync` to the offsite backup bucket will fail with a scope error unless:
- the VM scopes are updated via stop-start with `set-service-account` (C1), **or**
- a service account key is used (C2).

### 1.2 Existing Backup Evidence

| Evidence | Status | Reference |
|----------|--------|-----------|
| GCS offsite backup/restore | Exists but production-ready = **NO** | `artifacts/2026-05-12-sqlite-path2-target-host-partial-evidence.md` |
| Backup automation timer | Active; local backups created | `artifacts/2026-05-15-b3-b4-b5-delegated-signing-status.md` |
| Retention pruning | Verified with run id `20260515T1606Z-b3-retention` | `67-production-readiness-roadmap.md` P0.4 |

---

## 2. Path C1 — Stop-Start VM with GCS Write Scopes (Keyless, Preferred)

### 2.1 Description

Stop the VM, update the attached service account scopes using `gcloud compute instances set-service-account`, then start the VM.
This enables **keyless** backup using the VM's attached service account without deleting/recreating the VM.
If `set-service-account` fails or is unavailable, fall back to recreating the VM from a snapshot.

### 2.2 Pros and Cons

| Factor | C1 (Keyless) |
|--------|--------------|
| Security | No long-lived key file on disk; uses VM identity |
| Operational | Brief stop-start downtime; preserves disk, metadata, and network config |
| Risk | Scope change is reversible; DNS/IP remain stable |
| Complexity | Low–Medium — requires one gcloud command to update scopes |

### 2.3 Exact Command Sequence (Placeholder)

```bash
# WARNING: Stopping the VM will cause brief downtime.
# Ensure you have a maintenance window and a recent backup.

# 1. Verify current scopes
gcloud compute instances describe ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --format='value(serviceAccounts.scopes)'

# 2. Snapshot boot disk before any change
gcloud compute disks snapshot $(gcloud compute instances describe ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --format='value(disks[0].source)') \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --snapshot-names=ferrumgate-pre-c1-$(date +%Y%m%d%H%M%S)

# 3. Stop VM (DOWNTIME BEGINS)
gcloud compute instances stop ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4

# 4. Update service account and scopes (primary path — preserves VM)
gcloud compute instances set-service-account ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 \
  --service-account=905477274418-compute@developer.gserviceaccount.com \
  --scopes=storage-rw,logging-write,monitoring-write

# 5. Start VM (DOWNTIME ENDS)
gcloud compute instances start ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4

# 6. Verify service health
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'curl -s -o /dev/null -w "%{http_code}" http://localhost:19080/v1/healthz'
# Expected: 200

# 7. Verify keyless GCS write from VM
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil ls gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/'

# 8. Test backup sync
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil rsync -r /var/lib/ferrumgate/backups/ gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/'
```

#### Fallback C1b — Recreate VM from Snapshot (if set-service-account fails)

Use this **only** if Step 4 fails.

```bash
# Delete VM (keep disks)
gcloud compute instances delete ferrumgate-nonprod \
  --zone=asia-southeast1-a --project=fairy-b13f4 --quiet \
  --keep-disks=all

# Recreate from snapshot with proper scopes
gcloud compute instances create ferrumgate-nonprod \
  --zone=asia-southeast1-a \
  --project=fairy-b13f4 \
  --machine-type=e2-medium \
  --scopes=storage-rw,logging-write,monitoring-write \
  --service-account=905477274418-compute@developer.gserviceaccount.com \
  --tags=ferrumgate \
  --address=34.158.51.8 \
  --source-snapshot=ferrumgate-pre-c1-<TIMESTAMP> \
  # --metadata=... \
  # Add other flags to match original VM configuration.

# Re-deploy ferrumgate and Caddy; verify GCS access as in Step 7–8 above.
```

### 2.4 Rollback

| Step | Rollback |
|------|----------|
| Scope change caused issues | Stop VM, revert to original scopes with `set-service-account`, start VM |
| VM recreation failed (fallback) | Restore from snapshot; re-attach static IP |
| Service does not start | Check logs: `journalctl -u ferrumd -n 100`; verify env file and bearer token |
| GCS write still fails | Verify IAM binding: `gcloud projects get-iam-policy fairy-b13f4 --flatten='bindings[].members' --filter='bindings.role:roles/storage.objectAdmin'` |

---

## 3. Path C2 — Accept Key-Based Backup Risk with Rotation Procedure

### 3.1 Description

Operator explicitly accepts the risk of using a service account key JSON file stored on the VM.
The key is used by `gsutil` to authenticate GCS writes.
A rotation procedure mitigates the risk of key compromise.

### 3.2 Risk Acceptance Statement

> **Operator acceptance required**:
> I accept that a service account key file will reside on the VM filesystem at
> `/etc/ferrumgate/gcs-service-account.json`.
> I understand this key grants GCS object admin access to the backup bucket.
> I agree to rotate this key every 90 days or on suspected compromise,
> and to restrict VM access to authorized operators only.

**Signature block** (operator must sign):

| Field | Value |
|-------|-------|
| Accepted by | `___________________________` |
| Date | `___________________________` |
| Key rotation schedule | Every 90 days |
| Key storage permissions | `chmod 600`, owned by root |

### 3.3 Pros and Cons

| Factor | C2 (Key-Based) |
|--------|----------------|
| Security | Long-lived key on disk; requires rotation discipline |
| Operational | No VM recreation; zero downtime |
| Risk | Key exfiltration if VM is compromised |
| Complexity | Low — download key, upload to VM, configure gsutil |

### 3.4 Exact Command Sequence (Placeholder)

```bash
# 0. Define placeholders
# OPERATOR_BACKUP_SA_ID    = short ID for create, e.g., "ferrumgate-backup"
# OPERATOR_BACKUP_SA_EMAIL = full email, e.g., "OPERATOR_BACKUP_SA_ID@fairy-b13f4.iam.gserviceaccount.com"

# 1. Create a dedicated backup service account (RECOMMENDED)
#    Use OPERATOR_BACKUP_SA_ID (short ID) for create; do NOT use the full email.
gcloud iam service-accounts describe OPERATOR_BACKUP_SA_EMAIL \
  --project=fairy-b13f4 2>/dev/null || \
gcloud iam service-accounts create OPERATOR_BACKUP_SA_ID \
  --display-name="FerrumGate Backup" --project=fairy-b13f4

# 2. Grant the SA GCS bucket write permissions
#    Use the full email for IAM binding and key commands.
gcloud projects add-iam-policy-binding fairy-b13f4 \
  --member='serviceAccount:OPERATOR_BACKUP_SA_EMAIL' \
  --role='roles/storage.objectAdmin'

# 3. Create and download key (on secure admin host)
gcloud iam service-accounts keys create /tmp/ferrumgate-backup-key.json \
  --iam-account=OPERATOR_BACKUP_SA_EMAIL

# 4. Upload key to VM
gcloud compute scp /tmp/ferrumgate-backup-key.json \
  ubuntu@ferrumgate-nonprod:/tmp/ --zone=asia-southeast1-a --project=fairy-b13f4

# 5. Move key to secrets directory on VM
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo mkdir -p /etc/ferrumgate/secrets && \
   sudo mv /tmp/ferrumgate-backup-key.json /etc/ferrumgate/secrets/gcs-service-account.json && \
   sudo chmod 600 /etc/ferrumgate/secrets/gcs-service-account.json && \
   sudo chown root:root /etc/ferrumgate/secrets/gcs-service-account.json'

# 6. Activate key for gsutil (run on VM)
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'sudo bash -c "cat /etc/ferrumgate/secrets/gcs-service-account.json | gsutil auth activate-service-account - key_file=-"'

# 7. Verify GCS write from VM
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil ls gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/'

# 8. Test backup sync
gcloud compute ssh ubuntu@ferrumgate-nonprod --zone=asia-southeast1-a --project=fairy-b13f4 -- \
  'gsutil rsync -r /var/lib/ferrumgate/backups/ gs://ferrumgate-nonprod-backups-fairy-b13f4-20260509/ferrumgate/'
```

### 3.5 Key Rotation Procedure

| Step | Command / Action |
|------|-----------------|
| 1. Generate new key | `gcloud iam service-accounts keys create /tmp/new-key.json --iam-account=OPERATOR_BACKUP_SA_EMAIL` |
| 2. Upload and activate new key | Same as §3.4 steps 4–6, using `/tmp/new-key.json` |
| 3. Verify backup sync with new key | Same as §3.4 step 8 |
| 4. Revoke old key | `gcloud iam service-accounts keys delete OLD_KEY_ID --iam-account=OPERATOR_BACKUP_SA_EMAIL` |
| 5. Delete old key file from VM | `sudo rm -f /etc/ferrumgate/secrets/gcs-service-account.json.old` |
| 6. Record rotation evidence | Screenshot or log of successful `gsutil rsync` after rotation |

### 3.6 Rollback

| Step | Rollback |
|------|----------|
| Key does not work | Re-activate previous key file if kept as `.old`; verify IAM binding |
| SA deleted | Recreate SA with same email; re-grant IAM; generate new key |
| gsutil auth fails | Use `gsutil auth revoke` then re-activate with valid key |

---

## 4. Decision Matrix

| Criteria | C1 (Stop-start/keyless) | C2 (Key-Based) |
|----------|------------------------|----------------|
| Downtime acceptable? | Yes (brief stop-start) | No (zero downtime) |
| Operator GCP expertise | High (IAM + compute) | Low (key upload) |
| Security posture preference | Strongest (no key file) | Acceptable with rotation |
| Maintenance window available? | Yes | N/A |
| Production data volume imminent? | No (time to recreate) | Yes (need backup now) |

**Recommendation**: If a maintenance window is available and operator is comfortable with GCP compute operations, **C1 is preferred** for long-term security. If downtime is unacceptable or operator needs backup immediately, **C2 is acceptable with strict 90-day rotation and the risk acceptance signature above**.

---

## 5. Evidence Gates (Before Marking Block C Closed)

| Gate | Evidence Required |
|------|-------------------|
| G-C1 | Operator selects C1 or C2 and records decision with rationale |
| G-C2 | If C1: VM scopes updated via `set-service-account` to include `storage-rw`; `gsutil rsync` from VM succeeds without key file |
| G-C3 | If C2: Risk acceptance statement signed; key file present at `/etc/ferrumgate/secrets/gcs-service-account.json` with `chmod 600`; `gsutil rsync` succeeds |
| G-C4 | If C2: Key rotation procedure documented and schedule acknowledged |

---

## 6. Non-Claims

- NOT production-ready
- NOT keyless backup currently working (OAuth scope blocker confirmed)
- NOT service account key created or stored in repo
- NOT VM recreated or modified by this document
- This is a decision-support and risk-acceptance artifact only

---

*Artifact created: 2026-05-15. Key-based backup risk acceptance — docs-only, no secrets, no live mutation.*
