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

# Tasks: {{feature_title}}

## Dependency-ordered work

Use statuses TODO, IN_PROGRESS, BLOCKED, DONE. DONE requires evidence, not an agent assertion.

| Task | Outcome | Depends on | AC / NFR / decision | Components / native objects | Status | Owner role | Completion evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TASK-001 | {{verifiable_outcome}} | {{ids_or_none}} | {{traceability}} | {{allowed_components}} | TODO | engram-builder | {{required_evidence}}

## Task detail: TASK-001

Purpose and bounded change: {{purpose}}
Inputs and prerequisites: {{inputs}}
Allowed write scope and explicitly excluded changes: {{write_scope}}
Implementation steps and native configuration/code split: {{steps}}
Permission and negative cases: {{authorization_and_failures}}
Tests and expected outcomes: {{test_ids_and_results}}
Export/package/transport and manual configuration requirements: {{delivery_steps}}
Recovery or cleanup for task side effects: {{recovery}}
Completion criteria: {{observable_done}}
Blockers and escalation: {{blockers_or_none}}
Actual commit/export/run evidence, added by builder: {{actual_evidence_or_not_started}}

Repeat task detail for each nontrivial task. Keep small related changes together
when they share a verification boundary. Expose parallel work only when write
scopes and environment side effects do not overlap.

## Acceptance coverage

List every complete AC/NFR, task and test identifier explicitly in mapping rows;
avoid numeric ranges and omitted prefixes so literal searches preserve traceability.

| REQ / AC / NFR | Task IDs | Test IDs | Release / recovery step | Gap |
| --- | --- | --- | --- | --- |
| {{requirement_ids}} | {{tasks}} | {{tests}} | {{release_steps}} | {{gap_or_none}}

## Required delivery work

Configuration/data migration and reconciliation: {{task_ids_or_reasoned_na}}
Permission and denied-identity verification: {{task_ids}}
Integration, bulk, concurrency and failure handling: {{task_ids_or_reasoned_na}}
Package completeness and source reconciliation: {{task_ids}}
Release rehearsal, recovery evidence and runbook: {{task_ids}}
Documentation/operator handoff and follow-ups: {{task_ids}}

## Review rework tracking

| Cycle / round | Finding ID | Task ID | Resolution / commit / test | Status |
| --- | --- | --- | --- | --- |
| {{cycle_round_or_na}} | {{finding_or_na}} | {{task_or_na}} | {{evidence_or_na}} | {{state}}
