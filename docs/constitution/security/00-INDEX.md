# Security sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.6.

## Charter

Engram's authority, lineage and agent-install trust boundaries: LLM/model SDK exclusion from core/adapters, stub-free shipped code, tamper-evident audit chain, launcher install trust.

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (ASD-STE100 descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| SEC-101 | MCP operations MUST carry the launch-resolved scope: `org`→tenant and domain/subdomain or project→workspace; | `rules/SEC-101.md` |
| SEC-102 | Code register/bind/rebuild/inspect/reset MUST use the shared `CodeBindingAuthorizer`; | `rules/SEC-102.md` |
| SEC-103 | Code candidates MUST be constrained by published-generation eligibility before they reach reranking/context composition; | `rules/SEC-103.md` |
| SEC-104 | Code reset MUST require preview→owner gate→apply/resume over an immutable hash-bound exact target journal; | `rules/SEC-104.md` |
| SEC-105 | Bound Git acquisition MUST reject traversal/nonportable retained-path identities rather than rename them, enforce independent object/tree/depth/inflation/snapshot limits, and retain explicit cleanup-pending custody on owned cleanup failure; | `rules/SEC-105.md` |
| SEC-106 | Agent-bundle install/remove MUST touch only Engram-owned name-tagged entries, protect authored source trees and refuse symlinked destination components; | `rules/SEC-106.md` |
| SEC-107 | Recalled constitution text MUST remain background data, not executable approval/instructions. | `rules/SEC-107.md` |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
