# Security sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally. Rule IDs prefixed `SEC-` are globally
unique. Reference grammar: `governs` (authority, acyclic), `refines` (local
narrowing of a root rule), `enforced-by` (gate), `see` (information).

## Charter

Security obligations across the surfaces they constrain.

## Rules

| ID | Statement | References |
|---|---|---|
| SEC-001 | No LLM/model SDK may enter core or adapter manifests; stub-free shipped code | refines: PR-002, PC-001; enforced-by: check_production_completeness.py |

## Enforcement

Gates named above are registered in `docs/contracts/constitution/v1/gates/registry.json`;
the ci/ pack holds the reciprocal enforcement view.
