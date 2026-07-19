# Plan: Retire Knowledge In-Memory Adapter

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Retire the knowledge in-memory adapter by promoting SQLite's existing in-memory
connection mode to the test harness, then removing the duplicate crate and its
active references. The highest-risk part is not deletion; it is avoiding a quiet
loss of conformance coverage, so the first tasks inventory and strengthen SQLite
coverage before any workspace removal. This slice deliberately does not touch
the memory in-memory adapter, belief/hierarchy/consolidation harnesses, or public
knowledge port signatures.

Files expected to change: `adapters/ingest/Cargo.toml`,
`adapters/ingest/tests/*`, `adapters/knowledge/sqlite/tests/repository.rs`,
`Cargo.toml`, `Cargo.lock`, `.codex/hooks/`, `docs/architecture.md`,
`docs/specs/README.md`, and `AGENTS.md` or adapter-local instructions if they
still describe `adapters/knowledge/inmem` as active. Verification is Rust
integration tests plus dependency/search gates proving the crate is gone from
active code. Tempted to add a generic adapter conformance framework; declining
for this slice because the existing port surface is small enough to verify with
focused SQLite integration tests.

## Constraints

- ADR-0005 requires storage adapters to preserve behavior at the port boundary
  without leaking storage-specific fields into domain types.
- ADR-0006 chooses SQLite as the first SQL adapter and supports in-memory SQLite
  connections for tests and local fixtures.
- ADR-0008 makes durable ontology persistence part of
  `engram-store-knowledge-sqlite`; the old in-memory knowledge crate is no
  longer the only ontology implementation.
- `docs/specs/memory-knowledge-boundaries` keeps memory, knowledge, graph,
  ontology, and retrieval persistence separate.
- `.codex/hooks/check-knowledge-sqlite-isolation.sh` must keep passing after the
  retirement; SQLite knowledge must not reuse sibling store crates.

## Construction Tests

**Integration tests:** SQLite repository and ingestion tests listed under the
tasks below.

**Manual verification:** none; no user-invoked artifact changes.

## Design (LLD)

### Design Decisions

- Use `SqlKnowledgeStore::open_in_memory()` as the fast test store. It exercises
  SQLite schema, serialization, indexing, and scope logic without requiring a
  file-backed database.
- Keep a retirement check as a repository hook/script rather than relying on
  review memory. The check should fail if active manifests or code reintroduce
  `engram-store-knowledge-memory` or `InMemoryKnowledgeStore`.
- Treat historical documentation as immutable history unless it is actively
  misleading. Current architecture docs and instructions should say the adapter
  is retired; shipped specs can continue to describe what existed when they
  shipped.

### Data & Schema

No domain schema or SQLite table change is expected. If a coverage gap appears,
close it inside `engram-store-knowledge-sqlite` without changing
`engram-domain` or `engram-knowledge` port signatures.

### Interfaces & Contracts

No public interface changes. The implementation consumes existing
`KnowledgeRepository`, `KnowledgeGraphRepository`, `TaxonomyRepository`, and
`OntologyRepository` methods.

### Component / Module Decomposition

- `engram-store-knowledge-sqlite`: single executable knowledge store.
- `engram-ingest` tests: consume SQLite store through existing traits.
- `.codex/hooks`: dependency/readiness checks that prevent reintroducing the
  retired crate.
- Docs/instructions: current-state references only.

### Failure, Edge Cases & Resilience

- **Coverage regression:** deleting the in-memory tests could drop graph or
  ontology scenarios. Mitigation: add any missing SQLite cases before deletion.
- **Hidden dependency:** a dev-dependency or stale import keeps the crate in the
  workspace graph. Mitigation: retirement check runs `cargo tree`/`rg`.
- **Over-broad docs rewrite:** historical specs lose useful context. Mitigation:
  update only active architecture/instruction surfaces unless a historical doc
  says the adapter is active today.
- **Scope regression:** swapping stores could change inherited visibility for
  chunks, concepts, or ontology terms. Mitigation: scope-isolation tests stay in
  SQLite integration coverage.

## Tasks

### T1: Active dependency inventory is complete

**Depends on:** none

**Touches:** `docs/specs/retire-knowledge-inmem/plan.md`

**Tests:**
- Goal-based: run `rg "engram-store-knowledge-memory|InMemoryKnowledgeStore|adapters/knowledge/inmem" --glob '!target/**'` and classify each hit as active code, active docs, or historical docs.
- Goal-based: run `cargo tree -i engram-store-knowledge-memory` to identify reverse dependencies.

**Approach:**
- Record active callers and decide which are replaced, deleted, or left as historical references.
- Confirm no hidden TypeScript/package references exist.

**Done when:** the implementation has a precise replacement/removal list and no unclassified active reference remains.

### T2: SQLite knowledge coverage replaces in-memory coverage

**Depends on:** T1

**Touches:** `adapters/knowledge/sqlite/tests/repository.rs`

**Tests:**
- TDD/integration: add missing SQLite assertions for any behavior covered only by `adapters/knowledge/inmem/tests/knowledge_graph_repository.rs`.
- Goal-based: `cargo test -p engram-store-knowledge-sqlite --test repository`.

**Approach:**
- Compare the in-memory knowledge tests against SQLite tests.
- Add only behavior-level assertions that matter after retirement: source-owned chunk visibility, graph neighbors, limit ordering, ontology lookup/validation, and scope-hidden reads.

