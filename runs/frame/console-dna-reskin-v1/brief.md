---
schema_version: 2
artifact: "brief"
job_id: "console-dna-reskin-v1"
cycle: 1
status: "ratified"
author_role: "engram-frame"
profiles: []
source_snapshot: null
input_artifacts:
  - "runs/frame/console-dna-reskin-v1/feature.md"
updated_at: "2026-11-03T04:30:00+00:00"
size_tier: "M"
owner: "videogamer (project owner, pi session)"
ratified_at: "2026-11-03T04:30:00+00:00"
constitution_rules_touched: []
brief_ref: null
---

# Brief: bring the engram console app to the approved wireframe contract

> **Functional nature**: what the shipped app must look like and do visually,
> judged against the ratified design contract. Engine behavior is out of scope.

## Why act now?

Job 013 merged the owner-approved design contract to main
(`docs/design/console-wireframes/` — light-first DNA from the actual
TencentDB-Agent-Memory screenshots: #F8FAFC surfaces, #2563EB accent system,
6-12px rounded shadowed cards, underline tabs, header search, stat cards,
grid-backdropped graph, light-default with dark toggle). The live console app
(job 012, `apps/console`) still wears its original look. The owner directs:
make the app look like the contract, starting from the wireframes, using the
UI skills including the newly installed `better-*` set.

## The ask — in the owner's words

> next is to frame the requirement to make engram app look like that

> start with the wireframe and also all ui skills aI added better-*

The judgment moment is OUT-001: the owner opens the reskinned app and the
wireframe index side-by-side and the app reads as the contract realized —
light-first, blue accent, card language, tabs, search — judged by the owner
(dogfood, zero CLI).

## What matters, and what does not?

`MOS-001` is MUST: the app's six v1 views (inventory, partition browse,
recent additions, knowledge graph, anomaly panel, binding-health) visually
match the wireframe contract — light-default + dark toggle, #F8FAFC/#2563EB
token system, rounded shadowed cards, underline tabs, header search, stat
cards, grid-backdropped graph panel.
`MOS-002` is MUST: start FROM the wireframe set — the merged
`docs/design/console-wireframes/` is the reference implementation (tokens.css
as the token source adapted into the app's styling system; frames as the
layout reference per view).
`MOS-003` is MUST: the owner's `better-*` skills are used AND cited
(better-accessibility, better-colors, better-interface, better-layout,
better-typography, better-ui, better-writing — exact-value polish layered on
the contract), alongside the established UI skills (frontend-engineering,
token-architecture, component-contract, responsive-layout, ux-writing,
a11y-engineering, css-architecture, fe-status, rendering-strategy,
fe-performance).
`MOS-004` is MUST: all app behavior stays green — read-only guarantee,
origin pinning, named degraded states, isolation proofs, 50-test suite,
loopback-only surface; zero engine/schema changes (W-boundary).
`SH-001` is SHOULD: the contract's UI affordances wired to existing engine
reads (header search over partitions/views; underline tab navigation between
views) — no new engine capabilities.
`C-001` COULD: screenshot side-by-side evidence (app frame vs wireframe
frame) as verification artifacts.
`W-001` WON'T: any engine, adapter, schema, or MCP surface change; new
product capabilities; auth surfaces; the FUTURE frames (job monitoring,
pruning, central deployment) — those remain future increments.

**In scope:** MOS-001..004, SH-001, C-001.
**Out of scope:** W-001 in full.
**Open:** `none`.

## Decision ledger — already-bound choices

`LED-001` [HUMAN-DECIDED]: full contract parity, wireframe-first — owner:
"start with the wireframe" (Q1=B). The merged wireframe set is the normative
reference.
`LED-002` [HUMAN-DECIDED]: the owner's `better-*` skills are mandatory
method — owner: "also all ui skills aI added better-*".
`LED-003` [HUMAN-DECIDED]: the ask is visual contract realization for the
EXISTING v1 views — owner: "make engram app look like that" (the contract
being "that").
`LED-004` [DELEGATED: owner did not override the framing recommendation]:
theme behavior = light-default + dark toggle (contract-faithful); tier M;
light review mode. The spec MAY refine within the contract; it MUST NOT
raise scope (no FUTURE frames) and any reviewer escalation triggers revisit.

## What constrains the bet?

Constitution: no constitution rules touched by the brief itself
(`constitution_rules_touched: []`); implementation will honor standing rails
(engine-neutrality N/A — no engine change; PRD-107 N/A — no new capability
claims; the job-012 W-boundaries carry: read-only, no auth, loopback-only).
The job-013 design contract on main is binding context; the job-012 brief's
non-goals (W-004..006) still apply to the app.

## Owner suggestions — non-binding

- The wireframe set's tokens.css three-tier chain as the starting token
  source, adapted to the app's existing Tailwind/CSS setup rather than
  force-replaced.
- The new non-better skills (interface-review, state-machine, variant,
  build-design) are available if the analyzer finds them load-bearing.

## Consumption and supersession

- Consumption contract: the spec receives this brief by content hash; the
  decision ledger is BINDING; only explicitly `DELEGATED` boundaries may the
  spec decide (LED-004).
- Rework supersession: standard.

## Owner ratification — the human gate

| Actor | Channel / message reference | Date | Verbatim confirmation |
|---|---|---|---|
| videogamer (owner) | pi factory session, framing reply | 2026-11-03 | "start with the wireframe and also all ui skills aI added better-*" |

Fast-path ratification: the reply answered the framing round (wireframe-first
scope; better-* skills mandated) — tier/mode per LED-004 recommendation,
revisit trigger recorded.
