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
updated_at: "{{timestamp}}"
size_tier: "S"
brief_ref: null
constitution_rules_touched: []
required_reviewers: ["code", "security", "platform"]
---
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
(union frontmatter JSON-per-line with render placeholders, EARS + BCP 14,
REQ/AC/GATE IDs, gate-evidence tables, S/M/L scaling).
Section markers: (mandatory) at every tier · (M/L) added at larger tiers.
Integration unchanged: frontmatter union contract; IDs literal for
check_job_artifacts.py; S-tier = one scenario + one requirement.
-->


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
Acceptance: <AC IDs proving this journey>.

## Requirements (mandatory)

Named sections: EARS statement with BCP 14 force, scenarios as WHEN/THEN
bullets. Tag deltas against living contracts — `(added)` / `(modifies <existing-req>)` /
`(removes <existing-req>)`, citing the source revision or hash; a spec that changes existing behavior MUST
declare it, never silently redefine it.

### REQ-001: <short name> (added)

**MUST** — When <trigger>, the system shall <observable response>.
- Acceptance: <happy-path AC ID>, <negative/boundary AC ID or justified exception>.

Unresolved points are inline, honest, and block `status: "ready"`:
### REQ-<n>: <name> — [NEEDS CLARIFICATION: <exactly what is unknown, who answers>]

## Interfaces (mandatory when any public surface moves; else one line)

- <API / CLI / N-API / MCP tool surface touched, direction of change, consumer
  action required>. Example: `requireX` facade + `requireXApi` proxy (SP-001);
  no breaking payload change — additive field only.
- `none — gate-only change` is a complete answer.

## Quality attributes / NFRs (mandatory; ISO 25010 vocabulary)

Which characteristics this change moves beyond baseline. Each NFR is
measurable and gate-bound — an NFR without a gate is a wish:

- NFR-001 (reliability): missing declared scan inputs cannot silently bypass
  enforcement — measurable as GATE-001's missing-path case; acceptance AC-001.
- NFR-<n> (<characteristic>): <measurable statement + proving gate>.

`none beyond baseline` requires a change-specific reason, not a shrug.

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

## Acceptance criteria (mandatory — Gherkin)

The AC layer is the acceptance contract, written in Gherkin: `Rule:` groups a
behavior, `Scenario:` carries the AC ID, Given/When/Then steps state what
done looks like — including failure-path acceptance where the behavior is a
guard (fail-closed, denial, validation). The spec ends here on purpose:
**test scenarios — concrete test instances, fixtures, boundary permutations —
are NOT written here and are NOT Gherkin**. They are the builder's domain:
the gate/fixture suite implements them and verification.md reports raw
evidence per gate. An AC names the gate that proves it; it does not contain
its own test plan.

```gherkin
Rule: missing declared gated paths fail loudly

  Scenario: AC-001 declared path missing on disk
    Given a declared gated path absent on disk
    When the engine-neutrality gate runs
    Then it exits non-zero naming that missing declared path
```

| AC | REQ | Gate (proof) | Evidence |
|---|---|---|---|
| AC-001 | REQ-001 | GATE-001 | raw output → verification.md |

Gate IDs resolve to runnable commands in the PLAN's acceptance map — the spec
links behavior to a gate identity, the plan owns the executable command.

An AC without a named gate is unaccepted by definition — it is a finding at
review, not a TODO.

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
  stated reason; deltas tagged; every AC is Gherkin and names a runnable gate
  (test scenarios themselves belong to the gate suite, not this document).
