# Plan: Memory Knowledge Boundaries

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

Split the current core-owned memory and knowledge ports into dedicated crates
without introducing any concrete graph or database dependency. The first slice
adds a small shared runtime crate for portable errors/dependencies, moves
canonical trait ownership, keeps `engram-core` compatible through re-exports,
and extends the draft knowledge graph contract with ontology types and ports.
Adapters keep compiling through existing imports while the new crate boundaries
become the canonical homes.

## Constraints

- ADR-0003 keeps Rust 2024 as the core implementation stack and TypeScript as
  generated bindings/SDK.
- RFC-0001 keeps memory policy, provenance, retrieval, consolidation, and
  evaluation in scope.
- RFC-0002 requires code/doc knowledge sources to remain separate from agent
  memory and compose only through retrieval.
- `docs/domain-data-model.md` is the source of truth for contract semantics.

## Construction tests

**Integration tests:** `cargo check --workspace`; `cargo test --workspace` when
compile is green.
**Manual verification:** confirm `engram-core` no longer defines canonical
memory or knowledge ports directly.

## Design (LLD)

### Design decisions

- Create `engram-memory` for memory service/repository ports and memory-facing
  dependency traits. Traces to: AC1, AC3.
- Create `engram-runtime` for shared result/error and dependency traits so
  memory, knowledge, and core do not depend on each other for primitives.
  Traces to: AC4.
- Create `engram-knowledge` for knowledge repositories, knowledge graph
  repositories, ontology repositories, source readers, chunkers, and ingestion
  service ports. Traces to: AC2, AC5.
- Keep `engram-core` as a compatibility facade by re-exporting split ports
  while retaining retrieval, consolidation, hierarchy, belief, and evaluation
  orchestration ports until later specs split them. Traces to: AC3.
- Add typed ontology domain models instead of using metadata escape hatches.
  Traces to: AC5.

### Data & schema

- `engram-domain` adds draft extension types for knowledge graphs and
  ontologies. Accepted v1 JSON schemas remain unchanged.
- Ontology records include scope, policy, provenance, timestamps, imports,
  classes, properties, axioms, constraints, and validation findings.

### Interfaces & contracts

- `engram-memory` exposes `MemoryRepository`, `MemoryEventRepository`, and
  `MemoryService`.
- `engram-knowledge` exposes `KnowledgeRepository`,
  `KnowledgeGraphRepository`, `OntologyRepository`, `SourceReader`, `Chunker`,
  and `IngestionService`.
- `engram-core` re-exports those traits for compatibility during the transition.

### Component / module decomposition

- `crates/engram-memory`: memory-only behavior ports.
- `crates/engram-knowledge`: knowledge, graph, ontology, source, and ingestion
  behavior ports.
- `crates/engram-runtime`: shared error/result and dependency traits.
- `crates/engram-core`: orchestration facade and remaining cross-cutting ports.
- `crates/engram-domain`: storage-neutral contract types.

### Failure, edge cases & resilience

- Downstream adapters keep compiling through `engram-core` re-exports during
  the first split.
- No concrete Neo4j, PostgreSQL, RDF, vector, or embedding dependency is added
  in this slice.

## Tasks

### T1: Add canonical memory crate

**Depends on:** none

**Tests:**
- `cargo check -p engram-memory` proves memory ports compile independently.
- `cargo check -p engram-core` proves compatibility re-exports compile.

**Approach:**
- Add `crates/engram-memory`.
- Move canonical memory service/repository/dependency traits out of
  `engram-core`.
- Re-export the memory crate from `engram-core`.

**Done when:** memory ports compile from `engram-memory` and existing downstream
imports through `engram-core` still compile.

### T2: Add canonical knowledge and ontology crate

**Depends on:** T1

**Tests:**
- `cargo check -p engram-knowledge` proves knowledge graph and ontology ports
  compile independently.
- Domain serialization tests cover representative ontology and graph records.

**Approach:**
- Add `crates/engram-knowledge`.
- Move canonical knowledge repository, source reader, chunker, and ingestion
  service traits out of `engram-core`.
- Add `KnowledgeGraphRepository` and `OntologyRepository` traits.
- Re-export the knowledge crate from `engram-core`.

**Done when:** knowledge ports compile from `engram-knowledge`, including graph
and ontology surfaces, and existing ingestion code compiles.

### T3: Extend domain model for knowledge graph ontologies

**Depends on:** T2

**Tests:**
- Add Rust serde tests for ontology and knowledge graph records.
- Run `.codex/hooks/check-contracts.sh`.

**Approach:**
- Add storage-neutral ontology and graph identity types to `engram-domain`.
- Update `docs/domain-data-model.md` with graph and ontology semantics.
- Keep accepted v1 schemas unchanged because these are draft extension
  contracts.

**Done when:** contract docs and Rust domain types describe ontology-backed
knowledge graphs without storage-specific fields.

### T4: Wire workspace and phase ledger

**Depends on:** T1-T3

**Tests:**
- `cargo fmt --all --check`
- `cargo check --workspace`
- `cargo test --workspace`
- `.codex/hooks/check-docs.sh`

**Approach:**
- Add new crates to the workspace.
- Add the phase entry to `docs/implementation/phases.json`.
- Mark this spec shipped only after gates pass.

**Done when:** repository gates pass and the spec acceptance criteria are
checked.

### T5: Add in-memory graph and ontology baseline

**Depends on:** T2, T3

**Tests:**
- `cargo test -p engram-store-memory --test knowledge_graph_repository`
  proves graph and ontology repository behavior is executable.

**Approach:**
- Add graph, entity, relationship, ontology, class, property, and axiom maps to
  the in-memory adapter state.
- Implement `KnowledgeGraphRepository` and `OntologyRepository` for
  `InMemoryMemoryService`.
- Extend `KnowledgeRepository` to round-trip entities and relationships.

**Done when:** scoped graph lookup, neighbor traversal, and visible
graph/ontology validation tests pass.

## Rollout

This is a source-compatible crate-boundary split. Existing imports through
`engram-core` continue to work in this slice. Future specs may migrate adapters
to import directly from `engram-memory` and `engram-knowledge`.

## Risks

- Trait moves can create circular dependencies if `engram-core` remains the
  owner of shared errors or dependency traits.
- Ontology modeling can sprawl into RDF/OWL implementation details; this slice
  keeps the model storage-neutral and adapter-independent.
- Re-export compatibility can hide remaining coupling; follow-up specs should
  migrate adapters to direct crate imports once this split is stable.

## Changelog

- 2026-06-30: initial plan.
