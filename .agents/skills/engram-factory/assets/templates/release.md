---
schema_version: "1.0"
artifact_type: "release"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-tasks"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "{{timestamp}}"
---

# Release: {{feature_title}}

<!--
  Release v3: the ship record. What changed (by ID), how it was rehearsed,
  how it unwinds. L3 events close the audit chain at merge.
-->

## What ships

{{what_ships}}

## Rehearsal

{{rehearsal}}

## Rollback

{{rollback_steps}}

## Evidence chain (L3)

- job.released event: `.engram-factory/jobs/<job-id>/events.jsonl` carries
  the merge commit + artifact hashes (rail 06).
- Deviations opened/closed this job: <none or DW/DEV IDs>.

## Accepted limitations and follow-ups

{{limitations}}

## Operations notes

{{ops_notes}}

## Handoff manifest

<One line per consumer surface: who must do what after merge — or
`none: no runtime/packaging surface touched`>
