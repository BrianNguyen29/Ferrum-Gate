# DEP-5 Helm Scaffold Evidence

> **Date**: 2026-05-20
> **Blocker ID**: `BLK-DEP-5`
> **Owner**: Engineering
> **Scope**: Local-safe scaffold only. No live cluster deploy. No production-ready claim.

---

## Summary

The Helm chart scaffold for FerrumGate was created at `deploy/helm/ferrumgate/`.
This artifact records the files produced, the validation steps taken, and the explicit non-claims.

## Files produced

| File | Purpose |
|------|---------|
| `deploy/helm/ferrumgate/Chart.yaml` | Chart metadata with disclaimer annotations |
| `deploy/helm/ferrumgate/values.yaml` | Default values (placeholder secrets, SQLite in-memory, auth disabled) |
| `deploy/helm/ferrumgate/README.md` | Operator-facing usage docs with non-claims |
| `deploy/helm/ferrumgate/templates/_helpers.tpl` | Helm naming helpers |
| `deploy/helm/ferrumgate/templates/deployment.yaml` | ferrumd Deployment manifest |
| `deploy/helm/ferrumgate/templates/service.yaml` | ClusterIP Service manifest |
| `deploy/helm/ferrumgate/templates/secret.yaml` | Secret manifest (placeholder token) |
| `deploy/helm/ferrumgate/templates/serviceaccount.yaml` | ServiceAccount manifest |
| `deploy/helm/ferrumgate/templates/ingress.yaml` | Ingress manifest (disabled by default) |
| `deploy/helm/ferrumgate/templates/hpa.yaml` | HPA manifest (disabled by default) |
| `deploy/helm/ferrumgate/templates/NOTES.txt` | Post-install notes |

## Validation performed

### 1. Static review

- All templates inspected for correct Helm syntax.
- No hard-coded secrets (only `CHANGE_ME_TO_A_SECURE_TOKEN` placeholder).
- No real domain references (default host: `ferrumgate.local`, Ingress disabled).
- `auth_mode` defaults to `disabled` with `allow_insecure_nonlocal_bind=true` for local demo safety.
- Security context includes `runAsNonRoot: true`, `allowPrivilegeEscalation: false`, `drop: [ALL]`.

### 2. `helm lint`

> **Status**: PASSED — Helm 3.15.4 was installed locally under `/tmp/opencode/helm-download/` for validation.

Command:

```bash
/tmp/opencode/helm-download/linux-amd64/helm lint deploy/helm/ferrumgate
```

Output:

```text
==> Linting deploy/helm/ferrumgate
[INFO] Chart.yaml: icon is recommended

1 chart(s) linted, 0 chart(s) failed
```

### 3. `helm template`

> **Status**: PASSED — Helm rendered the chart locally without syntax errors.

Command:

```bash
/tmp/opencode/helm-download/linux-amd64/helm template ferrumgate deploy/helm/ferrumgate
```

Observed rendered resources:

- `ServiceAccount`
- `Secret` with placeholder `CHANGE_ME_TO_A_SECURE_TOKEN`
- `Service`
- `Deployment`

Ingress and HPA are disabled by default and therefore do not render in the default values path.

### 4. `git diff --check`

> **Status**: PASSED — no trailing whitespace or conflict markers introduced.

### 5. `python3 scripts/check_contract_consistency.py`

> **Status**: PASSED — no contract/schema changes were made; script exits cleanly.

### 6. `bash scripts/validate_repo_layout.sh`

> **Status**: PASSED — repository layout validation passed after adding `deploy/helm/ferrumgate/`.

### 7. `bash scripts/validate_config_examples.sh`

> **Status**: PASSED — config example validation returned `=== ALL CHECKS PASSED ===`.

## Non-claims

- **NOT production-ready**: This is a scaffold, not a validated production deployment.
- **NOT live-cluster tested**: No `helm install` was executed against any cluster.
- **NOT HA**: Single replica, no StatefulSet, no leader election.
- **NOT secrets-managed**: Placeholder Secret inline; production should use External Secrets Operator / Vault / cloud provider secret store.
- **NOT multi-tenant**: Single-tenant configuration only.
- **NOT all K8s distributions validated**: Scaffold only; no EKS/GKE/AKS/OpenShift testing.

## Prerequisites for live cluster install (P.6)

1. Operator provides a real K8s cluster.
2. Operator replaces `secrets.bearerToken` with a cryptographically random token.
3. Operator sets `config.authMode=bearer` and `config.allowInsecureNonlocalBind=false`.
4. Operator provides a persistent `storeDsn` (PostgreSQL recommended for production; SQLite file for single-node pilot).
5. (Optional) Operator enables Ingress with a real domain and TLS.

## Signoff

- **Engineering**: Scaffold created and locally validated with `helm lint` and `helm template`.
- **Operator**: N/A — no operator action required for scaffold.

---

*End of artifact — DEP-5 Helm scaffold evidence (local-safe only).*
