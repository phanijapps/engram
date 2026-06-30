# Spec: Memory Knowledge Boundaries

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, RFC-0001, RFC-0002
- **Brief:** none
- **Contract:** `docs/domain-data-model.md`, `core/domain`, `core/runtime`, `core/memory`, `core/knowledge`
- **Shape:** mixed

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram exposes memory and knowledge as separate Rust crate boundaries so
durable memory storage can use one backend while knowledge graph, ontology,
source, document, chunk, vector, or graph storage uses another. Memory owns
agent state across time. Knowledge owns source-grounded content, entities,
relationships, ontologies, and graph structure. Retrieval composes both
without forcing either side into the other's persistence model.

## Boundaries

### Always do

- Keep memory ports and knowledge ports in separate crates with storage-neutral
  trait surfaces.
- Treat ontologies as part of the knowledge graph contract, not as taxonomy
  metadata or adapter-specific payloads.
- Keep retrieval composition outside concrete memory and knowledge stores.

### Ask first

- Changing accepted v1 memory wire schemas.
- Adding a concrete Neo4j, PostgreSQL, RDF, or vector database dependency.
- Renaming public domain fields or changing enum meanings.

### Never do

- Do not make a memory adapter persist knowledge graph or ontology state by
  default.
- Do not make a knowledge graph adapter depend on SQL memory internals.
- Do not hide graph ontology semantics in untyped metadata when a typed contract
  is required.
- Do not create a god crate that owns domain, orchestration, storage, retrieval,
  graph modeling, ontology validation, and adapter translation together.

## Testing Strategy

- Crate-boundary checks use goal-based verification through `cargo check
  --workspace`; this proves the split compiles through downstream adapters.
- Contract model additions use TDD through focused Rust unit tests for
  serialization and typed ontology/graph construction.
- Documentation and compatibility checks use goal-based repository hooks to
  ensure the spec, domain model, and contracts remain coherent.

## Acceptance Criteria

- [x] `engram-memory` owns memory service and repository ports without owning
  knowledge source, graph, ontology, vector, or ingestion ports.
- [x] `engram-knowledge` owns knowledge source, document, chunk, entity,
  relationship, graph, and ontology ports without owning memory write,
  lifecycle event, or forget services.
- [x] `engram-core` remains an orchestration facade that may re-export ports for
  compatibility but does not define the canonical memory or knowledge port
  contracts.
- [x] Shared runtime errors and dependency traits live below memory, knowledge,
  and core so the split does not create circular dependencies.
- [x] The knowledge graph contract includes typed ontology support for graph
  identity, ontology identity, classes, properties, axioms, constraints,
  imports, provenance, scope, policy, and validation findings.
- [x] Existing in-memory, SQL, vector, ingestion, TypeScript, and evaluation
  crates compile against the split boundaries.
- [x] The domain model documents memory storage, knowledge graph storage, vector
  storage, and retrieval composition as separate replaceable concerns.
- [x] The in-memory knowledge adapter provides a testable knowledge graph and
  ontology repository baseline with scoped graph lookup, neighbor traversal, and
  ontology validation entry points.
- [x] The in-memory memory adapter no longer owns graph or ontology state; it
  keeps source/document/chunk records only as a quick retrieval fixture.

## Assumptions

- Technical: Rust workspace uses Rust 2024 and currently lacks separate
  memory/knowledge crates (source: `Cargo.toml`).
- Technical: memory and knowledge are explicitly distinct in the accepted
  domain principles (source: `docs/domain-data-model.md`).
- Technical: knowledge source extension requires memory and knowledge retrieval
  to compose without merging storage concerns (source:
  `docs/rfcs/0002-knowledge-source-extension.md`).
- Technical: current `engram-core` owns both memory and knowledge repository
  ports, so the split moves canonical ports out of core (source:
  `core/orchestration/src/lib.rs`).
- Process: contract-affecting changes update the domain model and run contract
  checks (source: `.codex/skills/engram-contract/SKILL.md`).
- Product: ontology support is mandatory in the knowledge graph contract and
  implementation proceeds without additional approval (source: user
  confirmation 2026-06-30).
