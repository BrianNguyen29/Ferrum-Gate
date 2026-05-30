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
ENTRYPOINT ["ferrumd"]
