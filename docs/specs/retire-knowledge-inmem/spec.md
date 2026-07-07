# Spec: Retire Knowledge In-Memory Adapter

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0005, ADR-0006, ADR-0008, RFC-0002, RFC-0003, RFC-0004, `docs/specs/memory-knowledge-boundaries`, `docs/specs/sqlite-knowledge-graph`
- **Brief:** none
- **Contract:** none (workspace/test-harness cleanup; no v1 wire-contract change)
- **Shape:** mixed (data + build-system + documentation)

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram uses `engram-store-knowledge-sqlite` as the only executable knowledge
store for local conformance, ingestion, graph, taxonomy, and ontology tests, and
removes the process-local `engram-store-knowledge-memory` crate from the Rust
workspace. The result is less duplicate adapter behavior: tests still run
quickly through `SqlKnowledgeStore::open_in_memory()`, but they exercise the same
SQLite-backed repository implementation used by the demo and durable knowledge
paths.

## Boundaries

### Always do

- Prove SQLite knowledge parity for every currently load-bearing in-memory
  knowledge use before deleting the crate: source/document/chunk persistence,
  graph identity and neighbor traversal, taxonomy, ontology round trips,
  advisory ontology validation, and scope isolation.
- Repoint `engram-ingest` and any other active callers from
  `InMemoryKnowledgeStore` to `SqlKnowledgeStore::open_in_memory()` or a
  temporary SQLite file, depending on the test's persistence need.
- Remove `adapters/knowledge/inmem` from the Cargo workspace, dependency graph,
  and active documentation once no active crate imports
  `engram-store-knowledge-memory`.
- Keep historical specs, changelog entries, and research notes readable as
  history; mark the in-memory knowledge adapter as retired rather than rewriting
  past shipped slices.
- Preserve the existing adapter isolation rule: knowledge SQLite must not depend
  on memory SQLite, memory in-memory, vector SQLite, or any sibling store
  adapter.

### Ask first

- Retiring `engram-store-memory` or changing memory, belief, hierarchy, or
  consolidation test harnesses.
- Changing `engram-knowledge` trait signatures or `engram-domain` knowledge,
  taxonomy, or ontology types.
- Adding a new conformance harness crate, migration framework, database engine,
  graph database, or vector dependency.
- Deleting historical docs that mention the old in-memory adapter as part of
  shipped work.

### Never do

- Delete the in-memory knowledge crate while an active crate, test, example, or
  validation hook still depends on it.
- Replace the crate with another fake or mock knowledge implementation.
- Move knowledge graph, ontology, taxonomy, source ingestion, vector, memory, or
  belief responsibilities into a shared catch-all adapter.
- Weaken scope filtering, inherited-source visibility, advisory ontology
  validation, or forbidden-import checks to make the deletion easier.
- Change accepted v1 JSON schemas, generated TypeScript contracts, or public
  memory wire behavior.

## Testing Strategy

- **TDD / integration:** SQLite knowledge repository tests cover the behavioral
  surface that made the in-memory crate load-bearing: source-owned chunk scope,
  graph neighbor traversal, list APIs, taxonomy scope inheritance, ontology
  round trips, advisory validation, and hidden-scope behavior. Integration is
  the right level because the risk is store behavior across the real SQLite
  adapter boundary, not pure domain construction.
- **TDD / integration:** ingestion scanner and graph-extractor tests run against
  `SqlKnowledgeStore::open_in_memory()` so source ingestion, entity extraction,
  relationship writes, chunk entity refs, and incremental scan behavior are
  proven without the removed crate.
- **Goal-based check:** `cargo tree`, `rg`, and the forbidden-import hook prove
  no active workspace member depends on or imports
  `engram-store-knowledge-memory`, and `engram-store-knowledge-sqlite` remains
  isolated from sibling adapters.
- **Goal-based repository gates:** Rust format/check/tests plus docs and
  contract hooks prove the workspace remains coherent and no public contract
  drift was introduced.

## Acceptance Criteria

- [x] No active Cargo workspace member, test, example, or package manifest
  depends on `engram-store-knowledge-memory`.
- [x] `adapters/knowledge/inmem` is removed from the workspace after its active
  usages are replaced by SQLite-backed tests.
- [x] `engram-ingest` scanner and graph-extractor tests pass while using
  `SqlKnowledgeStore::open_in_memory()` or a temporary SQLite file instead of
  `InMemoryKnowledgeStore`.
- [x] SQLite knowledge tests prove source/document/chunk, entity/relationship,
  graph traversal, taxonomy, ontology, advisory validation, and scope-isolation
  behavior formerly covered by the in-memory knowledge adapter.
- [x] Active architecture docs and local instructions describe the retired
  adapter accurately, while historical shipped specs and research notes remain
  intact as history.
- [x] `.codex/hooks/check-knowledge-sqlite-isolation.sh` passes, and an added or
  documented retirement check proves `engram-store-knowledge-memory` has not
  re-entered active dependencies.
- [x] `cargo fmt --all --check`, `cargo check --workspace`, relevant Rust tests,
  `.codex/hooks/check-contracts.sh`, and `.codex/hooks/check-docs.sh` pass.
- [x] No v1 contract schemas, generated TypeScript contracts, or public
  `engram-knowledge` port signatures change.

## Assumptions

- Technical: `SqlKnowledgeStore` already implements `KnowledgeRepository`,
  `KnowledgeGraphRepository`, `TaxonomyRepository`, and `OntologyRepository`
  (source: `adapters/knowledge/sqlite/src/service.rs`).
- Technical: current SQLite repository tests already cover graph, chunk scope,
  taxonomy, ontology, validation, and list filtering behavior (source:
  `adapters/knowledge/sqlite/tests/repository.rs`).
- Technical: active direct imports of `InMemoryKnowledgeStore` are limited to
  the in-memory crate's own tests and `engram-ingest` tests (source: `rg
  "InMemoryKnowledgeStore" --glob '!target/**'`).
- Technical: SQLite supports fast throwaway test stores without a second domain
  implementation through `:memory:` databases, which are private to the opening
  connection and deleted when it closes (source:
  <https://sqlite.org/inmemorydb.html>).
- Technical: temporary SQLite data may use memory or disk depending on SQLite
  configuration and pressure, so tests must not assert that a temporary database
  is purely in RAM unless they explicitly use `open_in_memory()` (source:
  <https://sqlite.org/tempfiles.html>).
- Process: prior research recommends "promote SQLite to the single
  implementation, then delete inmem" rather than deleting in-memory stores
  before SQLite carries the test harness (source:
  `docs/research/graphmind-prior-art-survey.md`).
- Process: this spec retires only the knowledge in-memory adapter; memory,
  belief, hierarchy, and consolidation retirement require separate specs because
  they have different ports and evaluation dependencies (source: AGENTS.md
  boundary rules).
