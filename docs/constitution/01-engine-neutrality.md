# Rail 01 — Engine neutrality

Rule classes: RULE-1 (single implementation), RULE-6 (no orphans).
Owner: core maintainers. Gate: `tools/ci/check_engine_neutrality.py` (CI: rust job).

## Rules

- **EN-001 (MUST)**: When CI scans the neutral boundary, the gate shall reject every
  engine type, engine import, or SQL literal matched by the committed lint patterns in
  that boundary.
- **EN-002 (MUST)**: When a pull request changes `GATED_PATHS` or the forbidden patterns,
  the rule review shall reject any unexplained reduction in enforcement scope.
- **EN-003 (MUST)**: When backend composition changes, implementation owners shall
  preserve core port contracts and keep engine operations in declared adapter or
  composition zones.
- **EN-004 (MUST)**: When a declared neutral path is absent, CI shall fail rather than
  silently skip that path.
- **EN-005 (SHOULD)**: When neutrality coverage expands to a presently excluded layer,
  maintainers shall declare its exact paths and measured obligations before asserting
  that layer is governed.

## Enforcement boundary (honest, as of 2026-10-31)

The executable `GATED_PATHS` covers domain, memory, knowledge, retrieval, belief,
hierarchy, consolidation, procedures, proposals, packs, selected `core/integration`
files, and `mcp/engram-mcp/src`. It does NOT scan all of `core/runtime`, `core/eval`,
`bindings/node`, or `core/integration/src/config.rs` — do not infer those are proven
neutral. A green run of this gate discharges EN-001 only; EN-002..EN-005 have known gaps
recorded below and are not discharged by a passing regex.

## Gate gaps (tracked, not waived)

- EN-004: the current script silently continues for missing paths — needs a fail-closed
  enhancement.
- EN-002: needs scope-regression comparison against an accepted baseline.
- EN-003: needs registry/dependency-direction validation and backend conformance tests
  rather than regex alone.

## Deviation path

Remove or relocate the forbidden dependency first. If temporary noncompliance remains,
record a scoped, approved, expiring exception in `DEVIATIONS.md` (RULE-5 failure stands).
A planned coverage expansion carries a ratified decay clause in `DECAY-WINDOWS.md` with a
measured baseline and terminal strict gate; a decay clause MUST NOT retroactively
legalize a new EN-001 regression.

## History

- 2026-10-31: rail locked at adoption (owner ratification, plan §7 ask 1).
