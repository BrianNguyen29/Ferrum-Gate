# syntax=docker/dockerfile:1
#
# ⚠️  DISCLAIMER: Local demo Dockerfile only.
#     Builds ferrumd from source for local container demo.
#     NOT for production use. No multi-arch, no hardening, no secrets management.
#

# --- Build stage ---
FROM rust:1.95-bookworm AS builder

WORKDIR /app
COPY . .

ARG FEATURES=""
RUN cargo build --release --bin ferrumd ${FEATURES:+--features "$FEATURES"}

# --- Runtime stage ---
FROM debian:bookworm-slim

# OCI image metadata (overridable at build time). Local demo defaults only.
ARG REVISION="unknown"
ARG VERSION="dev"
LABEL org.opencontainers.image.title="ferrumd" \
      org.opencontainers.image.description="FerrumGate gateway daemon (local demo image)" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.source="https://github.com/FerrumGate/Ferrum-Gate" \
      org.opencontainers.image.licenses="Apache-2.0"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy default dev config so auto-load works if env vars are not set
COPY --from=builder /app/configs/ferrumgate.dev.toml ./configs/ferrumgate.dev.toml
COPY --from=builder /app/target/release/ferrumd /usr/local/bin/ferrumd

# Run as non-root for local demo
RUN useradd -m -u 1000 ferrumgate
USER ferrumgate

EXPOSE 8080

# Graceful shutdown for the gateway daemon (matches ferrumd SIGTERM handling).
STOPSIGNAL SIGTERM

# Image-level healthcheck against the shallow liveness endpoint; curl is
# installed above and the endpoint is intentionally dependency-free.
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=5 \
    CMD curl -f http://127.0.0.1:8080/v1/healthz || exit 1

ENTRYPOINT ["ferrumd"]
