# Spec: Workspace Architecture Alignment

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** `docs/research/architecture-design-v2.md`, `docs/research/synthesis.md`, `docs/architecture/reference.md`, `docs/domain-data-model.md`, RFC-0001, RFC-0002, ADR-0009, ADR-0010
- **Brief:** user request: clean code and clean design; ultimately implement the research architecture
- **Contract:** none (workspace/documentation/tooling alignment; no wire-contract change)
- **Shape:** documentation + repository tooling

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Make the workspace easier to navigate and harder to steer back toward stale
architecture by aligning the current repository map, planning reference, and
shared scripts with the research-backed v2 architecture. This slice does not
move runtime crates or change behavior; it clears the first layer of clutter so
future implementation slices can focus on durable memory/knowledge architecture
work.

## Boundaries

### Always do

- Keep `docs/architecture/reference.md` as the normative architecture source and
  `docs/architecture/overview.md` as the descriptive repository map.
- Keep `docs/architecture.md` only as a compatibility pointer for older specs
  and hooks.
- Move reusable repository scripts under `tools/scripts/` and update live
  commands.
- Refresh the Engram planning crate map so it no longer names retired in-memory
  adapters as future targets.
- Preserve Rust and TypeScript behavior.

### Ask first

- Moving Rust crates, renaming Cargo packages, or changing public import paths.
- Physically moving all historical specs into lifecycle folders.
- Removing root GitHub-visible governance files.
- Changing accepted v1 contracts or generated public TypeScript shapes.

### Never do

- Reintroduce broad in-memory fixture crates as the clean-design answer.
- Collapse memory, knowledge, retrieval, belief, hierarchy, consolidation, or
  evaluation boundaries.
- Hide historical specs or research that explain why the current structure
  exists.
- Make `demo/` behavior canonical for `core/`.

## Acceptance Criteria

- [x] `docs/architecture.md` points to the current normative/descriptive
  architecture docs instead of duplicating the module map.
- [x] `docs/architecture/overview.md` reflects current post-inmem adapter paths,
  Codex/Claude tooling, and shared tooling layout.
- [x] `.codex/skills/engram-plan/references/crate-map.md` reflects the current
  crate/package map and retired in-memory adapters.
- [x] Shared Python automation lives under `tools/scripts/`, with live package,
  hook, README, contributor, contract, and release references updated.
- [x] No Rust behavior, TypeScript public API, contract schema, or generated
  contract output changes.
- [x] `pnpm run contracts:generate`, `.codex/hooks/check-contracts.sh`, and
  `.codex/hooks/check-docs.sh` pass after the move.

## Deferred Follow-Up

- Split `docs/specs/` into lifecycle groups: `active/`, `shipped/`, and
  `retired/legacy-inmem/`.
- Move `adapters/orchestration/belief-sqlite` to `adapters/belief/sqlite` in a
  dedicated crate-path normalization slice.
- Decide whether root governance files should remain full documents for GitHub
  discoverability or become stubs pointing into `docs/governance/`.
- Restore or remove the advertised `adapt-to-project` skill so session startup
  does not point at a missing local file.
