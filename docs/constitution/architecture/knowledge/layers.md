# Layer responsibilities (core, adapters, integration, N-API)

<!-- prerequisite: pipeline -->

## Layer responsibilities

Engram is layered so that **deterministic behavior is engine-neutral and lives
in the Rust core**, while **infrastructure (SQL, vectors, models, runtimes)
lives behind replaceable adapters**. A neutrality lint enforces that no engine
type (`Sql*`, `Surreal*`, …) leaks into the neutral layers — this is what makes
backend swap-by-config real ([ADR-0022](../adr/0022-engine-grid-vs-backend-recipe.md)).

### Core libraries (`core/`) — the deterministic brain, engine-neutral

Each crate owns one concern behind a trait; none may name SQL, a vector store,
an embedding provider, an async runtime, Node, or TypeScript.

| Crate | Owns | Key surface |
| --- | --- | --- |
| `engram-domain` | portable domain types, invariants, serde, version markers | `MemoryRecord`, `KnowledgeChunk`, `Belief`, `HierarchyNode`, `Policy`, `Provenance` |
| `engram-runtime` | shared primitives only | `CoreError`/`CoreResult`, clocks, ids, scope matching, policy authorizer traits |
| `engram-memory` | memory service + repository ports | `MemoryService`, `MemoryRepository` |
| `engram-knowledge` | source-grounded knowledge, graph, ontology, taxonomy, ingestion ports | `KnowledgeRepository`, `KnowledgeGraphRepository`, `TaxonomyRepository`, `OntologyRepository` |
| `engram-belief` | belief synthesis, contradiction, bi-temporal ports | `BeliefRepository`, `BeliefQuery` |
| `engram-hierarchy` | hierarchy build, navigation, aggregate ports | `HierarchyRepository`, `navigation::navigate` |
| `engram-consolidation` | consolidation planning, gated mutation, decay, audit | consolidation executors |
| `engram-reflection` | reflection synthesizer + consolidation executor (derived beliefs) | reflection pipeline |
| `engram-retrieval` | retrieval composition + fusion ports (store-free seam) | `VectorIndex`, `RetrievalIndex` → `RetrievalFusion` (RRF) → `ContextComposer` |
| `engram-integration` | the SDK facade | `EngramProvider`, `EngramConfig`, `CapabilityReport` |
| `engram-eval` | deterministic fixtures + regression harness | eval fixtures |
| `engram-graph-analytics` | pure, dependency-free graph algorithms | PageRank, betweenness, communities, reachability |

### Adapters (`adapters/`) — replaceable infrastructure, behind the core ports

Two kinds: **engine cells** (one crate per storage engine, implementing the
storage-backed ports) and **engine-agnostic adapters** (retrieval modes,
consolidation executors, ingestion — shared across all engines).

| Crate | Kind | Implements |
| --- | --- | --- |
| `engram-store-sqlite` | engine cell (SQLite) | memory + knowledge/taxonomy/ontology + belief + hierarchy + vector (sqlite-vec) — **all SQLite calls live here** |
| `engram-store-lexical` | agnostic (Tantivy) | `RetrievalMode::keyword` (BM25) |
| `engram-store-associative-graph` | agnostic | `RetrievalMode::Graph` (Personalized PageRank, HippoRAG-style) |
| `engram-store-community-summary` | agnostic | community-summary retrieval (GraphRAG) |
| `engram-rerank-cross-encoder` | agnostic | `RerankStrategy::cross_encoder` (injected scorer) |
| `engram-consolidation-decay` | agnostic | policy-expiry + Ebbinghaus-curve decay |
| `engram-ingest` | agnostic | filesystem/git ingestion + tree-sitter AST chunking |

Adding a new engine is a new cell — see
[Extending the storage layer](../guides/how-to/extend-storage.md).

### Integration facade (`engram-integration`) — the one entry point

`EngramProvider` is the canonical Rust SDK entry: a thin facade that holds
`Arc<dyn>` handles to every port and reports what is wired via
`CapabilityReport`. `EngramConfig` selects the backend by a config string + Cargo
feature; `bootstrap_sqlite` / `bootstrap_surreal` compose the right cells into a
provider. A consumer changes **one config line** to swap SQLite for SurrealDB —
no code change, no migration (fresh store on switch). Every capability is
reachable from here.

### N-API binding (`bindings/node` → `engram-node`) — the TypeScript transport

The N-API bridge is a **transport over Rust behavior, not a second
implementation**: JSON in, JSON out, delegating to the same `EngramProvider`
surface. It is what `@engram/node`, `@engram/client`, and the demo consume.
Surface parity ([AGENTS.md](../../AGENTS.md)) requires every capability reachable
from the Rust facade to also be reachable from the N-API binding — wiring one
surface and leaving the other unwired is not allowed.

