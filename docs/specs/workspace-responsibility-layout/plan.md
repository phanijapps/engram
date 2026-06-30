# Plan: Workspace Responsibility Layout

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Migrate by path first and behavior never. The first implementation keeps Rust
package names stable, moves directories into top-level responsibility groups,
updates workspace members and path dependencies, adds scoped `AGENTS.md` files,
and runs the full gates. Crate renames, compatibility shim removal,
`engram-memory-engine`, and TypeScript backend-selection ergonomics remain
separate follow-up specs so review can distinguish structural movement from
behavioral changes.

## Constraints

- `docs/domain-data-model.md`: portable contracts stay independent of storage
  engines, APIs, and language bindings.
- `docs/architecture.md`: storage-neutral boundaries and replaceable adapters
  remain distinct.
- `docs/rfcs/0001-memory-layer-scope.md`: memory must remain composable across
  runtimes and storage engines.
- `docs/rfcs/0002-knowledge-source-extension.md`: knowledge sources and
  adapters must not collapse into core memory.
- `AGENTS.md`: no god crates, god modules, or adapter concerns in domain/core
  contracts.

## Construction tests

**Integration tests:** full `cargo test --workspace`, `pnpm run check`, contract
hooks, and docs hooks.

**Manual verification:** compare `docs/specs/workspace-responsibility-layout/package-names.before`
against post-move `cargo metadata` package names; inspect
`find core adapters bindings -name AGENTS.md`.

## Design (LLD)

### Design decisions

- Move path layout before crate renames because stable package names keep the
  migration source-compatible and make behavioral regressions easier to catch.
  Traces to: AC1, AC2, AC8.
- Use responsibility groups instead of nested `crates/` categories because the
  top-level path should reveal architecture ownership. Traces to: AC1, AC3,
  AC4, AC5.
- Add local `AGENTS.md` files to make the grouped layout enforceable by future
  coding agents. Traces to: AC6, AC9.

### Data & schema

No v1 JSON Schema or domain model field changes are part of this slice. The
only contract surface is repository layout plus Rust workspace membership.
Traces to: AC2, AC8.

### Interfaces & contracts

Rust package names remain unchanged in `Cargo.toml` files. Path dependencies
change from `../engram-*` style references to relative paths that match the new
directory layout. TypeScript package names and public generated contract files
remain unchanged. Traces to: AC2, AC7, AC8.

### Component / module decomposition

Target physical layout:

```text
core/
  domain/                  # package: engram-domain
  runtime/                 # package: engram-runtime
  memory/                  # package: engram-memory
  knowledge/               # package: engram-knowledge
  retrieval/               # package: engram-retrieval
  orchestration/           # package: engram-core
  eval/                    # package: engram-eval

adapters/
  ingest/                  # package: engram-ingest, split deferred
  memory/
    inmem/                 # package: engram-store-memory, rename deferred
    sqlite/                # package: engram-store-sql, rename deferred
  knowledge/
    inmem/                 # package: engram-store-knowledge-memory, rename deferred
  retrieval/
    sqlite-vec/            # package: engram-store-vector, rename deferred

bindings/
  node/                    # package: engram-node
```

### State & control flow

There is no runtime state transition. Build tooling resolves the same package
graph from new paths, then tests prove runtime behavior is unchanged.

### Behavior & rules

- `core/*` crates must not depend on `adapters/*` or `bindings/*`.
- `adapters/*` crates may depend on `core/*` crates and other adapter crates
  only when the relationship is explicit and justified by the plan.
- `bindings/*` crates may depend on stable core crates and selected adapters,
  but must not redefine memory or knowledge behavior.
- Group-level `AGENTS.md` files are short, local boundary contracts, not
  duplicated architecture essays.
- The current `engram-ingest` crate stays under `adapters/ingest` because it
  exports filesystem and Git source readers. A later split may move its
  deterministic chunking and ingestion orchestration into `core/ingest` while
  leaving concrete source readers under adapters.

### Failure, edge cases & resilience

- Relative path dependencies are the main failure mode; `cargo check
  --workspace` is the primary proof.
- Stale docs and scripts that mention `crates/` can mislead future agents; grep
  and docs updates are part of the task list.
- Untracked local files such as `.claude/` must not be swept into this
  migration unless explicitly requested.

### Quality attributes (NFRs)

- Reviewability: the path-only migration should be mechanically obvious in the
  diff and avoid behavior edits.
- Compatibility: package names and public Rust/TypeScript surfaces remain
  stable.
- Maintainability: every group has local agent instructions that prevent
  boundary drift.

### Dependencies & integration

The workspace root `Cargo.toml` is the authoritative member list. Package path
dependencies, README layout docs, `AGENTS.md`, `docs/architecture.md`,
`docs/arch_divergence.md`, and any scripts with hard-coded crate paths are the
known integration points.

