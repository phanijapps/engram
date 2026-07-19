#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

.codex/hooks/check-contracts.sh

required_decisions=(
  "docs/adr/0001-workspace-boundaries.md"
  "docs/adr/0002-language-selection-criteria.md"
)

for file in "${required_decisions[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "missing required ADR: $file" >&2
    exit 1
  fi
done

runtime_manifests=(
  "Cargo.toml"
  "package.json"
  "pyproject.toml"
  "go.mod"
)

if [[ ! -f "docs/adr/0003-implementation-stack.md" ]]; then
  for manifest in "${runtime_manifests[@]}"; do
    if [[ -e "$manifest" ]]; then
      echo "runtime manifest exists before stack ADR: $manifest" >&2
      echo "create docs/adr/0003-implementation-stack.md before starting implementation code" >&2
      exit 1
    fi
  done
fi

echo "pre-implementation checks passed"
