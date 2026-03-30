# 15 — Deployment and operations

See also:
- [Single-Node Operations Runbook](./18-single-node-operations-runbook.md)
- [v1 Single-Node Operator Checks](./20-v1-single-node-operator-checks.md)

## Configuration

### Config file precedence
1. CLI arguments (highest priority)
2. Environment variables (`FERRUMD_*`)
3. Config file
4. Defaults (lowest priority)

### Config file locations
- Development: `configs/ferrumgate.dev.toml` (auto-loaded if present)
- Production: Specify via `--config` or `FERRUMD_CONFIG` env var

### Supported config fields

```toml
[server]
bind_addr = "127.0.0.1:8080"     # Socket address to bind to
store_dsn = "sqlite::memory:"    # Store DSN (sqlite::memory:, sqlite://file.db, etc.)
auth_mode = "disabled"           # "disabled" or "bearer"
bearer_token = ""                # Token for bearer auth mode
allow_insecure_nonlocal_bind = false  # Allow non-loopback bind when auth disabled
log_filter = "info"              # Log filter (debug, info, warn, error)
```

### Environment variables
- `FERRUMD_CONFIG` - Path to config file
- `FERRUMD_BIND_ADDR` - Bind address
- `FERRUMD_STORE_DSN` - Store DSN
- `FERRUMD_AUTH_MODE` - Auth mode
- `FERRUMD_BEARER_TOKEN` - Bearer token
- `FERRUMD_ALLOW_INSECURE_NONLOCAL_BIND` - Allow insecure bind
- `FERRUMD_LOG_FILTER` - Log filter

## Development
- Single process
- SQLite local or memory
- `auth_mode = "disabled"` acceptable
- `bind_addr = 127.0.0.1:8080`

## Staging / production-like
- Persistent store
- `auth_mode = "bearer"` with secure token
- `bind_addr = 0.0.0.0:8080` (requires bearer auth)
- provenance bat
- rollback bat
- strict manifest pinning nen bat
- logs khong lo secrets

## TLS

The API server does not terminate TLS. For production:
- Deploy behind TLS-terminating proxy (nginx, cloud LB)
- Or use platform-native TLS (e.g., Kubernetes ingress)

## Operations checklist
- Policy bundle dung environment
- Rollback khong bi tat
- Sanitize/DLP bat
- TTL hop ly
- Lineage query usable
- Bearer token securely stored (env var or secrets manager)
- Non-loopback bind only with `auth_mode = "bearer"`

## CLI (ferrumctl)

```bash
# Server URL and auth
export FERRUMCTL_SERVER_URL=http://localhost:8080
export FERRUMCTL_BEARER_TOKEN=your_token

# Commands
ferrumctl server health              # Health check
ferrumctl server inspect-execution <id>  # Get execution
ferrumctl server inspect-approvals   # List approvals (pagination and filtering via HTTP API at /v1/approvals)
ferrumctl server inspect-approval <id>  # Get approval
ferrumctl server inspect-lineage <id>   # Get lineage (text)
ferrumctl server inspect-lineage <id> --format json   # Get lineage as JSON
ferrumctl server inspect-lineage <id> --format dot --output lineage.dot   # Export as DOT (Graphviz)
ferrumctl server inspect-provenance --intent-id <intent_id>   # Query provenance events (intent-id-only via CLI; richer filters via POST /v1/provenance/query)
```
