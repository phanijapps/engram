<!--
PROPOSED (not landed) — owner verification copy from 06-redesigned-templates.md.
Integration: same frontmatter union contract as spec.md; STEP rows reference
REQ IDs (no orphan steps); verification map covers every AC.
-->

---
schema_version: 2
artifact: "plan"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-planner"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
spec_ref: null
constitution_rules_touched: []
---

# Plan: <short observable outcome>

## How does this design satisfy the spec?

<Open with the implementation argument: the chosen seam, why it meets the cited `REQ-*`, and why the rejected shape would be worse. Consume `DEC-*` by ID instead of restating requirements.>
For example, collect nonexistent declared paths during the existing scan and report them as violations; this preserves all existing pattern checks while making `REQ-001` fail closed.

## What changes, in what order, and how can we recover?

Each step is independently reviewable and names its requirements, rule, concrete paths, verification gate, and exact rollback.

| ID | Implement | REQ IDs | Rule IDs | Concrete paths | Verify | Rollback |
|---|---|---|---|---|---|---|
| STEP-001 | Collect missing declared paths and report each as a violation. | REQ-001 | EN-004 | `tools/ci/check_engine_neutrality.py` | GATE-001 | Revert the single behavior commit. |

Explain cross-step ordering in prose: `STEP-002` adds the fixture after `STEP-001` exposes the behavior; stop and escalate if its pre-fix failure or post-fix pass does not occur.
New consequential choices are `DEC-*` [DELEGATED: <authority>]: <decision, rejected alternative, and consequence>.

## What contracts and dependencies feel the change?

<State each affected surface and consumer in prose, including compatibility class and migration or documentation action.>
For example, this is an internal, compatible CI-gate behavior change; no stored format, public API, or consumer migration exists.
The blast radius is <concrete dependent paths or measured count>, not an adjective.
For example, `tools/ci/check_engine_neutrality.py` and `tools/ci/tests/test_engine_neutrality.py` are the two direct paths; a missing-path false positive is the failure shape owned by `STEP-001`.

## Does every acceptance criterion have a route to evidence?

| AC ID | Step IDs | Gate IDs | Omission or authorized deviation |
|---|---|---|---|
| AC-001 | STEP-001, STEP-002 | GATE-001 | none |

Describe partial-failure recovery after the table. For example, `STEP-001` and `STEP-002` have no persisted state; a single-commit revert restores the prior gate behavior.
