<!--
PROPOSED v3 (not landed) — spec.md, rewritten after competitive study:
- github/spec-kit spec-template.md: prioritized independently-testable user
  scenarios, [NEEDS CLARIFICATION] inline honesty, edge cases, key entities,
  measurable success criteria, assumptions
- Fission-AI/OpenSpec: Requirement-as-named-section + WHEN/THEN scenario
  bullets; delta framing (ADDED/MODIFIED/REMOVED) against a living spec catalog
- merged with engram's contracts: union frontmatter (JSON-per-line, {{render}}
  placeholders), EARS + BCP 14, REQ/AC/GATE IDs, gate evidence tables.
Integration: same frontmatter union contract as v2; IDs literal for
check_job_artifacts.py; S-tier = one user scenario + one requirement block.
-->

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
brief_ref: null
constitution_rules_touched: []
required_reviewers: ["code", "security", "platform"]
---

# Spec: <short observable outcome>

## The case in sixty seconds

<Problem → consequence → desired outcome, with the bet this work makes.
Source links for load-bearing claims; name uncertainty explicitly.>

For example: a declared engine-neutrality path can disappear and silently
stop being scanned (`tools/ci/check_engine_neutrality.py` missing-path skip).
We make that loss fail CI loudly (REQ-001) before a prohibited engine
reference can ship.

## User scenarios

Prioritized, independently testable journeys. At S-tier one scenario
suffices; M/L add journeys down the priority order. If a scenario cannot be
tested alone, it is not a scenario — split or merge it.

### Scenario U-1 (P1): <who does what, in plain language>

**Why this priority**: <value if only this ships>.
**Independent test**: <how to verify this journey alone>.
**Acceptance**:
1. **Given** <initial state>, **When** <action>, **Then** <observable outcome>.
2. **Given** <boundary state>, **When** <action>, **Then** <failure behavior>.

## Requirements

Each requirement is a named section: EARS statement with BCP 14 force,
scenarios as WHEN/THEN bullets. Tag deltas against living contracts:
`(added)` / `(modifies REQ-014)` / `(removes REQ-003)` — a spec that changes
an existing behavior MUST say so, never silently redefine it.

### REQ-001: <short name> (added)

**MUST** — When <trigger>, the system shall <observable response>.
- **WHEN** <concrete trigger> **THEN** <concrete response> (proves AC-001)
- **WHEN** <denial/boundary trigger> **THEN** <failure behavior> (negative case)

Unresolved points are honest, inline, and block `status: "ready"`:
**REQ-002**: When <trigger>, the system shall [NEEDS CLARIFICATION: <exactly
what is unknown and who answers>].

## Edge cases

- <Boundary condition> → <expected handling> (owner: REQ ID or explicit
  deferred-with-trigger entry).
- What happens when <error scenario>? → <behavior>.

## Key entities <(only when the domain model moves)>

- **<Entity>**: what it represents, key attributes, relationships — deltas
  against `docs/architecture/domain-data-model.md`, no implementation.

## Acceptance and gates

| AC | Scenarios / REQ | Gate | Expected evidence |
|---|---|---|---|
| AC-001 | U-1, REQ-001 | GATE-001: `<exact command>` | exit/result (raw output lands in verification.md) |

## What we learned, what we decided, what we assumed

- Evidence: <claim> — <`path:line` citation or command>, status proven/assumed.
- Decisions: `LED-*` disposition from the brief, one line each (resolved /
  delegated with boundary). New decisions discovered here get DEC-* with the
  rejected alternative and consequence.
- Assumptions: <defaults chosen because the brief was silent — each with a
  revisit trigger>.

## Outside this change

<Explicit out-list with Won't IDs from the brief; `none` is a valid answer.
Deferred items carry owner + trigger.>
