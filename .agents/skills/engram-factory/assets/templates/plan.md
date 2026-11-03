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
updated_at: "{{timestamp}}"
size_tier: "S"
spec_ref: null
constitution_rules_touched: []
---
<!--
PROPOSED (not landed) — architecture-first plan-template redesign.
Integration: preserve this JSON-per-line frontmatter union and literal REQ/AC/STEP/GATE IDs; the factory parser consumes it unchanged.
-->


# Plan: <short observable outcome>

## What this plan does (owner view)

Two to four plain sentences: how the work will unfold, in what order, and
why that order wins. Then three plain lines. No IDs, no file paths, no
mechanism vocabulary. The owner must grasp the whole execution from this
section alone, in under thirty seconds.

<e.g. "Prove today's behavior first, so nothing changes unnoticed. Then
change exactly one seam — the gate that watches engine-bearing paths — and
prove the new behavior with the same kind of test. Nothing else moves.">

- Order: <plain arc, one sentence>
- How we know it worked: <the observable proof, product terms>
- If it goes wrong: <rollback in product terms, e.g. "Revert the change;
  yesterday's behavior returns immediately. No data is touched.">

## Approach argument (mandatory)

Make the engineering case before listing work: name the chosen seam, explain why
it satisfies each cited `REQ-*`, consume applicable `DEC-*`, and state the
rejected alternative and its consequence inline. S tier is this argument,
steps, and at most one relevant diagram; M/L adds the architecture views below
only when they clarify a real boundary or runtime concern—never blank sections.

**Worked example — job 011.** Change the scan seam in
`tools/ci/check_engine_neutrality.py` to collect a nonexistent declared path as
a named violation. That fail-closed behavior satisfies `REQ-001`; NFR-001 (reliability)
keeps present-path scanning intact. Silently skipping the path is
rejected because it permits an unscanned engine-bearing surface to ship. No
`DEC-*` is consumed: the spec determines this gate-only shape.

## Who this change serves (M/L — stories and boundary)

No diagrams. Two structures carry everything a context diagram used to:

**Stories (BMAD form).** One row per user-visible capability, each tied to
the REQ it delivers. If a story cannot name a REQ, the work is scope creep;
if a REQ has no story, ask the owner who it serves.

| Story | As a… | I want… | so that… | Delivers |
|---|---|---|---|---|
| S-1 | maintainer | the neutrality gate to fail loudly when a watched path disappears | no engine-bearing surface ships unscanned | REQ-001 |
| S-2 | CI runner | a stable exit contract (0 clean / 1 named violation) | pipeline steps stay simple | REQ-001, NFR-001 |

**Boundary contract (replaces the C4 picture).** Three columns: what we
change, what we consume frozen, and the contract that crosses. A cell that
needs a paragraph becomes a named detail block below.

| Inside (we change) | Outside (frozen, consumed) | Contract crossing |
|---|---|---|
| `check_engine_neutrality.py` scan behavior | repository declared paths | paths are read-only scan input |
| the gate's fixture | CI pipeline step | exit 0/1 + named violation on stdout |

## How it runs (M/L — scenarios)

No sequence diagrams. Each runtime or failure flow becomes a named scenario
in GIVEN/WHEN/THEN prose (OpenSpec form) — concrete, checkable, and
diff-friendly. Cover at least: the happy path, one failure path, one
degraded/empty path. Every scenario names the STEP that implements it.

#### Scenario: declared path present
- GIVEN the repository declares a gated path that exists
- WHEN the gate runs
- THEN patterns scan as today; exit 0 (STEP-001)

#### Scenario: declared path missing
- GIVEN a declared path does not exist on disk
- WHEN the gate runs
- THEN a named missing-path violation joins the aggregate; exit 1 (STEP-001)

#### Scenario: fixture proves both
- GIVEN the fixture exercises the public gate behavior
- WHEN run pre-fix, the missing-path case fails for the right reason; WHEN
  run post-fix, both cases pass (STEP-002)

## Governed implementation, verification, and rollback (mandatory)
Order independently reviewable work so later evidence depends on earlier
behavior. Every step must cite one or more `REQ-*`; every requirement in the
referenced spec must occur in a STEP row. A new consequential choice is
`DEC-* [DELEGATED: authority]`: decision; rejected alternative; consequence.

| ID | Imperative | REQ IDs | Rule IDs | Concrete paths | Verify (GATE) | Rollback |
|---|---|---|---|---|---|---|
| STEP-001 | Collect missing declared paths as named violations without changing existing pattern checks. | REQ-001 | EN-004 | `tools/ci/check_engine_neutrality.py` | GATE-002 | Revert the behavior commit. |
| STEP-002 | Add present-path and missing-path fixture coverage through the gate boundary. | REQ-001 | EN-004 | `tools/ci/tests/test_engine_neutrality.py` | GATE-001 | Revert the fixture commit. |

Stop and escalate if the pre-fix missing-path fixture does not fail or the
post-fix fixture does not pass: that invalidates the seam argument rather than
just a test expectation. Roll back in reverse dependency order; job 011 has no
persisted state, schema migration, or partial deployment, so reverting the two
commits restores the prior gate behavior.

### Execution batches (mandatory)

Implementation work is dispatched to the builder in batches — one agent
window per batch, each batch ending at a task boundary with committed work
and a one-line state note. Keep each batch at or under about twelve tasks
(or one agent window of gate runs). If the implementation cannot fit in
three batches, do NOT author a monster plan: flag a split recommendation
at the top of the owner view ("this job wants to be N jobs") and escalate
to the orchestrator.

| Batch | Steps | Why this batch |
|---|---|---|
| 1 | STEP-001, STEP-002 | One seam, one proof; S-tier fits a single window. |

## Acceptance-to-evidence map (mandatory)
Every `AC-*` needs at least one STEP and a named runnable GATE. State an
approved omission/deviation explicitly; an empty cell is not evidence.

| AC ID | Step IDs | Gate IDs | Omission or authorized deviation |
|---|---|---|---|
| AC-001 | STEP-001, STEP-002 | GATE-001: `python3 tools/ci/tests/test_engine_neutrality.py`; GATE-002: `python3 tools/ci/check_engine_neutrality.py` | none |

## Quality attributes met by design (mandatory)

How the design delivers each spec NFR — mechanism, not restatement:
- NFR-001 (reliability) → fail-closed collection in scan() before any skip
  path exists; proven by GATE-001's missing-path case.
Or `no NFRs declared; spec quality line is none beyond baseline` with reason.

## Outside this change (mandatory — one line)

Mirror the spec's out-list by reference: <"see spec Outside this change" plus
any plan-level exclusions (tools, experiments, refactors)>.

## Deployment safety (mandatory — one short block)

State rollout order, mixed-version and persisted-state compatibility, rollback
trigger/owner, and irreversible effects; give a reason when none apply.

## Contracts and blast radius (mandatory)
State compatibility, consumers, and migration action concretely; say `none`
only with a reason. Worked example: this is a compatible internal CI-gate
behavior change. Consumers are `tools/ci/check_engine_neutrality.py` and its
fixture; no stored format, public API, or migration changes. The owned failure
shape is a false failure in a stripped checkout, surfaced as the named path.
