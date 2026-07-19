#!/usr/bin/env bash
# Retirement gate: the dedicated knowledge in-memory adapter has been replaced
# by SQLite-backed test stores. Historical docs may mention it, but active code
# and manifests must not depend on it again without a new spec/ADR.
set -euo pipefail

patterns='engram-store-knowledge-memory|InMemoryKnowledgeStore|adapters/knowledge/inmem|knowledge/inmem'

hits="$(
  {
    find . \
      \( -path './target' -o -path './.git' -o -path './docs' -o -path './adapters/knowledge/inmem' \) -prune \
      -o -type f \
      \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' -o -name 'AGENTS.md' -o -name 'README.md' \) \
      -print
    find docs \
      \( -path 'docs/specs/retire-knowledge-inmem' -o -path 'docs/research' -o -path 'docs/rfcs' -o -path 'docs/specs/memory-knowledge-boundaries' -o -path 'docs/specs/sqlite-knowledge-graph' -o -path 'docs/specs/workspace-responsibility-layout' \) -prune \
      -o -type f \
      \( -name '*.md' -o -name '*.toml' \) \
      -print
  } | xargs -r grep -nE "$patterns" || true
)"

if [[ -n "$hits" ]]; then
  echo "check-knowledge-inmem-retired: FAILED — retired knowledge in-memory adapter is referenced by active files:" >&2
  echo "$hits" >&2
  exit 1
fi

echo "check-knowledge-inmem-retired: ok"
