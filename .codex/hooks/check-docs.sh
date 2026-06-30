#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

marker_pattern="TO""DO|\\[TO""DO"
tracked_skill_files="$(git ls-files '.codex/skills/**' 2>/dev/null || true)"
if [[ -n "$tracked_skill_files" ]]; then
  # shellcheck disable=SC2086
  skill_marker_paths="$tracked_skill_files"
else
  skill_marker_paths=""
fi

# Only tracked repository docs and tracked repository skills are validated here.
# Developer-local untracked skills may contain examples or placeholders that are
# outside this repository's release surface.
if rg -n "$marker_pattern" README.md AGENTS.md docs contracts $skill_marker_paths; then
  echo "unresolved documentation or skill placeholder markers found" >&2
  exit 1
fi

validator="/home/videogamer/.codex/skills/.system/skill-creator/scripts/quick_validate.py"
if [[ -f "$validator" ]]; then
  mapfile -t tracked_skill_dirs < <(git ls-files '.codex/skills/**/SKILL.md' | xargs -r -n1 dirname | sort -u)
  for skill in "${tracked_skill_dirs[@]}"; do
    [[ -d "$skill" ]] || continue
    python3 "$validator" "$skill"
  done
else
  echo "warning: skill validator not found: $validator" >&2
fi

.codex/hooks/check-code-docs.sh

echo "documentation checks passed"
