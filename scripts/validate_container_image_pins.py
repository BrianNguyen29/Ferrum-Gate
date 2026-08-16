#!/usr/bin/env python3
"""Validate that managed container images are pinned to verified digests.

Scope:
  - Files explicitly managed for supply-chain pinning:
      .github/workflows/ci.yml
      Dockerfile
      docker-compose.postgres.yml
      docker-compose.postgres-demo.yml
      docker-compose.ha-local.yml
      scripts/run_secret_history_scan.sh
      scripts/setup_ha_local.sh
  - Images in scope (canonical tag -> verified digest):
      postgres:16
      prom/prometheus:v2.53.5
      prom/alertmanager:v0.27.0
      minio/minio:RELEASE.2025-01-20T14-49-07Z
      rust:1.95-bookworm
      debian:bookworm-slim
      ghcr.io/gitleaks/gitleaks:v8.24.0

Unsupported / out-of-scope surfaces (e.g. Helm charts, GitHub Actions service
variables, make target hints, Docker command examples in comments) are
explicitly allowed rather than falsely validated.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

# Canonical tag -> verified SHA-256 digest (lowercase, no prefix).
MANAGED_DIGESTS: dict[str, str] = {
    "postgres:16": "33f923b05f64ca54ac4401c01126a6b92afe839a0aa0a52bc5aeb5cc958e5f20",
    "prom/prometheus:v2.53.5": "7a34573f0b9c952286b33d537f233cd5b708e12263733aa646e50c33f598f16c",
    "prom/alertmanager:v0.27.0": "e13b6ed5cb929eeaee733479dce55e10eb3bc2e9c4586c705a4e8da41e5eacf5",
    "minio/minio:RELEASE.2025-01-20T14-49-07Z": "ed9be66eb5f2636c18289c34c3b725ddf57815f2777c77b5938543b78a44f144",
    "rust:1.95-bookworm": "6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1",
    "debian:bookworm-slim": "abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241",
    "ghcr.io/gitleaks/gitleaks:v8.24.0": "2bcceac45179b3a91bff11a824d0fb952585b429e54fc928728b1d4d5c3e5176",
}

MANAGED_FILES: list[Path] = [
    ROOT / ".github" / "workflows" / "ci.yml",
    ROOT / "Dockerfile",
    ROOT / "docker-compose.postgres.yml",
    ROOT / "docker-compose.postgres-demo.yml",
    ROOT / "docker-compose.ha-local.yml",
    ROOT / "scripts" / "run_secret_history_scan.sh",
    ROOT / "scripts" / "setup_ha_local.sh",
]

# SHA-256 digest regex: exactly 64 lowercase hex digits.
SHA256_RE = re.compile(r"^[a-f0-9]{64}$")


def expected_ref(canonical_tag: str) -> str:
    """Return the fully pinned image reference for a canonical tag."""
    return f"{canonical_tag}@sha256:{MANAGED_DIGESTS[canonical_tag]}"


def iter_managed_refs(content: str) -> list[tuple[str, str | None]]:
    """Find occurrences of managed canonical tags and their actual digest, if any.

    Returns a list of (canonical_tag, digest_or_none).  digest_or_none is the
    SHA-256 value found immediately after '@sha256:' if present, otherwise None.
    """
    findings: list[tuple[str, str | None]] = []
    for canonical_tag in sorted(MANAGED_DIGESTS, key=len, reverse=True):
        # Match the canonical tag, then optionally a digest.  Ensure the next
        # character is not a continuation of the image reference (e.g. a tag
        # suffix).  Docker image references can contain [a-zA-Z0-9_.-] in tags,
        # so we stop on any other character.
        escaped = re.escape(canonical_tag)
        pattern = re.compile(
            rf"(?<![a-zA-Z0-9_.\-/])({escaped})(?:@sha256:([a-f0-9]{{64}}))?\b(?![a-zA-Z0-9_.\-])"
        )
        for match in pattern.finditer(content):
            findings.append((canonical_tag, match.group(2)))
    return findings


def check_file(path: Path) -> list[str]:
    """Return validation errors for a single managed file."""
    errors: list[str] = []
    try:
        rel = path.relative_to(ROOT)
    except ValueError:
        rel = path.name
    if not path.exists():
        return [f"{rel}: managed file is missing"]

    content = path.read_text(encoding="utf-8")
    for canonical_tag, found_digest in iter_managed_refs(content):
        expected_digest = MANAGED_DIGESTS[canonical_tag]
        if found_digest is None:
            errors.append(
                f"{rel}: managed image {canonical_tag!r} is not pinned to a digest"
            )
        elif found_digest != expected_digest:
            errors.append(
                f"{rel}: managed image {canonical_tag!r} uses digest "
                f"{found_digest!r}, expected {expected_digest!r}"
            )

    return errors


def validate_all() -> list[str]:
    """Run the container image pinning validation across all managed files."""
    errors: list[str] = []
    for path in MANAGED_FILES:
        errors.extend(check_file(path))
    return errors


def main() -> int:
    errors = validate_all()
    if errors:
        print("CONTAINER IMAGE PINNING VALIDATION FAILED", file=sys.stderr)
        for error in errors:
            print(f" - {error}", file=sys.stderr)
        return 1

    print("CONTAINER IMAGE PINNING VALIDATION PASSED")
    print(f"  checked {len(MANAGED_FILES)} managed files; "
          f"{len(MANAGED_DIGESTS)} managed images")
    return 0


if __name__ == "__main__":
    sys.exit(main())
