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
updated_at: "{{timestamp}}"
size_tier: "S"
plan_ref: null
constitution_rules_touched: []
---

# Tasks: <short observable outcome>

## What this job does (owner view)

One line per task, product language. The owner reads ONLY this section and
knows exactly what the job will do — before reading any executor detail.
Rules: verb first; name the visible change; no IDs, no file paths, no
mechanism vocabulary; one sentence per task; the section must stay readable
in under thirty seconds.

- TASK-001 — <Plain sentence, e.g. "Record today's passing state before anything is touched.">
- TASK-002 — <e.g. "Bring the approved colors and spacing into the app.">
- TASK-003 — <e.g. "Rebuild the app frame: sidebar, search bar, tabs, theme toggle.">

## Phase 1 — Tests first (TDD red)

Write the failing fixtures BEFORE implementation. Each item names the fixture
file, the AC it proves, and the expected RED state.

1. [ ] <Write failing fixture for AC> — create `<path/to/test_file>`, cases:
   <case A>, <case B>; expected RED pre-fix (TASK-001, REQ-001, AC-001,
   GATE-001, STEP-001)
   - Verify: `GATE-001` fails for the right reason; record at `verification.md#gate-001`.

## Phase 2 — Implementation (green)

Make the red fixtures pass. One commit-sized action per item; name files and
the seam being changed; no scope beyond the spec.

2. [ ] <Imperative implementation action> in `<path/to/source>` — open the
line with the observable change (what the product gains), then the file and
seam; one commit-sized action per item; no scope beyond the spec
(TASK-002, REQ-001, AC-001, GATE-001, STEP-001)
   - Verify: `GATE-001` now GREEN; all pre-existing gates still GREEN (regression).

## Phase 3 — Full verification

Run the whole gate inventory from the plan's acceptance map; record raw
output per gate in verification.md.

3. [ ] Run every GATE-* from the plan map; every AC has PASS evidence or a
   linked authorized exception (TASK-003, all REQ/AC/GATE IDs)
   - Verify: verification.md complete; exceptions are not PASS.

## Phase 4 — Delivery concerns (only when real)

4. [ ] <migration / authorization / release-handoff action + TASK owner, or
   `none applies: <reason>`> (TASK-004)

## Rules (the whole governance, seven lines)

- TDD discipline: Phase 1 RED before Phase 2 GREEN; an expected red is
  recorded, never "fixed" by weakening the test.
- `[x]` only for PASS or a linked authorized exception — an exception is not PASS.
- Unchecked at tasks stage; the build-stage re-completer (`engram-builder`)
  checks only work evidenced in verification.md.
- Stop on unexpected gate failure, unresolved authority, or required
  deviation: record TASK, blocker, owner, resumption condition.
- Coverage: every REQ and AC from the spec appears in some item's ID list
  (reviewer-enforced until the gate derives it).
- verification.md is the sole mutable result record; failed runs are kept.
- Findings/dispositions live in review and decision artifacts, not here.

## Worked example — job 011 fail-closed gate (S-tier)

Owner view:

- TASK-001 — Write the test that proves the gate catches a missing watch path.
- TASK-002 — Make the gate fail loudly when a watched path disappears.
- TASK-003 — Re-run every check and record the raw proof.
- TASK-004 — Nothing to ship beyond the repo (no data or release surface).

1. [ ] Write failing fixture `tools/ci/tests/test_engine_neutrality.py`:
   missing-declared-path must exit 1 naming the path; present-path scans
   clean. Expected RED: current gate silently skips missing paths
   (TASK-001, REQ-001, AC-001, GATE-001, STEP-002)
   - Verify: `python3 tools/ci/tests/test_engine_neutrality.py` fails on the
     missing-path case, passes the present-path case.
2. [ ] Fail closed in `tools/ci/check_engine_neutrality.py` scan(): missing
   declared path appends a named violation instead of `continue`
   (TASK-002, REQ-001, AC-001, GATE-001, STEP-001)
   - Verify: fixture GREEN; `check_engine_neutrality.py` GREEN on real tree.
3. [ ] Full battery + record per-gate output in verification.md (TASK-003,
   REQ-001, AC-001, GATE-001, GATE-002)
4. [ ] None applies: gate-only change, no migration/authz/release surface (TASK-004).
