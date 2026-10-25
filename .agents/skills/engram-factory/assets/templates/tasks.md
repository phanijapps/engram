---
schema_version: "1.0"
artifact_type: "tasks"
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
  Tasks are imperative STE-style one-liners. The acceptance map lives in
  analysis.md §Acceptance — this file links ids, never restates them.
-->

# Tasks: {{feature_title}}

| Field | Value |
|---|---|
| Reader / question | Builder asking "what exactly do I execute, in what order, and what proves each task done?" |
| Status | cycle {{cycle}} · decomposes plan steps · tests start NOT_RUN |
| Budget | One line per task. Detail goes in named blocks below the table. |

## Task list

> One outcome per task, independently verifiable. Every task names its proof.
> Enforcement: a task without an evidence command fails build acceptance.

| Task | Outcome (one sentence, imperative) | Depends on | Covers | Evidence |
|---|---|---|---|---|
| TASK-001 | {{first_task}} | — | AC-001 | `<command>` → observable |
| TASK-002 | {{task_rows}} | TASK-001 | AC-002 | `<command>` → observable |

## Execution rules

> The builder's contract, restated as imperatives. Enforcement: two consecutive failed verifications on one task → stop and report.

{{execution_rules}}

*(Write: work in dependency order · verify each task with its own command before the next · one commit per task, task id in the message · smallest diff that satisfies the task · record command, result, and reference for every check.)*

## Coverage

> Links only. Every AC from analysis appears exactly once, or is named in a reasoned omission. Every TEST row starts NOT_RUN.

| Links | Ids |
|---|---|
| AC coverage | {{ac_links}} |
| Test inventory | TEST-001..n — one line per case, status NOT_RUN |

## Delivery checklist

> One line each: task ids, or a reasoned N/A. Silence is not allowed — a missing line fails review.

{{delivery_checklist}}

*(Lines: configuration/data migration · permissions incl. denied-identity checks · integration/bulk/concurrency/failure handling · package completeness and reconciliation · release rehearsal and runbook · documentation and operator handoff · follow-ups with owners.)*

## Review rework tracking

> REQUIRED every review round — this table is the supersession record for findings. An empty "N/A" row while a round exists fails validation.

| Round | Finding | Fixed by | Evidence | State |
|---|---|---|---|---|
| {{cycle_round_or_na}} | {{finding_or_na}} | {{task_or_na}} | {{evidence_or_na}} | open/resolved |

## Glossary deltas

{{glossary_deltas}}
