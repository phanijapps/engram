# Architecture sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.2.

## Charter

Where engram's semantics live: protect the real seams — composer fusion, adapter cells, registries — not a generic layered diagram.

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (ASD-STE100 descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| ARC-101 | Portable domain types/invariants MUST stay in `engram-domain`, conforming to the accepted model/contracts; | `rules/ARC-101.md` |
| ARC-102 | Engine operations MUST remain behind core ports in declared adapter/recipe zones. | `rules/ARC-102.md` |
| ARC-103 | Unified recall MUST obtain the facts lane through `MemoryService::retrieve`, other retrieval candidates through `RetrievalIndex` and beliefs through their repository, then reuse `ReciprocalRankFusion` and `compose_context`; | `rules/ARC-103.md` |
| ARC-104 | `engram-code` MUST own deterministic symbol identity, typed code edges and reference resolution without filesystem/git/storage/LLM behavior; | `rules/ARC-104.md` |
| ARC-105 | Graph algorithms MUST remain dependency-free in `engram-graph-analytics`; | `rules/ARC-105.md` |
| ARC-106 | N-API, TS packages and unified MCP handlers MUST delegate to canonical Rust ports/provider behavior; | `rules/ARC-106.md` |
| ARC-107 | Ontology-pack conversion/slicing MUST be deterministic bytes-to-domain-record behavior in `core/packs`; | `rules/ARC-107.md` |
| ARC-108 | Consolidation MUST remain an explicit request→plan/run operation through injected mutation executors, not an incidental write side effect; | `rules/ARC-108.md` |
| (see rail) | 01-engine-neutrality.md | `rules/rail-engine-neutrality.md` |
| (see rail) | 02-surface-parity.md | `rules/rail-surface-parity.md` |
| (see rail) | 03-canonical-implementations.md | `rules/rail-canonical-implementations.md` |
| (see rail) | 08-coherent-integration.md | `rules/rail-coherent-integration.md` |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.



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

- CLOSED 2026-10-31 (job 011): missing declared paths now fail as named violations,
  with a negative fixture in tools/ci/tests/test_engine_neutrality.py.
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


## Rules

- **OR-001 (MUST)**: Every named component — Rust workspace crate, TS package, transport
  binding, gate script, registry — MUST be declared in the component registry, and every
  declaration MUST resolve to an existing path (bidirectional bijection).
- **OR-002 (MUST)**: A component that exists on disk but is referenced by no registry is
  an orphan and a violation; a registry entry pointing at nothing on disk is a dangling
  declaration and equally a violation.
- **OR-003 (MUST)**: Documentation that declares repository shape (AGENTS.md tree
  diagrams) MUST agree with the registry inventory; drift between them is a violation of
  both this rail and documentation truth (tracked in DECAY-WINDOWS at adoption).
- **OR-004 (SHOULD)**: Factory job directories SHOULD contain only the canonical
  artifact set at top level; orchestration receipts (dispatch results, batch outputs,
  recovery files, rerun churn) MUST live under `runs/` subdirectories (see job-layout
  policy in the engram-factory skill; enforced by `check_job_artifacts.py` layout check).

## Enforcement boundary

Component registry seeded at adoption with the 35 workspace crates + 6 TS packages +
transports + gates. The bidirectional gate (planned) diffs registry ↔ disk. The
job-layout check ships now with `check_job_artifacts.py` (advisory on historical jobs —
see DECAY-WINDOWS for the job-010 sprawl window).

## Deviation path

None: an orphan is deleted or declared in the same change that introduces it.

## History

- 2026-10-31: rail locked at adoption; registry seeded.
