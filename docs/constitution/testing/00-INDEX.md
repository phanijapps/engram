# Testing sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.4.

## Charter

What demonstrates engram's promises: deterministic fixtures, per-backend conformance, the fail-closed operation×backend×transport matrix — prove the semantic traps (degraded lanes, supersession keys, belief valid-time).

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (ASD-STE100 descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| TST-101 | Retrieval changes MUST replay the accepted positive-recall, forbidden-recall, budget-omission and no-result fixtures; | `rules/TST-101.md` |
| TST-102 | Memory changes MUST preserve the write→retrieve→forget vertical, cross-tenant isolation, idempotent-write event behavior and delete-with-retained-event semantics; | `rules/TST-102.md` |
| TST-103 | Unified-recall changes MUST demonstrate multi-lane fusion, one-lane and all-lane degraded success, propagation of inner omissions/failures, and a final output budget applied after fusion rather than losing candidates at the inner facts cutoff. | `rules/TST-103.md` |
| TST-104 | Semantic-batch changes MUST prove that multiple memories under one batch key all land, retries converge, partial failure preserves succeeded writes, and a sourced revision retracts only that source's superseded assertions while shared/foreign/code-attributed records remain protected. | `rules/TST-104.md` |
| TST-105 | Bound-code acceptance MUST identify the actual operation × SQLite/pgvector × Rust/native/MCP-stdio/MCP-one-shot cells exercised, with independent receipts; | `rules/TST-105.md` |
| TST-106 | Vocabulary-pack changes MUST prove identical cached/offline reinstall, hard rejection of off-pin bytes, full structural writes versus launch-slice reporting, project isolation and deterministic offline widening; | `rules/TST-106.md` |
| TST-107 | Belief temporal acceptance MUST distinguish valid-time lookup before/after a boundary from unsupported record-time-history lookup; | `rules/TST-107.md` |
| TST-108 | Consolidation tests MUST exercise the production provider wiring with `dry_run` true, absent and explicitly false, proving preview requests do not mutate; | `rules/TST-108.md` |

## Deviations & decay (domain)

- DW-4 historical hash mismatches advisory (2026-11-28)
- DW-5 SDD frontmatter baseline → gate flips blocking (2026-11-28)
- DW-6 rust doc comments 77 + rustdoc 30 errors, strict at close (2026-11-28)
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.
