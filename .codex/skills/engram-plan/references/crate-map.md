# Planned Crate And Package Map

This map reflects the current post-inmem workspace and the architecture target
from `docs/research/architecture-design-v2.md`. Change public boundaries through
an ADR or a spec that names the affected crates.

## Rust Workspace

### Core

- `engram-domain`: portable domain types, invariants, serde, and version
  markers. No infrastructure dependencies.
- `engram-runtime`: shared portable errors, result type, clocks, id generation,
  scope matching, and policy authorizer traits.
- `engram-memory`: memory service and repository ports for write, retrieve,
  forget, idempotency, and lifecycle events.
- `engram-knowledge`: source-grounded knowledge, graph, ontology, taxonomy,
  source reader, chunker, and ingestion ports.
- `engram-retrieval`: storage-neutral retrieval indexes, fusion, context
  composition, and prediction helpers.
- `engram-belief`: belief and contradiction ports.
- `engram-hierarchy`: hierarchy repository, construction, and navigation ports.
- `engram-consolidation`: dry-run and gated consolidation services, executor
  port, planning, evaluation gate, and validation helpers.
- `engram-eval`: fixtures, deterministic harness, recall/leakage/ranking
  assertions, and report summaries.
- `engram-core`: compatibility facade and re-export layer only. It must not
  become the canonical owner of memory, knowledge, retrieval, belief,
  hierarchy, consolidation, or eval behavior again.

### Adapters

- `engram-ingest`: current mixed filesystem/Git source reader and deterministic
  ingestion crate. Split source adapters and pure ingestion orchestration when
  those boundaries need independent release/test cycles.
- `engram-store-sql`: SQLite memory persistence and `MemoryService`
  conformance adapter.
- `engram-store-knowledge-sqlite`: SQLite knowledge, graph, taxonomy, ontology,
  and graph retrieval index adapter.
- `engram-store-vector`: sqlite-vec retrieval index and optional FastEmbed query
  provider.
- `engram-store-belief-sqlite`: SQLite belief and contradiction repository
  adapter. Target path: `adapters/belief/sqlite`.
- `engram-store-hierarchy-sqlite`: SQLite hierarchy repository adapter.

The retired `engram-store-memory` and `engram-store-knowledge-memory` crates
must not re-enter the workspace without a new ADR/spec.

### Bindings

- `engram-node`: N-API bridge exposing stable Rust behavior to TypeScript as a
  JSON transport, not a second implementation.

## TypeScript Workspace

- `@engram/contracts`: generated types and JSON schemas from accepted contracts.
- `@engram/client`: ergonomic TypeScript SDK for application callers.
- `@engram/node`: native binding package wrapping `engram-node`.
- `@engram/adapters`: optional JS-side integrations for frameworks, tools, and
  gateway code.
- `@engram/eval`: fixture authoring helpers and CLI wrappers around the Rust
  eval harness.

## Boundary Rules

- Domain contracts stay independent of storage engines, model providers,
  language bindings, and runtime frameworks.
- Memory, knowledge, retrieval, belief, hierarchy, consolidation, and evaluation
  remain distinct unless an ADR explicitly changes the model.
- Adapters implement core ports and translate infrastructure-specific failures
  into stable runtime errors; they do not define domain truth.
- Retrieval composition stays store-free. New retrieval sources implement
  `RetrievalIndex`; fusion remains in `engram-retrieval`.
- TypeScript composes application workflows and exposes ergonomic APIs; Rust
  owns deterministic behavior.
- Generated contracts are reproducible from source and must not be hand-edited.
