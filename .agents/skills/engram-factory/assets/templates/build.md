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

<!--
  v2 template. House rules: references/templates.md §Writing rules.
  The builder's evidence ledger. Gate results are NOT restated here — link
  verification.md. Every task, finding, and deviation is accounted for.
-->

# Build: {{feature_title}}

| Field | Value |
|---|---|
| Reader / question | Reviewers and approver asking "what was built, executed, fixed, and honestly not done?" |
| Status | cycle {{cycle}} · snapshot {{snapshot_path}} |
| Budget | ≤ 300 narrative words. Tables carry the evidence. |

## What was built

> The implementation story in plain words. ≤ 150 words.

{{summary}}

## Execution evidence

> One row per task, from the task list. Commit + verify command + result.
> Enforcement: a task row without a commit or without a result fails acceptance.

| Task | Commit | Verified with | Result |
|---|---|---|---|
| TASK-001 | {{execution_evidence}} | `<command>` | one-line observable |

## Findings resolution

> One row per review finding, every round. This is the counterpart of tasks §Review rework tracking — both stay filled or both fail.

| Finding | Resolution | Proof |
|---|---|---|
| FIND-…-001 | fixed · documented · follow-up | commit / test / doc line |

## Deviations

> Every difference between plan and reality, each with a reason. An undocumented deviation found in review is a defect.

{{deviations}}

## Deferred and follow-ups

> `item — owner — trigger`. Deferred work is named, never silent.

{{followups}}

## Self-check

> Imperatives the builder ran before declaring done. Enforcement: approver spot-checks these against raw output.

*(Write: worktree clean and committed · every task verified with its own command · results recorded in verification.md · nothing pre-recorded PASS.)*
