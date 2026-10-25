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

# Human rework instruction

## Verbatim request

{{human_note}}

## Mechanical effects

Prior approval invalidated; new approval cycle created. Preserve previous evidence.
Classify scope impact through the analyzer. Repeat analysis, plan and tasks before
implementation if requirements, acceptance or design change. Otherwise retain
accepted upstream artifacts and pass this instruction to the builder.
