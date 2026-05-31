# FerrumGate Single-Node Terraform Module

> **Owner**: Engineering
> **Scope**: Local artifact generator only.

---

## Purpose

This module generates local deployment artifacts for a single-node FerrumGate
instance. It does **not** provision cloud infrastructure. It uses only the
`local_file` and `null_resource` providers so that no cloud credentials are
required.

## What it generates

- `ferrumd.env` — environment file with bearer token (mode `0600`)
- `ferrumgate.toml` — server configuration file
- `ferrumd.service` — systemd unit (optional)
- `docker-compose.ferrumgate.yml` — Docker Compose override (optional)

## Usage

```hcl
module "ferrumgate_single_node" {
  source = "./deploy/terraform/ferrumgate-single-node"

  install_dir         = "./out"
  auth_token          = var.my_token          # sensitive
  store_dsn           = var.my_dsn            # sensitive
  postgres_enabled    = true
  generate_systemd    = true
  generate_docker_compose = false
}
```

## Variables

| Name | Type | Default | Sensitive | Description |
|------|------|---------|-----------|-------------|
| `auth_token` | string | `""` | **yes** | Bearer token for ferrumd |
| `store_dsn` | string | `sqlite://...` | **yes** | Database DSN |
| `install_dir` | string | `./out` | no | Output directory |
| `user` | string | `ferrumgate` | no | OS user |
| `group` | string | `ferrumgate` | no | OS group |
| `ferrumd_binary_path` | string | `/usr/local/bin/ferrumd` | no | Binary path |
| `config_path` | string | `/etc/ferrumgate/ferrumgate.toml` | no | Config path |
| `data_dir` | string | `/var/lib/ferrumgate` | no | Data directory |
| `log_dir` | string | `/var/log/ferrumgate` | no | Log directory |
| `bind_address` | string | `127.0.0.1` | no | Bind address |
| `port` | number | `8080` | no | Listen port |
| `postgres_enabled` | bool | `false` | no | Use PostgreSQL |
| `generate_systemd` | bool | `true` | no | Generate systemd unit |
| `generate_docker_compose` | bool | `false` | no | Generate compose file |

## Notes

- **Local artifact generator only**: This module generates files. It does not harden the
  host, configure TLS termination, or set up automated backups.
- **NOT a managed service**: Self-hosted single-node template only.
- Single-node scope: one host only; clustering/failover are outside this module.
- **No real domain required**: Operates on loopback or IP bind by default.
- **No cloud providers**: `local_file` and `null_resource` only.

## Related docs

- [`docs/guides/hosted-deployment.md`](../../../docs/guides/hosted-deployment.md)
- [`docs/PRODUCTION_NOTES.md`](../../../docs/PRODUCTION_NOTES.md)
