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

<!--
  v2 template. House rules: references/templates.md §Writing rules.
  This artifact is the handoff to a future Release/Operations factory: it must
  stand alone for a reader who has never seen the job.
-->

# Release: {{feature_title}}

| Field | Value |
|---|---|
| Reader / question | A release operator asking "what ships, how do I undo it, and what am I accepting?" |
| Status | cycle {{cycle}} · rehearsal required before ready |
| Boundary | Source merge authorizes NO deployment or promotion |

## What ships

> One paragraph plus identity. Enforcement: approver binds this to the snapshot id.

{{what_ships}}

*(Shape: behavior delivered in plain words · exact commit/snapshot identity · surfaces touched.)*

## Rehearsal

> The fresh-run evidence: full gates on a clean store, matrix green. Link verification.md — do not restate results.

{{rehearsal}}

## Rollback

> Imperative steps, exact commands. Consumes plan §Recovery and rollback verbatim or explains the delta.

{{rollback_steps}}

## Accepted limitations and follow-ups

> Every known gap that ships anyway: `limitation — owner — revisit trigger`. No hidden asterisks.

{{limitations}}

## Operations notes

> What an operator watches after this lands: signals, growth paths, runbook links.

{{ops_notes}}

## Handoff manifest

> What a Release factory consumes from here: approved snapshot id · verification evidence · limitations list · promotion status (none until separately authorized).