**Done when:** SQLite tests cover the retired crate's active behavioral obligations and pass.

### T3: Ingestion tests use SQLite knowledge store

**Depends on:** T2

**Touches:** `adapters/ingest/Cargo.toml`, `adapters/ingest/tests/scanner.rs`, `adapters/ingest/tests/extractor.rs`

**Tests:**
- TDD/integration: `cargo test -p engram-ingest --test scanner`.
- TDD/integration: `cargo test -p engram-ingest --test extractor`.

**Approach:**
- Replace `engram-store-knowledge-memory` dev-dependency with
  `engram-store-knowledge-sqlite`.
- Replace `InMemoryKnowledgeStore::new()` test fixtures with
  `SqlKnowledgeStore::open_in_memory().expect("open store")`.
- Keep the tests trait-driven; do not introduce adapter-specific test behavior
  beyond construction.

**Done when:** ingest tests pass against SQLite and no ingest manifest imports the retired crate.

### T4: Retired crate is removed from active workspace

**Depends on:** T3

**Touches:** `Cargo.toml`, `Cargo.lock`, `adapters/knowledge/inmem/**`

**Tests:**
- Goal-based: `cargo metadata --no-deps` succeeds and does not list
  `engram-store-knowledge-memory`.
- Goal-based: `cargo tree -i engram-store-knowledge-memory` fails because no
  package by that name exists, or reports no reverse dependencies if Cargo still
  sees lockfile history before regeneration.

**Approach:**
- Remove `adapters/knowledge/inmem` from workspace members.
- Delete the retired crate directory after active dependencies are gone.
- Regenerate `Cargo.lock` through normal Cargo commands; do not hand-edit
  generated lockfile content.

**Done when:** the crate cannot be built as a workspace package and no active package depends on it.

### T5: Retirement guard prevents reintroduction

**Depends on:** T4

**Touches:** `.codex/hooks/check-knowledge-inmem-retired.sh`, `.codex/hooks/README.md`

**Tests:**
- Goal-based: run the new retirement hook.
- Goal-based: run `.codex/hooks/check-knowledge-sqlite-isolation.sh`.

**Approach:**
- Add a small shell check that scans active manifests and source code for
  `engram-store-knowledge-memory` and `InMemoryKnowledgeStore`, excluding
  historical docs and this spec.
- Document the hook alongside existing local checks.

**Done when:** the hook passes on the retired workspace and would fail on an active code/manifests reintroduction.

### T6: Current docs and instructions describe the retired state

**Depends on:** T4

**Touches:** `AGENTS.md`, `docs/architecture.md`, `docs/specs/README.md`, `README.md`, `ROADMAP.md`, `adapters/AGENTS.md`

**Tests:**
- Goal-based: `rg "knowledge/inmem|engram-store-knowledge-memory|InMemoryKnowledgeStore" AGENTS.md README.md ROADMAP.md docs/architecture.md docs/specs/README.md adapters --glob '!adapters/knowledge/inmem/**'` returns no active-current references except explicit retirement notes.
- Goal-based: `.codex/hooks/check-docs.sh`.

**Approach:**
- Remove `adapters/knowledge/inmem` from current target-shape diagrams.
- Update active documentation to name SQLite as the local knowledge conformance
  store.
- Leave historical shipped specs and research notes intact unless they claim the
  adapter is active current architecture.

**Done when:** current docs do not instruct contributors to use or maintain the retired crate.

### T7: Workspace gates are green

**Depends on:** T1-T6

**Touches:** none

**Tests:**
- Goal-based: `cargo fmt --all --check`.
- Goal-based: `cargo check --workspace`.
- Goal-based: `cargo test --workspace`.
- Goal-based: `.codex/hooks/check-contracts.sh`.
- Goal-based: `.codex/hooks/check-docs.sh`.
- Goal-based: `.codex/hooks/check-knowledge-sqlite-isolation.sh`.
- Goal-based: `.codex/hooks/check-knowledge-inmem-retired.sh`.

**Approach:**
- Run the full relevant Rust and documentation gate set after the deletion.
- Fix only regressions caused by this retirement.

**Done when:** all listed gates pass or any unrelated pre-existing failure is documented with evidence.

## Rollout

This is a single workspace cleanup with no runtime deployment or user data
migration. Rollback is restoring the crate and its workspace/dependency entries
from git if SQLite parity fails before merge. Once merged, reintroducing a
process-local knowledge store requires a new spec or ADR because the retirement
guard makes the old crate name an explicit non-goal.

## Risks

- SQLite tests may expose behavior gaps that the in-memory adapter tolerated.
  Those are real parity bugs and should be fixed in SQLite, not papered over by
  keeping the duplicate crate.
- Full `cargo test --workspace` may be slower after ingest tests use SQLite, but
  `open_in_memory()` should keep the cost small.
- Docs contain many historical mentions. Over-cleaning them would erase useful
  implementation history; under-cleaning active docs would confuse future
  agents. The plan separates current docs from historical records.

## Changelog

- 2026-07-02: initial full spec and implementation plan drafted from local
  architecture/RFC/ADR context, the prior-art survey, current code references,
  and SQLite primary documentation.
- 2026-07-02: shipped by replacing active in-memory knowledge test usage with
  SQLite, removing the retired crate, adding the retirement guard, and updating
  current architecture/docs references.
