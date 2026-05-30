variable "auth_token" {
  description = "Bearer token for ferrumd API authentication. Generate with: openssl rand -hex 32"
  type        = string
  sensitive   = true
  default     = ""
}

variable "store_dsn" {
  description = "Database DSN. Example for SQLite: sqlite:///var/lib/ferrumgate/ferrumgate.db. Example for PostgreSQL: postgres://user:pass@localhost/ferrumgate"
  type        = string
  sensitive   = true
  default     = "sqlite:///var/lib/ferrumgate/ferrumgate.db"
}

variable "install_dir" {
  description = "Directory where generated artifacts will be written."
  type        = string
  default     = "./out"
}

variable "user" {
  description = "OS user that will run ferrumd."
  type        = string
  default     = "ferrumgate"
}

variable "group" {
  description = "OS group that will run ferrumd."
  type        = string
  default     = "ferrumgate"
}

variable "ferrumd_binary_path" {
  description = "Absolute path to the ferrumd binary."
  type        = string
  default     = "/usr/local/bin/ferrumd"
}

variable "config_path" {
  description = "Absolute path to the ferrumgate TOML config file."
  type        = string
  default     = "/etc/ferrumgate/ferrumgate.toml"
}

variable "data_dir" {
  description = "Directory for SQLite database and state files."
  type        = string
  default     = "/var/lib/ferrumgate"
}

variable "log_dir" {
  description = "Directory for log files."
  type        = string
  default     = "/var/log/ferrumgate"
}

variable "bind_address" {
  description = "IP address to bind ferrumd to."
  type        = string
  default     = "127.0.0.1"
}

variable "port" {
  description = "Port for ferrumd to listen on."
  type        = number
  default     = 8080
}

variable "postgres_enabled" {
  description = "Set to true to use PostgreSQL instead of SQLite."
  type        = bool
  default     = false
}

variable "generate_systemd" {
  description = "Generate a systemd service unit file."
  type        = bool
  default     = true
}

variable "generate_docker_compose" {
  description = "Generate a Docker Compose override file."
  type        = bool
  default     = false
}
