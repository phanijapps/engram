# Ci sub-constitution

SUBORDINATE to `docs/constitution/00-INDEX.md` — one constitution, one root;
this pack narrows it locally with ENGRAM-GROUNDED law: every rule below is
false for a different codebase and carries repository evidence. Reference
grammar: `governs` / `refines` / `enforced-by` / `see`. Source analysis:
`docs/research/software-constitution/14-engram-grounded-packs.md` §2.5.

## Charter

Enforcement view only: which registered gate enforces which rule on every push. Not an eighth source of domain law.

## Knowledge — what CI runs

15 Python gates under `tools/ci/` (registry:
`docs/contracts/constitution/v1/gates/registry.json`): constitution umbrella,
contracts, engine-neutrality, surface-parity, implementation-registry,
coherent-integration (orphans), production-completeness (stubs+LLM-SDK lint),
distribution-drift, canonical-evidence (L3), job-artifacts (SDD pipeline),
docs umbrella (imports the three checkers), code-docs, bound-code matrix,
plus contracts codegen. Blocking vs advisory per the registry; advisory
windows tracked in root DECAY-WINDOWS. Runs on every push (ci.yml).

## Rules

| ID | Candidate statement | Typed references / present enforcement | Repository evidence |
|---|---|---|---|
| CIE-101 | PR/main CI MUST retain contract+constitution checks, the five standalone rule-class gate invocations, Rust formatting/Clippy/tests/neutrality/parity, documentation checks and TS generation-diff/build/typecheck/tests; its report MUST separately identify advisory SDD and path-selected live-pgvector coverage. | refines: CI-004, PC-002, SP-001, EN-001, OR-001; enforced-by: `.github/workflows/ci.yml` orchestration; branch-protection configuration not proved. | `.github/workflows/ci.yml:28-41,78-94,100-134,148-173`; `docs/contracts/constitution/v1/gates/registry.json:69-79`. |
| CIE-102 | Neutrality evidence MUST cover the committed `GATED_PATHS`, fail on an absent declared path, and disclose excluded bindings/runtime/eval/config rather than claim all neutral layers proved. Any scope reduction MUST retain EN-002 review, not obtain legitimacy from a green narrower regex. | refines: EN-001, EN-002, EN-004; enforced-by: check_engine_neutrality.py (presence/patterns); scope-regression review gap. | `tools/ci/check_engine_neutrality.py:7-10,27-61,65-80`; `tools/ci/tests/test_engine_neutrality.py:11-24`. |
| CIE-103 | Surface-parity receipts MUST distinguish provider `require_*`/N-API proxy-name coverage from actual operation/backend/transport behavior; a passing name check MUST NOT be recorded as bound-code matrix or MCP-profile proof. | refines: SP-001, SP-004; enforced-by: check_surface_parity.py (names); check_bound_code_matrix.py remains job-scoped. | `tools/ci/check_surface_parity.py:19-56`; `tools/ci/check_bound_code_matrix.py:31-32,140-185`; `docs/constitution/02-surface-parity.md:20-30`. |
| CIE-104 | The orphan check MUST reject undeclared Cargo workspace members and `packages/*` manifests, dangling component paths/manifests and missing registered existing gate files; evidence MUST NOT describe this as a complete transport/gate/registry/prototype bijection until those sets are independently enumerated. | refines: OR-001, OR-002, OR-003; enforced-by: check_coherent_integration.py (current enumerated subset); wider-inventory gap. | `tools/ci/check_coherent_integration.py:24-63`; `docs/contracts/constitution/v1/components/registry.json:261-294`; `pnpm-workspace.yaml:1-3`. |
| CIE-105 | Post-job-010 factory evidence checks MUST validate `engram.evidence.v1`, declared event families and required fields, require approval evidence at the merge boundary and release evidence after merge, and keep the historical exemption explicit; passing JSON checks MUST NOT be presented as authenticated owner identity or complete publication/amendment/CI-run emission. | refines: CE-001, CE-002, CE-003, CE-004; enforced-by: check_canonical_evidence.py (shape/stage subset only). | `tools/ci/check_canonical_evidence.py:19-21,31-88`; `docs/contracts/constitution/v1/events/registry.json:8-16`; `.agents/skills/engram-factory/scripts/factory.py:470-502`. |

## Deviations & decay (domain)

- DW-8 pre-L3 jobs 001-010 lack events.jsonl, advisory (2026-12-15)
- Global exceptions: root `DEVIATIONS.md` (zero-deviation law) and
  `DECAY-WINDOWS.md` (master ledger).

## Notes

- Enforcement claims verified against gate code + fixtures in the source
  analysis; partial enforcement is stated, not smoothed over.
- The ci/ pack holds the reciprocal enforcement view; the machine catalogue
  is `docs/contracts/constitution/v1/constitution-catalogue.json`.


## Root rails absorbed into this pack (IDs and gates unchanged)

### From 06-canonical-evidence.md

# Rail 06 — Canonical evidence

Rule class: RULE-4 (self-governance + canonical-first).
Gate: `tools/ci/check_canonical_evidence.py` (planned, month 2–3 — L3).
Registry: `docs/contracts/docs/constitution/v1/events/registry.json`.

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

