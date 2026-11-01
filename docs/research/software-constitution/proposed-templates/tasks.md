<!--
Integration: frontmatter is the union contract: JSON-per-line render values and literal IDs remain discoverable to job gates.
v5 provenance: OpenSpec numbered deltas; spec-kit numbered checklist + test-column spirit; Kiro [P] + per-task verification.
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

## Checklist

1. [ ] <Imperative implementation action> (TASK-001, REQ-001, AC-001, STEP-001, GATE-001, `path/to/file`)
   - Verify: `GATE-001` proves the action; record the run at `verification.md#gate-001`.
1. [ ] [P] <Independent imperative action> (TASK-002, REQ-002, AC-002, STEP-002, GATE-002, `path/to/file`)
   - Verify: `GATE-002` proves the action; record the run at `verification.md#gate-002`.

## Rules

- `[x]` means PASS, or a linked authorized exception; an exception is not PASS.
- Stop on an unexpected required-gate failure, unresolved authority, or required deviation.
- `verification.md` is the sole result record; link its exact gate anchor here.
- Coverage: every `REQ-*` and `AC-*` appears in an item tag until a gate derives it.
- Example: `(TASK-001, REQ-001, AC-001, STEP-001, GATE-001, `path/to/file`)`.
- `[P]` means independent files and no unmet dependency; omit it otherwise.

## Worked example — job 011 fail-closed gate

1. [ ] Fail closed and name every missing declared `GATED_PATH` (TASK-001, REQ-001, REQ-002, AC-001, AC-002, STEP-001, GATE-001, GATE-002, `tools/ci/check_engine_neutrality.py`)
   - Verify: `GATE-001` proves the named missing path and `GATE-002` proves the clean surface; evidence is `verification.md#gate-001` and `verification.md#gate-002`.
   1.1 [ ] Add the isolated missing-path fixture (AC-001, GATE-001, `tools/ci/tests/test_engine_neutrality.py`)
   - Verify: `GATE-001` runs the fixture; evidence is `verification.md#gate-001`.
2. [ ] Add the missing-path-fails and present-path-passes regression cases (TASK-002, REQ-002, AC-001, AC-002, STEP-002, GATE-001, `tools/ci/tests/test_engine_neutrality.py`)
   - Verify: `GATE-001` passes both cases; evidence is `verification.md#gate-001`.
