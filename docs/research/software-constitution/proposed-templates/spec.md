<!--
PROPOSED v4 (not landed) — the spec template, focused and final.
Synthesis of: github/spec-kit (prioritized independently-testable user
scenarios, [NEEDS CLARIFICATION], edge cases, key entities, assumptions),
Fission-AI/OpenSpec (named requirements + WHEN/THEN scenario bullets, delta
framing against living contracts), Gherkin/BDD Cucumber convention (ACs as
named Rule/Scenario blocks, happy + negative steps mapped to gates),
IEEE 830-1998/ISO 29148 (requirement
rubric: necessary/singular/implementation-free/feasible/verifiable/conforming;
interfaces section), RFC 7322 (mandatory-when-empty discipline, tier-marked
sections), ISO 25010 (quality-attribute vocabulary), engram's own contracts
(union frontmatter JSON-per-line + {{render}} placeholders, EARS + BCP 14,
REQ/AC/GATE IDs, gate-evidence tables, S/M/L scaling).
Section markers: (mandatory) at every tier · (M/L) added at larger tiers.
Integration unchanged: frontmatter union contract; IDs literal for
check_job_artifacts.py; S-tier = one scenario + one requirement.
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

## The case in sixty seconds (mandatory)

<Problem → consequence → desired outcome, and the bet this work makes.
Source links for load-bearing claims; name uncertainty explicitly.>

For example: a declared engine-neutrality path can disappear and silently
stop being scanned (`tools/ci/check_engine_neutrality.py` missing-path skip).
This change makes that loss fail CI loudly (REQ-001) before a prohibited
engine reference can ship.

## User scenarios (mandatory)

Prioritized, independently testable journeys. S-tier: one scenario. M/L add
journeys down the priority order. A scenario that cannot be tested alone is
not a scenario — split or merge it.

### U-1 (P1): <who does what, in plain language>

**Why this priority**: <value if only this ships>.
**Independent test**: <how to verify this journey alone>.
Journey-level acceptance lives here as one-line Given/When/Then bullets;
the formal executable ACs are the Gherkin blocks in Acceptance criteria —
write them there, reference U-1, do not duplicate.
1. **Given** <initial state>, **When** <action>, **Then** <observable outcome>.

## Requirements (mandatory)

Named sections: EARS statement with BCP 14 force, scenarios as WHEN/THEN
bullets. Tag deltas against living contracts — `(added)` / `(modifies
REQ-014)` / `(removes REQ-003)`; a spec that changes existing behavior MUST
declare it, never silently redefine it.

### REQ-001: <short name> (added)

**MUST** — When <trigger>, the system shall <observable response>.
- **WHEN** <concrete trigger> **THEN** <concrete response> (proves AC-001)
- **WHEN** <denial or boundary trigger> **THEN** <failure behavior>

Unresolved points are inline, honest, and block `status: "ready"`:
### REQ-002: <name> — [NEEDS CLARIFICATION: <exactly what is unknown, who answers>]

## Interfaces (mandatory when any public surface moves; else one line)

- <API / CLI / N-API / MCP tool surface touched, direction of change, consumer
  action required>. Example: `requireX` facade + `requireXApi` proxy (SP-001);
  no breaking payload change — additive field only.
- `none — gate-only change` is a complete answer.

## Quality attributes (mandatory; ISO 25010 vocabulary)

<Which characteristics this change moves beyond baseline — functional
suitability, performance efficiency, compatibility, usability, reliability,
security, maintainability, portability — each with its gate.>
`none beyond baseline — gate-only change` is a complete answer.

## Blast radius (mandatory — never silently absent)

<Affected surfaces and failure shape if this lands wrong, or
`none — <reason>`>. Example: tools/ci only; worst case a false-failing gate
on stripped checkouts (RISK-001, maintainer-owned).

## Edge cases (mandatory)

- <Boundary condition> → <expected handling> (owns: REQ ID, or
  deferred-with-owner-and-trigger).
- What happens when <error scenario>? → <behavior>.

## Key entities (only when the domain model moves)

- **<Entity>**: what it represents, key attributes, relationships — deltas
  against `docs/architecture/domain-data-model.md`, no implementation.

## Acceptance criteria (mandatory — Gherkin/BDD)

Each AC is a named Gherkin scenario block (Cucumber style): `Rule:` groups a
behavior, `Scenario:` carries the AC ID, steps are Given/When/Then — the
happy path AND the negative case as sibling scenarios under the same rule.
Steps are written to map 1:1 onto the gate/fixture that proves them; the
verification ledger reports per scenario.

```gherkin
Rule: missing declared gated paths fail loudly

  Scenario: AC-001 declared path missing on disk
    Given a gated path list containing "/nonexistent/gated/path"
    When the engine-neutrality gate runs
    Then it exits non-zero naming "/nonexistent/gated/path"

  Scenario: AC-001 negative — every declared path present
    Given the current repository gated paths
    When the engine-neutrality gate runs
    Then it exits zero with "gated surface clean"
```

| AC | REQ / scenarios | Gate (automation for the steps) | Evidence |
|---|---|---|---|
| AC-001 | REQ-001, both scenarios | GATE-001: `python3 tools/ci/tests/test_engine_neutrality.py` | raw output → verification.md |

A scenario without an automating gate is a finding, not a hope; a gate
covering only the happy scenario is half an AC.

## Evidence, decisions, assumptions (mandatory)

- **Evidence**: <claim> — `<path:line or command>`, proven|assumed.
- **Decisions**: `LED-*` dispositions one line each; new decisions get `DEC-*`
  with the rejected alternative and consequence.
- **Assumptions**: <default chosen because the brief was silent — revisit
  trigger>. Example: single-OS CI is assumed; revisit on Windows runners.

## Outside this change (mandatory)

<Won't IDs from the brief, explicit exclusions; deferred items carry owner +
trigger. `none` is a valid answer.>

## Definition of ready (mandatory — the gate to `status: "ready"`)

- Every REQ: necessary, singular, implementation-free, feasible, verifiable,
  conforming (ISO 29148 rubric) — reviewed, not self-declared.
- No `[NEEDS CLARIFICATION]` remains; every REQ has ≥1 negative case or a
  stated reason; deltas tagged; every AC names a runnable gate.
