---
schema_version: "1.0"
artifact_type: "rework"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "ready"
author_role: "orchestrator"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "{{timestamp}}"
---

<!--
  v2 template. The human instruction artifact — verbatim note, minimal framing.
  House rules: references/templates.md §Writing rules.
-->

# Rework cycle {{cycle}}: {{feature_title}}

## Human instruction (verbatim)

> {{human_note}}

## What this resets

- Cycle counter advances; round returns to zero.
- Prior approval and snapshot are void.
- Analysis, plan, and tasks are re-validated from scratch — changed acceptance is not a builder fix.

## What this preserves

- Source changes in the worktree — the analyzer assesses them.
- All prior evidence and review history — archived, never edited.

## Next

- Analyzer re-enters with the instruction above as input.
- Prior-cycle artifacts are archived under history/ before new ones are scaffolded.
