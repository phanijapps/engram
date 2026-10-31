---
schema_version: 2
artifact: "tasks"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-tasks"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
plan_ref: null
---
<!--
  Tasks template v2 (constitution adoption 2026-10-31).
  v2 changes: checkbox task list maps directly to plan STEP IDs and REQ IDs
  (machine-checkable); REQ×task coverage matrix must be total (CI-verifiable);
  gate inventory tracked with raw evidence locations; review findings are
  post-review links, not pre-filled N/A rows. Tiering: S jobs may omit
  Delivery applicability when no row would be "yes".
  Verified by tools/ci/check_job_artifacts.py.
-->


# Tasks: <short outcome>

## Executable task list

| Done | ID | Imperative outcome | STEP IDs | REQ IDs | Gate to run | Evidence reference |
|---|---|---|---|---|---|---|
| [ ] | TASK-001 | <one independently verifiable action> | STEP-001 | REQ-001 | GATE-001 | `verification.md#gate-001` |

## Ordering and stop conditions

- TASK-001 depends on: `none`.
- Stop/escalate on: <named risk, failed gate, or required deviation>.
- Commit/release rule: only when the plan requires it; do not impose
  one-commit-per-task by default.

## REQ × task coverage matrix

| REQ ID | TASK IDs | AC IDs | Coverage status |
|---|---|---|---|
| REQ-001 | TASK-001 | AC-001 | total |

## Gate inventory

| Gate ID | Owning TASK | Status | Raw evidence location |
|---|---|---|---|
| GATE-001 | TASK-001 | NOT_RUN | `verification.md#gate-001` |

## Delivery applicability

| Concern | Applies? | TASK/DEV ID or reason |
|---|---|---|
| migration | no | <no persisted format changes> |
| authorization/denial | no | <reason> |
| release/operator handoff | no | <reason> |

## Review-finding links (post-review only)

| Finding ID | Disposition | TASK/DEV ID | Evidence |
|---|---|---|---|
| none yet | pending review | — | — |
