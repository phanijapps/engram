# Ci sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.5.

## Charter

Enforcement view only: which registered gate enforces which rule on every push. Not an eighth source of domain law.

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (ASD-STE100 descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| CIE-101 | PR/main CI MUST retain contract+constitution checks, the five standalone rule-class gate invocations, Rust formatting/Clippy/tests/neutrality/parity, documentation checks and TS generation-diff/build/typecheck/tests; | `rules/CIE-101.md` |
| CIE-102 | Neutrality evidence MUST cover the committed `GATED_PATHS`, fail on an absent declared path, and disclose excluded bindings/runtime/eval/config rather than claim all neutral layers proved. | `rules/CIE-102.md` |
| CIE-103 | Surface-parity receipts MUST distinguish provider `require_*`/N-API proxy-name coverage from actual operation/backend/transport behavior; | `rules/CIE-103.md` |
| CIE-104 | The orphan check MUST reject undeclared Cargo workspace members and `packages/*` manifests, dangling component paths/manifests and missing registered existing gate files; | `rules/CIE-104.md` |
| CIE-105 | Post-job-010 factory evidence checks MUST validate `engram.evidence.v1`, declared event families and required fields, require approval evidence at the merge boundary and release evidence after merge, and keep the historical exemption explicit; | `rules/CIE-105.md` |
| (see rail) | 06-canonical-evidence.md | `rules/rail-canonical-evidence.md` |

## Deviations & decay (domain)

- DW-8 pre-L3 jobs 001-010 lack events.jsonl, advisory (2026-12-15)
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.



## Rules

- **CE-001 (MUST)**: Every privileged action MUST emit a canonical evidence event:
  factory job `approve`, `rework`, `release`, deviation `open`/`close`, constitution
  amendment, and publication (DD-004).
- **CE-002 (MUST)**: Event families MUST be declared in the event registry BEFORE any
  code or agent flow references them; references to undeclared families fail the gate.
- **CE-003 (MUST)**: Evidence MUST be sufficient to re-derive the decision later: event
  records carry actor, timestamp, job/PR reference, rule IDs touched, and artifact hashes.
- **CE-004 (SHOULD)**: Every CI run SHOULD emit a run-level evidence event (gate set,
  results) enabling the L3 audit chain.

## Enforcement boundary

Event families are declared at adoption (registry seeded) so the vocabulary exists
canonical-first. Emission wiring is month 2–3: factory controller appends to a
job-scoped `events.jsonl`, and the gate validates declared-family compliance +
record shape. Until wiring exists, this rail is structural only (L1).

## Deviation path

A privileged action taken without evidence emission (e.g. emergency merge) requires a
DEVIATIONS entry and a backfilled event within 48 hours.

## History

- 2026-10-31: rail locked at adoption; event families declared (canonical-first).
