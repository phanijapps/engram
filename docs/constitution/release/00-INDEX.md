# Release sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally. Rule IDs prefixed `REL-` are globally
unique. Reference grammar: `governs` (authority, acyclic), `refines` (local
narrowing of a root rule), `enforced-by` (gate), `see` (information).

## Charter

Ship discipline: versioning, evidence chain, rollback.

## Rules

| ID | Statement | References |
|---|---|---|
| REL-001 | No merge without owner approval and the L3 released event; rollback path stated | refines: CE-001, ZD-002; enforced-by: check_canonical_evidence.py |

## Enforcement

Gates named above are registered in `docs/contracts/constitution/v1/gates/registry.json`;
the ci/ pack holds the reciprocal enforcement view.