## Tasks

### T1: Capture package-name baseline

**Depends on:** none

**Touches:** `docs/specs/workspace-responsibility-layout/package-names.before`

**Tests:**
- Goal-based:
  `cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print("\n".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))' > docs/specs/workspace-responsibility-layout/package-names.before`
  records current package names for AC2.

**Approach:**
- Capture the current workspace package names before moving paths.
- Use the package-name list as the compatibility baseline for T5.

**Done when:** the implementer can compare pre/post package names without
guessing.

### T2: Move Rust crates into responsibility groups

**Depends on:** T1

**Touches:** `Cargo.toml`, `core/**`, `adapters/**`, `bindings/**`

**Tests:**
- Goal-based: `cargo metadata --no-deps --format-version 1` succeeds after path
  movement and reports the same package names captured in T1.

**Approach:**
- Move storage-neutral crates into `core/`.
- Move memory, knowledge, and retrieval infrastructure adapters into
  `adapters/`.
- Move the current mixed `engram-ingest` crate into `adapters/ingest` until a
  later split separates deterministic ingest orchestration from filesystem and
  Git source readers.
- Move `engram-node` into `bindings/node`.
- Update root workspace members.
- Update all crate path dependencies using relative paths from the new
  locations.

**Done when:** Cargo resolves the full workspace from the grouped layout.

### T3: Add local `AGENTS.md` boundary files

**Depends on:** T2

**Touches:** `core/AGENTS.md`, `adapters/AGENTS.md`, `bindings/AGENTS.md`,
`adapters/ingest/AGENTS.md`, `adapters/memory/inmem/AGENTS.md`,
`adapters/memory/sqlite/AGENTS.md`, `adapters/knowledge/inmem/AGENTS.md`,
`adapters/retrieval/sqlite-vec/AGENTS.md`

**Tests:**
- Goal-based:
  `find core adapters bindings -name AGENTS.md -print | sort` shows all
  required files for AC6.
- Docs hook: `.codex/hooks/check-docs.sh` passes.

**Approach:**
- Keep each file short.
- Start each file with a pointer to the nearest parent/root instructions.
- State what the group owns and what must not be added there.

**Done when:** local instructions make the boundary obvious without duplicating
  the full root `AGENTS.md`.

### T4: Update docs and hard-coded path references

**Depends on:** T2, T3

**Touches:** `AGENTS.md`, `README.md`, `docs/architecture.md`,
`docs/arch_divergence.md`, `.codex/skills/engram-plan/references/crate-map.md`,
and any scripts or docs that reference old `crates/engram-*` paths.

**Tests:**
- Goal-based:
  `rg -n "crates/engram|crates/" README.md AGENTS.md docs .codex scripts packages --glob '!docs/research/**'`
  has only intentional historical references.
- Docs hook: `.codex/hooks/check-docs.sh` passes.

**Approach:**
- Replace target layout diagrams and crate maps with grouped paths.
- Keep historical research and changelog references only when they are clearly
  historical.
- Update divergence tracker with the new layout alignment and deferred renames.

**Done when:** docs direct future work to `core/`, `adapters/`, and
`bindings/` instead of the obsolete flat crate layout.

### T5: Run full gates and prove behavior unchanged

**Depends on:** T1-T4

**Touches:** no planned source edits beyond fixes needed by gates.

**Tests:**
- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `pnpm run check`
- `.codex/hooks/check-contracts.sh`
- `.codex/hooks/check-docs.sh`
- `git diff --check`

**Approach:**
- Run gates after the path migration and docs update.
- Compare package names against the T1 baseline:
  `cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print("\n".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))' > /tmp/engram-package-names.after && diff -u docs/specs/workspace-responsibility-layout/package-names.before /tmp/engram-package-names.after`
- Fix only path/doc breakage in this spec.

**Done when:** all gates pass and package names remain stable.

## Rollout

This migration is source-compatible at the package-name level. It should land as
one structural PR after the current root `specs/` deletion and `CLAUDE.md`
changes are either committed or deliberately included, so unrelated file churn
does not obscure the path migration.

## Risks

- A pure move can still break relative includes, examples, package metadata, or
  docs links.
- Reviewers can miss behavior changes hidden inside path churn; implementation
  should avoid non-mechanical source edits.
- Deferring crate renames creates temporary naming awkwardness such as
  `adapters/memory/sqlite` containing package `engram-store-sql`; the follow-up
  rename spec should be explicit.
- Group-level `AGENTS.md` can contradict root instructions if copied too
  broadly instead of written as local deltas.

## Changelog

- 2026-06-30: initial plan.
- 2026-06-30: shipped grouped Rust workspace layout with stable package names.
