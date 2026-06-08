# FerrumGate Helm Chart

> **⚠️ DISCLAIMER: LOCAL-SAFE SCAFFOLD ONLY**
>
> This Helm chart is a deployment scaffold for local evaluation and development.
> - Local evaluation and development only.
> - No HA, no managed PostgreSQL, no real domain configuration.
> - Secrets are placeholder-only; the operator must supply real values.
>
> See [`docs/guides/hosted-deployment.md`](../../../docs/guides/hosted-deployment.md) for scope.

---

## Prerequisites

- Kubernetes cluster (local or remote) OR `kind` / `minikube` for local testing.
- Helm 3.x installed.
- (Optional) `kubeconform` for manifest validation.

## Chart structure

```
deploy/helm/ferrumgate/
├── Chart.yaml
├── values.yaml
├── README.md
└── templates/
    ├── _helpers.tpl
    ├── deployment.yaml
    ├── service.yaml
    ├── secret.yaml
    ├── serviceaccount.yaml
    ├── ingress.yaml      # optional; disabled by default
    ├── hpa.yaml          # optional; disabled by default
    └── NOTES.txt
```

## Quick start (local dry-run)

```bash
cd deploy/helm/ferrumgate

# Validate chart syntax. Defaults are fail-closed, so pass a non-production token
# for local rendering.
helm lint . --set secrets.bearerToken=local-dry-run-token-not-for-production

# Render templates to stdout (no cluster required)
helm template ferrumgate . \
  --namespace ferrumgate \
  --create-namespace \
  --set secrets.bearerToken=local-dry-run-token-not-for-production

# Dry-run against a cluster (requires kubectl context)
helm install --dry-run --debug ferrumgate . \
  --namespace ferrumgate \
  --create-namespace \
  --set secrets.bearerToken=local-dry-run-token-not-for-production
```

## Local cluster smoke test (kind)

```bash
# Create a local cluster
kind create cluster --name ferrumgate-test

# Install the chart with default single-replica values (file-backed SQLite + PVC,
# bearer auth). Replace the token for anything beyond a local smoke test.
helm install ferrumgate ./deploy/helm/ferrumgate \
  --namespace ferrumgate \
  --create-namespace \
  --set secrets.bearerToken=local-kind-token-not-for-production

# Wait for pod readiness
kubectl wait --namespace ferrumgate \
  --for=condition=ready pod \
  --selector=app.kubernetes.io/name=ferrumgate \
  --timeout=60s

# Port-forward and test healthz
kubectl port-forward -n ferrumgate svc/ferrumgate 8080:8080 &
curl http://localhost:8080/v1/healthz

# Cleanup
helm uninstall ferrumgate --namespace ferrumgate
kind delete cluster --name ferrumgate-test
```

## Configuration

### Required operator overrides

Before any real deployment, the operator must override placeholder secrets:

```yaml
# my-values.yaml
secrets:
  bearerToken: "<generate with openssl rand -hex 32>"

config:
  authMode: "bearer"
  allowInsecureNonlocalBind: "false"
  storeDsn: "postgres://user:pass@pg-host:5432/ferrumgate"
```

Install with overrides:

```bash
helm install ferrumgate ./deploy/helm/ferrumgate -f my-values.yaml
```

To use an externally managed Kubernetes Secret instead of inline values:

```yaml
secrets:
  existingSecret: "ferrumgate-bearer-token"
  existingSecretKey: "bearer-token"
  bearerToken: ""
```

### Values reference

| Key | Default | Description |
|-----|---------|-------------|
| `replicaCount` | `1` | Number of replicas. Not HA-ready. |
| `image.repository` | `ferrumgate/ferrumd` | Container image. |
| `image.tag` | `Chart appVersion` | Image tag. |
| `service.type` | `ClusterIP` | Kubernetes service type. |
| `service.port` | `8080` | Service port. |
| `config.bindAddr` | `0.0.0.0:8080` | ferrumd bind address. |
| `config.storeDsn` | `sqlite:///var/lib/ferrumgate/ferrumgate.db` | Database DSN. SQLite is single-replica only. |
| `config.authMode` | `bearer` | `disabled`, `bearer`, `scoped`, or `oidc`. |
| `config.logFilter` | `info` | Log level. |
| `config.gitRepoRoots` | `""` | Empty disables Git mutation adapter. |
| `config.sqliteDbRoots` | `""` | Empty disables SQLite mutation adapter. |
| `persistence.enabled` | `true` | Creates or uses a PVC for `/var/lib/ferrumgate`. |
| `persistence.existingClaim` | `""` | Existing PVC name. Empty creates one. |
| `secrets.existingSecret` | `""` | Existing Secret name for bearer auth. |
| `secrets.existingSecretKey` | `bearer-token` | Secret key containing the token. |
| `secrets.bearerToken` | `CHANGE_ME_TO_A_SECURE_TOKEN` | Placeholder. Chart rendering fails in bearer mode until replaced or existingSecret is set. |
| `ingress.enabled` | `false` | Enable Ingress. Requires real domain + TLS. |
| `autoscaling.enabled` | `false` | HPA. Requires PostgreSQL; chart rejects SQLite multi-replica rendering. |

## Notes

- **Local evaluation only**: This chart packages ferrumd for K8s local testing.
- **Single replica by default**: SQLite rendering is rejected for multi-replica or autoscaling.
- **Single-tenant configuration**.
- For shared deployments, use External Secrets Operator, Vault, or cloud provider secret stores instead of inline `values.yaml` secrets.
- **NOT validated on all K8s distributions**: Tested only with `kind` locally.

## Related docs

- [`docs/guides/hosted-deployment.md`](../../../docs/guides/hosted-deployment.md)
- [`docs/PRODUCTION_NOTES.md`](../../../docs/PRODUCTION_NOTES.md)
- [`configs/ferrumgate.prod.toml`](../../configs/ferrumgate.prod.toml)
