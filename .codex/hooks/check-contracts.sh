#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

required_files=(
  "docs/domain-data-model.md"
  "docs/architecture.md"
  "docs/research/synthesis.md"
  "docs/rfcs/0001-memory-layer-scope.md"
  "docs/rfcs/0002-knowledge-source-extension.md"
  "contracts/schemas/memory-record.schema.json"
  "contracts/schemas/memory-query.schema.json"
  "contracts/v1/README.md"
  "contracts/v1/acceptance.md"
  "contracts/v1/compatibility.md"
  "contracts/v1/changelog.md"
  "contracts/v1/review-checklist.md"
  "contracts/v1/schemas/engram-v1.schema.json"
  "contracts/v1/schemas/memory-record.schema.json"
  "contracts/v1/schemas/retrieval-request.schema.json"
  "contracts/v1/schemas/context-payload.schema.json"
  "contracts/v1/schemas/write-memory-request.schema.json"
  "contracts/v1/schemas/write-memory-response.schema.json"
  "contracts/v1/schemas/forget-request.schema.json"
  "contracts/v1/schemas/forget-result.schema.json"
  "contracts/v1/schemas/evaluation-fixture.schema.json"
  "contracts/v1/examples/write-memory-request.json"
  "contracts/v1/examples/write-memory-response.json"
  "contracts/v1/examples/retrieval-request.json"
  "contracts/v1/examples/context-payload.json"
  "contracts/v1/examples/forget-request.json"
  "contracts/v1/examples/forget-result.json"
  "contracts/v1/examples/evaluation-fixture.json"
  "contracts/v1/examples/invalid/write-memory-request.missing-scope-tenant.json"
  "contracts/v1/examples/invalid/write-memory-request.training-export.json"
  "contracts/v1/examples/invalid/memory-record.missing-status.json"
  "contracts/v1/examples/invalid/memory-record.missing-provenance-actor.json"
  "contracts/v1/examples/invalid/retrieval-request.missing-requester.json"
  "contracts/v1/examples/invalid/context-payload.redacted-content.json"
  "docs/specs/README.md"
  "docs/specs/memory-contract-fixture-runners/spec.md"
  "docs/specs/accepted-retrieval-fixtures/spec.md"
  "docs/specs/forget-mode-contract-examples/spec.md"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "missing required contract file: $file" >&2
    exit 1
  fi
done

required_sections=(
  "## Contract Freeze Policy"
  "## Memory Model"
  "## Belief Network Model"
  "## Knowledge Model"
  "## Hierarchy Model"
  "## Retrieval Model"
  "## Operation Payloads"
  "## Invariants"
  "## V1 Acceptance Decisions"
  "## Deferred Questions For Extension Contracts"
)

for section in "${required_sections[@]}"; do
  if ! rg -q --fixed-strings "$section" docs/domain-data-model.md; then
    echo "missing required domain model section: $section" >&2
    exit 1
  fi
done

if command -v jq >/dev/null 2>&1; then
  while IFS= read -r json_file; do
    jq empty "$json_file" >/dev/null
  done < <(find contracts -name '*.json' -type f | sort)
else
  echo "warning: jq not found; skipped JSON syntax validation" >&2
fi

if rg -n '"training_export"' contracts/v1/schemas docs/domain-data-model.md; then
  echo "training_export is excluded from accepted v1 contract artifacts" >&2
  exit 1
fi

if command -v python3 >/dev/null 2>&1; then
  python3 scripts/validate_contracts.py
else
  echo "python3 is required for contract validation" >&2
  exit 1
fi

echo "contract checks passed"
