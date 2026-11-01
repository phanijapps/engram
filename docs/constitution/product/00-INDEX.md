# Product sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally. Rule IDs prefixed `PRD-` are globally
unique. Reference grammar: `governs` (authority, acyclic), `refines` (local
narrowing of a root rule), `enforced-by` (gate), `see` (information).

## Charter

Scope, outcomes, functional obligations — what this software is for.

## Rules

| ID | Statement | References |
|---|---|---|
| PRD-001 | Every feature brief MUST name its judgment moment (OUT-*) and non-goals | governs: root RULE-1; enforced-by: check_job_artifacts.py |

## Enforcement

Gates named above are registered in `docs/contracts/constitution/v1/gates/registry.json`;
the ci/ pack holds the reciprocal enforcement view.
