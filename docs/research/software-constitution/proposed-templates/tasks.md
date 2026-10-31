<!--
PROPOSED (not landed) — owner verification copy from 06-redesigned-templates.md.
Integration: same frontmatter union contract; TASK rows reference STEP+REQ IDs;
coverage section must satisfy the coverage-totality check (critic criterion 4
allows compact prose mapping - gate matches IDs, not table shape).
-->

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
constitution_rules_touched: []
---

# Tasks: <short observable outcome>

## What will the builder do, and what proves each action is done?

<Open with the delivery argument: name the ordered implementation seam, the user-visible result, and the condition that stops work rather than allowing a silent deviation.>
For example, deliver fail-closed path validation before its fixture, and stop if the new fixture does not fail against the pre-fix behavior or pass against the fix.

| Done | ID | Imperative outcome | STEP IDs | REQ IDs | Gate to run | Evidence reference |
|---|---|---|---|---|---|---|
| [ ] | TASK-001 | Make missing declared paths fail the gate and name each path. | STEP-001 | REQ-001 | GATE-001 | `verification.md#gates` |
| [ ] | TASK-002 | Author the fixture proving AC-001: two concrete test instances (missing-path fails named; present-path clean) in `tools/ci/tests/test_engine_neutrality.py`. | STEP-002 | REQ-002, AC-001 | GATE-001 | `verification.md#gates` |

**Test scenarios are code, not documents.** The spec carries the Gherkin AC;
here the work of proving it is an ordinary TASK row bound to the AC and gate
it serves (IDs + fixture path). No separate test-scenario artifact exists in
the pipeline; the format is whatever the owning suite uses (unittest, cargo
test, fixtures), and `NOT_RUN` below is the only honest pre-execution state.

## What is the execution order and the stop condition?

`TASK-002` depends on `TASK-001`; <state every other dependency or `none`>.
Stop and escalate on <named failed gate, risk, or required deviation>; do not silently broaden scope.
A commit or release is required only when the plan says so; this template does not impose one commit per task.

## Does every requirement have total task coverage?

| REQ ID | TASK IDs | AC IDs | Coverage status |
|---|---|---|---|
| REQ-001 | TASK-001 | AC-001 | total |

The matrix MUST contain every `REQ-*` defined by the spec, with literal IDs rather than ranges.

## What did verification actually report?

Keep gate-to-result evidence tabular. `NOT_RUN` is honest before execution; a completed build records `PASS`, `FAIL`, or an authorized exception with its evidence location.

| Gate ID | Owning TASK | Result | Raw evidence location |
|---|---|---|---|
| GATE-001 | TASK-001 | NOT_RUN | `verification.md#gates` |

## What delivery concerns apply, and what did review find?

For each applicable concern, write a short sentence linking its `TASK-*` or `DEV-*` owner: migration, authorization or denial, and release/operator handoff.
For example, migration does not apply because no persisted format changes; authorization does not apply because the gate has no identity boundary; release handoff does not apply because CI runs the existing command.
After review, record each `FIND-*` in prose with its disposition, linked task or deviation, and evidence; before review, state `no findings yet` rather than manufacturing rows.
