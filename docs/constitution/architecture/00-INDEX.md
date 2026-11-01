# Architecture sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally. Rule IDs prefixed `ARC-` are globally
unique. Reference grammar: `governs` (authority, acyclic), `refines` (local
narrowing of a root rule), `enforced-by` (gate), `see` (information).

## Charter

Structure: boundaries, ownership, coherence, the shape new code conforms to.

## Rules

| ID | Statement | References |
|---|---|---|
| ARC-001 | A new component MUST be declared in the components registry in the change that introduces it | refines: OR-001; enforced-by: check_coherent_integration.py |
| ARC-002 | Engine-bearing logic MUST stay in adapter/backend cells | refines: EN-001, EN-003; enforced-by: check_engine_neutrality.py |

## Enforcement

Gates named above are registered in `docs/contracts/constitution/v1/gates/registry.json`;
the ci/ pack holds the reciprocal enforcement view.
