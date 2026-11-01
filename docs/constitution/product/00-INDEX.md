# Product sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.1.

## Charter

What engram promises: identity, the four substrates (memory / source-grounded knowledge / derived belief / code intelligence), observable write-recall-forget guarantees, freshness semantics, honest capability status.

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (ASD-STE100 descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| PRD-101 | Engram's product scope MUST remain governed agent memory, source-grounded knowledge, derived belief and code intelligence, delivered by the Rust engine through SDK/MCP surfaces; | `rules/PRD-101.md` |
| PRD-102 | Memory writes MUST produce governed `MemoryRecord` state and auditable lifecycle events; | `rules/PRD-102.md` |
| PRD-103 | `store_knowledge`/semantic batches MUST distinguish source-derived memory facts from graph entities/relationships, expose `BestEffort` per-step outcomes and retain succeeded steps after failure. | `rules/PRD-103.md` |
| PRD-104 | Beliefs MUST remain derived, revisable stances distinct from memory and source truth, with valid time separate from record time; | `rules/PRD-104.md` |
| PRD-105 | Recall MUST return a policy-filtered `ContextPayload` that explains fused provenance, omissions and unavailable sources; | `rules/PRD-105.md` |
| PRD-106 | Bound-code answers MUST identify the authorized scope/`RepoId`, exact `refs/heads/...` binding and published generation; | `rules/PRD-106.md` |
| PRD-107 | A capability MUST NOT be called shipped merely because an adapter exists: it MUST be callable through the provider, N-API/TS and supported MCP surface or carry the root-required explicit exception, and `CapabilityReport` MUST describe the selected recipe's actual support. | `rules/PRD-107.md` |
| PRD-108 | Ontology/taxonomy MUST remain first-class governed vocabulary, not opaque metadata or a claim of automatic truth enforcement. | `rules/PRD-108.md` |

## Deviations & decay (domain)

- none open
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
