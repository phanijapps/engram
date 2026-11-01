# Context manifest — what each stage MUST read before writing

No stage writes blind. Read the sources for your stage, cite what you used
(EVD rows / evidence links), and treat `docs/architecture/domain-data-model.md`
as domain SSOT. When engram recall is available it MAY supplement, never
replace, these files.

## engram-frame (before the interview)

- `docs/product/engram.md` — mission + scope: what is in/out for this product
- `docs/README.md` — the docs front door (know what exists before asking)
- Owner's words — the only authority for intent

## engram-analyzer (before writing spec.md)

1. `docs/architecture/overview.md` — the map: layout, crates, layers
2. `docs/architecture/reference.md` — the golden path (how new code is shaped)
3. `docs/architecture/domain-data-model.md` — domain SSOT (Key entities cite it)
4. `docs/constitution/` — root rails AND the domain packs (product/ architecture/ build/ testing/ ci/ security/ release/ — each pack INDEX narrows root rules for its domain):
   - `00-INDEX.md` universal rules + rail table, then the specific rails
   - every REQ touching a rail cites its rule ID (e.g. `EN-004`);
   - `constitution_rules_touched` frontmatter lists every cited rule
5. `docs/product/engram.md` — scope boundaries for the out-list
6. Nearest ADRs (`docs/adr/README.md` index) for the area under change

## engram-planner (before writing plan.md)

1. Everything the analyzer read (the spec's evidence is your floor, not ceiling)
2. `docs/contracts/constitution/v1/components/registry.json` — component
   inventory; name real crates/paths in steps
3. `docs/contracts/constitution/v1/gates/registry.json` — which gate proves
   what; STEP verify columns name gates that exist
4. `docs/architecture/reference.md` — component stereotypes for the seam
5. ADRs the approach consumes or amends

## engram-tasks / engram-builder (before writing tasks.md / build.md)

1. `docs/contracts/constitution/v1/gates/registry.json` — gate commands
2. `tools/ci/tests/` — test conventions for fixtures (Phase 1 RED)
3. The verification template — where every gate run is recorded

## Reviewers (each discipline)

- `docs/constitution/` rails matching your discipline (code → 01/03;
  security → 05/06; platform → 02/04/08) — a finding may cite a rail.
