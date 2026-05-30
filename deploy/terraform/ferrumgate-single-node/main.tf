# ferrumgate-single-node — Terraform local artifact generator
# Provider-neutral: uses local_file and null_resource only.
# No cloud credentials or remote providers required.

terraform {
  required_version = ">= 1.0"
}

# ------------------------------------------------------------------------------
# Local artifact generation
# ------------------------------------------------------------------------------

resource "local_file" "ferrumd_env" {
  count = var.auth_token != "" ? 1 : 0

  content = templatefile("${path.module}/templates/ferrumd.env.tmpl", {
    auth_token = var.auth_token
  })
  filename = "${var.install_dir}/ferrumd.env"
  file_permission = "0600"
}

resource "local_file" "ferrumd_service" {
  count = var.generate_systemd ? 1 : 0

  content = templatefile("${path.module}/templates/ferrumd.service.tmpl", {
    user           = var.user
    group          = var.group
    env_file       = "${var.install_dir}/ferrumd.env"
    config_path    = var.config_path
    binary_path    = var.ferrumd_binary_path
    data_dir       = var.data_dir
    log_dir        = var.log_dir
  })
  filename = "${var.install_dir}/ferrumd.service"
  file_permission = "0644"
}

resource "local_file" "ferrumgate_config" {
  content = templatefile("${path.module}/templates/ferrumgate.toml.tmpl", {
    bind_address   = var.bind_address
    port           = var.port
    data_dir       = var.data_dir
    log_dir        = var.log_dir
    postgres_enabled = var.postgres_enabled
    store_dsn      = var.store_dsn
  })
  filename = "${var.install_dir}/ferrumgate.toml"
  file_permission = "0644"
}

resource "local_file" "docker_compose" {
  count = var.generate_docker_compose ? 1 : 0

  content = templatefile("${path.module}/templates/docker-compose.yml.tmpl", {
    bind_address   = var.bind_address
    port           = var.port
    data_dir       = var.data_dir
    log_dir        = var.log_dir
    postgres_enabled = var.postgres_enabled
    store_dsn      = var.store_dsn
  })
  filename = "${var.install_dir}/docker-compose.ferrumgate.yml"
  file_permission = "0644"
}

# ------------------------------------------------------------------------------
# Dry-run validation (null_resource)
# ------------------------------------------------------------------------------

resource "null_resource" "validate_artifacts" {
  triggers = {
    env_hash    = var.auth_token != "" ? local_file.ferrumd_env[0].content_md5 : "no-auth-token"
    svc_hash    = var.generate_systemd ? local_file.ferrumd_service[0].content_md5 : "no-systemd"
    cfg_hash    = local_file.ferrumgate_config.content_md5
    compose_hash = var.generate_docker_compose ? local_file.docker_compose[0].content_md5 : "no-compose"
  }

  provisioner "local-exec" {
    command = <<-EOT
      echo "=== FerrumGate single-node artifact validation ==="
      echo "Install dir: ${var.install_dir}"
      echo "Systemd: ${var.generate_systemd}"
      echo "Docker Compose: ${var.generate_docker_compose}"
      echo "PostgreSQL: ${var.postgres_enabled}"
      echo "---"
      echo "Dry-run checks:"
      if command -v systemd-analyze >/dev/null 2>&1 && [ "${var.generate_systemd}" = "true" ]; then
        systemd-analyze verify "${var.install_dir}/ferrumd.service" || echo "Warning: systemd-analyze verify found issues (non-fatal for dry-run)"
      else
        echo "systemd-analyze not available; skipping service validation"
      fi
      if command -v docker >/dev/null 2>&1 && [ "${var.generate_docker_compose}" = "true" ]; then
        docker compose -f "${var.install_dir}/docker-compose.ferrumgate.yml" config >/dev/null 2>&1 || echo "Warning: docker compose config found issues (non-fatal for dry-run)"
      else
        echo "docker not available; skipping compose validation"
      fi
      echo "---"
      echo "Validation complete. No secrets were logged."
    EOT
  }
}
