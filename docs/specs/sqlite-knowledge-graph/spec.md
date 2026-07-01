# Spec: SQLite knowledge-graph + taxonomy adapter (demo Slice 1)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0003, ADR-0003, ADR-0006, ADR-0007, `docs/specs/memory-knowledge-boundaries`, `docs/specs/sql-service-conformance`
- **Contract:** none (no v1 schema change; `TaxonomyRepository` is a new core port, additive)
- **Shape:** data

> **Spec contract:** defines what "done" means for this slice.

## Objective

Engram stores source-grounded knowledge durably behind its existing ports. This
slice adds `engram-store-knowledge-sqlite` — a SQLite adapter implementing
`KnowledgeRepository`, `KnowledgeGraphRepository`, and a new `TaxonomyRepository`
port — so a demo (and later production) can persist sources, documents, chunks,
entities, relationships, graphs, and taxonomy concept schemes/concepts/relations
across restarts. `OntologyRepository` is deliberately deferred (the demo is
taxonomy-only). Each storage concern stays behind its own crate: the adapter must
not depend on the memory or vector adapters.

## Boundaries

### Always do
- Mirror the `engram-store-sql` pattern: store contract payloads losslessly as
  JSON, index scope and lookup columns, translate SQLite errors to
  `CoreError::Adapter`.
- Apply scope visibility exactly as the in-memory knowledge adapter does:
  scoped records filtered directly; chunks/documents/concepts/relations inherit
  visibility from their owning source or concept scheme.
- Keep `engram-store-knowledge-sqlite` free of any dependency on
  `engram-store-sql`, `engram-store-vector`, or `engram-store-memory` (the
  forbidden-import gate enforces it).

### Ask first
- Adding the `TaxonomyRepository` port to `engram-knowledge` (a new core contract
  surface) — pre-authorized by RFC-0003 D4 / ADR-0007.
- Sharing one SQLite file/connection with the memory adapter (contingent on
  RFC-0003 OQ2; this slice uses an independent connection).

### Never do
- Implement `OntologyRepository` (deferred — taxonomy only).
- Put SQL, async-runtime, vector, or embedding concerns in `engram-domain` or
  `engram-knowledge`.
- Change v1 contract fields or generated TypeScript types.
- Couple knowledge persistence to memory or vector persistence (cross-adapter
  SQL, shared connection objects across crate boundaries).

## Testing Strategy

- **TDD / integration (Rust):** round-trip tests prove each port —
  `KnowledgeRepository` (source/document/chunk with inherited scope; entity;
  relationship), `KnowledgeGraphRepository` (graph get + neighbors with scope and
  limit), `TaxonomyRepository` (scheme get + concept list with scope, relation
  put). Why integration: the invariant is "store → reload → scope-filter
  faithfully", best proved across the SQLite boundary, not by a unit.
- **Goal-based check:** the forbidden-import gate
  (`.codex/hooks/check-knowledge-sqlite-isolation.sh`) proves crate isolation.

## Acceptance Criteria

- [x] `engram-store-knowledge-sqlite` compiles and implements
  `KnowledgeRepository`, `KnowledgeGraphRepository`, and `TaxonomyRepository`.
- [x] `TaxonomyRepository` (concept scheme / concept / relation / list) is a port
  in `engram-knowledge`, implemented by the SQLite adapter.
- [x] Scope visibility matches the in-memory adapter: scoped records filtered
  directly; chunks/concepts inherit from their owner (tested).
- [x] The forbidden-import gate passes (no dep on `engram-store-sql` /
  `engram-store-vector` / `engram-store-memory`).
- [x] `cargo fmt --all --check`, `cargo clippy --workspace -- -D warnings`,
  `cargo test --workspace`, and the contract/docs hooks pass with no drift.
- [x] `NativeKnowledgeEngine` exposes knowledge + taxonomy over the binding; demo
  backend (`/knowledge/*`, `/taxonomy/*`) + UI maintain taxonomy. (Graph
  visualization from real ingestion lands in Slice 2.)

## Assumptions

- Technical: `engram-store-sql` is the SQLite-adapter template (JSON blob + scope
  index; `open_in_memory`/`open_file`; `scope_allows`) (source:
  `adapters/memory/sqlite/src/{schema,service,scope}.rs`).
- Technical: knowledge port method shapes and scope semantics are defined by the
  in-memory reference adapter (source: `adapters/knowledge/inmem/src/lib.rs`).
- Technical: `Concept`/`ConceptRelation` have no own scope; visibility is the
  scheme's (source: `core/domain/src/taxonomy.rs`).
- Process: `TaxonomyRepository` is an additive core port; ADR-0007 authorizes the
  binding surface growth that consumes it (source: ADR-0007; user confirmation
  2026-06-30).
