# Ci sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally. Rule IDs prefixed `CIE-` are globally
unique. Reference grammar: `governs` (authority, acyclic), `refines` (local
narrowing of a root rule), `enforced-by` (gate), `see` (information).

## Charter

Enforcement view: which gate enforces which rule on every push — owns no other domain's rules.

## Rules

| ID | Statement | References |
|---|---|---|
| CIE-001 | Every blocking gate in the gates registry MUST run on every push | governs: all packs; enforced-by: .github/workflows/ci.yml |

## Enforcement

Gates named above are registered in `docs/contracts/constitution/v1/gates/registry.json`;
the ci/ pack holds the reciprocal enforcement view.
