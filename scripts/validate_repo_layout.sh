#!/usr/bin/env bash
set -euo pipefail

required=(
  "docs"
  "contracts"
  "openapi"
  "schemas"
  "prompts"
  "crates"
  "bins"
)

for path in "${required[@]}"; do
  if [ ! -e "$path" ]; then
    echo "Missing required path: $path"
    exit 1
  fi
done

echo "Repository layout looks OK"
