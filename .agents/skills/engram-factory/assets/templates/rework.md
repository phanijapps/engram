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

# Rework cycle {{cycle}}: {{feature_title}}

<!--
  Rework v3: the owner's correction, verbatim-first. What resets, what
  survives (the brief ALWAYS survives — it is the intent contract), what
  happens next.
-->

## Human instruction (verbatim)

{{human_note}}

## What this resets

<Stages/artifacts re-opened this cycle — typically spec onward; evidence
archives preserved, never edited>

## What this preserves

- brief.md (intent contract, immutable); owner ratification; LED decisions
  unless the instruction explicitly reopens one.
- Prior cycles' evidence under history/ (read-only).

## Next

<The analyzer re-runs from the instruction; new cycle number; prior
REQ/AC/GATE IDs may be superseded — delta-tag per the spec template.>
