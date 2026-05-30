# FerrumGate Helm Chart

> **⚠️ DISCLAIMER: LOCAL-SAFE SCAFFOLD ONLY**
>
> This Helm chart is a deployment scaffold for local evaluation and development.
> - No production-ready claim.
> - No HA, no managed PostgreSQL, no real domain configuration.
> - Secrets are placeholder-only; the operator must supply real values.
> - Temporary domain remains conditional-only; a real owned domain is required for any production-ready claim.
>
> See [`docs/guides/hosted-deployment.md`](../../docs/guides/hosted-deployment.md) and [`docs/security/non-claims.md`](../../docs/security/non-claims.md) for scope.

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

# Validate chart syntax
helm lint .

# Render templates to stdout (no cluster required)
helm template ferrumgate . --namespace ferrumgate --create-namespace

# Dry-run against a cluster (requires kubectl context)
helm install --dry-run --debug ferrumgate . --namespace ferrumgate --create-namespace
```

## Local cluster smoke test (kind)

```bash
# Create a local cluster
kind create cluster --name ferrumgate-test

# Install the chart with default demo values (SQLite in-memory, auth disabled)
helm install ferrumgate ./deploy/helm/ferrumgate \
  --namespace ferrumgate \
  --create-namespace

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

### Values reference

| Key | Default | Description |
|-----|---------|-------------|
| `replicaCount` | `1` | Number of replicas. NOT HA-ready. |
| `image.repository` | `ferrumgate/ferrumd` | Container image. |
| `image.tag` | `Chart appVersion` | Image tag. |
| `service.type` | `ClusterIP` | Kubernetes service type. |
| `service.port` | `8080` | Service port. |
| `config.bindAddr` | `0.0.0.0:8080` | ferrumd bind address. |
| `config.storeDsn` | `sqlite::memory:` | Database DSN. In-memory = data lost on restart. |
| `config.authMode` | `disabled` | `disabled` or `bearer`. |
| `config.logFilter` | `info` | Log level. |
| `secrets.bearerToken` | `CHANGE_ME_TO_A_SECURE_TOKEN` | Placeholder only. |
| `ingress.enabled` | `false` | Enable Ingress. Requires real domain + TLS. |
| `autoscaling.enabled` | `false` | HPA. Not validated. |

## Non-claims

- **NOT production-ready**: This chart packages ferrumd for K8s, but production readiness requires operator signoff, real domain, SLO evidence, and more.
- **NOT HA**: Single replica by default; no StatefulSet or leader election.
- **NOT multi-tenant**: Uses single-tenant configuration.
- **NOT a substitute for secrets management**: In production, use External Secrets Operator, Vault, or cloud provider secret stores instead of inline `values.yaml` secrets.
- **NOT validated on all K8s distributions**: Tested only with `kind` locally.

## Related docs

- [`docs/guides/hosted-deployment.md`](../../docs/guides/hosted-deployment.md)
- [`docs/security/non-claims.md`](../../docs/security/non-claims.md)
- [`docs/PRODUCTION_NOTES.md`](../../docs/PRODUCTION_NOTES.md)
- [`configs/ferrumgate.prod.toml`](../../configs/ferrumgate.prod.toml)
