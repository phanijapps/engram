#!/usr/bin/env bash
# Retirement gate: the broad memory in-memory adapter has been replaced by
# SQLite-backed local conformance stores. Historical docs may mention it, but
# active code and manifests must not depend on it again without a new spec/ADR.
set -euo pipefail

patterns='engram-store-memory|InMemoryMemoryService|InMemoryConsolidationExecutor|adapters/memory/inmem|memory/inmem'

hits="$(
  {
    find . \
      \( -path './target' -o -path './.git' -o -path './docs' -o -path './adapters/memory/inmem' \) -prune \
      -o -type f \
      \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' -o -name 'AGENTS.md' -o -name 'README.md' \) \
      -print
    find docs \
      \( -path 'docs/adr' -o -path 'docs/research' -o -path 'docs/rfcs' -o -path 'docs/implementation' -o -path 'docs/implementation-roadmap.md' -o -path 'docs/specs/retire-memory-inmem' -o -path 'docs/specs/retire-knowledge-inmem' -o -path 'docs/specs/in-memory-*' -o -path 'docs/specs/local-benchmark-smoke' -o -path 'docs/specs/local-runtime-examples' -o -path 'docs/specs/memory-contract-fixture-runners' -o -path 'docs/specs/workspace-responsibility-layout' -o -path 'docs/specs/memory-knowledge-boundaries' -o -path 'docs/specs/retrieval-composition-boundary' -o -path 'docs/specs/sqlite-knowledge-graph' -o -path 'docs/specs/temporal-cue-retrieval' -o -path 'docs/specs/knowledge-ingestion' -o -path 'docs/specs/accepted-retrieval-fixtures' -o -path 'docs/specs/belief-network' -o -path 'docs/specs/hierarchy-navigation' -o -path 'docs/specs/belief-contradiction-bitemporal' -o -path 'docs/specs/fastembed-query-provider' \) -prune \
      -o -type f \
      \( -name '*.md' -o -name '*.toml' \) \
      -print
  } | xargs -r grep -nE "$patterns" || true
)"

if [[ -n "$hits" ]]; then
  echo "check-memory-inmem-retired: FAILED - retired memory in-memory adapter is referenced by active files:" >&2
  echo "$hits" >&2
  exit 1
fi

echo "check-memory-inmem-retired: ok"
