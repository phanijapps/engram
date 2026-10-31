# Proposed narrative-first SDD templates

These are proposals only; the live factory templates remain unchanged.
The redesign makes each artifact begin with the engineering case a reviewer needs to evaluate, then uses IDs as links inside that case rather than as a substitute for it.
The controller contract is retained: frontmatter is one JSON value per line, every rendered job has the common v2 metadata, requirements use EARS and BCP 14 language, trace IDs remain literal, and gate results plus REQ-to-task coverage remain tables.
S-tier authors keep only the headings that answer a live review question and target 60 lines or fewer; M and L tiers add evidence, alternatives, risks, and rollout narrative rather than empty scaffolding.
Each enumeration below includes a realistic worked row to teach the expected level of specificity; replace it before an artifact is `ready`.

## Proposed `brief.md`

```markdown
---
schema_version: 2
artifact: "brief"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-frame"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
owner: "OWNER-REF"
ratified_at: null
constitution_rules_touched: []
brief_ref: null
---

# Brief: <the owner’s outcome in one line>

## Why act now?

<Open with the affected user or system, the harmful outcome, and the bet this work makes. State the problem rather than prescribing a mechanism.>

For example: a declared engine-neutrality path can disappear and silently stop being scanned; fail-closed validation makes that loss visible in CI before a prohibited engine reference can ship.

## What did the owner ask for?

> <Quote the owner’s words verbatim. Do not paraphrase or merge an earlier request with a replacement request.>

The judgment moment is OUT-001: <state one observable success signal and who can judge it>.
For example, OUT-001 is satisfied when deleting a declared gated path makes the gate exit non-zero and name that path.

## What matters, and what does not?

Write priorities as a short argument, with literal IDs embedded in the sentences.
`MOS-001` is MUST because <required observable behavior>; `MOS-002` is SHOULD because <valuable but non-essential behavior>.
`MOS-003` is WON'T in this job: <explicit excluded behavior>.
For example, `MOS-001` requires a missing declared path to fail the gate, `MOS-002` requires a fixture proving absent and present paths, and `MOS-003` excludes adding newly gated layers.

**In scope:** <MOS/OUT IDs and bounded areas>.
**Out of scope:** <MOS IDs and explicit exclusions>.
**Open:** `none`, or the blocking `LED-*` IDs.

## Which decisions are already bound?

`LED-001` [HUMAN-DECIDED]: <choice>; the owner said “<verbatim bound>”. Revisit only when <concrete trigger>.
For example, `LED-001` [HUMAN-DECIDED] permits light review because this is an S-tier, one-file gate change; revisit on any reviewer escalation.
State each delegated boundary as `LED-*` [DELEGATED: <reason>], including what the later spec MAY decide and what it MUST NOT decide.

## What constrains the bet?

<Name the authority, deadline, or constitution rule and its practical effect in prose.>
For example, `EN-004` applies because engine-neutrality must remain fail-closed; `DW-7` makes the path-validation gap due in its stated window.
If no constitution rule is touched, write `constitution_rules_touched: []` and explain why the boundary is unaffected.

## Has the owner ratified this?

<Before ratification, state the missing owner action and keep `status` as `draft`.>
After ratification: owner reference `<channel/message>` on `<YYYY-MM-DD>` confirmed “<verbatim confirmation>”.
The spec receives this brief by content hash and MAY decide only the explicitly delegated `LED-*` boundaries.
```

## Proposed `spec.md`

```markdown
---
schema_version: 2
artifact: "spec"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-analyzer"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
constitution_rules_touched: []
brief_ref: null
required_reviewers: ["code", "security", "platform"]
---

# Spec: <short observable outcome>

## What problem are we solving, and why will this work?

<Open with the brief’s problem, the narrow behavioral bet, and the consequence of being wrong. Cite the brief IDs; do not repeat the brief wholesale.>
For example, consuming `MOS-001` by rejecting a missing declared path closes the silent-enforcement gap without expanding the set of gated layers.

## What did we learn, and what did we decide?

`EVD-001` proves <one falsifiable claim> at `<path:line-range or immutable source>`.
For example, `EVD-001` proves `scan()` skips a nonexistent declared path at `tools/ci/check_engine_neutrality.py:74`.
`DEC-001` [DELEGATED: <brief authority>] chooses <choice> over <rejected alternative> because <evidence-based reason>; the observable consequence is <effect>.
If the brief made the choice, say that no new `DEC-*` is needed rather than inventing one.

## What behavior MUST hold?

Requirements are the canonical behavior source. Use one EARS sentence per row and BCP 14 keywords only for normative obligations.

| ID | Priority | Testable statement | Rules | Evidence |
|---|---|---|---|---|
| REQ-001 | MUST | When a declared `GATED_PATH` does not exist on disk, the gate SHALL exit 1 and name the missing path. | EN-004 | EVD-001 |

Explain any interaction, ordering, or denial semantics the row cannot safely carry in prose: `REQ-001` fails before a missing path can be silently excluded from scanning.

## How will a reviewer recognize success or failure?

`AC-001` links `REQ-001`: given a path list containing a missing directory, when the gate runs, then it exits 1 and names that directory; with present-only paths, it exits 0.
Do not hide failure behavior: each `AC-*` MUST state the relevant denial, error, or regression case.

| Gate ID | Command or fixture | Proves AC IDs | Expected evidence |
|---|---|---|---|
| GATE-001 | `python3 tools/ci/tests/test_engine_neutrality.py` | AC-001 | two passing fixture cases, including missing-path failure |

## What remains outside this change?

**In:** `REQ-001` in `<named paths>`.
**Out:** <named behavior, path, or `MOS-*` ID>; `none` is valid.
**Deferred:** <owner, trigger, and `DEV-*` or `LED-*` ID>; never use an unnamed omission.
State compatibility directly: <public/internal/none> is <compatible/breaking/none> because <consumer or migration consequence>.

## What could invalidate the plan, and who must review it?

`RISK-001`: <scenario>; owner <role>; escalate when <observable trigger>.
Declare `required_reviewers` in frontmatter and justify every applicable discipline in prose.
For example, code reviews the changed gate, security confirms fail-closed enforcement, and platform confirms the fixture works in supported checkouts.
Add terms or environment facts only when they change requirement interpretation or verification.
```

## Proposed `plan.md`

```markdown
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
```

## Proposed `tasks.md`

```markdown
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
```
