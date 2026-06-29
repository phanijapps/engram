#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

marker_pattern="TO""DO|\\[TO""DO"
if rg -n "$marker_pattern" README.md AGENTS.md docs contracts .codex/skills; then
  echo "unresolved documentation or skill placeholder markers found" >&2
  exit 1
fi

validator="/home/videogamer/.codex/skills/.system/skill-creator/scripts/quick_validate.py"
if [[ -f "$validator" ]]; then
  for skill in .codex/skills/*; do
    [[ -d "$skill" ]] || continue
    python3 "$validator" "$skill"
  done
else
  echo "warning: skill validator not found: $validator" >&2
fi

.codex/hooks/check-code-docs.sh

echo "documentation checks passed"
