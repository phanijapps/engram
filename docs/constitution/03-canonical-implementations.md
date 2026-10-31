# Rail 03 — Canonical implementations

Rule class: RULE-1 (single implementation).
Gate: `tools/ci/check_implementation_registry.py` (planned, month 1).
Registry: `docs/contracts/docs/constitution/v1/implementation/registry.json`.

## Rules

- **CI-001 (MUST)**: Every named capability MUST have exactly one canonical
  implementation (`ssot_module`) recorded in the implementation registry.
- **CI-002 (MUST)**: Transport wrappers (TS client, N-API, MCP tools) MUST import or
  delegate to the canonical implementation; a wrapper that reimplements the concept's
  logic is a violation.
- **CI-003 (MUST)**: A second implementation of an existing capability MUST NOT be added;
  if a legitimate replacement is intended, the registry entry's migration block MUST
  record status and deadline, and the old implementation MUST be removed by then.
- **CI-004 (MUST)**: Domain truth lives in `docs/architecture/domain-data-model.md` until Rust domain
  types are accepted as the generated-contract source; generated contracts
  (`docs/contracts/v1`, `packages/contracts`) MUST be reproducible from source and MUST NOT be
  hand-edited.

## Enforcement boundary

Registry seeded at adoption with the capability set (memory, knowledge, retrieval,
belief, hierarchy, consolidation, procedures, code-index, provider facade, MCP server,
launcher). The drift gate (planned) fails on: duplicate `concept_id`, a concept whose
`ssot_module` path does not exist, or a wrapper that does not import the SSOT package.

## Deviation path

None for CI-001 duplicates; migrations carry a decay window with a terminal removal date.

## History

- 2026-10-31: rail locked at adoption; registry seeded (L1 structural).
