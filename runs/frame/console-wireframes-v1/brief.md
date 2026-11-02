---
schema_version: 2
artifact: "brief"
job_id: "console-wireframes-v1"
cycle: 1
status: "ratified"
author_role: "engram-frame"
profiles: []
source_snapshot: null
input_artifacts:
  - "runs/frame/console-wireframes-v1/feature.md"
updated_at: "2026-11-02T14:40:00-05:00"
size_tier: "S"
owner: "videogamer (project owner, pi session)"
ratified_at: "2026-11-02T14:40:00-05:00"
constitution_rules_touched: []
brief_ref: null
---

# Brief: professional wireframe set for the stunning console UI

> **Functional nature**: what the wireframe set must communicate, for whom, and
> why — no implementation. The console app itself is untouched.

## Why act now?

Job 012 shipped the read-only console (merged `94f90e06` + fix `49136b7e`): the
UI works but was built to spec, not to the professional design bar the owner
wants. Before any further UI implementation, establish the design contract —
wireframes every future console job builds against — so "stunning" is a
systematic property (tokens, contracts, states), not a coat of paint.

## The ask — in the owner's words

> as part of the next job I want you to use all the ui skills to only come up with wireframes I want this to be stunning ui console

> Q1 A , Q2 B , Q3 - I agree with you here too. This needs to be professional design. It will start read only but we will improve upon this

The judgment moment is OUT-001: when the owner opens the wireframe set in a
browser, every v1 console view and every future-trajectory view is represented
as a clickable, token-annotated frame meeting a professional design bar —
judged by the owner (dogfood, zero tooling).

## What matters, and what does not?

`MOS-001` is MUST: interactive HTML wireframe set — standalone, clickable,
dark/light themed — covering the v1 views (inventory → partition browse →
recent additions → knowledge graph → anomaly panel → binding-health line) plus
clearly-labeled future-trajectory frames (ingest/maintain job monitoring,
pruning review, central-deployment views — LED-004 of job 012's brief).
`MOS-002` is MUST: the UI skills are actually used and cited —
frontend-engineering pre-flight, archify format, component-contract per view,
token-architecture three-tier token layer, responsive-layout behavior,
ux-writing microcopy per state, a11y-engineering keyboard/focus contracts,
plus css-architecture/fe-status/rendering-strategy/fe-performance constraints
the frames must respect.
`MOS-003` is MUST: read-only posture is designed-in — every frame reflects the
v1 constraint (no write affordances; future frames label their write
capabilities as future) — with the improvement trajectory visible.
`SH-001` is SHOULD: frames annotate their anchors to the job-012 spec REQ ids
and the current app's real states (named degraded states appear in frames).
`C-001` COULD: motion/interaction notes; screenshot-ready hero states.
`W-001` WON'T: any change to apps/console code or any implementation commit.
`W-002` WON'T: new engine capabilities, schema changes, auth/user management.

**In scope:** MOS-001..003, SH-001, C-001.
**Out of scope:** W-001, W-002, and platform/deployment concerns.
**Open:** `none`.

## Decision ledger — already-bound choices

`LED-001` [HUMAN-DECIDED]: wireframes ONLY — owner: "use all the ui skills to
only come up with wireframes". No console implementation in this job.
`LED-002` [HUMAN-DECIDED]: interactive HTML wireframes (Q1 "A").
`LED-003` [HUMAN-DECIDED]: v1 views + labeled future-trajectory frames (Q2 "B").
`LED-004` [HUMAN-DECIDED]: S tier + light mode (Q3 "I agree with you here too").
`LED-005` [HUMAN-DECIDED]: professional design bar — "This needs to be
professional design. It will start read only but we will improve upon this."

## What constrains the bet?

Constitution: no rules touched (`constitution_rules_touched: []`) — design
artifacts only, no governed paths. The job-012 brief's ledger remains binding
context (especially LED-006 stunning, LED-004 trajectory, W-004..006
non-goals); wireframes must not contradict ratified boundaries (read-only v1,
no auth surfaces, no Tencent collaboration ontology).

## Owner suggestions — non-binding

- The existing `engram-visualize` D3 precedent and the shipped console's
  actual component/state inventory (apps/console in the repo) as grounding.
- TencentDB-Agent-Memory screenshots remain visual reference only (LED-008 of
  job 012).

## Consumption and supersession

- Consumption contract: the spec receives this brief by content hash; the
  decision ledger is BINDING; only explicitly `DELEGATED` boundaries may the
  spec decide (none — all rows decided).
- Rework supersession: standard.

## Owner ratification — the human gate

| Actor | Channel / message reference | Date | Verbatim confirmation |
|---|---|---|---|
| videogamer (owner) | pi factory session, wireframe framing reply | 2026-11-02 | "Q1 A , Q2 B , Q3 - I agree with you here too. This needs to be professional design. It will start read only but we will improve upon this" |

Fast-path ratification covers the complete brief as presented (form A, scope
B, S/light tier, professional bar, read-only start). The spec receives this
brief by content hash.
