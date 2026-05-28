output "install_dir" {
  description = "Directory containing all generated artifacts."
  value       = var.install_dir
}

output "env_file_path" {
  description = "Path to the generated ferrumd.env file."
  value       = var.auth_token != "" ? local_file.ferrumd_env[0].filename : null
}

output "systemd_service_path" {
  description = "Path to the generated systemd service file."
  value       = var.generate_systemd ? local_file.ferrumd_service[0].filename : null
}

output "config_file_path" {
  description = "Path to the generated ferrumgate.toml file."
  value       = local_file.ferrumgate_config.filename
}

output "docker_compose_path" {
  description = "Path to the generated Docker Compose file."
  value       = var.generate_docker_compose ? local_file.docker_compose[0].filename : null
}

output "dry_run_validation" {
  description = "Null resource trigger map for dry-run validation."
  value       = null_resource.validate_artifacts.triggers
}
