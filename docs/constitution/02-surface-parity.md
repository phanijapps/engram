# Rail 02 — Surface parity

Rule classes: RULE-1, RULE-6. Owner: core + bindings maintainers.
Gates: `tools/ci/check_surface_parity.py` (existing), `tools/ci/check_bound_code_matrix.py` (job-scoped).

## Rules

- **SP-001 (MUST)**: Every runtime capability, operation, and retrieval mode exposed by
  the Rust `EngramProvider` facade MUST also be reachable through the N-API binding
  (`bindings/node`) and any other supported transport, and MUST be reflected in
  `CapabilityReport`.
- **SP-002 (MUST)**: A capability is not "shipped" until every surface can invoke it;
  wiring one surface and leaving another unwired (transport asymmetry) is a violation.
- **SP-003 (MUST)**: If a surface genuinely cannot carry a capability, the reason MUST be
  recorded in an ADR referenced from `implementation/registry.json` (transport_wrappers
  entry), not left implicit.
- **SP-004 (SHOULD)**: Parity checks SHOULD verify operation-level reachability, not only
  symbol-name presence.

## Enforcement boundary

`check_surface_parity.py` verifies that every facade `require_*` capability has an N-API
`require*Api` proxy (name-level check, SPECIAL_CASES ledger). Operation-level parity is
partially covered by `check_bound_code_matrix.py` (fail-closed operation × backend ×
transport matrix), currently run job-scoped rather than in CI.

## Gate gaps

- SP-004: promote the bound-code matrix into CI; extend name checks toward
  operation-level checks.

## Deviation path

Same as rail 01: fix the asymmetry; otherwise a scoped, expiring DEVIATIONS entry while
RULE-5 stands. Sequencing an adapter unit ahead of its wiring is legitimate (see
`lexical-keyword-retrieval` + `lexical-wiring` precedent) only with a decay window
declaring the wiring deadline.

## History

- 2026-10-31: rail locked at adoption.
