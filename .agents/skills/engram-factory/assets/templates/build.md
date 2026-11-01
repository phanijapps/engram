---
schema_version: "1.0"
artifact_type: "build"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-builder"
profiles: {{profiles_json}}
source_snapshot: "{{snapshot_path}}"
input_artifacts: []
updated_at: "{{timestamp}}"
---

# Build: {{feature_title}}

<!--
  Build v3: the execution report — what the builder actually did, per TASK,
  with the deviation honesty the tasks template demands. snapshot-bound.
-->

| Status | cycle {{cycle}} · snapshot `{{snapshot_path}}` |

## What was built

{{summary}}

## Execution evidence

{{execution_evidence}}

## Findings resolution

<Each builder finding: what, disposition (fixed / deferred with owner+trigger
/ escalated). Or `none`>

## Deviations

{{deviations}}

## Deferred and follow-ups

{{followups}}

## Self-check

<The builder confirms: all TASK rows updated; coverage intact (every REQ/AC
in some row); verification.md complete; no scope beyond the spec.>
