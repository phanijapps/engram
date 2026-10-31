# Rail 06 — Canonical evidence

Rule class: RULE-4 (self-governance + canonical-first).
Gate: `tools/ci/check_canonical_evidence.py` (planned, month 2–3 — L3).
Registry: `docs/contracts/docs/constitution/v1/events/registry.json`.

## Rules

- **CE-001 (MUST)**: Every privileged action MUST emit a canonical evidence event:
  factory job `approve`, `rework`, `release`, deviation `open`/`close`, constitution
  amendment, and publication (DD-004).
- **CE-002 (MUST)**: Event families MUST be declared in the event registry BEFORE any
  code or agent flow references them; references to undeclared families fail the gate.
- **CE-003 (MUST)**: Evidence MUST be sufficient to re-derive the decision later: event
  records carry actor, timestamp, job/PR reference, rule IDs touched, and artifact hashes.
- **CE-004 (SHOULD)**: Every CI run SHOULD emit a run-level evidence event (gate set,
  results) enabling the L3 audit chain.

## Enforcement boundary

Event families are declared at adoption (registry seeded) so the vocabulary exists
canonical-first. Emission wiring is month 2–3: factory controller appends to a
job-scoped `events.jsonl`, and the gate validates declared-family compliance +
record shape. Until wiring exists, this rail is structural only (L1).

## Deviation path

A privileged action taken without evidence emission (e.g. emergency merge) requires a
DEVIATIONS entry and a backfilled event within 48 hours.

## History

- 2026-10-31: rail locked at adoption; event families declared (canonical-first).
