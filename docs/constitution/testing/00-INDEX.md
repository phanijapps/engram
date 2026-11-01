# Testing sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally. Rule IDs prefixed `TST-` are globally
unique. Reference grammar: `governs` (authority, acyclic), `refines` (local
narrowing of a root rule), `enforced-by` (gate), `see` (information).

## Charter

Proof: what counts as demonstrated — fixtures, coverage, measurement.

## Rules

| ID | Statement | References |
|---|---|---|
| TST-001 | Every AC names a gate; fixtures land RED before GREEN (TDD spine) | refines: PC-002; enforced-by: check_job_artifacts.py |
| TST-002 | Production-reachable code carries zero stub markers | refines: PC-001; enforced-by: check_production_completeness.py |

## Enforcement

Gates named above are registered in `docs/contracts/constitution/v1/gates/registry.json`;
the ci/ pack holds the reciprocal enforcement view.
