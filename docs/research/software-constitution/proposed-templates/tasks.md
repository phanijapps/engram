<!--
PROPOSED v4 (not landed) — owner verification copy.
Merged from builder-workflow (gpt-6.1-sol) + lane-coherence (gpt-5.6-terra)
reviews (10-tasks-review-*.md, 2026-11-01). Fixes vs v3: AC IDs get their own
column; checkbox semantics defined; TASK rows are canonical coverage (compact
map until the gate derives it); verification.md is the SOLE result record (no
duplicate table here); build-stage re-completion lifecycle documented;
findings live in review/decision artifacts.
Integration: frontmatter union contract (JSON-per-line + {{render}}
placeholders); literal REQ/AC/STEP/GATE/TASK IDs for check_job_artifacts.py.
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

For S-tier, start with the task list; add an execution-constraint sentence
only when it changes how work proceeds.

| Done | ID | Imperative outcome / fixture path | STEP IDs | REQ IDs | AC IDs | Gates to run | Evidence reference |
|---|---|---|---|---|---|---|---|
| [ ] | TASK-001 | Make missing declared paths fail the gate and name each path (`tools/ci/check_engine_neutrality.py`). | STEP-001 | REQ-001 | AC-001 | GATE-001 | `verification.md#gate-001` |
| [ ] | TASK-002 | Author the fixture proving AC-001: two concrete test instances (missing-path fails named; present-path clean) in `tools/ci/tests/test_engine_neutrality.py`. | STEP-002 | REQ-002 | AC-001 | GATE-001 | `verification.md#gate-001` |

**Test scenarios are code, not documents** — the spec carries the Gherkin AC;
proving it is TASK work bound to the AC and gate it serves. Keep `[ ]` until
the bounded outcome is implemented and every required gate has recorded
evidence; `[x]` only for PASS or an explicitly authorized exception linked
here — an exception is not PASS. Every gate ID MUST resolve to a runnable
command/fixture in the spec or verification record.

## Execution order and stop condition

State each dependency or `none`. For regression fixtures, name the isolated
pre-fix baseline or run the fixture first; record an expected baseline
failure separately from an unexpected gate failure. Stop on an unexpected
required-gate failure, unresolved authority, or required deviation — record
TASK, blocker, owner, and the condition that permits resumption; do not
broaden scope silently.

## Build-stage re-completion

Tasks-stage author (`engram-tasks`): `source_snapshot: null`, current
spec/plan input hashes. Build-stage author (`engram-builder`): supplies the
current snapshot and required hashes, preserves IDs/mappings/coverage unless
escalating a changed fact, refreshes completion state and verification
links. At tasks stage all Done boxes are unchecked; the builder checks only
work completed and evidenced in verification.md.

## Coverage

TASK rows are the canonical links. Until semantic coverage validation is
implemented in the gate, keep one compact mapping — every `REQ-*` from the
spec with its covering TASKs, STEPs, and spec ACs; every listed AC MUST also
appear in a TASK row (reviewer-enforced until the gate derives it):

- REQ-001 → TASK-001 (STEP-001) → AC-001 → GATE-001
- REQ-002 → TASK-002 (STEP-002) → AC-001 → GATE-001

## Verification handoff

The builder records commands, expected and actual outcomes, status, and raw
run evidence in **verification.md — the sole mutable result record**; TASK
rows link exact anchors (`verification.md#gate-001`). Do not duplicate
NOT_RUN/PASS/FAIL state here. Preserve failed and superseded runs. Task
completion does not imply independent verification — that is the review's
judgment.

## Applicable delivery work (one line each, only when real)

- migration / authorization / release-handoff obligations linked to their
  TASK owner — or `none applies: <one-line reason>`.
- Reviewer findings and dispositions live in the review and decision
  artifacts; a rework cycle updates this plan only via the approved re-plan.
